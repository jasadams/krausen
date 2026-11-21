// SPDX-License-Identifier: AGPL-3.0-or-later

//! krausen-auth — Authentication & encryption for the Krausen backend.

pub mod auth_service;
pub mod cookies;
pub mod email_service;
pub mod encryption;
pub mod google_oauth;
pub mod jwt;
pub mod mcp_session_manager;
pub mod middleware;
pub mod notifications;
pub mod onboarding_service;
pub mod password;
pub mod rate_limiter;
pub mod redis_ops;
pub mod request_meta;
pub mod security_service;
pub mod session;
pub mod sync_log_service;
pub mod token_refresh;
pub mod token_service;
pub mod totp;
pub mod user_service;
pub mod webauthn;
pub mod webauthn_challenge_purpose;
pub mod websocket;
pub mod workspace_service;

/// Select Ring as the fallback TLS provider, preserving an embedding application's choice.
///
/// Call before constructing clients that use Rustls's implicit provider builder.
/// Rustls 0.23.45 `CryptoProvider::install_default` installs atomically and cannot
/// replace an existing provider; a concurrent install losing the race is harmless.
/// Ring matches our existing Reqwest and SQLx backend selection.
pub fn initialize_tls_provider() {
    if rustls::crypto::CryptoProvider::get_default().is_none() {
        let _ = rustls::crypto::ring::default_provider().install_default();
    }
}

/// Build a shared HTTP client with a proper User-Agent header.
pub fn http_client() -> krausen_core::Result<reqwest::Client> {
    initialize_tls_provider();
    reqwest::Client::builder()
        .user_agent("Krausen/1.0")
        .build()
        .map_err(|e| krausen_core::Error::Internal(format!("Failed to build HTTP client: {e}")))
}

