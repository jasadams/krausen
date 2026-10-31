// SPDX-License-Identifier: AGPL-3.0-or-later

//! Google OAuth service — authorization URL construction, token exchange, user info.
//!
//! Uses direct HTTP calls (reqwest) instead of a Google SDK.

use serde::{Deserialize, Serialize};

/// Standard OAuth token response (normalized across providers).
#[derive(Debug, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_in: Option<i64>,
    pub scope: Option<String>,
    pub token_type: Option<String>,
}

// ---------------------------------------------------------------------------
// Google API endpoints
// ---------------------------------------------------------------------------

pub const GOOGLE_AUTH_URI: &str = "https://accounts.google.com/o/oauth2/auth";
pub const GOOGLE_TOKEN_URI: &str = "https://oauth2.googleapis.com/token";
pub const GOOGLE_USER_INFO_URI: &str = "https://www.googleapis.com/oauth2/v2/userinfo";

// ---------------------------------------------------------------------------
// Scopes
// ---------------------------------------------------------------------------

/// Minimal scopes for login (identify the user).
pub const LOGIN_SCOPES: &[&str] = &[
    "https://www.googleapis.com/auth/userinfo.email",
    "https://www.googleapis.com/auth/userinfo.profile",
];

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Google userinfo response.
#[derive(Debug, Deserialize)]
pub struct GoogleUserInfo {
    pub id: String,
    pub email: String,
    pub name: Option<String>,
    pub picture: Option<String>,
    pub verified_email: Option<bool>,
}

/// Structured OAuth data stored encrypted in `users.oauth_data`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OAuthData {
    pub google_id: Option<String>,
    pub oauth_provider: Option<String>,
    pub picture: Option<String>,
    pub last_oauth_login: Option<String>,
}

// ---------------------------------------------------------------------------
// Authorization URL
// ---------------------------------------------------------------------------

/// Build a Google OAuth authorization URL.
pub fn build_authorization_url(
    client_id: &str,
    redirect_uri: &str,
    state: &str,
    scopes: &[&str],
    force_consent: bool,
    offline_access: bool,
) -> String {
    let scope = scopes.join(" ");

    let mut params = url::form_urlencoded::Serializer::new(String::new());
    params
        .append_pair("client_id", client_id)
        .append_pair("redirect_uri", redirect_uri)
        .append_pair("response_type", "code")
        .append_pair("scope", &scope)
        .append_pair("state", state);

    if offline_access {
        params.append_pair("access_type", "offline");
    }

    if force_consent {
        params.append_pair("prompt", "consent");
    }

    format!("{GOOGLE_AUTH_URI}?{}", params.finish())
}

// ---------------------------------------------------------------------------
// Token exchange
// ---------------------------------------------------------------------------

/// Exchange an authorization code for tokens.
pub async fn exchange_code_for_tokens(
    client_id: &str,
    client_secret: &str,
    code: &str,
    redirect_uri: &str,
) -> krausen_core::Result<TokenResponse> {
    let client = crate::http_client()?;

    let resp = client
        .post(GOOGLE_TOKEN_URI)
        .form(&[
            ("client_id", client_id),
            ("client_secret", client_secret),
            ("code", code),
            ("grant_type", "authorization_code"),
            ("redirect_uri", redirect_uri),
        ])
        .send()
        .await
        .map_err(|e| krausen_core::Error::Internal(format!("Google token exchange failed: {e}")))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(krausen_core::Error::BadRequest(format!(
            "Google token exchange failed ({status}): {body}"
        )));
    }

    resp.json::<TokenResponse>()
        .await
        .map_err(|e| krausen_core::Error::Internal(format!("Failed to parse token response: {e}")))
}

// ---------------------------------------------------------------------------
// User info
// ---------------------------------------------------------------------------

/// Fetch user info from Google using an access token.
pub async fn get_user_info(access_token: &str) -> krausen_core::Result<GoogleUserInfo> {
    let client = crate::http_client()?;

    let resp = client
        .get(GOOGLE_USER_INFO_URI)
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|e| krausen_core::Error::Internal(format!("Google userinfo request failed: {e}")))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(krausen_core::Error::BadRequest(format!(
            "Google userinfo request failed ({status}): {body}"
        )));
    }

    resp.json::<GoogleUserInfo>()
        .await
        .map_err(|e| krausen_core::Error::Internal(format!("Failed to parse userinfo: {e}")))
}

// ---------------------------------------------------------------------------
// OAuth data helpers
// ---------------------------------------------------------------------------

/// Decrypt and parse `users.oauth_data` from the database.
pub fn parse_oauth_data(
    encrypted: Option<&str>,
    key: &[u8; 32],
) -> krausen_core::Result<Option<OAuthData>> {
    let Some(encrypted) = encrypted else {
        return Ok(None);
    };

    if encrypted.is_empty() {
        return Ok(None);
    }

    let json_str = crate::encryption::decrypt(encrypted, key)?;
    let data: OAuthData = serde_json::from_str(&json_str)?;
    Ok(Some(data))
}

/// Serialize and encrypt `OAuthData` for storage in `users.oauth_data`.
pub fn build_oauth_data(
    data: &OAuthData,
    key: &[u8; 32],
) -> krausen_core::Result<String> {
    let json_str = serde_json::to_string(data)?;
    crate::encryption::encrypt(&json_str, key)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params_of(url: &str) -> std::collections::HashMap<String, String> {
        let query = url.split_once('?').expect("url has a query string").1;
        url::form_urlencoded::parse(query.as_bytes())
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect()
    }

    #[test]
    fn authorization_url_targets_google_with_login_scopes() {
        let url = build_authorization_url(
            "client-123",
            "http://localhost:8003/auth/google/callback",
            "csrf-state-abc",
            LOGIN_SCOPES,
            false,
            false,
        );

        assert!(url.starts_with(GOOGLE_AUTH_URI));
        let p = params_of(&url);
        assert_eq!(p["client_id"], "client-123");
        assert_eq!(p["redirect_uri"], "http://localhost:8003/auth/google/callback");
        assert_eq!(p["state"], "csrf-state-abc");
        assert_eq!(p["response_type"], "code");
        assert_eq!(
            p["scope"],
            "https://www.googleapis.com/auth/userinfo.email \
             https://www.googleapis.com/auth/userinfo.profile"
        );
    }

    #[test]
    fn plain_login_asks_for_nothing_extra() {
        let url = build_authorization_url("c", "http://x/cb", "s", LOGIN_SCOPES, false, false);
        let p = params_of(&url);
        // Re-requesting previously granted scopes would silently restore access
        // a user had revoked, so a plain sign-in must not ask for it.
        assert!(!p.contains_key("include_granted_scopes"));
        assert!(!p.contains_key("access_type"));
        assert!(!p.contains_key("prompt"));
    }

    #[test]
    fn offline_and_consent_flags_are_opt_in() {
        let url = build_authorization_url("c", "http://x/cb", "s", LOGIN_SCOPES, true, true);
        let p = params_of(&url);
        assert_eq!(p["access_type"], "offline");
        assert_eq!(p["prompt"], "consent");
    }

    #[test]
    fn oauth_data_round_trips_through_encryption() {
        let key = [7u8; 32];
        let data = OAuthData {
            google_id: Some("google-uid-1".into()),
            oauth_provider: Some("google".into()),
            picture: Some("https://example.test/p.png".into()),
            last_oauth_login: Some("2026-01-01T00:00:00Z".into()),
        };

        let encrypted = build_oauth_data(&data, &key).expect("encrypts");
        assert!(!encrypted.contains("google-uid-1"), "stored blob must not be plaintext");

        let decoded = parse_oauth_data(Some(&encrypted), &key)
            .expect("decrypts")
            .expect("some");
        assert_eq!(decoded.google_id.as_deref(), Some("google-uid-1"));
        assert_eq!(decoded.oauth_provider.as_deref(), Some("google"));
        assert_eq!(decoded.picture.as_deref(), Some("https://example.test/p.png"));
    }

    #[test]
    fn parse_oauth_data_treats_absent_and_empty_as_none() {
        let key = [7u8; 32];
        assert!(parse_oauth_data(None, &key).expect("ok").is_none());
        assert!(parse_oauth_data(Some(""), &key).expect("ok").is_none());
    }
}
