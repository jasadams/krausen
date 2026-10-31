// SPDX-License-Identifier: AGPL-3.0-or-later

//! Token service — refresh token and verification token operations.
//!
//! Refresh tokens: stored as SHA-256 hash of opaque `rt_<base64url>` value.
//! Verification tokens: stored as bcrypt hash.
//!
//! ## Token Rotation
//!
//! Each login creates a "token family" (shared `family_id`). On refresh,
//! the old token is marked as replaced (`replaced_at = NOW()`) and a new
//! token is created in the same family. A 30-second grace period allows
//! multi-tab race conditions. If an old token is used after the grace
//! period, the entire family is revoked (theft detection).

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Duration, Utc};
use krausen_core::DbPool;
use krausen_core::models::VerificationToken;
use krausen_core::sql_compat;
use rand::Rng;
use sha2::{Digest, Sha256};

/// Device info extracted from the HTTP request.
#[derive(Debug, Clone)]
pub struct DeviceInfo {
    pub user_agent: Option<String>,
    pub ip_address: Option<String>,
    pub country_code: Option<String>,
    pub oauth_client_id: Option<String>,
}

// ---------------------------------------------------------------------------
// Refresh token verification result
// ---------------------------------------------------------------------------

/// Result of verifying a refresh token, accounting for rotation state.
pub enum RefreshTokenVerifyResult {
    /// Token is valid and current (not replaced). Caller should rotate it.
    Valid(RefreshTokenUserData),
    /// Token was replaced but is within the grace period. Caller should
    /// rotate again so each tab gets a fresh token in its cookie.
    GracePeriod(RefreshTokenUserData),
    /// Token was replaced AND used after the grace period expired.
    /// This indicates theft — the entire family has been revoked.
    TheftDetected { family_id: String, user_id: String },
    /// Token not found, expired, or revoked.
    Invalid,
}

// ---------------------------------------------------------------------------
// Refresh token operations
// ---------------------------------------------------------------------------

/// Hash a raw refresh token using SHA-256 for storage/lookup.
pub fn hash_refresh_token(raw_token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(raw_token.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Generate a new family_id for a login session.
pub fn generate_family_id() -> String {
    let random_bytes: [u8; 16] = rand::rng().random();
    format!("fam_{}", URL_SAFE_NO_PAD.encode(random_bytes))
}

/// Store a new refresh token in the database.
///
/// Returns the token_id (format: `"rt_{token_urlsafe(16)}"`).
pub async fn store_refresh_token(
    pool: &DbPool,
    user_id: &str,
    token_hash: &str,
    expires_at: DateTime<Utc>,
    device_info: &DeviceInfo,
    family_id: &str,
) -> krausen_core::Result<String> {
    let random_bytes: [u8; 16] = rand::rng().random();
    let token_id = format!("rt_{}", URL_SAFE_NO_PAD.encode(random_bytes));

    let mut tx = pool.begin_write().await?;
    #[derive(sqlx::FromRow)]
    struct ActiveUser {
        user_id: String,
    }
    let account_sql = if pool.is_postgres() {
        "SELECT user_id FROM users WHERE user_id = $1 AND active = TRUE FOR SHARE"
    } else {
        "SELECT user_id FROM users WHERE user_id = $1 AND active = 1"
    };
    if krausen_core::dbtx_fetch_optional!(&mut tx, ActiveUser, account_sql, user_id)?.is_none() {
        return Err(krausen_core::Error::Unauthorized(
            "Account unavailable".into(),
        ));
    }
    krausen_core::dbtx_execute!(
        &mut tx,
        "INSERT INTO auth_credentials (credential_id, user_id, kind, issuer, audience, expires_at) VALUES ($1, $2, 'browser', $3, $4, $5)",
        family_id,
        user_id,
        crate::jwt::BROWSER_ISSUER,
        crate::jwt::BROWSER_AUDIENCE,
        expires_at.timestamp()
    )?;
    krausen_core::dbtx_execute!(
        &mut tx,
        "INSERT INTO refresh_tokens \
         (token_id, user_id, token_hash, expires_at, user_agent, ip_address, country_code, family_id, oauth_client_id) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
        &token_id,
        user_id,
        token_hash,
        &expires_at,
        &device_info.user_agent,
        &device_info.ip_address,
        &device_info.country_code,
        family_id,
        &device_info.oauth_client_id
    )?;

    tx.commit().await?;

    Ok(token_id)
}

/// Verify a raw refresh token, handling rotation grace period and theft detection.
pub async fn verify_refresh_token(
    pool: &DbPool,
    raw_token: &str,
) -> krausen_core::Result<RefreshTokenVerifyResult> {
    let token_hash = hash_refresh_token(raw_token);
    let is_pg = pool.is_postgres();
    let now = sql_compat::now(is_pg);
    let rt_unexpired = sql_compat::is_future(is_pg, "rt.expires_at");
    let bt = sql_compat::bool_true(is_pg);

    // Fetch the token + user data in one query (include replaced_at and family_id)
    let sql = format!(
        "SELECT rt.token_id, rt.user_id, rt.family_id, rt.replaced_at, \
                u.email, u.name, u.extra_metadata, u.active as user_active \
         FROM refresh_tokens rt \
         JOIN users u ON u.user_id = rt.user_id \
         WHERE rt.token_hash = $1 \
           AND rt.is_active = {bt} \
           AND {rt_unexpired}"
    );
    let row = krausen_core::db_fetch_optional!(pool, RefreshTokenWithUser, &sql, &token_hash)?;

    let Some(row) = row else {
        return Ok(RefreshTokenVerifyResult::Invalid);
    };

    if !row.user_active {
        return Ok(RefreshTokenVerifyResult::Invalid);
    }

    // Extract roles from extra_metadata
    let roles = row
        .extra_metadata
        .as_ref()
        .and_then(|m| m.get("roles"))
        .and_then(|r| serde_json::from_value::<Vec<String>>(r.clone()).ok())
        .unwrap_or_else(|| vec!["user".to_string()]);

    let user_data = RefreshTokenUserData {
        user_id: row.user_id.clone(),
        email: row.email,
        name: row.name,
        roles,
        token_id: row.token_id,
        family_id: row.family_id.clone(),
    };

    // Check if this token has been replaced (rotated)
    if let Some(replaced_at) = row.replaced_at {
        let grace_seconds = krausen_core::constants::get()
            .jwt
            .refresh_token_grace_period_seconds;
        let grace_deadline = replaced_at + Duration::seconds(grace_seconds);

        if Utc::now() <= grace_deadline {
            // Within grace period — allow but don't rotate again
            // Update last_used
            let update_sql =
                format!("UPDATE refresh_tokens SET last_used = {now} WHERE token_id = $1");
            krausen_core::db_execute!(pool, &update_sql, &user_data.token_id)?;

            return Ok(RefreshTokenVerifyResult::GracePeriod(user_data));
        } else {
            // Past grace period — theft detected! Revoke entire family.
            tracing::warn!(
                family_id = %row.family_id,
                user_id = %row.user_id,
                token_id = %user_data.token_id,
                "refresh token reuse after grace period — revoking token family (theft detection)"
            );
            revoke_token_family(pool, &row.family_id).await?;
            return Ok(RefreshTokenVerifyResult::TheftDetected {
                family_id: row.family_id,
                user_id: row.user_id,
            });
        }
    }

    // Token is current (not replaced) — update last_used
    let update_sql = format!("UPDATE refresh_tokens SET last_used = {now} WHERE token_id = $1");
    krausen_core::db_execute!(pool, &update_sql, &user_data.token_id)?;

    Ok(RefreshTokenVerifyResult::Valid(user_data))
}

/// Rotate a refresh token: mark the old one as replaced, create a new one in the same family.
///
/// Returns the new token_id.
pub async fn rotate_refresh_token(
    pool: &DbPool,
    old_token_id: &str,
    user_id: &str,
    family_id: &str,
    new_token_hash: &str,
    expires_at: DateTime<Utc>,
    device_info: &DeviceInfo,
) -> krausen_core::Result<String> {
    let is_pg = pool.is_postgres();
    let now = sql_compat::now(is_pg);

    // Mark the old token as replaced
    let replace_sql = format!("UPDATE refresh_tokens SET replaced_at = {now} WHERE token_id = $1");
    krausen_core::db_execute!(pool, &replace_sql, old_token_id)?;

    // Create the new token in the same family
    let random_bytes: [u8; 16] = rand::rng().random();
    let new_token_id = format!("rt_{}", URL_SAFE_NO_PAD.encode(random_bytes));

    krausen_core::db_execute!(
        pool,
        "INSERT INTO refresh_tokens \
         (token_id, user_id, token_hash, expires_at, user_agent, ip_address, country_code, family_id) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        &new_token_id,
        user_id,
        new_token_hash,
        &expires_at,
        &device_info.user_agent,
        &device_info.ip_address,
        &device_info.country_code,
        family_id
    )?;

    Ok(new_token_id)
}

/// Revoke all tokens in a family (theft detection or logout).
pub async fn revoke_token_family(pool: &DbPool, family_id: &str) -> krausen_core::Result<u64> {
    #[derive(sqlx::FromRow)]
    struct UserId {
        user_id: String,
    }
    let mut tx = pool.begin_write().await?;
    let user = krausen_core::dbtx_fetch_optional!(
        &mut tx,
        UserId,
        "SELECT user_id FROM refresh_tokens WHERE family_id = $1 LIMIT 1",
        family_id
    )?;
    let Some(user) = user else { return Ok(0) };
    let pg = pool.is_postgres();
    let lock = if pg {
        "SELECT user_id FROM users WHERE user_id = $1 FOR SHARE"
    } else {
        "SELECT user_id FROM users WHERE user_id = $1"
    };
    let _ = krausen_core::dbtx_fetch_optional!(&mut tx, UserId, lock, &user.user_id)?;
    krausen_core::dbtx_execute!(
        &mut tx,
        "UPDATE auth_credentials SET revoked_at = $1 WHERE credential_id = $2 AND user_id = $3 AND revoked_at IS NULL",
        Utc::now().timestamp(),
        family_id,
        &user.user_id
    )?;
    let false_value = sql_compat::bool_false(pg);
    let true_value = sql_compat::bool_true(pg);
    let sql = format!(
        "UPDATE refresh_tokens SET is_active = {false_value}, revoked_at = CURRENT_TIMESTAMP WHERE family_id = $1 AND is_active = {true_value}"
    );
    let result = krausen_core::dbtx_execute!(&mut tx, &sql, family_id)?;
    tx.commit().await?;
    Ok(result.rows_affected())
}

/// Revoke a specific refresh token owned by a user — revokes the entire family.
///
/// Looks up the token's family_id, then revokes all tokens in that family.
pub async fn revoke_user_refresh_token(
    pool: &DbPool,
    user_id: &str,
    token_id: &str,
) -> krausen_core::Result<bool> {
    let is_pg = pool.is_postgres();
    let bt = sql_compat::bool_true(is_pg);
    let sql = format!(
        "SELECT family_id FROM refresh_tokens \
         WHERE token_id = $1 AND user_id = $2 AND is_active = {bt}"
    );

    // Use a helper struct for the scalar-like query
    #[derive(sqlx::FromRow)]
    struct FamilyIdRow {
        family_id: String,
    }

    let row = krausen_core::db_fetch_optional!(pool, FamilyIdRow, &sql, token_id, user_id)?;

    let Some(row) = row else {
        return Ok(false);
    };

    // Revoke the entire family
    let count = revoke_token_family(pool, &row.family_id).await?;
    Ok(count > 0)
}

/// Revoke ALL refresh tokens for a user. Returns count of revoked tokens.
pub async fn revoke_all_user_refresh_tokens(
    pool: &DbPool,
    user_id: &str,
) -> krausen_core::Result<u64> {
    #[derive(sqlx::FromRow)]
    struct UserId {
        user_id: String,
    }
    let mut tx = pool.begin_write().await?;
    let pg = pool.is_postgres();
    let lock = if pg {
        "SELECT user_id FROM users WHERE user_id = $1 FOR UPDATE"
    } else {
        "SELECT user_id FROM users WHERE user_id = $1"
    };
    let user = krausen_core::dbtx_fetch_optional!(&mut tx, UserId, lock, user_id)?;
    if user.is_none() {
        return Ok(0);
    }
    krausen_core::dbtx_execute!(
        &mut tx,
        "UPDATE auth_credentials SET revoked_at = $1 WHERE user_id = $2 AND kind = 'browser' AND revoked_at IS NULL",
        Utc::now().timestamp(),
        user_id
    )?;
    let false_value = sql_compat::bool_false(pg);
    let true_value = sql_compat::bool_true(pg);
    let sql = format!(
        "UPDATE refresh_tokens SET is_active = {false_value}, revoked_at = CURRENT_TIMESTAMP WHERE user_id = $1 AND is_active = {true_value}"
    );
    let result = krausen_core::dbtx_execute!(&mut tx, &sql, user_id)?;
    tx.commit().await?;
    Ok(result.rows_affected())
}

/// Get all active sessions (refresh tokens) for a user.
///
/// Only returns the *current* token per family (where `replaced_at IS NULL`),
/// so rotated-away tokens don't clutter the session list.
///
/// Postgres uses `DISTINCT ON`; SQLite uses a subquery with GROUP BY.
pub async fn get_user_sessions(
    pool: &DbPool,
    user_id: &str,
) -> krausen_core::Result<Vec<SessionInfo>> {
    let is_pg = pool.is_postgres();
    let rt_unexpired = sql_compat::is_future(is_pg, "rt.expires_at");
    let bt = sql_compat::bool_true(is_pg);

    let sql = if is_pg {
        format!(
            "SELECT s.token_id, s.token_hash, s.family_id, s.created_at, s.last_used, \
                    s.expires_at, s.user_agent, s.ip_address, s.country_code, \
                    s.oauth_client_name \
             FROM ( \
                 SELECT DISTINCT ON (rt.family_id) \
                        rt.token_id, rt.token_hash, rt.family_id, rt.created_at, rt.last_used, \
                        rt.expires_at, rt.user_agent, rt.ip_address, rt.country_code, \
                        oc.name AS oauth_client_name \
                 FROM refresh_tokens rt \
                 LEFT JOIN oauth_clients oc ON rt.oauth_client_id = oc.client_id \
                 WHERE rt.user_id = $1 AND rt.is_active = {bt} AND {rt_unexpired} \
                       AND rt.replaced_at IS NULL \
                 ORDER BY rt.family_id, rt.created_at DESC \
             ) s \
             ORDER BY s.last_used DESC NULLS LAST"
        )
    } else {
        // SQLite: replaced_at IS NULL already gives us one token per family
        // (the current non-replaced token). No DISTINCT ON needed.
        format!(
            "SELECT rt.token_id, rt.token_hash, rt.family_id, rt.created_at, rt.last_used, \
                    rt.expires_at, rt.user_agent, rt.ip_address, rt.country_code, \
                    oc.name AS oauth_client_name \
             FROM refresh_tokens rt \
             LEFT JOIN oauth_clients oc ON rt.oauth_client_id = oc.client_id \
             WHERE rt.user_id = $1 AND rt.is_active = {bt} AND {rt_unexpired} \
                   AND rt.replaced_at IS NULL \
             ORDER BY COALESCE(rt.last_used, '1970-01-01') DESC"
        )
    };

    let sessions = krausen_core::db_fetch_all!(pool, SessionInfo, &sql, user_id)?;
    Ok(sessions)
}

// ---------------------------------------------------------------------------
// Verification token operations
// ---------------------------------------------------------------------------

/// Create a new email verification token.
///
/// Returns the raw (unhashed) token value for inclusion in the verification link.
///
/// `expire_hours` overrides the default from constants.toml — used for short-lived
/// recovery tokens (15 min = 0.25 hours). Pass `None` for the default.
pub async fn create_verification_token(
    pool: &DbPool,
    email: &str,
    token_type: &str,
) -> krausen_core::Result<String> {
    create_verification_token_with_expiry(pool, email, token_type, None).await
}

/// Create a verification token with custom expiry (in hours).
pub async fn create_verification_token_with_expiry(
    pool: &DbPool,
    email: &str,
    token_type: &str,
    expire_hours: Option<f64>,
) -> krausen_core::Result<String> {
    let random_bytes: [u8; 32] = rand::rng().random();
    let raw_token = URL_SAFE_NO_PAD.encode(random_bytes);

    // Hash with bcrypt (verification tokens are low-entropy, need slow hash)
    let token_hash = bcrypt::hash(&raw_token, bcrypt::DEFAULT_COST)
        .map_err(|e| krausen_core::Error::Internal(format!("bcrypt hash failed: {e}")))?;

    let id_bytes: [u8; 16] = rand::rng().random();
    let token_id = format!("vt_{}", URL_SAFE_NO_PAD.encode(id_bytes));

    let default_hours = krausen_core::constants::get()
        .jwt
        .email_verification_expire_hours as f64;
    let hours = expire_hours.unwrap_or(default_hours);
    let expires_at = Utc::now() + Duration::seconds((hours * 3600.0) as i64);

    krausen_core::db_execute!(
        pool,
        "INSERT INTO verification_tokens \
         (token_id, email, token_hash, token_type, expires_at) \
         VALUES ($1, $2, $3, $4, $5)",
        &token_id,
        email,
        &token_hash,
        token_type,
        &expires_at
    )?;

    Ok(raw_token)
}

/// Verify an email verification token.
///
/// Checks all unused, unexpired tokens of the given type and compares with bcrypt.
/// If valid, marks the token as used and returns the associated email.
pub async fn verify_verification_token(
    pool: &DbPool,
    raw_token: &str,
    token_type: &str,
) -> krausen_core::Result<Option<String>> {
    let is_pg = pool.is_postgres();
    let now = sql_compat::now(is_pg);
    let unexpired = sql_compat::is_future(is_pg, "expires_at");
    let bf = sql_compat::bool_false(is_pg);
    let bt = sql_compat::bool_true(is_pg);

    let fetch_sql = format!(
        "SELECT * FROM verification_tokens \
         WHERE token_type = $1 AND used = {bf} AND {unexpired}"
    );
    let tokens = krausen_core::db_fetch_all!(pool, VerificationToken, &fetch_sql, token_type)?;

    for db_token in tokens {
        let is_match = bcrypt::verify(raw_token, &db_token.token_hash).unwrap_or(false);

        if is_match {
            // Mark as used
            let update_sql = format!(
                "UPDATE verification_tokens \
                 SET used = {bt}, used_at = {now} \
                 WHERE token_id = $1"
            );
            krausen_core::db_execute!(pool, &update_sql, &db_token.token_id)?;

            return Ok(Some(db_token.email));
        }
    }

    Ok(None)
}

// ---------------------------------------------------------------------------
// Helper types
// ---------------------------------------------------------------------------

/// User data returned by refresh token verification.
#[derive(Debug, Clone)]
pub struct RefreshTokenUserData {
    pub user_id: String,
    pub email: String,
    pub name: Option<String>,
    pub roles: Vec<String>,
    pub token_id: String,
    pub family_id: String,
}

/// Internal query result for refresh token + user join.
#[derive(Debug, sqlx::FromRow)]
struct RefreshTokenWithUser {
    token_id: String,
    user_id: String,
    family_id: String,
    replaced_at: Option<DateTime<Utc>>,
    email: String,
    name: Option<String>,
    extra_metadata: Option<serde_json::Value>,
    user_active: bool,
}

/// Session info returned to the frontend.
#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct SessionInfo {
    pub token_id: String,
    /// Used internally for `is_current` comparison — never serialized.
    #[serde(skip)]
    pub token_hash: String,
    /// Family ID for this session — used for current-session comparison.
    #[serde(skip)]
    pub family_id: String,
    pub created_at: DateTime<Utc>,
    pub last_used: Option<DateTime<Utc>>,
    pub expires_at: DateTime<Utc>,
    pub user_agent: Option<String>,
    pub ip_address: Option<String>,
    pub country_code: Option<String>,
    pub oauth_client_name: Option<String>,
}

/// Rotate a browser refresh token atomically. Reuse revokes the entire durable
/// credential family; an old unbound token cannot create a new session.
pub async fn rotate_browser_refresh(
    pool: &DbPool,
    raw_token: &str,
    device: &DeviceInfo,
) -> krausen_core::Result<(String, String, String)> {
    #[derive(sqlx::FromRow)]
    struct TokenRef {
        token_id: String,
        user_id: String,
        family_id: String,
    }
    #[derive(sqlx::FromRow)]
    struct Id {
        id: String,
    }
    #[derive(sqlx::FromRow)]
    struct TokenState {
        used: i64,
    }
    let hash = hash_refresh_token(raw_token);
    let mut tx = pool.begin_write().await?;
    let token = krausen_core::dbtx_fetch_optional!(
        &mut tx,
        TokenRef,
        "SELECT token_id,user_id,family_id FROM refresh_tokens WHERE token_hash = $1",
        &hash
    )?
    .ok_or_else(|| krausen_core::Error::Unauthorized("Invalid refresh token".into()))?;
    let pg = pool.is_postgres();
    let account_sql = if pg {
        "SELECT user_id AS id FROM users WHERE user_id = $1 AND active = TRUE FOR SHARE"
    } else {
        "SELECT user_id AS id FROM users WHERE user_id = $1 AND active = 1"
    };
    let account = krausen_core::dbtx_fetch_optional!(&mut tx, Id, account_sql, &token.user_id)?;
    if account.is_none() {
        return Err(krausen_core::Error::Unauthorized(
            "Invalid refresh token".into(),
        ));
    }
    let credential_sql = if pg {
        "SELECT credential_id AS id FROM auth_credentials WHERE credential_id = $1 AND user_id = $2 AND kind = 'browser' AND revoked_at IS NULL AND expires_at > $3 FOR UPDATE"
    } else {
        "SELECT credential_id AS id FROM auth_credentials WHERE credential_id = $1 AND user_id = $2 AND kind = 'browser' AND revoked_at IS NULL AND expires_at > $3"
    };
    let now_ts = Utc::now().timestamp();
    let credential = krausen_core::dbtx_fetch_optional!(
        &mut tx,
        Id,
        credential_sql,
        &token.family_id,
        &token.user_id,
        now_ts
    )?;
    if credential.is_none() {
        return Err(krausen_core::Error::Unauthorized(
            "Invalid refresh token".into(),
        ));
    }
    let active = sql_compat::bool_true(pg);
    let future = sql_compat::is_future(pg, "expires_at");
    let lock = if pg { " FOR UPDATE" } else { "" };
    let sql = format!(
        "SELECT CAST(CASE WHEN replaced_at IS NULL THEN 0 ELSE 1 END AS BIGINT) AS used FROM refresh_tokens WHERE token_id = $1 AND is_active = {active} AND {future}{lock}"
    );
    let state = krausen_core::dbtx_fetch_optional!(&mut tx, TokenState, &sql, &token.token_id)?
        .ok_or_else(|| krausen_core::Error::Unauthorized("Invalid refresh token".into()))?;
    if state.used != 0 {
        krausen_core::dbtx_execute!(
            &mut tx,
            "UPDATE auth_credentials SET revoked_at = $1 WHERE credential_id = $2 AND revoked_at IS NULL",
            now_ts,
            &token.family_id
        )?;
        let false_value = sql_compat::bool_false(pg);
        let sql = format!(
            "UPDATE refresh_tokens SET is_active = {false_value}, revoked_at = CURRENT_TIMESTAMP WHERE family_id = $1"
        );
        krausen_core::dbtx_execute!(&mut tx, &sql, &token.family_id)?;
        tx.commit().await?;
        return Err(krausen_core::Error::Unauthorized(
            "Refresh token replayed".into(),
        ));
    }
    let replaced = krausen_core::dbtx_execute!(
        &mut tx,
        "UPDATE refresh_tokens SET replaced_at = CURRENT_TIMESTAMP WHERE token_id = $1 AND replaced_at IS NULL",
        &token.token_id
    )?;
    if replaced.rows_affected() != 1 {
        return Err(krausen_core::Error::Conflict(
            "Refresh token changed".into(),
        ));
    }
    let new_raw = crate::jwt::create_refresh_token();
    let new_hash = hash_refresh_token(&new_raw);
    let id = format!("rt_{}", uuid::Uuid::new_v4());
    let expires =
        Utc::now() + Duration::days(krausen_core::constants::get().jwt.refresh_token_expire_days);
    krausen_core::dbtx_execute!(
        &mut tx,
        "INSERT INTO refresh_tokens (token_id,user_id,token_hash,expires_at,user_agent,ip_address,country_code,family_id) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",
        &id,
        &token.user_id,
        &new_hash,
        &expires,
        &device.user_agent,
        &device.ip_address,
        &device.country_code,
        &token.family_id
    )?;
    tx.commit().await?;
    Ok((token.user_id, token.family_id, new_raw))
}
