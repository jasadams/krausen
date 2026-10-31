// SPDX-License-Identifier: AGPL-3.0-or-later

//! Axum middleware for JWT-based authentication.
//!
//! Provides the `AuthUser` extractor that validates the JWT, loads the user
//! from the database, and enriches with workspace context.

use axum::{
    extract::{FromRef, FromRequestParts},
    http::request::Parts,
};
use chrono::Utc;

use krausen_core::enums::{WorkspaceRole, WorkspaceStatus};

use crate::jwt;

/// Shared state needed by the auth extractor.
#[derive(Clone)]
pub struct AuthState {
    pub jwt_secret: String,
    pub db: krausen_core::DbPool,
}

/// Workspace context enriched from the database.
#[derive(Debug, Clone, Default)]
pub struct WorkspaceContext {
    pub workspace_id: Option<String>,
    pub workspace_name: Option<String>,
    pub workspace_roles: Vec<WorkspaceRole>,
    pub workspace_status: Option<WorkspaceStatus>,
    pub is_owner: bool,
}

/// Authenticated user extracted from the request.
///
/// Use as an axum extractor: `AuthUser` in handler params.
/// Rejects with 401 if the token is missing, expired, or invalid.
///
/// The `user_id` is a String (format: `"user-{token_urlsafe(16)}"`),
/// NOT a UUID — user IDs are `user-<token>` strings.
#[derive(Debug, Clone)]
pub struct AuthUser {
    /// User ID from the database (String, not UUID).
    pub user_id: String,
    pub credential_id: String,
    /// User's email address.
    pub email: String,
    /// User's display name.
    pub name: Option<String>,
    /// User's roles (from extra_metadata).
    pub roles: Vec<String>,
    /// Whether the user account is active.
    pub active: bool,
    /// Whether the user's email is verified.
    pub verified: bool,
    /// Workspace context (enriched from DB).
    pub workspace: WorkspaceContext,
    /// JWT claims (for token_exp, jti access).
    pub token_exp: Option<i64>,
    pub token_jti: Option<String>,
}

impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync,
    AuthState: FromRef<S>,
{
    type Rejection = krausen_core::Error;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let auth_state = AuthState::from_ref(state);

        // Try Authorization header first, then cookie
        let token = extract_token(parts)?;

        let token_data = jwt::validate_browser_token(&token, &auth_state.jwt_secret)?;

        // Get user_id from claims — carried in the `extra` map as "user_id"
        let user_id = token_data
            .claims
            .extra
            .get("user_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| token_data.claims.sub.clone());
        if user_id != token_data.claims.sub {
            return Err(krausen_core::Error::Unauthorized(
                "Invalid browser credential".into(),
            ));
        }
        let credential_id = token_data
            .claims
            .extra
            .get("credential_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| krausen_core::Error::Unauthorized("Browser credential required".into()))?
            .to_string();
        let credential = krausen_core::db_fetch_optional!(
            &auth_state.db,
            CredentialIdRow,
            "SELECT credential_id FROM auth_credentials WHERE credential_id = $1 AND user_id = $2 AND kind = 'browser' AND issuer = $3 AND audience = $4 AND revoked_at IS NULL AND expires_at > $5",
            &credential_id,
            &user_id,
            jwt::BROWSER_ISSUER,
            jwt::BROWSER_AUDIENCE,
            Utc::now().timestamp()
        )?;
        if credential.as_ref().map(|row| row.credential_id.as_str()) != Some(credential_id.as_str())
        {
            return Err(krausen_core::Error::Unauthorized(
                "Browser credential revoked or expired".into(),
            ));
        }

        // Load user from database
        let user = crate::user_service::get_user_by_id(&auth_state.db, &user_id)
            .await
            .map_err(|e| {
                tracing::error!("database error loading user: {e}");
                krausen_core::Error::Internal("database error".into())
            })?
            .ok_or_else(|| krausen_core::Error::Unauthorized("User not found".into()))?;

        if !user.active {
            return Err(krausen_core::Error::Unauthorized(
                "User account is inactive".into(),
            ));
        }

        // Build workspace context from JWT's workspace_id claim
        let mut workspace_ctx = WorkspaceContext::default();

        let jwt_workspace_id = token_data
            .claims
            .extra
            .get("workspace_id")
            .and_then(|v| v.as_str());

        if let Some(ws_id) = jwt_workspace_id {
            // Fetch fresh workspace details from database
            match crate::user_service::get_workspace(&auth_state.db, ws_id).await {
                Ok(Some(ws)) => {
                    match crate::user_service::get_workspace_user(&auth_state.db, ws_id, &user_id)
                        .await
                    {
                        Ok(Some(wu)) => {
                            workspace_ctx.workspace_id = Some(ws_id.to_string());
                            workspace_ctx.workspace_name = ws.name.clone();
                            workspace_ctx.workspace_roles = vec![wu.role];
                            workspace_ctx.workspace_status = Some(ws.status);
                            workspace_ctx.is_owner = ws.owner_user_id == user_id;
                        }
                        Ok(None) => {
                            // User was removed from this workspace
                            return Err(krausen_core::Error::Unauthorized(
                                "Workspace membership revoked. Please log in again.".into(),
                            ));
                        }
                        Err(e) => {
                            tracing::warn!("could not load workspace user: {e}");
                        }
                    }
                }
                Ok(None) => {
                    tracing::warn!("workspace {ws_id} not found");
                }
                Err(e) => {
                    tracing::warn!("could not load workspace: {e}");
                }
            }
        }

        // Check if token is near expiry (< 5 min) — set header via extensions
        // The actual header is set in the response layer, not here.
        // We store the expiry time for the handler to check.
        let token_exp = Some(token_data.claims.exp);
        let token_jti = token_data.claims.jti.clone();

        let roles = user.roles();
        Ok(AuthUser {
            user_id: user.user_id,
            credential_id,
            email: user.email,
            name: user.name,
            roles,
            active: user.active,
            verified: user.verified,
            workspace: workspace_ctx,
            token_exp,
            token_jti,
        })
    }
}

#[derive(sqlx::FromRow)]
struct CredentialIdRow {
    credential_id: String,
}

impl AuthUser {
    /// Check if the access token is near expiry (< 5 minutes remaining).
    pub fn token_needs_refresh(&self) -> bool {
        if let Some(exp) = self.token_exp {
            let now = Utc::now().timestamp();
            let time_until_expiry = exp - now;
            time_until_expiry < 300 // 5 minutes in seconds
        } else {
            false
        }
    }
}

/// Extract a bearer token from the Authorization header or `access_token` cookie.
fn extract_token(parts: &Parts) -> krausen_core::Result<String> {
    // Check Authorization: Bearer <token>
    if let Some(auth_header) = parts.headers.get("authorization") {
        let value = auth_header
            .to_str()
            .map_err(|_| krausen_core::Error::Unauthorized("invalid auth header".into()))?;

        if let Some(token) = value.strip_prefix("Bearer ") {
            return Ok(token.to_string());
        }
    }

    // Fallback: access_token cookie (name from shared/constants.toml)
    let cookie_name = &krausen_core::constants::get().cookies.access_token_name;
    let cookie_prefix = format!("{cookie_name}=");
    if let Some(cookie_header) = parts.headers.get("cookie") {
        let cookies = cookie_header
            .to_str()
            .map_err(|_| krausen_core::Error::Unauthorized("invalid cookie header".into()))?;

        for cookie in cookies.split(';') {
            let cookie = cookie.trim();
            if let Some(token) = cookie.strip_prefix(&cookie_prefix) {
                return Ok(token.to_string());
            }
        }
    }

    Err(krausen_core::Error::Unauthorized(
        "Not authenticated".into(),
    ))
}

// Identity impl — when the state IS AuthState directly.
// (Axum's FromRef blanket impl handles this for types that impl Clone.)
// The AppState → AuthState impl is in apps/server/src/state.rs.
