// SPDX-License-Identifier: AGPL-3.0-or-later
//! Transaction-participating immutable brew-plan persistence. The caller owns DbTx.
use krausen_core::{DbTx, Error, Result};
use krausen_types::brewery::{BrewPlanV1, BrewStatusV1};

pub const BREW_PLAN_SCHEMA_VERSION: i32 = 1;

#[derive(Debug, thiserror::Error)]
pub enum StoredPlanError {
    #[error("unsupported brew-plan schema version {0}")]
    UnsupportedVersion(i32),
    #[error("brew plan is not valid JSON: {0}")]
    InvalidJson(#[from] serde_json::Error),
    #[error("brew-plan schema version does not match its envelope")]
    VersionMismatch,
    #[error("brew plan cannot be canonicalized: {0}")]
    Canonical(#[from] krausen_core::canonical_json::CanonicalError),
}

/// Canonical bytes and content identity for one immutable plan.
pub fn encode_plan(plan: &BrewPlanV1) -> std::result::Result<(String, String), StoredPlanError> {
    if plan.schema_version as i32 != BREW_PLAN_SCHEMA_VERSION {
        return Err(StoredPlanError::UnsupportedVersion(
            plan.schema_version as i32,
        ));
    }
    let value = krausen_core::canonical_json::from_serializable(plan)?;
    let digest = krausen_core::canonical_json::sha256(&value)?;
    let json = String::from_utf8(krausen_core::canonical_json::bytes(&value)?)
        .expect("canonical JSON is UTF-8");
    Ok((json, digest))
}

pub fn read_plan(
    schema_version: i32,
    json: &str,
) -> std::result::Result<BrewPlanV1, StoredPlanError> {
    if schema_version != BREW_PLAN_SCHEMA_VERSION {
        return Err(StoredPlanError::UnsupportedVersion(schema_version));
    }
    let plan: BrewPlanV1 = serde_json::from_str(json)?;
    if plan.schema_version as i32 != schema_version {
        return Err(StoredPlanError::VersionMismatch);
    }
    Ok(plan)
}

#[derive(Debug, sqlx::FromRow)]
struct BrewRow {
    workspace_id: String,
    brew_id: String,
    recipe_id: String,
    selected_recipe_revision: i64,
    status: String,
    revision: i64,
    started_at: i64,
    started_by: String,
    plan_schema_version: i32,
    plan_json: String,
    plan_content_digest: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StoredBrew {
    pub workspace_id: String,
    pub brew_id: String,
    pub recipe_id: String,
    pub selected_recipe_revision: i64,
    pub status: BrewStatusV1,
    pub revision: i64,
    pub started_at: i64,
    pub started_by: String,
    pub plan_schema_version: i32,
    pub plan_content_digest: String,
    pub plan: BrewPlanV1,
}

/// Insert the immutable plan row. The caller's transaction owns commit/rollback.
pub async fn insert(
    tx: &mut DbTx<'_>,
    workspace_id: &str,
    brew_id: &str,
    recipe_id: &str,
    selected_recipe_revision: i64,
    status: BrewStatusV1,
    revision: i64,
    started_at: i64,
    started_by: &str,
    plan_schema_version: i32,
    plan_json: &str,
    plan_content_digest: &str,
) -> Result<()> {
    let sql = "INSERT INTO brews (workspace_id, brew_id, recipe_id, selected_recipe_revision, status, revision, started_at, started_by, plan_schema_version, plan_json, plan_content_digest) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)";
    let affected = match &mut *tx {
        DbTx::Postgres(inner) => sqlx::query(sql)
            .bind(workspace_id)
            .bind(brew_id)
            .bind(recipe_id)
            .bind(selected_recipe_revision)
            .bind(status.as_str())
            .bind(revision)
            .bind(started_at)
            .bind(started_by)
            .bind(plan_schema_version)
            .bind(plan_json)
            .bind(plan_content_digest)
            .execute(&mut **inner)
            .await?
            .rows_affected(),
        DbTx::Sqlite(inner) => sqlx::query(sql)
            .bind(workspace_id)
            .bind(brew_id)
            .bind(recipe_id)
            .bind(selected_recipe_revision)
            .bind(status.as_str())
            .bind(revision)
            .bind(started_at)
            .bind(started_by)
            .bind(plan_schema_version)
            .bind(plan_json)
            .bind(plan_content_digest)
            .execute(&mut **inner)
            .await?
            .rows_affected(),
    };
    if affected != 1 {
        return Err(Error::Internal("brew insert did not create one row".into()));
    }
    Ok(())
}

/// Read one brew and its immutable plan using the stored schema version.
pub async fn load(
    tx: &mut DbTx<'_>,
    workspace_id: &str,
    brew_id: &str,
) -> Result<Option<StoredBrew>> {
    let sql = "SELECT workspace_id, brew_id, recipe_id, selected_recipe_revision, status, revision, started_at, started_by, plan_schema_version, plan_json, plan_content_digest FROM brews WHERE workspace_id = $1 AND brew_id = $2";
    let row = match &mut *tx {
        DbTx::Postgres(inner) => {
            sqlx::query_as::<_, BrewRow>(sql)
                .bind(workspace_id)
                .bind(brew_id)
                .fetch_optional(&mut **inner)
                .await?
        }
        DbTx::Sqlite(inner) => {
            sqlx::query_as::<_, BrewRow>(sql)
                .bind(workspace_id)
                .bind(brew_id)
                .fetch_optional(&mut **inner)
                .await?
        }
    };
    let Some(row) = row else {
        return Ok(None);
    };
    let status = BrewStatusV1::parse(&row.status)
        .ok_or_else(|| Error::Internal("stored brew status is not supported".into()))?;
    let plan = read_plan(row.plan_schema_version, &row.plan_json)
        .map_err(|error| Error::Internal(format!("stored brew plan cannot be read: {error}")))?;
    let (canonical_json, digest) = encode_plan(&plan).map_err(|error| {
        Error::Internal(format!("stored brew plan cannot be verified: {error}"))
    })?;
    if canonical_json != row.plan_json || digest != row.plan_content_digest {
        return Err(Error::Internal(
            "stored brew plan content does not match its digest".into(),
        ));
    }
    Ok(Some(StoredBrew {
        workspace_id: row.workspace_id,
        brew_id: row.brew_id,
        recipe_id: row.recipe_id,
        selected_recipe_revision: row.selected_recipe_revision,
        status,
        revision: row.revision,
        started_at: row.started_at,
        started_by: row.started_by,
        plan_schema_version: row.plan_schema_version,
        plan_content_digest: row.plan_content_digest,
        plan,
    }))
}
