// SPDX-License-Identifier: AGPL-3.0-or-later
//! Installation-local OAuth grant storage. Secrets are opaque and stored hashed.
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::Utc;
use krausen_core::{DbPool, DbTx};
use rand::Rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

fn secret() -> String {
    let bytes: [u8; 32] = rand::rng().random();
    URL_SAFE_NO_PAD.encode(bytes)
}
fn hash(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}
fn invalid() -> krausen_core::Error {
    krausen_core::Error::Unauthorized("Invalid or expired authorization grant".into())
}
#[derive(sqlx::FromRow)]
struct IdRow {
    id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AuthorizationRequest {
    pub client_id: String,
    pub redirect_uri: String,
    pub resource: String,
    pub state: String,
    pub code_challenge: String,
    pub code_challenge_method: String,
    pub response_type: String,
}
#[derive(sqlx::FromRow)]
struct Client {
    redirect_uris: String,
    client_name: String,
}
#[derive(Debug, sqlx::FromRow)]
struct Pending {
    client_id: String,
    redirect_uri: String,
    resource: String,
    state: String,
    code_challenge: String,
    user_id: Option<String>,
    credential_id: Option<String>,
    expires_at: i64,
    consumed_at: Option<i64>,
}
#[derive(sqlx::FromRow)]
struct Code {
    client_id: String,
    user_id: String,
    initiating_credential_id: String,
    redirect_uri: String,
    resource: String,
    code_challenge: String,
    expires_at: i64,
    consumed_at: Option<i64>,
}
#[derive(sqlx::FromRow)]
struct Refresh {
    credential_id: String,
    client_id: String,
    expires_at: i64,
    consumed_at: Option<i64>,
}
#[derive(sqlx::FromRow)]
struct Credential {
    user_id: String,
    audience: String,
    issuer: String,
    expires_at: i64,
    revoked_at: Option<i64>,
}
#[derive(Debug, Serialize)]
pub struct RegisteredClient {
    pub client_id: String,
    pub client_name: String,
    pub redirect_uris: Vec<String>,
    pub token_endpoint_auth_method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_secret: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_secret_expires_at: Option<i64>,
}
#[derive(Debug, Serialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub token_type: &'static str,
    pub expires_in: i64,
    pub refresh_token: String,
}
#[derive(Debug, Clone)]
pub struct Principal {
    pub user_id: String,
    pub credential_id: String,
    pub access_expires_at: i64,
}

pub async fn register_client(
    pool: &DbPool,
    name: &str,
    redirects: &[String],
) -> krausen_core::Result<RegisteredClient> {
    register_client_with_method(pool, name, redirects, "none").await
}

pub async fn register_client_with_method(
    pool: &DbPool,
    name: &str,
    redirects: &[String],
    method: &str,
) -> krausen_core::Result<RegisteredClient> {
    if !matches!(method, "none" | "client_secret_basic") {
        return Err(krausen_core::Error::BadRequest(
            "Unsupported client authentication".into(),
        ));
    }
    if name.trim().is_empty() || name.len() > 128 || redirects.is_empty() || redirects.len() > 16 {
        return Err(krausen_core::Error::BadRequest(
            "Invalid client metadata".into(),
        ));
    }
    for redirect in redirects {
        let u = url::Url::parse(redirect)
            .map_err(|_| krausen_core::Error::BadRequest("Invalid redirect URI".into()))?;
        let loopback = matches!(u.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"));
        if u.fragment().is_some()
            || !u.username().is_empty()
            || u.password().is_some()
            || !(u.scheme() == "https" || (u.scheme() == "http" && loopback))
        {
            return Err(krausen_core::Error::BadRequest(
                "Invalid redirect URI".into(),
            ));
        }
    }
    let id = format!("client_{}", secret());
    let redirects_json = serde_json::to_string(redirects)?;
    let client_secret = (method == "client_secret_basic").then(secret);
    let secret_hash = client_secret.as_deref().map(hash);
    krausen_core::db_execute!(
        pool,
        "INSERT INTO mcp_oauth_clients (client_id,client_name,redirect_uris,auth_method,secret_hash,created_at) VALUES ($1,$2,$3,$4,$5,$6)",
        &id,
        name.trim(),
        &redirects_json,
        method,
        &secret_hash,
        Utc::now().timestamp()
    )?;
    Ok(RegisteredClient {
        client_id: id,
        client_name: name.trim().into(),
        redirect_uris: redirects.to_vec(),
        token_endpoint_auth_method: method.into(),
        client_secret_expires_at: client_secret.as_ref().map(|_| 0),
        client_secret,
    })
}

pub async fn begin_authorization(
    pool: &DbPool,
    request: &AuthorizationRequest,
    expected_resource: &str,
) -> krausen_core::Result<String> {
    if request.response_type != "code"
        || request.code_challenge_method != "S256"
        || request.resource != expected_resource
        || request.state.is_empty()
        || request.state.len() > 512
        || request.code_challenge.len() != 43
        || !request
            .code_challenge
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(krausen_core::Error::BadRequest(
            "Invalid authorization request".into(),
        ));
    }
    let client = krausen_core::db_fetch_optional!(
        pool,
        Client,
        "SELECT redirect_uris,client_name FROM mcp_oauth_clients WHERE client_id = $1",
        &request.client_id
    )?
    .ok_or_else(invalid)?;
    let redirects: Vec<String> = serde_json::from_str(&client.redirect_uris)?;
    if !redirects.contains(&request.redirect_uri) {
        return Err(invalid());
    }
    let raw = secret();
    krausen_core::db_execute!(
        pool,
        "INSERT INTO mcp_oauth_pending (pending_hash,client_id,redirect_uri,resource,state,code_challenge,expires_at) VALUES ($1,$2,$3,$4,$5,$6,$7)",
        hash(&raw),
        &request.client_id,
        &request.redirect_uri,
        &request.resource,
        &request.state,
        &request.code_challenge,
        Utc::now().timestamp() + 600
    )?;
    Ok(raw)
}

async fn lock_principal(
    tx: &mut DbTx<'_>,
    pg: bool,
    user_id: &str,
    credential_id: &str,
    exclusive: bool,
) -> krausen_core::Result<()> {
    let account = if pg {
        "SELECT user_id AS id FROM users WHERE user_id = $1 AND active = TRUE FOR SHARE"
    } else {
        "SELECT user_id AS id FROM users WHERE user_id = $1 AND active = 1"
    };
    if krausen_core::dbtx_fetch_optional!(tx, IdRow, account, user_id)?.is_none() {
        return Err(invalid());
    }
    let credential = if pg && exclusive {
        "SELECT credential_id AS id FROM auth_credentials WHERE credential_id = $1 AND user_id = $2 AND revoked_at IS NULL AND expires_at > $3 FOR UPDATE"
    } else if pg {
        "SELECT credential_id AS id FROM auth_credentials WHERE credential_id = $1 AND user_id = $2 AND revoked_at IS NULL AND expires_at > $3 FOR SHARE"
    } else {
        "SELECT credential_id AS id FROM auth_credentials WHERE credential_id = $1 AND user_id = $2 AND revoked_at IS NULL AND expires_at > $3"
    };
    if krausen_core::dbtx_fetch_optional!(
        tx,
        IdRow,
        credential,
        credential_id,
        user_id,
        Utc::now().timestamp()
    )?
    .is_none()
    {
        return Err(invalid());
    }
    Ok(())
}

pub async fn bind_pending(
    pool: &DbPool,
    pending: &str,
    principal: &Principal,
) -> krausen_core::Result<String> {
    let mut tx = pool.begin_write().await?;
    lock_principal(
        &mut tx,
        pool.is_postgres(),
        &principal.user_id,
        &principal.credential_id,
        false,
    )
    .await?;
    if principal.access_expires_at <= Utc::now().timestamp() {
        return Err(invalid());
    }
    let count = krausen_core::dbtx_execute!(
        &mut tx,
        "UPDATE mcp_oauth_pending SET user_id = $1, credential_id = $2 WHERE pending_hash = $3 AND expires_at > $4 AND consumed_at IS NULL AND (user_id IS NULL OR (user_id = $1 AND credential_id = $2))",
        &principal.user_id,
        &principal.credential_id,
        hash(pending),
        Utc::now().timestamp()
    )?;
    if count.rows_affected() != 1 {
        return Err(invalid());
    }
    let client = krausen_core::dbtx_fetch_optional!(&mut tx, Client, "SELECT c.redirect_uris,c.client_name FROM mcp_oauth_clients c JOIN mcp_oauth_pending p ON c.client_id = p.client_id WHERE p.pending_hash = $1", hash(pending))?.ok_or_else(invalid)?;
    tx.commit().await?;
    Ok(client.client_name)
}

pub async fn approve(
    pool: &DbPool,
    pending: &str,
    principal: &Principal,
) -> krausen_core::Result<(String, String, String)> {
    let mut tx = pool.begin_write().await?;
    let pg = pool.is_postgres();
    lock_principal(
        &mut tx,
        pg,
        &principal.user_id,
        &principal.credential_id,
        false,
    )
    .await?;
    if principal.access_expires_at <= Utc::now().timestamp() {
        return Err(invalid());
    }
    let sql = if pg {
        "SELECT client_id,redirect_uri,resource,state,code_challenge,user_id,credential_id,expires_at,consumed_at FROM mcp_oauth_pending WHERE pending_hash = $1 FOR UPDATE"
    } else {
        "SELECT client_id,redirect_uri,resource,state,code_challenge,user_id,credential_id,expires_at,consumed_at FROM mcp_oauth_pending WHERE pending_hash = $1"
    };
    let record = krausen_core::dbtx_fetch_optional!(&mut tx, Pending, sql, hash(pending))?
        .ok_or_else(invalid)?;
    if record.user_id.as_deref() != Some(&principal.user_id)
        || record.credential_id.as_deref() != Some(&principal.credential_id)
        || record.expires_at <= Utc::now().timestamp()
        || record.consumed_at.is_some()
    {
        return Err(invalid());
    }
    let now = Utc::now().timestamp();
    let changed = krausen_core::dbtx_execute!(
        &mut tx,
        "UPDATE mcp_oauth_pending SET consumed_at = $1 WHERE pending_hash = $2 AND consumed_at IS NULL",
        now,
        hash(pending)
    )?;
    if changed.rows_affected() != 1 {
        return Err(invalid());
    }
    let code = secret();
    krausen_core::dbtx_execute!(
        &mut tx,
        "INSERT INTO mcp_oauth_authorization_codes (code_hash,client_id,user_id,initiating_credential_id,redirect_uri,resource,code_challenge,expires_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",
        hash(&code),
        &record.client_id,
        &principal.user_id,
        &principal.credential_id,
        &record.redirect_uri,
        &record.resource,
        &record.code_challenge,
        now + 300
    )?;
    tx.commit().await?;
    Ok((record.redirect_uri, record.state, code))
}

/// Authenticate the registered client; secrets are never stored in plaintext.
pub async fn authenticate_client(
    pool: &DbPool,
    client_id: &str,
    client_secret: Option<&str>,
) -> krausen_core::Result<()> {
    use subtle::ConstantTimeEq;
    #[derive(sqlx::FromRow)]
    struct Auth {
        auth_method: String,
        secret_hash: Option<String>,
    }
    let client = krausen_core::db_fetch_optional!(
        pool,
        Auth,
        "SELECT auth_method,secret_hash FROM mcp_oauth_clients WHERE client_id = $1",
        client_id
    )?
    .ok_or_else(invalid)?;
    match (
        client.auth_method.as_str(),
        client.secret_hash.as_deref(),
        client_secret,
    ) {
        ("none", None, None) => Ok(()),
        ("client_secret_basic", Some(expected), Some(secret))
            if bool::from(expected.as_bytes().ct_eq(hash(secret).as_bytes())) =>
        {
            Ok(())
        }
        _ => Err(invalid()),
    }
}

pub async fn exchange_code(
    pool: &DbPool,
    issuer: &str,
    client_id: &str,
    client_secret: Option<&str>,
    redirect_uri: &str,
    resource: &str,
    code: &str,
    verifier: &str,
) -> krausen_core::Result<TokenResponse> {
    authenticate_client(pool, client_id, client_secret).await?;
    if !(43..=128).contains(&verifier.len())
        || !verifier
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~'))
    {
        return Err(invalid());
    }
    let mut tx = pool.begin_write().await?;
    let pg = pool.is_postgres();
    let query = "SELECT client_id,user_id,initiating_credential_id,redirect_uri,resource,code_challenge,expires_at,consumed_at FROM mcp_oauth_authorization_codes WHERE code_hash = $1";
    let source = krausen_core::dbtx_fetch_optional!(&mut tx, Code, query, hash(code))?
        .ok_or_else(invalid)?;
    lock_principal(
        &mut tx,
        pg,
        &source.user_id,
        &source.initiating_credential_id,
        false,
    )
    .await?;
    let locked_query = if pg {
        "SELECT client_id,user_id,initiating_credential_id,redirect_uri,resource,code_challenge,expires_at,consumed_at FROM mcp_oauth_authorization_codes WHERE code_hash = $1 FOR UPDATE"
    } else {
        query
    };
    let source = krausen_core::dbtx_fetch_optional!(&mut tx, Code, locked_query, hash(code))?
        .ok_or_else(invalid)?;
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let now = Utc::now().timestamp();
    if source.client_id != client_id
        || source.redirect_uri != redirect_uri
        || source.resource != resource
        || source.code_challenge != challenge
        || source.expires_at <= now
        || source.consumed_at.is_some()
    {
        return Err(invalid());
    }
    let updated = krausen_core::dbtx_execute!(
        &mut tx,
        "UPDATE mcp_oauth_authorization_codes SET consumed_at = $1 WHERE code_hash = $2 AND consumed_at IS NULL",
        now,
        hash(code)
    )?;
    if updated.rows_affected() != 1 {
        return Err(invalid());
    }
    let credential_id = format!("mcp_{}", secret());
    let access = secret();
    let refresh = secret();
    let end = now + 30 * 86400;
    krausen_core::dbtx_execute!(
        &mut tx,
        "INSERT INTO auth_credentials (credential_id,user_id,kind,issuer,audience,expires_at) VALUES ($1,$2,'mcp',$3,$4,$5)",
        &credential_id,
        &source.user_id,
        issuer,
        resource,
        end
    )?;
    krausen_core::dbtx_execute!(
        &mut tx,
        "INSERT INTO mcp_oauth_access_tokens (token_hash,credential_id,resource,expires_at) VALUES ($1,$2,$3,$4)",
        hash(&access),
        &credential_id,
        resource,
        now + 3600
    )?;
    krausen_core::dbtx_execute!(
        &mut tx,
        "INSERT INTO mcp_oauth_refresh_tokens (token_hash,credential_id,client_id,expires_at) VALUES ($1,$2,$3,$4)",
        hash(&refresh),
        &credential_id,
        client_id,
        end
    )?;
    tx.commit().await?;
    Ok(TokenResponse {
        access_token: access,
        token_type: "Bearer",
        expires_in: 3600,
        refresh_token: refresh,
    })
}

pub async fn refresh(
    pool: &DbPool,
    issuer: &str,
    client_id: &str,
    client_secret: Option<&str>,
    resource: &str,
    raw: &str,
) -> krausen_core::Result<TokenResponse> {
    authenticate_client(pool, client_id, client_secret).await?;
    let mut tx = pool.begin_write().await?;
    let pg = pool.is_postgres();
    let token_query = "SELECT credential_id,client_id,expires_at,consumed_at FROM mcp_oauth_refresh_tokens WHERE token_hash = $1";
    let token = krausen_core::dbtx_fetch_optional!(&mut tx, Refresh, token_query, hash(raw))?
        .ok_or_else(invalid)?;
    let credential = krausen_core::dbtx_fetch_optional!(&mut tx, Credential, "SELECT user_id,audience,issuer,expires_at,revoked_at FROM auth_credentials WHERE credential_id = $1", &token.credential_id)?.ok_or_else(invalid)?;
    lock_principal(&mut tx, pg, &credential.user_id, &token.credential_id, true).await?;
    let lock_query = if pg {
        "SELECT credential_id,client_id,expires_at,consumed_at FROM mcp_oauth_refresh_tokens WHERE token_hash = $1 FOR UPDATE"
    } else {
        token_query
    };
    let token = krausen_core::dbtx_fetch_optional!(&mut tx, Refresh, lock_query, hash(raw))?
        .ok_or_else(invalid)?;
    let now = Utc::now().timestamp();
    if token.client_id != client_id
        || credential.audience != resource
        || credential.issuer != issuer
        || credential.expires_at <= now
        || credential.revoked_at.is_some()
        || token.expires_at <= now
    {
        return Err(invalid());
    }
    if token.consumed_at.is_some() {
        krausen_core::dbtx_execute!(
            &mut tx,
            "UPDATE auth_credentials SET revoked_at = $1 WHERE credential_id = $2 AND revoked_at IS NULL",
            now,
            &token.credential_id
        )?;
        tx.commit().await?;
        return Err(invalid());
    }
    let updated = krausen_core::dbtx_execute!(
        &mut tx,
        "UPDATE mcp_oauth_refresh_tokens SET consumed_at = $1 WHERE token_hash = $2 AND consumed_at IS NULL",
        now,
        hash(raw)
    )?;
    if updated.rows_affected() != 1 {
        return Err(invalid());
    }
    let access = secret();
    let new_refresh = secret();
    krausen_core::dbtx_execute!(
        &mut tx,
        "INSERT INTO mcp_oauth_access_tokens (token_hash,credential_id,resource,expires_at) VALUES ($1,$2,$3,$4)",
        hash(&access),
        &token.credential_id,
        resource,
        now + 3600
    )?;
    krausen_core::dbtx_execute!(
        &mut tx,
        "INSERT INTO mcp_oauth_refresh_tokens (token_hash,credential_id,client_id,expires_at) VALUES ($1,$2,$3,$4)",
        hash(&new_refresh),
        &token.credential_id,
        client_id,
        credential.expires_at
    )?;
    tx.commit().await?;
    Ok(TokenResponse {
        access_token: access,
        token_type: "Bearer",
        expires_in: 3600,
        refresh_token: new_refresh,
    })
}

pub async fn verify_access(
    pool: &DbPool,
    issuer: &str,
    resource: &str,
    raw: &str,
) -> krausen_core::Result<Principal> {
    #[derive(sqlx::FromRow)]
    struct Row {
        user_id: String,
        credential_id: String,
        access_expires_at: i64,
    }
    let now = Utc::now().timestamp();
    let active = krausen_core::sql_compat::bool_true(pool.is_postgres());
    let sql = format!(
        "SELECT c.user_id,c.credential_id,t.expires_at AS access_expires_at FROM mcp_oauth_access_tokens t JOIN auth_credentials c ON c.credential_id = t.credential_id JOIN users u ON u.user_id = c.user_id WHERE t.token_hash = $1 AND t.resource = $2 AND t.expires_at > $3 AND c.kind = 'mcp' AND c.issuer = $4 AND c.audience = $2 AND c.revoked_at IS NULL AND c.expires_at > $3 AND u.active = {active}"
    );
    let row = krausen_core::db_fetch_optional!(pool, Row, &sql, hash(raw), resource, now, issuer)?
        .ok_or_else(invalid)?;
    Ok(Principal {
        user_id: row.user_id,
        credential_id: row.credential_id,
        access_expires_at: row.access_expires_at,
    })
}

/// Revoke the connection associated with a refresh token. Unknown tokens are
/// idempotently ignored; possession plus the original client id is required.
pub async fn revoke(
    pool: &DbPool,
    client_id: &str,
    client_secret: Option<&str>,
    raw: &str,
) -> krausen_core::Result<()> {
    authenticate_client(pool, client_id, client_secret).await?;
    let mut tx = pool.begin_write().await?;
    let pg = pool.is_postgres();
    let token = krausen_core::dbtx_fetch_optional!(
        &mut tx,
        Refresh,
        "SELECT credential_id,client_id,expires_at,consumed_at FROM mcp_oauth_refresh_tokens WHERE token_hash = $1",
        hash(raw)
    )?;
    let Some(token) = token else { return Ok(()) };
    if token.client_id != client_id {
        return Ok(());
    }
    let credential = krausen_core::dbtx_fetch_optional!(
        &mut tx,
        Credential,
        "SELECT user_id,audience,issuer,expires_at,revoked_at FROM auth_credentials WHERE credential_id = $1",
        &token.credential_id
    )?;
    let Some(credential) = credential else {
        return Ok(());
    };
    if credential.revoked_at.is_some() {
        return Ok(());
    }
    lock_principal(&mut tx, pg, &credential.user_id, &token.credential_id, true).await?;
    krausen_core::dbtx_execute!(
        &mut tx,
        "UPDATE auth_credentials SET revoked_at = $1 WHERE credential_id = $2 AND revoked_at IS NULL",
        Utc::now().timestamp(),
        &token.credential_id
    )?;
    tx.commit().await?;
    Ok(())
}
