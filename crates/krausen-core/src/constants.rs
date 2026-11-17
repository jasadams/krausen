// SPDX-License-Identifier: AGPL-3.0-or-later

//! Shared constants loaded from `shared/constants.toml`.
//!
//! Single source of truth for values the server and its crates share. NEVER
//! hardcode values that belong here.

use serde::Deserialize;
use std::path::Path;
use std::sync::OnceLock;

/// Embedded fallback for standalone binary mode (no filesystem access needed).
/// Path: from crates/krausen-core/src/ → repo root/shared/constants.toml
const EMBEDDED_CONSTANTS: &str = include_str!("../../../shared/constants.toml");

/// Global singleton — loaded once at startup.
static CONSTANTS: OnceLock<SharedConstants> = OnceLock::new();

/// Load constants from the shared TOML file.
///
/// Searches for the file at the given path, or falls back to common
/// locations relative to the workspace root.
pub fn load(path: &Path) -> crate::Result<&'static SharedConstants> {
    let mut parsed = parse_file(path)?;
    apply_runtime_overrides(&mut parsed);
    CONSTANTS
        .set(parsed)
        .map_err(|_| crate::Error::Internal("constants already loaded".into()))?;
    CONSTANTS
        .get()
        .ok_or_else(|| crate::Error::Internal("constants not initialized".into()))
}

/// Load constants from disk, falling back to the embedded copy for standalone mode.
///
/// Tries [`find_constants_file`] first. If the file is not found on disk,
/// parses the compile-time embedded copy of `shared/constants.toml`.
pub fn load_with_fallback() -> crate::Result<&'static SharedConstants> {
    CONSTANTS
        .set(build_with_fallback()?)
        .map_err(|_| crate::Error::Internal("constants already loaded".into()))?;
    CONSTANTS
        .get()
        .ok_or_else(|| crate::Error::Internal("constants not initialized".into()))
}

fn build_with_fallback() -> crate::Result<SharedConstants> {
    let content = match find_constants_file() {
        Ok(path) => {
            tracing::info!("Loaded shared constants from {}", path.display());
            std::fs::read_to_string(&path).map_err(|e| {
                crate::Error::Internal(format!(
                    "failed to read shared constants from {}: {e}",
                    path.display()
                ))
            })?
        }
        Err(_) => {
            tracing::info!("constants.toml not found on disk, using embedded fallback");
            EMBEDDED_CONSTANTS.to_string()
        }
    };

    let mut parsed: SharedConstants = toml::from_str(&content)
        .map_err(|e| crate::Error::Internal(format!("failed to parse shared constants: {e}")))?;

    apply_runtime_overrides(&mut parsed);
    Ok(parsed)
}

/// Get the constants, loading them (disk first, then the embedded copy) on first
/// use. Krausen's crates and tests call this without a prior startup `load`, so
/// unlike Kyomi it initialises itself instead of panicking; a malformed embedded
/// file is a build defect and panics.
pub fn get() -> &'static SharedConstants {
    CONSTANTS.get_or_init(|| build_with_fallback().expect("shared constants must load"))
}

/// Apply runtime overrides to parsed constants based on environment.
///
/// `shared/constants.toml` is the default configuration.
/// At runtime we adjust settings that depend on the deployment context:
///
/// - **Cookie `Secure` flag**: Inferred from `FRONTEND_URL` scheme. When the
///   frontend is served over plain HTTP (standalone binary, local dev), the
///   browser will reject `Secure` cookies. We detect this and disable the flag.
///   Users who put a TLS reverse proxy in front and set `FRONTEND_URL=https://…`
///   get `Secure` cookies automatically.
///
/// - **CORS origins**: Standalone/self-hosted mode adds `FRONTEND_URL` to the
///   allowed origins list so the app works on any host/port combination.
///
/// - **HSTS**: Disabled when serving over HTTP (no point advertising HSTS
///   on a plain HTTP connection, and it can confuse browsers).
fn apply_runtime_overrides(constants: &mut SharedConstants) {
    let frontend_url = std::env::var("FRONTEND_URL").unwrap_or_default();
    let is_https = frontend_url.starts_with("https://");

    // Cookie Secure flag: match the frontend protocol
    if !is_https && constants.cookies.secure {
        tracing::info!(
            "FRONTEND_URL is not HTTPS — disabling Secure flag on cookies"
        );
        constants.cookies.secure = false;
    }

    // HSTS: only meaningful over HTTPS
    if !is_https {
        constants.security_headers.hsts = String::new();
    }

    // CORS: ensure FRONTEND_URL is in allowed origins for self-hosted deployments
    if !frontend_url.is_empty()
        && !constants.cors.allowed_origins.contains(&frontend_url)
    {
        constants.cors.allowed_origins.push(frontend_url);
    }

    // Version: prefer build-time tag injected by CI over the TOML default
    if let Some(v) = option_env!("KRAUSEN_VERSION") {
        constants.api.version = v.to_string();
    }
}

fn parse_file(path: &Path) -> crate::Result<SharedConstants> {
    let content = std::fs::read_to_string(path).map_err(|e| {
        crate::Error::Internal(format!(
            "failed to read shared constants from {}: {e}",
            path.display()
        ))
    })?;

    toml::from_str(&content).map_err(|e| {
        crate::Error::Internal(format!("failed to parse shared constants: {e}"))
    })
}

/// Find the constants file.
///
/// `shared/constants.toml` lives at a fixed location relative to this crate's
/// manifest (`crates/krausen-core/`), so it is located by anchoring to
/// `CARGO_MANIFEST_DIR` — a compile-time constant — rather than guessing at
/// the process's current working directory, which varies by invocation
/// (workspace root for `cargo run`, the package directory for `cargo test`,
/// an arbitrary directory for a packaged binary).
///
/// Searches in this order:
/// 1. `$SHARED_CONSTANTS_PATH` env var — explicit override, e.g. for
///    deployments that ship the file at a different path.
/// 2. `<CARGO_MANIFEST_DIR>/../../shared/constants.toml` — repo-relative path
///    from `crates/krausen-core/`, stable regardless of working directory.
/// 3. `shared/constants.toml` — relative to the current working directory,
///    for invocations with a known cwd (e.g. `cargo run` from repo root).
///
/// Returns the first path that exists, or an error if none are found. Note
/// that most callers don't need this at all — [`load_with_fallback`] embeds
/// the file at compile time and only calls this to prefer the on-disk copy
/// when present.
pub fn find_constants_file() -> crate::Result<std::path::PathBuf> {
    let candidates = [
        std::env::var("SHARED_CONSTANTS_PATH")
            .ok()
            .map(std::path::PathBuf::from),
        Some(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../shared/constants.toml")),
        Some(std::path::PathBuf::from("shared/constants.toml")),
    ];

    for candidate in candidates.into_iter().flatten() {
        if candidate.exists() {
            return Ok(candidate);
        }
    }

    Err(crate::Error::Internal(
        "shared/constants.toml not found — set SHARED_CONSTANTS_PATH env var".into(),
    ))
}

// ---------------------------------------------------------------------------
// Typed structs matching the TOML structure
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct SharedConstants {
    pub api: ApiConstants,
    pub jwt: JwtConstants,
    pub cookies: CookieConstants,
    pub cors: CorsConstants,
    pub security_headers: SecurityHeaderConstants,
    pub redis: RedisConstants,
    pub rate_limits: RateLimitConstants,
    pub workspace: WorkspaceConstants,
}

/// API-level constants (version, etc.).
#[derive(Debug, Clone, Deserialize)]
pub struct ApiConstants {
    pub version: String,
}

/// JWT configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct JwtConstants {
    pub algorithm: String,
    pub access_token_expire_minutes: i64,
    pub refresh_token_expire_days: i64,
    pub password_reset_expire_minutes: i64,
    pub email_verification_expire_hours: i64,
    pub refresh_token_prefix: String,
    pub refresh_token_min_length: usize,
    pub refresh_token_grace_period_seconds: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CookieConstants {
    pub access_token_name: String,
    pub refresh_token_name: String,
    pub httponly: bool,
    pub secure: bool,
    pub samesite: String,
    pub path: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CorsConstants {
    pub allowed_origins: Vec<String>,
    pub allowed_methods: Vec<String>,
    pub allow_credentials: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SecurityHeaderConstants {
    pub x_frame_options: String,
    pub x_content_type_options: String,
    pub x_xss_protection: String,
    pub hsts: String,
    pub content_security_policy: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RedisConstants {
    pub key_prefixes: RedisKeyPrefixes,
    pub ttls: RedisTtls,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RedisKeyPrefixes {
    pub websocket_user: String,
    pub oauth_state: String,
    pub pending_signup: String,
    pub pending_terms: String,
    pub totp_setup: String,
    pub webauthn_challenge: String,
    pub recovery_session: String,
    pub rate_limit_ip: String,
    pub rate_limit_user: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RedisTtls {
    pub oauth_state: u64,
    pub pending_signup: u64,
    pub totp_setup: u64,
    pub webauthn_challenge: u64,
    pub recovery_session: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RateLimitConstants {
    pub login: RateLimitConfig,
    pub register: RateLimitConfig,
    pub refresh: RateLimitConfig,
    pub api_call: RateLimitConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RateLimitConfig {
    pub ip_capacity: u32,
    pub ip_refill_interval: u64,
    pub user_capacity: u32,
    pub user_refill_interval: u64,
    pub window_seconds: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WorkspaceConstants {
    pub roles: WorkspaceRoleConstants,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WorkspaceRoleConstants {
    pub admin: String,
    pub user: String,
}

/// Map a stored DB workspace-role token (e.g. `workspace_admin`) to a
/// human-readable display label (e.g. `"Admin"`).
///
/// Any value that doesn't match a known role constant falls back to
/// `"Member"`, mirroring the generic non-admin wording the invitation
/// email uses (note: `krausen-auth::email_service` matches on a separate
/// UI-level role string, so it shares the outcome, not this mechanism).
/// This fallback is also what absorbs any stray legacy `workspace_viewer`
/// value left over from before KYO-183 removed that role.
pub fn humanize_workspace_role(role: &str) -> &'static str {
    let roles = &get().workspace.roles;
    if role == roles.admin {
        "Admin"
    } else {
        "Member"
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_constants_file() {
        let path = match find_constants_file() {
            Ok(p) => p,
            Err(_) => {
                eprintln!("skipping constants test — shared/constants.toml not found");
                return;
            }
        };
        let constants = parse_file(&path).expect("should parse");

        // Spot-check values
        assert_eq!(constants.jwt.algorithm, "HS256");
        assert_eq!(constants.jwt.access_token_expire_minutes, 15);
        assert_eq!(constants.jwt.refresh_token_expire_days, 7);
        assert_eq!(constants.cookies.access_token_name, "access_token");
        assert_eq!(constants.cookies.samesite, "strict");
        // `parse_file` returns the raw static TOML content — the deployment's
        // own FRONTEND_URL is appended at runtime by `apply_runtime_overrides`,
        // which this test does not call. `shared/constants.toml`'s `[cors]`
        // section has no entries.
        assert!(constants.cors.allowed_origins.is_empty());
        assert!(constants.cors.allow_credentials);
        assert_eq!(constants.security_headers.x_frame_options, "DENY");
        assert_eq!(
            constants.security_headers.content_security_policy,
            "default-src 'none'; frame-ancestors 'none'"
        );
        assert_eq!(constants.redis.ttls.oauth_state, 300);
        assert_eq!(constants.rate_limits.login.ip_capacity, 10);
        assert_eq!(constants.workspace.roles.admin, "workspace_admin");
        assert_eq!(constants.workspace.roles.user, "workspace_user");
    }

    #[test]
    fn auth_constants_match_the_shared_file() {
        let constants = parse_file(&find_constants_file().expect("shared/constants.toml exists"))
            .expect("should parse");
        // SameSite is always strict; only `Secure` follows FRONTEND_URL.
        assert_eq!(constants.cookies.samesite, "strict");
        let rl = &constants.rate_limits;
        assert_eq!((rl.login.ip_capacity, rl.login.user_capacity, rl.login.window_seconds), (10, 5, 900));
        assert_eq!((rl.register.ip_capacity, rl.register.user_capacity, rl.register.window_seconds), (5, 1, 3600));
        assert_eq!((rl.refresh.ip_capacity, rl.refresh.user_capacity, rl.refresh.window_seconds), (30, 20, 300));
        assert_eq!((rl.api_call.ip_capacity, rl.api_call.user_capacity, rl.api_call.window_seconds), (1000, 500, 60));
        let prefixes = &constants.redis.key_prefixes;
        assert_eq!(prefixes.rate_limit_ip, "ratelimit:ip:{ip}:{endpoint}");
        assert_eq!(prefixes.rate_limit_user, "ratelimit:user:{user_id}:{endpoint}");
        assert_eq!(prefixes.pending_signup, "auth:pending_signup:{token}");
        assert_eq!(prefixes.pending_terms, "auth:pending_terms:{token}");
        assert_eq!(prefixes.webauthn_challenge, "auth:webauthn_challenge:{challenge_id}");
        assert_eq!(prefixes.totp_setup, "auth:totp_setup:{user_id}");
        assert_eq!(prefixes.recovery_session, "auth:recovery_session:{session_id}");
        assert_eq!(prefixes.websocket_user, "ws:user:{user_id}");
        assert_eq!(constants.jwt.password_reset_expire_minutes, 15);
        assert_eq!(constants.security_headers.hsts, "max-age=31536000; includeSubDomains");
    }

    #[test]
    fn embedded_copy_equals_the_file_on_disk() {
        // The binary falls back to the embedded copy; it must be the shipped file.
        let disk = std::fs::read_to_string(find_constants_file().unwrap()).unwrap();
        assert_eq!(EMBEDDED_CONSTANTS, disk);
    }

    #[test]
    fn humanize_workspace_role_maps_known_roles() {
        // Ensure the global singleton is populated (embedded fallback always
        // succeeds, so this is safe regardless of `find_constants_file()`
        // resolving in this test's working directory or test execution order).
        let _ = load_with_fallback();
        let roles = &get().workspace.roles;

        assert_eq!(humanize_workspace_role(&roles.admin), "Admin");
        assert_eq!(humanize_workspace_role(&roles.user), "Member");
    }

    #[test]
    fn humanize_workspace_role_falls_back_to_member_for_unknown_value() {
        let _ = load_with_fallback();

        // Also covers the KYO-183 removed `workspace_viewer` role: any
        // legacy row still holding that string must display as "Member",
        // not panic or show a stale "Viewer" label.
        assert_eq!(humanize_workspace_role("some_unrecognized_role"), "Member");
        assert_eq!(humanize_workspace_role("workspace_viewer"), "Member");
    }
}
