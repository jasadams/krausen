// SPDX-License-Identifier: AGPL-3.0-or-later
//! Workspace-scoped helpers. The application owns the transaction.
use krausen_core::{DbTx, Error, Result};
use krausen_types::{
    brewery::EquipmentSettingsV1,
    equipment::{EquipmentProfile, EquipmentProvenance},
};

macro_rules! execute { ($tx:expr,$sql:expr $(,$arg:expr)* $(,)?) => {{ match &mut *$tx {
    DbTx::Postgres(t)=>sqlx::query($sql)$(.bind($arg))*.execute(&mut **t).await?.rows_affected(),
    DbTx::Sqlite(t)=>sqlx::query($sql)$(.bind($arg))*.execute(&mut **t).await?.rows_affected(),
}}}; }
#[derive(sqlx::FromRow)]
struct Row {
    workspace_id: String,
    profile_id: String,
    name: String,
    current_revision: i64,
    metadata_revision: i64,
    archived: i64,
    settings_json: String,
    provenance_json: String,
    actor_id: String,
    recorded_at: i64,
}
impl Row {
    fn into_profile(self) -> Result<EquipmentProfile> {
        let settings: EquipmentSettingsV1 = serde_json::from_str(&self.settings_json)?;
        if settings.schema_version != 1 {
            return Err(Error::BadRequest("unsupported equipment schema".into()));
        }
        Ok(EquipmentProfile {
            workspace_id: self.workspace_id,
            profile_id: self.profile_id,
            name: self.name,
            revision: self.current_revision,
            metadata_revision: self.metadata_revision,
            archived: self.archived != 0,
            settings,
            provenance: serde_json::from_str(&self.provenance_json)?,
            saved_by: self.actor_id,
            saved_at: self.recorded_at,
        })
    }
}
const SELECT: &str = "SELECT p.workspace_id,p.profile_id,p.name,p.current_revision,p.metadata_revision,CAST(CASE WHEN p.archived THEN 1 ELSE 0 END AS BIGINT) AS archived,r.settings_json,r.provenance_json,r.actor_id,r.recorded_at FROM equipment_profiles p JOIN equipment_revisions r ON r.workspace_id=p.workspace_id AND r.profile_id=p.profile_id AND r.revision=p.current_revision";
/// Load an aggregate by workspace and ID, optionally locking the source row.
pub async fn load(
    tx: &mut DbTx<'_>,
    workspace: &str,
    id: &str,
    lock: bool,
) -> Result<Option<EquipmentProfile>> {
    // Lock the aggregate before reading its joined immutable revision. A joined
    // SELECT FOR UPDATE can wait on an edit after taking its statement snapshot,
    // leaving joined revision data from before the wait under READ COMMITTED.
    if lock {
        if let DbTx::Postgres(t) = &mut *tx {
            let _locked = sqlx::query("SELECT profile_id FROM equipment_profiles WHERE workspace_id=$1 AND profile_id=$2 FOR UPDATE")
                .bind(workspace)
                .bind(id)
                .fetch_optional(&mut **t)
                .await?;
        }
    }
    let sql = format!("{SELECT} WHERE p.workspace_id=$1 AND p.profile_id=$2");
    let row = match &mut *tx {
        DbTx::Postgres(t) => {
            sqlx::query_as::<_, Row>(&sql)
                .bind(workspace)
                .bind(id)
                .fetch_optional(&mut **t)
                .await?
        }
        DbTx::Sqlite(t) => {
            sqlx::query_as::<_, Row>(&sql)
                .bind(workspace)
                .bind(id)
                .fetch_optional(&mut **t)
                .await?
        }
    };
    row.map(Row::into_profile).transpose()
}
/// Bounded stable list with optional archived profiles for recovery.
pub async fn list(
    tx: &mut DbTx<'_>,
    workspace: &str,
    after: &str,
    limit: i64,
    include_archived: bool,
) -> Result<Vec<EquipmentProfile>> {
    let sql = format!(
        "{SELECT} WHERE p.workspace_id=$1 AND p.profile_id>$2 AND ($3 OR p.archived=FALSE) ORDER BY p.profile_id LIMIT $4"
    );
    let rows = match &mut *tx {
        DbTx::Postgres(t) => {
            sqlx::query_as::<_, Row>(&sql)
                .bind(workspace)
                .bind(after)
                .bind(include_archived)
                .bind(limit)
                .fetch_all(&mut **t)
                .await?
        }
        DbTx::Sqlite(t) => {
            sqlx::query_as::<_, Row>(&sql)
                .bind(workspace)
                .bind(after)
                .bind(include_archived)
                .bind(limit)
                .fetch_all(&mut **t)
                .await?
        }
    };
    rows.into_iter().map(Row::into_profile).collect()
}
fn encode(
    name: &str,
    settings: &EquipmentSettingsV1,
    provenance: &EquipmentProvenance,
) -> Result<(String, String)> {
    let v = krausen_core::canonical_json::from_serializable(settings)
        .map_err(|e| Error::BadRequest(e.to_string()))?;
    let content = krausen_core::canonical_json::from_serializable(&(name, settings, provenance))
        .map_err(|e| Error::BadRequest(e.to_string()))?;
    let digest = krausen_core::canonical_json::sha256(&content)
        .map_err(|e| Error::BadRequest(e.to_string()))?;
    Ok((
        String::from_utf8(
            krausen_core::canonical_json::bytes(&v)
                .map_err(|e| Error::BadRequest(e.to_string()))?,
        )
        .map_err(|e| Error::Internal(e.to_string()))?,
        digest,
    ))
}
pub async fn create(
    tx: &mut DbTx<'_>,
    workspace: &str,
    id: &str,
    name: &str,
    settings: &EquipmentSettingsV1,
    provenance: &EquipmentProvenance,
    actor: &str,
    now: i64,
) -> Result<()> {
    let (json, digest) = encode(name, settings, provenance)?;
    let provenance_json = serde_json::to_string(provenance)?;
    execute!(
        tx,
        "INSERT INTO equipment_profiles (workspace_id,profile_id,name,current_revision,metadata_revision,archived) VALUES ($1,$2,$3,1,1,FALSE)",
        workspace,
        id,
        name
    );
    execute!(
        tx,
        "INSERT INTO equipment_revisions (workspace_id,profile_id,revision,name,schema_version,settings_json,provenance_json,content_digest,actor_id,recorded_at) VALUES ($1,$2,1,$3,1,$4,$5,$6,$7,$8)",
        workspace,
        id,
        name,
        &json,
        &provenance_json,
        &digest,
        actor,
        now
    );
    Ok(())
}
pub async fn append(
    tx: &mut DbTx<'_>,
    workspace: &str,
    id: &str,
    name: &str,
    settings: &EquipmentSettingsV1,
    provenance: &EquipmentProvenance,
    actor: &str,
    now: i64,
    expected: i64,
) -> Result<bool> {
    let (json, digest) = encode(name, settings, provenance)?;
    let provenance_json = serde_json::to_string(provenance)?;
    let next = expected + 1;
    let changed = execute!(
        tx,
        "UPDATE equipment_profiles SET name=$3,current_revision=$4,metadata_revision=metadata_revision+1 WHERE workspace_id=$1 AND profile_id=$2 AND current_revision=$5 AND archived=FALSE",
        workspace,
        id,
        name,
        next,
        expected
    );
    if changed == 0 {
        return Ok(false);
    }
    execute!(
        tx,
        "INSERT INTO equipment_revisions (workspace_id,profile_id,revision,name,schema_version,settings_json,provenance_json,content_digest,actor_id,recorded_at) VALUES ($1,$2,$3,$4,1,$5,$6,$7,$8,$9)",
        workspace,
        id,
        next,
        name,
        &json,
        &provenance_json,
        &digest,
        actor,
        now
    );
    Ok(true)
}
pub async fn archive(
    tx: &mut DbTx<'_>,
    workspace: &str,
    id: &str,
    archived: bool,
    expected: i64,
    actor: &str,
    now: i64,
) -> Result<bool> {
    let next = expected + 1;
    let changed = execute!(
        tx,
        "UPDATE equipment_profiles SET archived=$3,metadata_revision=$4 WHERE workspace_id=$1 AND profile_id=$2 AND metadata_revision=$5",
        workspace,
        id,
        archived,
        next,
        expected
    );
    if changed == 0 {
        return Ok(false);
    }
    execute!(
        tx,
        "INSERT INTO equipment_archive_events (workspace_id,profile_id,metadata_revision,archived,actor_id,recorded_at) VALUES ($1,$2,$3,$4,$5,$6)",
        workspace,
        id,
        next,
        archived,
        actor,
        now
    );
    Ok(true)
}

/// Read a retained immutable settings revision, including for archived profiles.
pub async fn load_revision(
    tx: &mut DbTx<'_>,
    workspace: &str,
    id: &str,
    revision: i64,
) -> Result<Option<krausen_types::equipment::EquipmentRevision>> {
    #[derive(sqlx::FromRow)]
    struct RevisionRow {
        workspace_id: String,
        profile_id: String,
        revision: i64,
        name: String,
        schema_version: i32,
        settings_json: String,
        provenance_json: String,
        content_digest: String,
        actor_id: String,
        recorded_at: i64,
    }
    let sql = "SELECT workspace_id,profile_id,revision,name,schema_version,settings_json,provenance_json,content_digest,actor_id,recorded_at FROM equipment_revisions WHERE workspace_id=$1 AND profile_id=$2 AND revision=$3";
    let row = match &mut *tx {
        DbTx::Postgres(t) => {
            sqlx::query_as::<_, RevisionRow>(sql)
                .bind(workspace)
                .bind(id)
                .bind(revision)
                .fetch_optional(&mut **t)
                .await?
        }
        DbTx::Sqlite(t) => {
            sqlx::query_as::<_, RevisionRow>(sql)
                .bind(workspace)
                .bind(id)
                .bind(revision)
                .fetch_optional(&mut **t)
                .await?
        }
    };
    row.map(|r| {
        if r.schema_version != 1 {
            return Err(Error::BadRequest("unsupported equipment schema".into()));
        }
        let settings = serde_json::from_str(&r.settings_json)?;
        Ok(krausen_types::equipment::EquipmentRevision {
            workspace_id: r.workspace_id,
            profile_id: r.profile_id,
            revision: r.revision,
            name: r.name,
            settings,
            provenance: serde_json::from_str(&r.provenance_json)?,
            content_digest: r.content_digest,
            saved_by: r.actor_id,
            saved_at: r.recorded_at,
        })
    })
    .transpose()
}
