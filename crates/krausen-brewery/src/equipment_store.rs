// SPDX-License-Identifier: AGPL-3.0-or-later
//! Workspace-scoped helpers. The application owns the transaction.
use krausen_core::{DbTx, Error, Result};
use krausen_types::{
    brewery::{EquipmentAboutV1, EquipmentSettingsV1},
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
    created_at: i64,
    metadata_revision: i64,
    archived: i64,
    is_default: i64,
    settings_json: String,
    about_json: String,
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
        let settings = settings.into_profile_settings();
        Ok(EquipmentProfile {
            workspace_id: self.workspace_id,
            profile_id: self.profile_id,
            created_at: self.created_at,
            name: self.name,
            revision: self.current_revision,
            metadata_revision: self.metadata_revision,
            archived: self.archived != 0,
            is_default: self.is_default != 0,
            settings,
            about: serde_json::from_str(&self.about_json)?,
            provenance: serde_json::from_str(&self.provenance_json)?,
            saved_by: self.actor_id,
            saved_at: self.recorded_at,
        })
    }
}
const SELECT: &str = "SELECT p.workspace_id,p.profile_id,initial.recorded_at AS created_at,p.name,p.current_revision,p.metadata_revision,CAST(CASE WHEN p.archived THEN 1 ELSE 0 END AS BIGINT) AS archived,CAST(CASE WHEN d.profile_id IS NULL THEN 0 ELSE 1 END AS BIGINT) AS is_default,r.settings_json,r.about_json,r.provenance_json,r.actor_id,r.recorded_at FROM equipment_profiles p LEFT JOIN workspace_default_equipment d ON d.workspace_id=p.workspace_id AND d.profile_id=p.profile_id JOIN equipment_revisions r ON r.workspace_id=p.workspace_id AND r.profile_id=p.profile_id AND r.revision=p.current_revision JOIN equipment_revisions initial ON initial.workspace_id=p.workspace_id AND initial.profile_id=p.profile_id AND initial.revision=1";
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
    after_created_at: i64,
    after_id: &str,
    limit: i64,
    include_archived: bool,
) -> Result<Vec<EquipmentProfile>> {
    let sql = format!(
        "{SELECT} WHERE p.workspace_id=$1 AND (initial.recorded_at>$2 OR (initial.recorded_at=$2 AND p.profile_id>$3)) AND ($4 OR p.archived=FALSE) ORDER BY initial.recorded_at,p.profile_id LIMIT $5"
    );
    let rows = match &mut *tx {
        DbTx::Postgres(t) => {
            sqlx::query_as::<_, Row>(&sql)
                .bind(workspace)
                .bind(after_created_at)
                .bind(after_id)
                .bind(include_archived)
                .bind(limit)
                .fetch_all(&mut **t)
                .await?
        }
        DbTx::Sqlite(t) => {
            sqlx::query_as::<_, Row>(&sql)
                .bind(workspace)
                .bind(after_created_at)
                .bind(after_id)
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
    about: &EquipmentAboutV1,
    provenance: &EquipmentProvenance,
) -> Result<(String, String, String)> {
    let v = krausen_core::canonical_json::from_serializable(settings)
        .map_err(|e| Error::BadRequest(e.to_string()))?;
    let content = krausen_core::canonical_json::from_serializable(&(name, settings, about, provenance))
        .map_err(|e| Error::BadRequest(e.to_string()))?;
    let digest = krausen_core::canonical_json::sha256(&content)
        .map_err(|e| Error::BadRequest(e.to_string()))?;
    Ok((
        String::from_utf8(
            krausen_core::canonical_json::bytes(&v)
                .map_err(|e| Error::BadRequest(e.to_string()))?,
        )
        .map_err(|e| Error::Internal(e.to_string()))?,
        serde_json::to_string(about)?,
        digest,
    ))
}
pub async fn create(
    tx: &mut DbTx<'_>,
    workspace: &str,
    id: &str,
    name: &str,
    settings: &EquipmentSettingsV1,
    about: &EquipmentAboutV1,
    provenance: &EquipmentProvenance,
    actor: &str,
    now: i64,
) -> Result<()> {
    let (json, about_json, digest) = encode(name, settings, about, provenance)?;
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
        "INSERT INTO equipment_revisions (workspace_id,profile_id,revision,name,schema_version,settings_json,about_json,provenance_json,content_digest,actor_id,recorded_at) VALUES ($1,$2,1,$3,1,$4,$5,$6,$7,$8,$9)",
        workspace,
        id,
        name,
        &json,
        &about_json,
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
    about: &EquipmentAboutV1,
    provenance: &EquipmentProvenance,
    actor: &str,
    now: i64,
    expected: i64,
) -> Result<bool> {
    let (json, about_json, digest) = encode(name, settings, about, provenance)?;
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
        "INSERT INTO equipment_revisions (workspace_id,profile_id,revision,name,schema_version,settings_json,about_json,provenance_json,content_digest,actor_id,recorded_at) VALUES ($1,$2,$3,$4,1,$5,$6,$7,$8,$9,$10)",
        workspace,
        id,
        next,
        name,
        &json,
        &about_json,
        &provenance_json,
        &digest,
        actor,
        now
    );
    Ok(true)
}
/// Make `id` the workspace default, replacing any other. The caller has checked
/// the profile is active and holds the workspace write lock.
pub async fn set_default(tx: &mut DbTx<'_>, workspace: &str, id: &str) -> Result<()> {
    execute!(
        tx,
        "INSERT INTO workspace_default_equipment (workspace_id,profile_id) VALUES ($1,$2) ON CONFLICT (workspace_id) DO UPDATE SET profile_id=excluded.profile_id",
        workspace,
        id
    );
    Ok(())
}
/// Clear the default only when it is `id`.
pub async fn clear_default(tx: &mut DbTx<'_>, workspace: &str, id: &str) -> Result<()> {
    execute!(
        tx,
        "DELETE FROM workspace_default_equipment WHERE workspace_id=$1 AND profile_id=$2",
        workspace,
        id
    );
    Ok(())
}
/// The workspace's active default profile, if one is set.
pub async fn load_default(tx: &mut DbTx<'_>, workspace: &str) -> Result<Option<EquipmentProfile>> {
    let id: Option<String> = match &mut *tx {
        DbTx::Postgres(t) => {
            sqlx::query_scalar("SELECT profile_id FROM workspace_default_equipment WHERE workspace_id=$1")
                .bind(workspace)
                .fetch_optional(&mut **t)
                .await?
        }
        DbTx::Sqlite(t) => {
            sqlx::query_scalar("SELECT profile_id FROM workspace_default_equipment WHERE workspace_id=$1")
                .bind(workspace)
                .fetch_optional(&mut **t)
                .await?
        }
    };
    match id {
        Some(id) => load(tx, workspace, &id, false).await,
        None => Ok(None),
    }
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
    if archived {
        // An archived profile cannot be the workspace default.
        execute!(
            tx,
            "DELETE FROM workspace_default_equipment WHERE workspace_id=$1 AND profile_id=$2",
            workspace,
            id
        );
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
        about_json: String,
        provenance_json: String,
        content_digest: String,
        actor_id: String,
        recorded_at: i64,
    }
    let sql = "SELECT workspace_id,profile_id,revision,name,schema_version,settings_json,about_json,provenance_json,content_digest,actor_id,recorded_at FROM equipment_revisions WHERE workspace_id=$1 AND profile_id=$2 AND revision=$3";
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
        let settings = serde_json::from_str::<EquipmentSettingsV1>(&r.settings_json)?
            .into_profile_settings();
        Ok(krausen_types::equipment::EquipmentRevision {
            workspace_id: r.workspace_id,
            profile_id: r.profile_id,
            revision: r.revision,
            name: r.name,
            settings,
            about: serde_json::from_str(&r.about_json)?,
            provenance: serde_json::from_str(&r.provenance_json)?,
            content_digest: r.content_digest,
            saved_by: r.actor_id,
            saved_at: r.recorded_at,
        })
    })
    .transpose()
}
