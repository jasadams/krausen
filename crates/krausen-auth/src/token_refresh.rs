// SPDX-License-Identifier: AGPL-3.0-or-later

//! Shared refresh-token flow used by both the explicit `POST /auth/refresh`
//! REST endpoint and the transparent auto-refresh middleware.
//!
//! This module encapsulates the pure token-minting + rotation logic so that
//! callers can wrap it with whatever HTTP concerns they need (rate limiting,
//! cookie setting, JSON body assembly) without duplicating the core flow.

use std::collections::HashMap;

use crate::{
    jwt, token_service,
    token_service::{DeviceInfo, RefreshTokenVerifyResult},
    user_service,
};

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
/// Handles grace-period and theft detection via
/// [`token_service::verify_refresh_token`], then mints the access token and only
/// then rotates: the rotation re-checks revocation under the user lock, so a
/// revocation event that landed after the mint fails the refresh and the minted
/// token is discarded. Returns `Err` on verification failure, theft detection,
/// revocation or database errors.
///
/// Does NOT perform rate limiting — callers layer that on top if they want it.
/// Does NOT set cookies — callers assemble the HTTP response themselves.
pub async fn refresh_tokens(
    db: &krausen_core::DbPool,
    jwt_secret: &str,
    refresh_token_value: &str,
    device: &DeviceInfo,
) -> krausen_core::Result<RefreshedTokens> {
    // Verify refresh token (handles grace period + theft detection)
    let verify_result = token_service::verify_refresh_token(db, refresh_token_value).await?;

    let user_data = match verify_result {
        RefreshTokenVerifyResult::Valid(data) | RefreshTokenVerifyResult::GracePeriod(data) => data,
        RefreshTokenVerifyResult::TheftDetected { .. } => {
            return Err(krausen_core::Error::Unauthorized(
                "Refresh token has been revoked (possible token theft detected)".into(),
            ));
        }
        RefreshTokenVerifyResult::Invalid => {
            return Err(krausen_core::Error::Unauthorized(
                "Invalid or expired refresh token".into(),
            ));
        }
    };

    if user_data.oauth_client_id.is_some() {
        return Err(krausen_core::Error::Unauthorized(
            "OAuth client refresh token cannot refresh a browser session".into(),
        ));
    }

    refresh_verified_tokens(db, jwt_secret, refresh_token_value, user_data, device).await
}

/// Mint the browser access token for a verified refresh token.
///
/// Reads the user's current revocation state first, so the signed issuance is a
/// strict successor of the cutoff; the rotation that follows rechecks the token
/// under the same user lock revocation uses.
async fn mint_access_token(
    db: &krausen_core::DbPool,
    jwt_secret: &str,
    user_data: &token_service::RefreshTokenUserData,
) -> krausen_core::Result<String> {
    // Build JWT claims — mirrors POST /auth/refresh
    let mut extra = HashMap::new();
    extra.insert("user_id".into(), serde_json::json!(user_data.user_id));
    extra.insert("email".into(), serde_json::json!(user_data.email));
    extra.insert("name".into(), serde_json::json!(user_data.name));
    extra.insert("roles".into(), serde_json::json!(user_data.roles));

    // Load workspace context (same behaviour as the REST handler: best-effort)
    if let Ok(Some((ws, wu))) =
        user_service::get_user_workspace_context(db, &user_data.user_id).await
    {
        extra.insert("workspace_id".into(), serde_json::json!(ws.workspace_id));
        extra.insert("workspace_roles".into(), serde_json::json!(vec![wu.role]));
    }

    let jwt_config = &krausen_core::constants::get().jwt;
    let user = user_service::get_user_by_id(db, &user_data.user_id)
        .await?
        .ok_or_else(|| krausen_core::Error::Unauthorized("User not found".into()))?;
    jwt::create_session_access_token_str(
        &user_data.user_id,
        jwt_secret,
        jwt_config.access_token_expire_minutes,
        extra,
        user.sessions_valid_from,
    )
}

// Verification is deliberately separate from the transactional rotation: the
// latter rechecks revocation under lock, regardless of when verification ran.
async fn refresh_verified_tokens(
    db: &krausen_core::DbPool,
    jwt_secret: &str,
    refresh_token_value: &str,
    user_data: token_service::RefreshTokenUserData,
    device: &DeviceInfo,
) -> krausen_core::Result<RefreshedTokens> {
    let new_access_token = mint_access_token(db, jwt_secret, &user_data).await?;

    // Always rotate: every tab gets a fresh token (prevents multi-tab sign-out bug)
    let (_, family_id, new_raw_refresh) =
        token_service::rotate_browser_refresh(db, refresh_token_value, device).await?;
    if family_id != user_data.family_id {
        return Err(krausen_core::Error::Conflict("Refresh token changed".into()));
    }

    let jwt_config = &krausen_core::constants::get().jwt;
    Ok(RefreshedTokens {
        access_token: new_access_token,
        raw_refresh_token: new_raw_refresh,
        access_expires_in_secs: jwt_config.access_token_expire_minutes * 60,
        user_id: user_data.user_id,
        email: user_data.email,
        name: user_data.name,
        roles: user_data.roles,
    })
}

#[cfg(test)]
mod tests {
    //! Refresh against revocation: the real `refresh_verified_tokens` flow, run on
    //! SQLite and PostgreSQL with the verification/rotation boundary held explicitly.
    use super::*;
    use crate::auth_service::signup_tests::{both_backends, email, postgres_db, sqlite_db};
    use axum::extract::FromRequestParts;
    use krausen_core::DbPool;

    const SECRET: &str = "cutoff-race-test-secret";

    fn device() -> DeviceInfo {
        DeviceInfo {
            user_agent: None,
            ip_address: None,
            country_code: None,
            oauth_client_id: None,
        }
    }

    async fn new_user(db: &DbPool) -> krausen_core::models::User {
        let _ = krausen_core::constants::load_with_fallback();
        let e = email("refresh");
        let user = user_service::create_user(db, &e, None, true).await.unwrap();
        user_service::create_workspace_for_user(db, &user.user_id, None, &e, None)
            .await
            .unwrap();
        user_service::get_user_by_id(db, &user.user_id)
            .await
            .unwrap()
            .unwrap()
    }

    /// Authenticate `token` through the real extractor: only the revocation
    /// cutoff can refuse it.
    async fn refused_by_cutoff(db: &DbPool, token: &str) -> bool {
        let state = crate::middleware::AuthState {
            jwt_secret: SECRET.into(),
            db: db.clone(),
        };
        let (mut parts, _) = axum::http::Request::builder()
            .uri("/x")
            .header("authorization", format!("Bearer {token}"))
            .body(())
            .unwrap()
            .into_parts();
        match crate::middleware::AuthUser::from_request_parts(&mut parts, &state).await {
            Err(krausen_core::Error::Unauthorized(msg)) => msg.contains("Session revoked"),
            _ => false,
        }
    }

    /// A refresh verified before a revocation event must not rotate after it, must
    /// leave no replacement refresh token, and an access token minted before the
    /// event is cut off by the cutoff.
    async fn controlled_refresh_race(db: &DbPool) {
        let user = new_user(db).await;
        let kv = krausen_core::kv_store_memory::InMemoryKVStore::new_pool();
        let session = crate::session::create_authenticated_session(db, &kv, SECRET, &user, &device())
            .await
            .unwrap();
        let (verified_tx, verified_rx) = tokio::sync::oneshot::channel();
        let (resume_tx, resume_rx) = tokio::sync::oneshot::channel();
        let race_db = db.clone();
        let refresh = session.refresh_token.clone();
        // The real flow's verification/rotation boundary is held explicitly.
        let worker = tokio::spawn(async move {
            let result = token_service::verify_refresh_token(&race_db, &refresh)
                .await
                .unwrap();
            let RefreshTokenVerifyResult::Valid(data) = result else {
                panic!("valid pre-event refresh");
            };
            verified_tx.send(()).unwrap();
            resume_rx.await.unwrap();
            refresh_verified_tokens(&race_db, SECRET, &refresh, data, &device()).await
        });
        verified_rx.await.unwrap();
        token_service::revoke_all_user_sessions(db, &user.user_id)
            .await
            .unwrap();
        resume_tx.send(()).unwrap();
        let result = worker.await.unwrap();
        assert!(
            matches!(result, Err(krausen_core::Error::Unauthorized(_))),
            "a verified old refresh must not rotate after revocation"
        );
        let sessions = token_service::get_user_sessions(db, &user.user_id)
            .await
            .unwrap();
        assert!(
            sessions.is_empty(),
            "race must not leave an active replacement refresh"
        );
        assert!(refused_by_cutoff(db, &session.access_token).await);

        // Reverse order: an entire real refresh commits, then the event cuts it off.
        let fresh = crate::session::create_authenticated_session(db, &kv, SECRET, &user, &device())
            .await
            .unwrap();
        let rotated = refresh_tokens(db, SECRET, &fresh.refresh_token, &device())
            .await
            .unwrap();
        token_service::revoke_all_user_sessions(db, &user.user_id)
            .await
            .unwrap();
        assert!(refused_by_cutoff(db, &rotated.access_token).await);
        assert!(
            refresh_tokens(db, SECRET, &rotated.raw_refresh_token, &device())
                .await
                .is_err()
        );
    }

    /// An OAuth client's refresh token must not refresh a browser session.
    async fn oauth_client_token_cannot_refresh_a_browser_session(db: &DbPool) {
        let user = new_user(db).await;
        let raw = jwt::create_refresh_token();
        let hash = token_service::hash_refresh_token(&raw);
        let mut oauth_device = device();
        oauth_device.oauth_client_id = Some("client-1".into());
        token_service::store_session_refresh_token(
            db,
            &user.user_id,
            &hash,
            chrono::Utc::now() + chrono::Duration::days(1),
            &oauth_device,
            &token_service::generate_family_id(),
            chrono::Utc::now().timestamp_micros(),
        )
        .await
        .unwrap();

        let result = refresh_tokens(db, SECRET, &raw, &device()).await;
        match result {
            Err(krausen_core::Error::Unauthorized(msg)) => assert!(
                msg.contains("OAuth client refresh token cannot refresh a browser session"),
                "{msg}"
            ),
            Err(other) => panic!("expected Unauthorized, got {other:?}"),
            Ok(_) => panic!("an OAuth client token must not refresh a browser session"),
        }
        assert_eq!(
            token_service::get_user_sessions(db, &user.user_id)
                .await
                .unwrap()
                .len(),
            1,
            "the refused token is neither rotated nor revoked"
        );
    }

    /// A rotation queued behind a revocation event observes the event's outcome.
    /// This pins the serialisation only: on SQLite the single pooled connection,
    /// and on PostgreSQL the account row lock, would also hold the rotation back,
    /// so it does not prove the explicit user lock itself (Kyomi's test does not
    /// either).
    async fn a_rotation_queued_behind_an_event_sees_its_outcome(db: &DbPool) {
        use std::future::Future;
        let user = new_user(db).await;
        let kv = krausen_core::kv_store_memory::InMemoryKVStore::new_pool();
        let session = crate::session::create_authenticated_session(db, &kv, SECRET, &user, &device())
            .await
            .unwrap();

        // Hold the user lock the way a revocation event does, and deactivate the
        // user's refresh tokens inside it.
        let mut held = db.begin_write().await.unwrap();
        krausen_core::dbtx_execute!(
            &mut held,
            "UPDATE users SET sessions_valid_from = sessions_valid_from WHERE user_id = $1",
            &user.user_id
        )
        .unwrap();
        let off = krausen_core::sql_compat::bool_false(db.is_postgres());
        let off_sql = format!("UPDATE refresh_tokens SET is_active = {off} WHERE user_id = $1");
        krausen_core::dbtx_execute!(&mut held, &off_sql, &user.user_id).unwrap();

        let rotation_device = device();
        let rotation =
            token_service::rotate_browser_refresh(db, &session.refresh_token, &rotation_device);
        let mut rotation = std::pin::pin!(rotation);
        std::future::poll_fn(|cx| {
            assert!(
                rotation.as_mut().poll(cx).is_pending(),
                "the rotation must queue behind the held event transaction"
            );
            std::task::Poll::Ready(())
        })
        .await;
        held.commit().await.unwrap();

        assert!(
            matches!(rotation.await, Err(krausen_core::Error::Unauthorized(_))),
            "the rotation must observe the event it queued behind"
        );
        assert!(token_service::get_user_sessions(db, &user.user_id)
            .await
            .unwrap()
            .is_empty());
    }

    both_backends!(
        controlled_refresh_race,
        oauth_client_token_cannot_refresh_a_browser_session,
        a_rotation_queued_behind_an_event_sees_its_outcome,
    );
}
