// SPDX-License-Identifier: AGPL-3.0-or-later

//! Application configuration loaded from environment variables.

use std::env;

/// Deployment mode for the Krausen backend.
///
/// Set via `KRAUSEN_MODE` env var. Falls back to `SELF_HOSTED` for backward compat.
/// - `saas`: Multi-tenant hosted service
/// - `self_hosted`: Team server with full auth (password, optional OAuth)
/// - `personal`: Single-user desktop app — zero auth, SQLite, localhost only
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KrausenMode {
    Saas,
    SelfHosted,
    Personal,
}

impl KrausenMode {
    /// Derive the legacy `self_hosted` bool from the mode.
    fn self_hosted(&self) -> bool {
        matches!(self, KrausenMode::SelfHosted | KrausenMode::Personal)
    }
}

/// The three SMTP environment variables, in the order an operator sets them.
///
/// One list, so [`SmtpSettings::classify`]'s presence check and every
/// diagnostic derived from it can never name a different set of variables.
static SMTP_ENV_VARS: [&str; 3] = ["SMTP_HOST", "SMTP_USER", "SMTP_PASSWORD"];

/// Completeness of the SMTP settings — the single definition of "can this
/// deployment send mail?".
///
/// The mailer (`krausen_auth::email_service::EmailService`) is the authoritative
/// reader: it is the thing that actually opens the SMTP connection, and it
/// authenticates with lettre's `Credentials::new(user, password)` before it can
/// submit a message. All three of host, user and password are therefore
/// required — a host/user pair with no password cannot deliver anything,
/// however much the rest of the app would like to believe otherwise.
///
/// [`Config::smtp_configured`] and `EmailService::is_configured()` both route
/// through this type, and neither may re-spell the conjunction. They had
/// drifted (Kyomi KYO-685): the config flag required two variables and the mailer
/// three, so an install with `SMTP_HOST` and `SMTP_USER` set but
/// `SMTP_PASSWORD` unset reported `smtp_configured = true` while the mailer
/// refused to send. Self-hosted signup took the SaaS "we emailed you a
/// verification link" branch instead of `signup_smtp_less_new_user` and no
/// account could be created at all — with nothing said at boot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SmtpSettings {
    /// Host, user and password are all present: mail can be sent.
    Complete,
    /// None of the three is set. A supported deployment, not an error — a
    /// self-hosted install with no mail server runs the SMTP-less signup and
    /// recovery paths.
    Absent,
    /// Some are set and some are not. Always operator error; `missing` names
    /// the variables still needed, in [`SMTP_ENV_VARS`] order.
    Incomplete { missing: Vec<&'static str> },
}

impl SmtpSettings {
    /// Classify the three SMTP parts by presence.
    ///
    /// Pure: takes the values rather than reading the environment, so both
    /// readers — and their tests — can share it without mutating process env.
    pub fn classify(host: Option<&str>, user: Option<&str>, password: Option<&str>) -> Self {
        let missing: Vec<&'static str> = SMTP_ENV_VARS
            .iter()
            .zip([host.is_some(), user.is_some(), password.is_some()])
            .filter_map(|(name, present)| (!present).then_some(*name))
            .collect();

        if missing.is_empty() {
            Self::Complete
        } else if missing.len() == SMTP_ENV_VARS.len() {
            Self::Absent
        } else {
            Self::Incomplete { missing }
        }
    }

    /// Whether mail can be sent — the predicate [`Config::smtp_configured`]
    /// and `EmailService::is_configured()` share.
    pub fn can_send(&self) -> bool {
        matches!(self, Self::Complete)
    }

    /// The environment variables that still need setting: empty when
    /// [`Complete`](Self::Complete), all three when [`Absent`](Self::Absent).
    pub fn missing_vars(&self) -> &[&'static str] {
        match self {
            Self::Complete => &[],
            Self::Absent => &SMTP_ENV_VARS,
            Self::Incomplete { missing } => missing,
        }
    }

    /// The boot-time diagnostic an operator must see, or `None` when there is
    /// nothing unambiguous to report.
    ///
    /// Only a *partial* configuration warrants this: all three set works, none
    /// set is a deliberate choice, but some-but-not-all is always a mistake —
    /// and it is otherwise invisible until a user fails to sign up.
    pub fn startup_warning(&self) -> Option<String> {
        match self {
            Self::Complete | Self::Absent => None,
            Self::Incomplete { missing } => Some(format!(
                "SMTP is only partially configured: {} not set. Email sending stays \
                 disabled, so verification, recovery, invitation and alert emails will \
                 not be delivered. Set the missing variable(s), or unset the SMTP \
                 variables entirely to run without email.",
                missing.join(", ")
            )),
        }
    }
}

/// Central application configuration.
///
/// Loaded from environment variables.
#[derive(Debug, Clone)]
pub struct Config {
    /// PostgreSQL connection string
    pub database_url: String,

    /// Redis connection string.
    ///
    /// When `None`, the server falls back to the in-memory KV store
    /// (suitable for single-instance / self-hosted deployments).
    pub redis_url: Option<String>,

    /// JWT signing secret (HS256)
    pub jwt_secret: String,

    /// AES-256-GCM encryption key for credentials at rest
    pub encryption_key: String,

    /// Server listen port
    pub port: u16,

    /// Self-hosted mode
    pub self_hosted: bool,

    /// Deployment mode. Determines auth strategy, database backend, and UI surface.
    pub mode: KrausenMode,

    /// Whether SMTP is configured at startup time.
    ///
    /// True only when all three of `SMTP_HOST`, `SMTP_USER` and
    /// `SMTP_PASSWORD` are set: the mailer authenticates before it can submit
    /// a message, so two of the three is not enough to send anything.
    ///
    /// Computed by [`SmtpSettings::classify`], the same predicate
    /// `EmailService::is_configured()` calls, so this flag cannot disagree
    /// with the mailer about whether mail can be sent.
    pub smtp_configured: bool,

    // ── Auth Methods ─────────────────────────────────────────────────────
    /// Enable passkey (WebAuthn) authentication. Defaults to true.
    pub passkeys_enabled: bool,

    /// Enable password-based authentication. Defaults to true.
    pub password_auth_enabled: bool,

    // ── Google OAuth ────────────────────────────────────────────────────
    /// Google OAuth client ID
    pub google_oauth_client_id: Option<String>,

    /// Google OAuth client secret
    pub google_oauth_client_secret: Option<String>,

    // ── WebAuthn (Passkeys) ─────────────────────────────────────────────
    /// Relying Party ID for WebAuthn (e.g., "localhost" or "krausen.ai")
    pub webauthn_rp_id: String,

    /// Relying Party display name
    pub webauthn_rp_name: String,

    // ── Notifications ──────────────────────────────────────────────────
    /// Slack webhook URL for admin notifications (signups, etc.)
    pub slack_feedback_webhook_url: Option<String>,

    /// Support email address (for admin notification emails)
    pub support_email: String,

    // ── Frontend ────────────────────────────────────────────────────────
    /// Frontend URL for constructing callback/redirect URLs
    pub frontend_url: String,

    /// Backend base URL for constructing OAuth redirect URIs
    pub base_url: String,
}

impl Config {
    /// Load configuration from environment variables.
    ///
    /// Panics on missing required variables — fail fast at startup.
    pub fn from_env() -> Self {
        // Determine deployment mode.
        // KRAUSEN_MODE takes precedence; fall back to SELF_HOSTED for backward compat.
        let mode = match env::var("KRAUSEN_MODE").unwrap_or_default().to_lowercase().as_str() {
            "personal" => KrausenMode::Personal,
            "self_hosted" | "selfhosted" => KrausenMode::SelfHosted,
            "saas" => KrausenMode::Saas,
            _ => {
                // Backward compat: check legacy SELF_HOSTED bool
                if env::var("SELF_HOSTED")
                    .unwrap_or_else(|_| "false".into())
                    .parse()
                    .unwrap_or(false)
                {
                    KrausenMode::SelfHosted
                } else {
                    KrausenMode::Saas
                }
            }
        };

        let base_url = env::var("BASE_URL")
            .unwrap_or_else(|_| "http://localhost:8003".into());
        let frontend_url = env::var("FRONTEND_URL")
            .unwrap_or_else(|_| base_url.clone());

        // One classification of the SMTP settings, used for both the boot
        // diagnostic and the `smtp_configured` flag every downstream reader
        // consumes.
        let smtp = SmtpSettings::classify(
            env::var("SMTP_HOST").ok().as_deref(),
            env::var("SMTP_USER").ok().as_deref(),
            env::var("SMTP_PASSWORD").ok().as_deref(),
        );
        if let Some(warning) = smtp.startup_warning() {
            tracing::warn!("{warning}");
        }

        Self {
            database_url: required_env("DATABASE_URL"),
            redis_url: env::var("REDIS_URL").ok(),
            jwt_secret: required_env("JWT_SECRET_KEY"),
            encryption_key: required_env("ENCRYPTION_KEY"),
            port: env::var("PORT")
                .unwrap_or_else(|_| "8003".into())
                .parse()
                .expect("PORT must be a valid u16"),
            self_hosted: mode.self_hosted(),
            mode,
            smtp_configured: smtp.can_send(),
            passkeys_enabled: env::var("PASSKEYS_ENABLED")
                .unwrap_or_else(|_| "true".into())
                .parse()
                .unwrap_or(true),
            password_auth_enabled: env::var("PASSWORD_AUTH_ENABLED")
                .unwrap_or_else(|_| "true".into())
                .parse()
                .unwrap_or(true),
            google_oauth_client_id: env::var("GOOGLE_OAUTH_CLIENT_ID").ok(),
            google_oauth_client_secret: env::var("GOOGLE_OAUTH_CLIENT_SECRET").ok(),
            webauthn_rp_id: env::var("WEBAUTHN_RP_ID").unwrap_or_else(|_| {
                url::Url::parse(&frontend_url)
                    .ok()
                    .and_then(|u| u.host_str().map(String::from))
                    .unwrap_or_else(|| "localhost".into())
            }),
            webauthn_rp_name: env::var("WEBAUTHN_RP_NAME")
                .unwrap_or_else(|_| "Krausen".into()),
            slack_feedback_webhook_url: env::var("SLACK_FEEDBACK_WEBHOOK_URL").ok(),
            support_email: env::var("SUPPORT_EMAIL")
                .unwrap_or_else(|_| "support@krausen.ai".into()),
            frontend_url,
            base_url,
        }
    }

    /// Load configuration for tests with sensible defaults.
    #[cfg(any(test, feature = "test-helpers"))]
    pub fn test_config() -> Self {
        const TEST_ENCRYPTION_KEY_B64: &str = "dGVzdC1hZXMta2V5LWZvci11bml0LXRlc3RzISEhISE=";

        Self {
            database_url: env::var("DATABASE_URL")
                .unwrap_or_else(|_| "postgres://krausen_test:test@localhost:5434/krausen_test".into()),
            redis_url: env::var("REDIS_URL").ok(),
            jwt_secret: env::var("JWT_SECRET_KEY")
                .unwrap_or_else(|_| "test-jwt-secret-not-for-production".into()),
            encryption_key: env::var("ENCRYPTION_KEY")
                .unwrap_or_else(|_| TEST_ENCRYPTION_KEY_B64.into()),
            port: 0,
            self_hosted: false,
            mode: KrausenMode::Saas,
            smtp_configured: false,
            passkeys_enabled: true,
            password_auth_enabled: true,
            google_oauth_client_id: Some("test-google-client-id".into()),
            google_oauth_client_secret: Some("test-google-client-secret".into()),
            webauthn_rp_id: "localhost".into(),
            webauthn_rp_name: "Krausen Test".into(),
            slack_feedback_webhook_url: None,
            support_email: "test@krausen.ai".into(),
            frontend_url: "http://localhost:5173".into(),
            base_url: "http://localhost:8003".into(),
        }
    }
}

impl Config {
    /// Returns true if the server is running in personal (desktop) mode.
    pub fn is_personal(&self) -> bool {
        self.mode == KrausenMode::Personal
    }

    /// Returns true if SMTP is configured.
    pub fn smtp_configured(&self) -> bool {
        self.smtp_configured
    }
}

fn required_env(key: &str) -> String {
    env::var(key).unwrap_or_else(|_| panic!("{key} environment variable is required"))
}

#[cfg(test)]
mod tests;
