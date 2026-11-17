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

    krausen_core::db_execute!(
        pool,
        "INSERT INTO refresh_tokens \
         (token_id, user_id, token_hash, expires_at, user_agent, ip_address, country_code, family_id, oauth_client_id) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
        &token_id, user_id, token_hash, &expires_at,
        &device_info.user_agent, &device_info.ip_address,
        &device_info.country_code, family_id, &device_info.oauth_client_id
    )?;

    Ok(token_id)
}

/// Persist a successful MCP refresh without rotating its opaque token.
///
/// The guarded update serializes with revocation and keeps the later expiry
/// when concurrent requests finish out of order. Replaced tokens remain usable
/// only within their original grace period; neither deadline is extended.
/// Returns false if the grant stopped being usable after verification.
pub async fn renew_oauth_refresh_token(
    pool: &DbPool,
    token_id: &str,
    oauth_client_id: &str,
    expires_at: DateTime<Utc>,
) -> krausen_core::Result<bool> {
    let is_pg = pool.is_postgres();
    let bt = sql_compat::bool_true(is_pg);
    let grace_seconds = krausen_core::constants::get().jwt.refresh_token_grace_period_seconds;
    // SQLite stores both RFC3339 timestamps (chrono binds) and SQL datetime
    // strings. Compare instants, not their different textual representations.
    let (later, unexpired, within_grace) = if is_pg {
        (
            "expires_at < $3".to_string(),
            "expires_at > clock_timestamp()".to_string(),
            format!("replaced_at + INTERVAL '{grace_seconds} seconds' >= clock_timestamp()"),
        )
    } else {
        (
            "julianday(expires_at) < julianday($3)".to_string(),
            "julianday(expires_at) > julianday('now')".to_string(),
            format!("julianday(replaced_at) + {grace_seconds} / 86400.0 >= julianday('now')"),
        )
    };
    let sql = format!(
        "UPDATE refresh_tokens \
         SET expires_at = CASE WHEN replaced_at IS NULL AND {later} THEN $3 ELSE expires_at END \
         WHERE token_id = $1 AND oauth_client_id = $2 \
           AND is_active = {bt} AND revoked_at IS NULL AND {unexpired} \
           AND (replaced_at IS NULL OR {within_grace}) \
           AND EXISTS (SELECT 1 FROM users u WHERE u.user_id = refresh_tokens.user_id AND u.active = {bt})"
    );
    let result = krausen_core::db_execute!(pool, &sql, token_id, oauth_client_id, &expires_at)?;
    Ok(result.rows_affected() > 0)
}

/// Persist a browser login's refresh token only if its signed access issuance
/// still follows the current cutoff. Serializes with revocation so a pre-event
/// mint cannot insert a surviving refresh token after the event commits.
///
/// Returns the token_id (format: `"rt_{token_urlsafe(16)}"`).
pub async fn store_session_refresh_token(
    pool: &DbPool,
    user_id: &str,
    token_hash: &str,
    expires_at: DateTime<Utc>,
    device_info: &DeviceInfo,
    family_id: &str,
    issued_at_us: i64,
) -> krausen_core::Result<String> {
    let random_bytes: [u8; 16] = rand::rng().random();
    let token_id = format!("rt_{}", URL_SAFE_NO_PAD.encode(random_bytes));

    let mut tx = pool.begin_write().await?;
    // Lock order is always user then tokens, shared with all-session
    // revocation. SQLite's first write obtains the database write lock;
    // PostgreSQL locks this row.
    #[derive(sqlx::FromRow)]
    struct Cutoff {
        sessions_valid_from: Option<i64>,
    }
    let cutoff = krausen_core::dbtx_fetch_optional!(
        &mut tx,
        Cutoff,
        "UPDATE users SET sessions_valid_from = sessions_valid_from WHERE user_id = $1 RETURNING sessions_valid_from",
        user_id
    )?
    .ok_or_else(|| krausen_core::Error::Unauthorized("Account unavailable".into()))?
    .sessions_valid_from;
    if cutoff.is_some_and(|cutoff| issued_at_us <= cutoff) {
        return Err(krausen_core::Error::Unauthorized(
            "Session revoked during login".into(),
        ));
    }
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

// SQLite stores both RFC3339 (Chrono binds) and SQL timestamp text. Compare
// parsed instants at SQLite millisecond precision, rather than their separators.
fn unexpired_token_sql(is_pg: bool, column: &str) -> String {
    if is_pg {
        format!("{column} > NOW()")
    } else {
        format!("julianday({column}) > julianday('now')")
    }
}

/// Verify a raw refresh token, handling rotation grace period and theft detection.
pub async fn verify_refresh_token(
    pool: &DbPool,
    raw_token: &str,
) -> krausen_core::Result<RefreshTokenVerifyResult> {
    let token_hash = hash_refresh_token(raw_token);
    let is_pg = pool.is_postgres();
    let now = sql_compat::now(is_pg);
    let rt_unexpired = unexpired_token_sql(is_pg, "rt.expires_at");
    let bt = sql_compat::bool_true(is_pg);

    // Fetch the token + user data in one query (include replaced_at and family_id)
    let sql = format!(
        "SELECT rt.token_id, rt.user_id, rt.family_id, rt.replaced_at, rt.oauth_client_id, \
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
        oauth_client_id: row.oauth_client_id,
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
    let random_bytes: [u8; 16] = rand::rng().random();
    let new_token_id = format!("rt_{}", URL_SAFE_NO_PAD.encode(random_bytes));
    // Lock order is always user then tokens, shared with all-session revocation.
    // SQLite's first write obtains the database write lock; Postgres locks this row.
    let mut tx = pool.begin_write().await?;
    krausen_core::dbtx_execute!(
        &mut tx,
        "UPDATE users SET sessions_valid_from = sessions_valid_from WHERE user_id = $1",
        user_id
    )?;
    let is_pg = pool.is_postgres();
    let now = sql_compat::now(is_pg);
    let bt = sql_compat::bool_true(is_pg);
    let unexpired = unexpired_token_sql(is_pg, "expires_at");
    let sql = format!(
        "UPDATE refresh_tokens SET replaced_at = {now} WHERE token_id = $1 AND user_id = $2 AND family_id = $3 AND is_active = {bt} AND {unexpired}"
    );
    let replaced = krausen_core::dbtx_execute!(&mut tx, &sql, old_token_id, user_id, family_id)?;
    if replaced.rows_affected() == 0 {
        return Err(krausen_core::Error::Unauthorized(
            "Refresh token has been revoked".into(),
        ));
    }
    krausen_core::dbtx_execute!(
        &mut tx,
        "INSERT INTO refresh_tokens (token_id, user_id, token_hash, expires_at, user_agent, ip_address, country_code, family_id) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        &new_token_id,
        user_id,
        new_token_hash,
        &expires_at,
        &device_info.user_agent,
        &device_info.ip_address,
        &device_info.country_code,
        family_id
    )?;
    tx.commit().await?;
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

/// Immediately revoke browser access sessions and all user refresh tokens.
/// Both writes commit together even with no refresh tokens. The cutoff advances
/// strictly under concurrent/repeated events, including wall-clock rollback.
pub async fn revoke_all_user_sessions(pool: &DbPool, user_id: &str) -> krausen_core::Result<u64> {
    let mut tx = pool.begin_write().await?;
    // Sample issuance cutoff only after acquiring the serialization lock.
    // A queued event must include sessions minted while it was waiting.
    krausen_core::dbtx_execute!(
        &mut tx,
        "UPDATE users SET sessions_valid_from = sessions_valid_from WHERE user_id = $1",
        user_id
    )?;
    let candidate = Utc::now().timestamp_micros();
    let advanced = krausen_core::dbtx_execute!(
        &mut tx,
        "UPDATE users SET sessions_valid_from = CASE WHEN sessions_valid_from >= $1 THEN sessions_valid_from + 1 ELSE $1 END WHERE user_id = $2",
        candidate,
        user_id
    )?;
    if advanced.rows_affected() == 0 {
        return Err(krausen_core::Error::Unauthorized("User not found".into()));
    }
    let pg = pool.is_postgres();
    let false_value = sql_compat::bool_false(pg);
    let true_value = sql_compat::bool_true(pg);
    let sql = format!(
        "UPDATE refresh_tokens SET is_active = {false_value}, revoked_at = CURRENT_TIMESTAMP WHERE user_id = $1 AND is_active = {true_value}"
    );
    let count = krausen_core::dbtx_execute!(&mut tx, &sql, user_id)?.rows_affected();
    tx.commit().await?;
    Ok(count)
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
    let rt_unexpired = unexpired_token_sql(is_pg, "rt.expires_at");
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
    let unexpired = unexpired_token_sql(is_pg, "expires_at");
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
    /// Set when the token was issued to an OAuth client rather than a browser.
    pub oauth_client_id: Option<String>,
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
    oauth_client_id: Option<String>,
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

/// Rotate a browser refresh token atomically, with Kyomi's grace-period semantics.
///
/// A current token rotates. A token that was already replaced is still accepted
/// inside `refresh_token_grace_period_seconds` (a lost response or another tab racing
/// the same cookie) and rotates again so each caller gets a fresh token; reuse AFTER the
/// grace period is theft and revokes the whole token family.
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
    struct TokenState {
        replaced_at: Option<DateTime<Utc>>,
    }
    let hash = hash_refresh_token(raw_token);
    let mut tx = pool.begin_write().await?;
    let token = krausen_core::dbtx_fetch_optional!(
        &mut tx,
        TokenRef,
        "SELECT token_id,user_id,family_id FROM refresh_tokens WHERE token_hash = $1",
        &hash
    )?
    .ok_or_else(|| krausen_core::Error::Unauthorized("Invalid or expired refresh token".into()))?;
    // Lock order is always user then tokens, shared with all-session revocation
    // and login persistence. SQLite's first write obtains the database write
    // lock; PostgreSQL locks this row.
    krausen_core::dbtx_execute!(
        &mut tx,
        "UPDATE users SET sessions_valid_from = sessions_valid_from WHERE user_id = $1",
        &token.user_id
    )?;
    let pg = pool.is_postgres();
    let active = sql_compat::bool_true(pg);
    let future = unexpired_token_sql(pg, "expires_at");
    let lock = if pg { " FOR UPDATE" } else { "" };
    let sql = format!(
        "SELECT replaced_at FROM refresh_tokens WHERE token_id = $1 AND is_active = {active} AND {future}{lock}"
    );
    let state = krausen_core::dbtx_fetch_optional!(&mut tx, TokenState, &sql, &token.token_id)?
        .ok_or_else(|| krausen_core::Error::Unauthorized("Invalid or expired refresh token".into()))?;
    let past_grace = state.replaced_at.is_some_and(|replaced_at| {
        let grace = krausen_core::constants::get()
            .jwt
            .refresh_token_grace_period_seconds;
        Utc::now() > replaced_at + Duration::seconds(grace)
    });
    if past_grace {
        tracing::warn!(
            family_id = %token.family_id,
            user_id = %token.user_id,
            token_id = %token.token_id,
            "refresh token reuse after grace period — revoking token family (theft detection)"
        );
        let false_value = sql_compat::bool_false(pg);
        let sql = format!(
            "UPDATE refresh_tokens SET is_active = {false_value}, revoked_at = CURRENT_TIMESTAMP WHERE family_id = $1"
        );
        krausen_core::dbtx_execute!(&mut tx, &sql, &token.family_id)?;
        tx.commit().await?;
        return Err(krausen_core::Error::Unauthorized(
            "Refresh token has been revoked (possible token theft detected)".into(),
        ));
    }
    // As Kyomi's verify + rotate: record the use, then mark the token replaced now. A
    // re-rotation inside the grace period therefore restarts that token's grace window.
    let now = sql_compat::now(pg);
    let used_sql = format!("UPDATE refresh_tokens SET last_used = {now} WHERE token_id = $1");
    krausen_core::dbtx_execute!(&mut tx, &used_sql, &token.token_id)?;
    let replace_sql = format!("UPDATE refresh_tokens SET replaced_at = {now} WHERE token_id = $1");
    let replaced = krausen_core::dbtx_execute!(&mut tx, &replace_sql, &token.token_id)?;
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

#[cfg(test)]
mod sliding_expiry_tests;
#[cfg(test)]
mod unexpired_tests;
