// SPDX-License-Identifier: AGPL-3.0-or-later

//! Version-dispatched native recipe reader.

use krausen_types::brewery::RecipeDocumentV1;

/// A loss-aware legacy conversion, prepared before the transaction writes its
/// revision and immutable provenance together.
pub struct PreparedBackfill {
    pub document_json: String,
    pub content_digest: String,
    pub source_record_json: String,
    pub diagnostics_json: String,
}

#[derive(Debug, thiserror::Error)]
pub enum BackfillError {
    #[error("legacy recipe is unreadable: {0}")]
    Legacy(#[from] serde_json::Error),
    #[error("native recipe cannot be canonicalized: {0}")]
    Encode(#[from] EncodeError),
}

#[derive(Debug, thiserror::Error)]
pub enum EncodeError {
    #[error("unsupported native recipe or equipment schema version")]
    UnsupportedVersion,
    #[error(transparent)]
    Canonical(#[from] krausen_core::canonical_json::CanonicalError),
}

/// Canonical bytes and content identity for a structurally valid native recipe.
pub fn encode_document(document: &RecipeDocumentV1) -> Result<(String, String), EncodeError> {
    if document.schema_version != 1 || document.equipment_overrides.schema_version != 1 {
        return Err(EncodeError::UnsupportedVersion);
    }
    let value = krausen_core::canonical_json::from_serializable(document)?;
    let digest = krausen_core::canonical_json::sha256(&value)?;
    let json = String::from_utf8(krausen_core::canonical_json::bytes(&value)?)
        .expect("canonical JSON is UTF-8");
    Ok((json, digest))
}

/// Preserve the parsed legacy record and report ambiguity; no actor or source
/// bytes are invented when the original database did not store them.
pub fn prepare_backfill(
    legacy_json: &str,
    provenance_id: &str,
) -> Result<PreparedBackfill, BackfillError> {
    let legacy: krausen_beerxml::Recipe = serde_json::from_str(legacy_json)?;
    let mut converted = krausen_beerxml::native::from_legacy(&legacy)?;
    converted.document.import_provenance_id = Some(provenance_id.to_owned());
    let (document_json, content_digest) = encode_document(&converted.document)?;
    Ok(PreparedBackfill {
        document_json,
        content_digest,
        source_record_json: legacy_json.to_owned(),
        diagnostics_json: serde_json::to_string(&converted.warnings)?,
    })
}

#[derive(Debug)]
pub enum ReadError {
    InvalidJson(serde_json::Error),
    MissingVersion,
    UnsupportedVersion(u64),
    UnsupportedEquipmentVersion(u32),
    InvalidDocument(serde_json::Error),
}

/// Never reinterpret an unknown version as the current schema.
pub fn read_document(json: &str) -> Result<RecipeDocumentV1, ReadError> {
    let value: serde_json::Value = serde_json::from_str(json).map_err(ReadError::InvalidJson)?;
    let version = value
        .get("schema_version")
        .and_then(serde_json::Value::as_u64)
        .ok_or(ReadError::MissingVersion)?;
    if version != 1 {
        return Err(ReadError::UnsupportedVersion(version));
    }
    let document: RecipeDocumentV1 =
        serde_json::from_value(value).map_err(ReadError::InvalidDocument)?;
    if document.equipment_overrides.schema_version != 1 {
        return Err(ReadError::UnsupportedEquipmentVersion(
            document.equipment_overrides.schema_version,
        ));
    }
    Ok(document)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unknown_versions_and_recovers_v1() {
        assert!(matches!(
            read_document("{\"schema_version\":2}"),
            Err(ReadError::UnsupportedVersion(2))
        ));
        assert!(matches!(
            read_document("{}"),
            Err(ReadError::MissingVersion)
        ));
        let original = RecipeDocumentV1::default();
        assert_eq!(
            read_document(&serde_json::to_string(&original).unwrap()).unwrap(),
            original
        );
    }

    #[test]
    fn prepares_backfill_with_source_and_diagnostics() {
        let recipe =
            krausen_beerxml::parse("<RECIPE><NAME>Legacy</NAME><VENDOR>kept</VENDOR></RECIPE>")
                .unwrap()
                .recipes
                .remove(0);
        let source = serde_json::to_string(&recipe).unwrap();
        let prepared = prepare_backfill(&source, "provenance-1").unwrap();
        assert_eq!(
            read_document(&prepared.document_json).unwrap().title,
            "Legacy"
        );
        assert_eq!(prepared.content_digest.len(), 64);
        assert_eq!(
            serde_json::from_str::<krausen_beerxml::Recipe>(&prepared.source_record_json).unwrap(),
            recipe
        );
        assert!(
            prepared
                .diagnostics_json
                .contains("original_bytes_unavailable")
        );
        assert!(prepare_backfill("not json", "provenance-1").is_err());
    }

    #[test]
    fn recipe_identity_rejects_non_finite_values_and_tracks_content() {
        let mut document = RecipeDocumentV1::default();
        let (_, first) = encode_document(&document).unwrap();
        document.title = "Changed".into();
        assert_ne!(first, encode_document(&document).unwrap().1);
        document.batch_size_l = Some(f64::NAN);
        assert!(matches!(
            encode_document(&document),
            Err(EncodeError::Canonical(
                krausen_core::canonical_json::CanonicalError::NonFinite
            ))
        ));
    }
}
