// SPDX-License-Identifier: AGPL-3.0-or-later
//! Transactional, revision-checked account profile changes.
use crate::{ActorContext, Error, RequestIdentity, authorize_account, boundary_error, db_error};
use krausen_core::{DbPool, DbTx};
use krausen_types::account::{Profile, ProfileChange};

/// Save one account field, with durable retry before revision comparison.
pub async fn save(
    pool: &DbPool,
    actor: &ActorContext,
    change: ProfileChange,
    expected: i64,
    request_id: uuid::Uuid,
) -> Result<Profile, Error> {
    let change = match change {
        ProfileChange::Name { name } => {
            let name = name.trim().to_owned();
            if name.is_empty() || name.chars().count() > 255 {
                return Err(Error::Invalid("name"));
            }
            ProfileChange::Name { name }
        }
        ProfileChange::Theme { theme } => {
            if !matches!(theme.as_str(), "light" | "dark" | "system") {
                return Err(Error::Invalid("theme"));
            }
            ProfileChange::Theme { theme }
        }
    };
    let digest = krausen_core::canonical_json::sha256(
        &serde_json::json!({"change":change,"expected_revision":expected}),
    )
    .map_err(|_| Error::Invalid("profile identity"))?;
    let mut tx = pool.begin_write().await.map_err(boundary_error)?;
    let account_sql = if matches!(tx, DbTx::Postgres(_)) {
        "SELECT user_id FROM users WHERE user_id = $1 AND active = TRUE FOR UPDATE"
    } else {
        "SELECT user_id FROM users WHERE user_id = $1 AND active = TRUE"
    };
    if scalar!(&mut tx, String, account_sql, &actor.actor_id)
        .map_err(db_error)?
        .is_none()
    {
        return Err(Error::Forbidden);
    }
    authorize_account(&mut tx, actor).await?;
    let row = krausen_core::dbtx_fetch_optional!(&mut tx, super::ProfileRow, "SELECT user_id, email, name, CAST(extra_metadata AS TEXT) AS metadata, account_revision AS revision FROM users WHERE user_id = $1", &actor.actor_id).map_err(db_error)?.ok_or(Error::Forbidden)?;
    let account_actor = ActorContext {
        workspace_id: String::new(),
        ..actor.clone()
    };
    let request = request_id.to_string();
    let key = RequestIdentity {
        actor: &account_actor,
        operation: "account.profile.save.v1",
        request_id: &request,
        payload_digest: &digest,
    };
    if let Some(result) = crate::recover_request(&mut tx, &key).await? {
        let profile = serde_json::from_value(result)?;
        tx.commit().await.map_err(boundary_error)?;
        return Ok(profile);
    }
    if row.revision != expected {
        return Err(Error::Conflict {
            expected,
            actual: row.revision,
        });
    }
    let mut metadata = match row.metadata {
        Some(value) => serde_json::from_str::<serde_json::Value>(&value)?,
        None => serde_json::json!({}),
    };
    if metadata.is_null() {
        metadata = serde_json::json!({});
    }
    if !metadata.is_object() {
        return Err(Error::Invalid("account metadata"));
    }
    let name = match change {
        ProfileChange::Name { name } => Some(name),
        ProfileChange::Theme { theme } => {
            metadata["theme"] = serde_json::Value::String(theme);
            row.name
        }
    };
    let theme = metadata
        .get("theme")
        .and_then(|v| v.as_str())
        .unwrap_or("system")
        .to_owned();
    let metadata = serde_json::to_string(&metadata)?;
    let sql = if matches!(tx, DbTx::Postgres(_)) {
        "UPDATE users SET name=$1, extra_metadata=CAST($2 AS JSON), account_revision=account_revision+1 WHERE user_id=$3"
    } else {
        "UPDATE users SET name=$1, extra_metadata=$2, account_revision=account_revision+1 WHERE user_id=$3"
    };
    query!(&mut tx, sql, &name, &metadata, &actor.actor_id).map_err(db_error)?;
    let profile = Profile {
        user_id: row.user_id,
        email: row.email,
        name,
        theme,
        revision: expected + 1,
    };
    crate::record_request(
        &mut tx,
        &key,
        &serde_json::to_value(&profile)?,
        Some(&actor.actor_id),
    )
    .await?;
    crate::record_change(
        &mut tx,
        &account_actor,
        "account.profile.save.v1",
        &actor.actor_id,
        profile.revision,
    )
    .await?;
    tx.commit().await.map_err(boundary_error)?;
    Ok(profile)
}
