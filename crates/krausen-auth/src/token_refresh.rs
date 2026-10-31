// SPDX-License-Identifier: AGPL-3.0-or-later

//! Shared refresh-token flow used by both the explicit `POST /auth/refresh`
//! REST endpoint and the transparent auto-refresh middleware.
//!
//! This module encapsulates the pure token-minting + rotation logic so that
//! callers can wrap it with whatever HTTP concerns they need (rate limiting,
//! cookie setting, JSON body assembly) without duplicating the core flow.

use std::collections::HashMap;

use crate::{jwt, token_service, token_service::DeviceInfo, user_service};

/// Result of a successful refresh: new tokens + user context for downstream
/// response assembly (cookies, JSON body, etc.).
pub struct RefreshedTokens {
    pub access_token: String,
    pub raw_refresh_token: String,
    pub access_expires_in_secs: i64,
    pub user_id: String,
    pub email: String,
    pub name: Option<String>,
    pub roles: Vec<String>,
}

/// Attempt to mint a new access token and rotate the refresh token.
///
/// Atomically rotates the durable credential family. Reuse revokes the family;
/// verification failures and database errors return `Err`.
///
/// Does NOT perform rate limiting — callers layer that on top if they want it.
/// Does NOT set cookies — callers assemble the HTTP response themselves.
pub async fn refresh_tokens(
    db: &krausen_core::DbPool,
    jwt_secret: &str,
    refresh_token_value: &str,
    device: &DeviceInfo,
) -> krausen_core::Result<RefreshedTokens> {
    let (user_id, family_id, new_raw_refresh) =
        token_service::rotate_browser_refresh(db, refresh_token_value, device).await?;
    let user = user_service::get_user_by_id(db, &user_id)
        .await?
        .ok_or_else(|| krausen_core::Error::Unauthorized("Account unavailable".into()))?;
    if !user.active {
        return Err(krausen_core::Error::Unauthorized(
            "Account unavailable".into(),
        ));
    }
    let roles = user.roles();
    // Build JWT claims — mirrors POST /auth/refresh
    let mut extra = HashMap::new();
    extra.insert("user_id".into(), serde_json::json!(user_id));
    extra.insert("email".into(), serde_json::json!(user.email));
    extra.insert("name".into(), serde_json::json!(user.name));
    extra.insert("roles".into(), serde_json::json!(roles));
    extra.insert("credential_id".into(), serde_json::json!(&family_id));
    extra.insert("iss".into(), serde_json::json!(jwt::BROWSER_ISSUER));
    extra.insert("aud".into(), serde_json::json!(jwt::BROWSER_AUDIENCE));

    // Load workspace context (same behaviour as the REST handler: best-effort)
    if let Ok(Some((ws, wu))) = user_service::get_user_workspace_context(db, &user_id).await {
        extra.insert("workspace_id".into(), serde_json::json!(ws.workspace_id));
        extra.insert("workspace_roles".into(), serde_json::json!(vec![wu.role]));
    }

    let jwt_config = &krausen_core::constants::get().jwt;
    let new_access_token = jwt::create_access_token_str(
        &user_id,
        jwt_secret,
        jwt_config.access_token_expire_minutes,
        extra,
    )?;

    Ok(RefreshedTokens {
        access_token: new_access_token,
        raw_refresh_token: new_raw_refresh,
        access_expires_in_secs: jwt_config.access_token_expire_minutes * 60,
        user_id: user_id,
        email: user.email,
        name: user.name,
        roles: roles,
    })
}
