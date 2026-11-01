// SPDX-License-Identifier: AGPL-3.0-or-later

//! Server functions for Security settings.
//!
//! These replace the REST API calls for password, TOTP, session, and passkey management:
//! - `POST /auth/set-password` -> `set_password()`
//! - `POST /auth/change-password` -> `change_password()`
//! - `GET  /auth/2fa/status` -> `get_totp_status()`
//! - `POST /auth/2fa/setup` -> `setup_totp()`
//! - `POST /auth/2fa/enable` -> `enable_totp()`
//! - `POST /auth/2fa/disable` -> `disable_totp()`
//! - `GET  /auth/sessions` -> `get_sessions()`
//! - `DELETE /auth/sessions/{id}` -> `revoke_session()`
//! - `POST /auth/logout-all` -> `logout_all_sessions()`
//! - `GET  /auth/passkeys/list` -> `list_passkeys()`
//! - `POST /auth/passkeys/add/start` -> `start_passkey_registration()`
//! - `POST /auth/passkeys/add/complete` -> `complete_passkey_registration()`
//! - `DELETE /auth/passkeys/{id}` -> `delete_passkey()`
//! - `PATCH /auth/passkeys/{id}` -> `rename_passkey()`
//!
//! Calls the same service-layer code as `apps/server/src/routes/auth_password.rs`,
//! `apps/server/src/routes/auth_totp.rs`, `apps/server/src/routes/auth_passkeys.rs`,
//! and `apps/server/src/routes/auth.rs`.

use leptos::prelude::*;
use serde::{Deserialize, Serialize};

#[cfg(feature = "ssr")]
use super::{extract_auth, extract_context, IntoServerFnError};

/// TOTP status returned by `get_totp_status()`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TotpStatus {
    pub enabled: bool,
}

/// TOTP setup data returned by `setup_totp()`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TotpSetup {
    pub secret: String,
    pub qr_uri: String,
}

/// Check whether the current user has a password set.
#[server(prefix = "/leptos-api")]
pub async fn has_password() -> Result<bool, ServerFnError> {
    let auth = extract_auth().await?;
    let ctx = extract_context()?;

    let actor = super::verified_account_actor(&auth)?;
    match krausen_app::security_status::project_password_status(
        krausen_app::security_status::password_status(&ctx.db, &actor).await,
    ) {
        krausen_types::security_status::PasswordStatusOutcome::Available { has_password } => Ok(has_password),
        _ => Err(ServerFnError::new("Unable to load password status")),
    }
}

/// Set a password for a user who does not yet have one (e.g. OAuth-only users).
///
/// Mirrors the validation in `apps/server/src/routes/auth_password.rs::set_password`:
/// - Password must be at least 8 characters.
/// - User must NOT already have a password.
#[server(prefix = "/leptos-api")]
pub async fn set_password(new_password: String) -> Result<String, ServerFnError> {
    let auth = extract_auth().await?;
    let ctx = extract_context()?;

    // Validate password length
    if new_password.len() < 8 {
        return Err(ServerFnError::new(
            "Password must be at least 8 characters",
        ));
    }

    // Check user does NOT already have a password
    let has_pw = krausen_auth::user_service::has_password(&ctx.db, &auth.user_id)
        .await
        .into_sfn()?;
    if has_pw {
        return Err(ServerFnError::new(
            "Password already set. Use change-password to update it.",
        ));
    }

    // Hash and store
    let hash = krausen_auth::password::hash_password(&new_password)
        .into_sfn()?;
    let auth_data = serde_json::json!({"hash": hash});
    krausen_auth::user_service::upsert_auth_method(&ctx.db, &auth.user_id, "password", &auth_data)
        .await
        .into_sfn()?;

    Ok("Password set successfully".to_string())
}

/// Change password for a user who already has one.
///
/// Mirrors the validation in `apps/server/src/routes/auth_password.rs::change_password`:
/// - New password must be at least 8 characters.
/// - Current password must be verified first.
#[server(prefix = "/leptos-api")]
pub async fn change_password(
    current_password: String,
    new_password: String,
) -> Result<String, ServerFnError> {
    let auth = extract_auth().await?;
    let ctx = extract_context()?;

    if new_password.len() < 8 {
        return Err(ServerFnError::new(
            "New password must be at least 8 characters",
        ));
    }

    krausen_auth::security_service::change_password(
        &ctx.db,
        &auth.user_id,
        &current_password,
        &new_password,
    )
    .await
    .into_sfn()?;

    Ok("Password changed successfully".to_string())
}

// ---------------------------------------------------------------------------
// TOTP 2FA server functions
// ---------------------------------------------------------------------------

/// Check whether the current user has TOTP 2FA enabled.
///
/// Mirrors `GET /auth/2fa/status` in `apps/server/src/routes/auth_totp.rs`.
#[server(prefix = "/leptos-api")]
pub async fn get_totp_status() -> Result<TotpStatus, ServerFnError> {
    let auth = extract_auth().await?;
    let ctx = extract_context()?;

    let actor = super::verified_account_actor(&auth)?;
    let enabled = match krausen_app::security_status::project_totp_status(krausen_app::security_status::totp_status(&ctx.db, &actor).await) {
        krausen_types::security_status::TotpStatusOutcome::Available { enabled } => enabled,
        _ => return Err(ServerFnError::new("Unable to load 2FA status")),
    };

    Ok(TotpStatus { enabled })
}

/// Begin 2FA setup: generate a secret and QR code data URI.
///
/// The secret is stored in Redis (10 min TTL) until the user confirms with
/// a verification code via `enable_totp()`.
///
/// Mirrors `POST /auth/2fa/setup` in `apps/server/src/routes/auth_totp.rs`.
#[server(prefix = "/leptos-api")]
pub async fn setup_totp() -> Result<TotpSetup, ServerFnError> {
    let auth = extract_auth().await?;
    let ctx = extract_context()?;

    let kv = ctx.kv()?;

    let result =
        krausen_auth::security_service::setup_totp(&ctx.db, &kv, &auth.user_id, &auth.email)
            .await
            .into_sfn()?;

    Ok(TotpSetup {
        secret: result.secret,
        qr_uri: result.qr_uri,
    })
}

/// Confirm 2FA setup by verifying a TOTP code against the pending secret.
///
/// On success the secret is persisted in `user_auth_methods`. On failure the
/// pending secret is re-stored in Redis so the user can retry.
///
/// Mirrors `POST /auth/2fa/enable` in `apps/server/src/routes/auth_totp.rs`.
#[server(prefix = "/leptos-api")]
pub async fn enable_totp(code: String) -> Result<String, ServerFnError> {
    let auth = extract_auth().await?;
    let ctx = extract_context()?;

    let kv = ctx.kv()?;

    krausen_auth::security_service::enable_totp(&ctx.db, &kv, &auth.user_id, &code)
        .await
        .into_sfn()?;

    Ok("2FA has been successfully enabled".to_string())
}

/// Disable 2FA for the authenticated user.
///
/// Mirrors `POST /auth/2fa/disable` in `apps/server/src/routes/auth_totp.rs`.
/// The REST handler does not require a TOTP code — it simply removes the auth
/// method for the already-authenticated user.
#[server(prefix = "/leptos-api")]
pub async fn disable_totp() -> Result<String, ServerFnError> {
    let auth = extract_auth().await?;
    let ctx = extract_context()?;

    let enabled = krausen_auth::user_service::has_totp_enabled(&ctx.db, &auth.user_id)
        .await
        .into_sfn()?;
    if !enabled {
        return Err(ServerFnError::new("2FA is not currently enabled"));
    }

    krausen_auth::user_service::remove_auth_method(&ctx.db, &auth.user_id, "totp")
        .await
        .into_sfn()?;

    Ok("2FA has been successfully disabled".to_string())
}

// ---------------------------------------------------------------------------
// Session management server functions
// ---------------------------------------------------------------------------

/// A single session entry returned by `get_sessions()`.
///
/// Maps to the `SessionInfo` fields from `krausen_auth::token_service` plus
/// the `is_current` flag computed by comparing the caller's refresh token
/// cookie family against each session's family.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SessionEntry {
    pub token_id: String,
    pub created_at: String,
    pub last_used: Option<String>,
    pub expires_at: String,
    pub user_agent: Option<String>,
    pub ip_address: Option<String>,
    pub country_code: Option<String>,
    pub oauth_client_name: Option<String>,
    pub is_current: bool,
}

/// Get all active sessions for the current user.
///
/// Mirrors `GET /auth/sessions` in `apps/server/src/routes/auth.rs`.
/// Determines the current session by comparing the refresh token cookie's
/// family_id against each session's family_id.
#[server(prefix = "/leptos-api")]
pub async fn get_sessions() -> Result<Vec<SessionEntry>, ServerFnError> {
    let auth = extract_auth().await?;
    let ctx = extract_context()?;

    let actor = super::verified_account_actor(&auth)?;
    list_shared_sessions(&ctx.db, &actor).await
}

#[cfg(feature = "ssr")]
async fn list_shared_sessions(db: &krausen_core::DbPool, actor: &krausen_app::invitation::AccountActor) -> Result<Vec<SessionEntry>, ServerFnError> {
    let mut cursor = None;
    let mut entries = Vec::new();
    loop {
        let outcome = krausen_app::session_read::project(krausen_app::session_read::sessions(db, actor, cursor.as_deref(), 100).await);
        let krausen_types::session_read::SessionListOutcome::Available { page } = outcome else { return Err(ServerFnError::new("Unable to load sessions")); };
        entries.extend(page.items.into_iter().map(|s| SessionEntry { token_id: s.token_id, created_at: s.created_at, last_used: s.last_used, expires_at: s.expires_at, user_agent: s.user_agent, ip_address: s.ip_address, country_code: s.country_code, oauth_client_name: s.oauth_client_name, is_current: s.is_current }));
        cursor = page.next_cursor;
        if cursor.is_none() { break; }
    }
    entries.sort_by(|left, right| right.last_used.cmp(&left.last_used));
    Ok(entries)
}

/// Revoke a specific session by token ID.
///
/// Mirrors `DELETE /auth/sessions/{token_id}` in `apps/server/src/routes/auth.rs`.
/// Revokes the entire token family so rotated tokens in the same session are
/// also invalidated.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SessionRevokeResponse {
    pub outcome: krausen_types::session_revoke::SessionRevokeOutcome,
    pub current_browser_revoked: bool,
}

#[server(prefix = "/leptos-api")]
pub async fn revoke_session(token_id: String, request_id: String) -> Result<SessionRevokeResponse, ServerFnError> {
    let auth = extract_auth().await?;
    let ctx = extract_context()?;
    let actor = super::verified_account_actor(&auth)?;
    // Recovered results describe the original caller. Determine whether this
    // browser is the target independently before using that result for cookies.
    let current_browser_is_target = list_shared_sessions(&ctx.db, &actor).await?
        .iter().any(|session| session.token_id == token_id && session.is_current);
    let command = krausen_types::session_revoke::SessionRevokeCommand { expected_current_token_id: token_id, request_id };
    let outcome = krausen_app::session_revoke::project(krausen_app::session_revoke::revoke(&ctx.db, &actor, &command).await);
    let current_browser_revoked = current_browser_is_target && matches!(outcome, krausen_types::session_revoke::SessionRevokeOutcome::Revoked { .. });
    if current_browser_revoked {
        let response_options = leptos::prelude::expect_context::<leptos_axum::ResponseOptions>();
        let mut headers = axum::http::HeaderMap::new();
        krausen_auth::cookies::clear_token_cookies(&mut headers);
        for (name, value) in headers.iter() {
            if name == axum::http::header::SET_COOKIE { response_options.append_header(name.clone(), value.clone()); }
        }
    }
    Ok(SessionRevokeResponse { outcome, current_browser_revoked })
}

/// Log out the current session.
///
/// Mirrors `POST /auth/logout` in `apps/server/src/routes/auth.rs`:
/// 1. Reads the refresh token cookie.
/// 2. Revokes the entire token family so rotated tokens are also invalidated.
/// 3. Clears both auth cookies (access_token + refresh_token) via `ResponseOptions`.
///
/// Does NOT require `extract_auth()` — the token may already be invalid
/// (e.g. if the access token expired) but we still want to clear cookies.
#[server(prefix = "/leptos-api")]
pub async fn logout() -> Result<(), ServerFnError> {
    let ctx = extract_context()?;

    let headers: axum::http::HeaderMap = leptos_axum::extract()
        .await
        .map_err(|e| ServerFnError::new(format!("Failed to extract headers: {e}")))?;

    let refresh_token_name = &krausen_core::constants::get().cookies.refresh_token_name;
    let raw_token = krausen_auth::cookies::get_cookie_value(&headers, refresh_token_name);

    krausen_auth::security_service::logout(&ctx.db, raw_token)
        .await
        .into_sfn()?;

    // Clear both HTTPOnly cookies so the browser forgets the session.
    let response_options = leptos::prelude::expect_context::<leptos_axum::ResponseOptions>();
    let mut cookie_headers = axum::http::HeaderMap::new();
    krausen_auth::cookies::clear_token_cookies(&mut cookie_headers);
    for (name, value) in cookie_headers.iter() {
        if name == axum::http::header::SET_COOKIE {
            response_options.append_header(name.clone(), value.clone());
        }
    }

    Ok(())
}

/// Log out from all devices by revoking every refresh token for the user.
///
/// Mirrors `POST /auth/logout-all` in `apps/server/src/routes/auth.rs`.
#[server(prefix = "/leptos-api")]
pub async fn logout_all_sessions() -> Result<String, ServerFnError> {
    let auth = extract_auth().await?;
    let ctx = extract_context()?;

    let revoked_count =
        krausen_auth::token_service::revoke_all_user_refresh_tokens(&ctx.db, &auth.user_id)
            .await
            .into_sfn()?;

    // Clear both HTTPOnly cookies so the browser forgets the current session.
    let response_options = leptos::prelude::expect_context::<leptos_axum::ResponseOptions>();
    let mut cookie_headers = axum::http::HeaderMap::new();
    krausen_auth::cookies::clear_token_cookies(&mut cookie_headers);
    for (name, value) in cookie_headers.iter() {
        if name == axum::http::header::SET_COOKIE {
            response_options.append_header(name.clone(), value.clone());
        }
    }

    Ok(format!(
        "Logged out from all devices successfully ({revoked_count} sessions revoked)"
    ))
}

// ---------------------------------------------------------------------------
// Passkey management server functions
// ---------------------------------------------------------------------------

/// A single passkey entry returned by `list_passkeys()`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PasskeyInfo {
    pub credential_id: String,
    pub name: String,
    pub created_at: Option<String>,
    pub last_used: Option<String>,
}

/// List all passkeys for the authenticated user.
///
/// Mirrors `GET /auth/passkeys/list` in `apps/server/src/routes/auth_passkeys.rs`.
#[server(prefix = "/leptos-api")]
pub async fn list_passkeys() -> Result<Vec<PasskeyInfo>, ServerFnError> {
    let auth = extract_auth().await?;
    let ctx = extract_context()?;

    let creds = krausen_auth::user_service::get_passkey_credentials(&ctx.db, &auth.user_id)
        .await
        .into_sfn()?;

    let passkeys: Vec<PasskeyInfo> = creds
        .iter()
        .map(|(cred_id, data)| PasskeyInfo {
            credential_id: cred_id.clone(),
            name: data
                .get("device_name")
                .and_then(|v| v.as_str())
                .unwrap_or("Unnamed Device")
                .to_string(),
            created_at: data
                .get("created_at")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            last_used: data
                .get("last_used")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
        })
        .collect();

    Ok(passkeys)
}

/// Start passkey registration for the authenticated user.
///
/// Returns a JSON string containing the challenge_id and WebAuthn options
/// that the browser needs for `navigator.credentials.create()`.
///
/// Mirrors `POST /auth/passkeys/add/start` in `apps/server/src/routes/auth_passkeys.rs`.
#[server(prefix = "/leptos-api")]
pub async fn start_passkey_registration(
    device_name: String,
) -> Result<String, ServerFnError> {
    let auth = extract_auth().await?;
    let ctx = extract_context()?;

    let webauthn = ctx.webauthn()?;
    let kv = ctx.kv()?;

    let device_name = if device_name.trim().is_empty() {
        "Unknown Device".to_string()
    } else {
        device_name.trim().to_string()
    };

    let (ccr, challenge_id) = krausen_auth::security_service::start_passkey_registration(
        &ctx.db,
        &kv,
        webauthn,
        &auth.user_id,
        &device_name,
    )
    .await
    .into_sfn()?;

    let result = serde_json::json!({
        "challenge_id": challenge_id,
        "options": ccr,
    });

    serde_json::to_string(&result)
        .map_err(|e| ServerFnError::new(format!("Serialize response: {e}")))
}

/// Complete passkey registration by verifying the browser credential.
///
/// Receives the challenge_id and the PublicKeyCredential JSON from the browser.
///
/// Mirrors `POST /auth/passkeys/add/complete` in `apps/server/src/routes/auth_passkeys.rs`.
#[server(prefix = "/leptos-api")]
pub async fn complete_passkey_registration(
    credential_json: String,
) -> Result<String, ServerFnError> {
    use webauthn_rs::prelude::RegisterPublicKeyCredential;

    let auth = extract_auth().await?;
    let ctx = extract_context()?;

    let webauthn = ctx.webauthn()?;
    let kv = ctx.kv()?;

    let data: serde_json::Value = serde_json::from_str(&credential_json)
        .map_err(|e| ServerFnError::new(format!("Invalid credential JSON: {e}")))?;

    let challenge_id = data["challenge_id"]
        .as_str()
        .ok_or_else(|| ServerFnError::new("Missing challenge_id"))?;

    let credential: RegisterPublicKeyCredential =
        serde_json::from_value(data["credential"].clone())
            .map_err(|e| ServerFnError::new(format!("Invalid credential: {e}")))?;

    let device_name = krausen_auth::security_service::complete_passkey_registration(
        &ctx.db,
        &kv,
        webauthn,
        &auth.user_id,
        challenge_id,
        &credential,
    )
    .await
    .into_sfn()?;

    Ok(format!("Passkey '{}' added successfully", device_name))
}

/// Delete a passkey for the authenticated user.
///
/// Mirrors `DELETE /auth/passkeys/{credential_id}` in `apps/server/src/routes/auth_passkeys.rs`.
#[server(prefix = "/leptos-api")]
pub async fn delete_passkey(credential_id: String) -> Result<String, ServerFnError> {
    let auth = extract_auth().await?;
    let ctx = extract_context()?;

    match krausen_auth::user_service::delete_passkey_from_user(
        &ctx.db,
        &auth.user_id,
        &credential_id,
    )
    .await
    .into_sfn()?
    {
        None => Ok("Passkey deleted successfully".to_string()),
        Some(error_msg) => Err(ServerFnError::new(error_msg)),
    }
}

/// Rename a passkey for the authenticated user.
///
/// Mirrors `PATCH /auth/passkeys/{credential_id}` in `apps/server/src/routes/auth_passkeys.rs`.
#[server(prefix = "/leptos-api")]
pub async fn rename_passkey(
    credential_id: String,
    name: String,
) -> Result<String, ServerFnError> {
    let auth = extract_auth().await?;
    let ctx = extract_context()?;

    let trimmed = name.trim().to_string();

    if trimmed.is_empty() {
        return Err(ServerFnError::new("Device name cannot be empty"));
    }

    if trimmed.len() > 100 {
        return Err(ServerFnError::new(
            "Device name cannot exceed 100 characters",
        ));
    }

    let updated = krausen_auth::user_service::update_passkey_device_name(
        &ctx.db,
        &auth.user_id,
        &credential_id,
        &trimmed,
    )
    .await
    .into_sfn()?;

    if !updated {
        return Err(ServerFnError::new("Passkey not found"));
    }

    Ok("Passkey renamed successfully".to_string())
}

#[cfg(all(test, feature = "ssr"))]
mod session_inventory_tests {
    use super::*;

    #[tokio::test]
    async fn sessions_adapter_follows_pages_and_marks_current() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("../../apps/server/migrations-sqlite").run(&pool).await.unwrap();
        sqlx::query("INSERT INTO users (user_id,email) VALUES ('u','u@example.test')").execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO auth_credentials (credential_id,user_id,kind,issuer,audience,expires_at) VALUES ('family-000','u','browser','test','test',4102444800)").execute(&pool).await.unwrap();
        for i in 0..101 {
            sqlx::query("INSERT INTO refresh_tokens (token_id,user_id,token_hash,family_id,expires_at,is_active) VALUES ($1,'u',$2,$3,$4,TRUE)")
                .bind(format!("token-{i:03}")).bind(format!("hash-{i:03}")).bind(format!("family-{i:03}"))
                .bind(chrono::Utc::now() + chrono::Duration::days(1)).execute(&pool).await.unwrap();
        }
        let db = krausen_core::DbPool::Sqlite(pool);
        let actor = krausen_app::invitation::AccountActor { actor_id: "u".into(), credential_id: "family-000".into(), access_expires_at: 4102444800 };
        let sessions = list_shared_sessions(&db, &actor).await.unwrap();
        assert_eq!(sessions.len(), 101);
        assert_eq!(sessions.iter().filter(|s| s.is_current).count(), 1);
        assert!(sessions.iter().any(|s| s.token_id == "token-000" && s.is_current));
    }
}
