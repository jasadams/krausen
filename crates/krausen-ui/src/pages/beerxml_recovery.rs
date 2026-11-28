// SPDX-License-Identifier: AGPL-3.0-or-later
//! Browser-local retention of an already-sent BeerXML import command (INT-004).
//! The record is the original scoped request plus its request identity, never a
//! draft: it is written before dispatch, survives dismissal/navigation/reload,
//! and is removed only after a known outcome or an explicit named acknowledgement.
//!
//! This deliberately does not share `evidence_recovery.rs`: evidence keys many
//! concurrent writes by brew and request, keeps receipts and reviewed states and
//! uses a live-tab lock. Import is one finite command per installation/account/
//! workspace with no receipt; a second tab resending the same identity is safe
//! because the server replays by request id. Only the storage-key convention and
//! status vocabulary are mirrored.
use krausen_types::{interchange::ImportRequest, request_context::OperationIntent};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum RecoveryStatus {
    /// The command was sent and its result has not been confirmed.
    #[default]
    Unknown,
    /// The server no longer holds the original result; creation is not disproved.
    Expired { resource_id: Option<String> },
    /// The identity is bound to different content; resending cannot help.
    IdentityConflict,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(super) struct ImportRecovery {
    pub version: u8,
    pub installation: String,
    pub request: ImportRequest,
    pub request_id: String,
    #[serde(default)]
    pub status: RecoveryStatus,
}

impl ImportRecovery {
    pub fn new(installation: &str, request: &ImportRequest, request_id: &str) -> Self {
        Self {
            version: 1,
            installation: installation.to_owned(),
            request: request.clone(),
            request_id: request_id.to_owned(),
            status: RecoveryStatus::Unknown,
        }
    }
    pub fn matches(&self, installation: &str, intent: &OperationIntent) -> bool {
        self.version == 1
            && !self.request_id.is_empty()
            && self.installation == installation
            && self.request.intent == *intent
    }
}

/// JSON tuple encoding prevents ambiguous separators in provider-supplied IDs.
pub(super) fn key(installation: &str, intent: &OperationIntent) -> String {
    format!(
        "krausen-import-v1:{}",
        serde_json::to_string(&(installation, &intent.account_id, &intent.workspace_id))
            .expect("scope contains only strings")
    )
}

/// Decide what a retention write may do given any record already in storage.
/// An existing record is never replaced by different content or another identity.
pub(super) fn merge_retained(
    existing: Option<&str>,
    record: &ImportRecovery,
) -> Result<Option<String>, String> {
    let Some(existing) = existing else {
        return serde_json::to_string(record)
            .map(Some)
            .map_err(|_| "The import request cannot be retained.".to_owned());
    };
    let previous: ImportRecovery = serde_json::from_str(existing)
        .map_err(|_| "An earlier import request is retained but unreadable.".to_owned())?;
    if previous.request == record.request && previous.request_id == record.request_id {
        Ok(None)
    } else {
        Err("An earlier import request is still retained. Check its result first.".into())
    }
}

#[cfg(target_arch = "wasm32")]
fn storage() -> Result<web_sys::Storage, String> {
    web_sys::window()
        .ok_or("Browser unavailable")?
        .local_storage()
        .map_err(|_| "Import recovery storage is unavailable.")?
        .ok_or_else(|| "Import recovery storage is unavailable.".into())
}

/// Retain the exact request before it is dispatched. Failure must stop the send.
pub(super) fn retain(record: &ImportRecovery) -> Result<(), String> {
    #[cfg(target_arch = "wasm32")]
    {
        let storage = storage()?;
        let key = key(&record.installation, &record.request.intent);
        let existing = storage
            .get_item(&key)
            .map_err(|_| "Unable to read import recovery.")?;
        if let Some(value) = merge_retained(existing.as_deref(), record)? {
            storage
                .set_item(&key, &value)
                .map_err(|_| "Unable to retain the import request before sending.")?;
        }
    }
    let _ = record;
    Ok(())
}

pub(super) fn load(
    installation: &str,
    intent: &OperationIntent,
) -> Result<Option<ImportRecovery>, String> {
    #[cfg(target_arch = "wasm32")]
    {
        let Some(raw) = storage()?
            .get_item(&key(installation, intent))
            .map_err(|_| "Unable to read import recovery.")?
        else {
            return Ok(None);
        };
        let record = serde_json::from_str::<ImportRecovery>(&raw)
            .map_err(|_| "A retained import request is unreadable; no new import is permitted.")?;
        if !record.matches(installation, intent) {
            return Err(
                "A retained import request does not match its original scope; no new import is permitted."
                    .into(),
            );
        }
        return Ok(Some(record));
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (installation, intent);
        Ok(None)
    }
}

pub(super) fn set_status(
    installation: &str,
    intent: &OperationIntent,
    request_id: &str,
    status: RecoveryStatus,
) -> Result<(), String> {
    #[cfg(target_arch = "wasm32")]
    {
        let Some(mut record) = load(installation, intent)? else {
            return Ok(());
        };
        if record.request_id != request_id {
            return Ok(());
        }
        record.status = status.clone();
        let value = serde_json::to_string(&record).map_err(|_| "Unable to update recovery.")?;
        storage()?
            .set_item(&key(installation, intent), &value)
            .map_err(|_| "Unable to update the retained import status.")?;
    }
    let _ = (installation, intent, request_id, status);
    Ok(())
}

/// Explicit named escape for a record that cannot be read or matched. It removes
/// the scoped slot unconditionally and is only offered for that failure.
pub(super) fn discard_unreadable(
    installation: &str,
    intent: &OperationIntent,
) -> Result<(), String> {
    #[cfg(target_arch = "wasm32")]
    storage()?
        .remove_item(&key(installation, intent))
        .map_err(|_| "Unable to discard the unreadable retained request.")?;
    let _ = (installation, intent);
    Ok(())
}

/// Remove only the record for this exact request identity, so a late completion
/// can never erase recovery that belongs to a newer attempt.
pub(super) fn remove(
    installation: &str,
    intent: &OperationIntent,
    request_id: &str,
) -> Result<(), String> {
    #[cfg(target_arch = "wasm32")]
    if let Some(record) = load(installation, intent)? {
        if record.request_id == request_id {
            storage()?
                .remove_item(&key(installation, intent))
                .map_err(|_| "Unable to remove the resolved import recovery.")?;
        }
    }
    let _ = (installation, intent, request_id);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use krausen_types::{brewery::EquipmentSettingsV1, interchange::ImportChoices};

    fn intent(account: &str, workspace: &str) -> OperationIntent {
        OperationIntent {
            account_id: account.into(),
            workspace_id: workspace.into(),
        }
    }
    fn request(xml: &str) -> ImportRequest {
        ImportRequest {
            intent: intent("a", "w"),
            xml: xml.into(),
            recipe_index: 0,
            choices: ImportChoices::default(),
            equipment_profile_id: None,
            expected_profile_id: None,
            expected_no_profile: false,
            expected_profile_revision: None,
            expected_profile_metadata_revision: None,
            equipment_draft: EquipmentSettingsV1::default(),
            draft_generation: "g".into(),
        }
    }

    #[test]
    fn keys_bind_installation_account_and_workspace_without_ambiguity() {
        let base = key("https://x", &intent("a", "w"));
        assert_ne!(base, key("https://y", &intent("a", "w")));
        assert_ne!(base, key("https://x", &intent("b", "w")));
        assert_ne!(base, key("https://x", &intent("a", "v")));
        assert_ne!(
            key("https://x", &intent("a\",\"w", "")),
            key("https://x", &intent("a", "w"))
        );
    }

    #[test]
    fn a_record_is_only_visible_to_its_original_scope_and_round_trips_the_exact_payload() {
        let record = ImportRecovery::new("https://x", &request("<RECIPES/>"), "id-1");
        let parsed: ImportRecovery =
            serde_json::from_str(&serde_json::to_string(&record).unwrap()).unwrap();
        assert_eq!(parsed, record);
        assert!(parsed.matches("https://x", &intent("a", "w")));
        assert!(!parsed.matches("https://x", &intent("b", "w")));
        assert!(!parsed.matches("https://x", &intent("a", "other")));
        assert!(!parsed.matches("https://other", &intent("a", "w")));
        let mut anonymous = parsed.clone();
        anonymous.request_id.clear();
        assert!(!anonymous.matches("https://x", &intent("a", "w")));
    }

    #[test]
    fn retention_never_replaces_a_different_request_or_identity() {
        let first = ImportRecovery::new("https://x", &request("<A/>"), "id-1");
        let stored = merge_retained(None, &first).unwrap().unwrap();
        assert_eq!(merge_retained(Some(&stored), &first), Ok(None));
        let changed = ImportRecovery::new("https://x", &request("<B/>"), "id-1");
        assert!(merge_retained(Some(&stored), &changed).is_err());
        let other_identity = ImportRecovery::new("https://x", &request("<A/>"), "id-2");
        assert!(merge_retained(Some(&stored), &other_identity).is_err());
        assert!(merge_retained(Some("{broken"), &first).is_err());
    }

    #[test]
    fn a_record_without_a_status_is_an_unknown_result() {
        let mut value = serde_json::to_value(ImportRecovery::new("i", &request("x"), "id")).unwrap();
        value.as_object_mut().unwrap().remove("status");
        let parsed: ImportRecovery = serde_json::from_value(value).unwrap();
        assert_eq!(parsed.status, RecoveryStatus::Unknown);
    }
}
