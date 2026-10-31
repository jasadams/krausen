// SPDX-License-Identifier: AGPL-3.0-or-later
//! Account operations shared by browser and MCP adapters.
use crate::{ActorContext, Error, authorize_account, boundary_error, db_error};
use krausen_core::DbPool;
use krausen_types::account::Profile;

#[derive(sqlx::FromRow)]
struct ProfileRow {
    user_id: String,
    email: String,
    name: Option<String>,
    metadata: Option<String>,
}

/// Read only the current authenticated account, independently of workspace selection.
pub async fn profile(pool: &DbPool, actor: &ActorContext) -> Result<Profile, Error> {
    let mut tx = pool.begin_write().await.map_err(boundary_error)?;
    authorize_account(&mut tx, actor).await?;
    let row = krausen_core::dbtx_fetch_optional!(&mut tx, ProfileRow, "SELECT user_id, email, name, CAST(extra_metadata AS TEXT) AS metadata FROM users WHERE user_id = $1", &actor.actor_id).map_err(db_error)?.ok_or(Error::Forbidden)?;
    let metadata = match row.metadata {
        Some(value) => serde_json::from_str::<serde_json::Value>(&value)?,
        None => serde_json::Value::Null,
    };
    let theme = metadata
        .get("theme")
        .and_then(|v| v.as_str())
        .unwrap_or("system")
        .to_owned();
    tx.commit().await.map_err(boundary_error)?;
    Ok(Profile {
        user_id: row.user_id,
        email: row.email,
        name: row.name,
        theme,
    })
}
