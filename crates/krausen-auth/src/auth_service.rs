// SPDX-License-Identifier: AGPL-3.0-or-later

//! Auth orchestration service functions — extracted from Leptos server_fns.
//!
//! These functions contain the business logic that was previously inlined in
//! the UI server_fns. Server functions are now thin wrappers that delegate
//! to these service functions and apply HTTP concerns (cookie setting via
//! `ResponseOptions`) to the returned results.
//!
//! All functions take `&DbPool` as the first argument and return
//! `krausen_core::Result<T>`. KV, config, and encryption key args follow.

use krausen_core::{DbPool, KVPool};

use crate::session::{AuthenticatedSession, create_authenticated_session};
use crate::token_service::DeviceInfo;

// ---------------------------------------------------------------------------
// Login
// ---------------------------------------------------------------------------

/// Outcome of `login_with_password_service`.
pub enum LoginServiceResult {
    /// Authenticated successfully — server_fn should set cookies from `session`.
    Success(Box<AuthenticatedSession>),
    /// TOTP challenge needed.
    TwoFactorRequired { email: String },
    /// Email not yet verified.
    VerificationRequired { email: String },
    /// Rate limited.
    RateLimited { retry_after_secs: u64 },
    /// Invalid credentials or other non-fatal error.
    InvalidCredentials,
}

/// Parameters for `login_with_password_service`.
pub struct LoginWithPasswordParams<'a> {
    pub db: &'a DbPool,
    pub kv: &'a KVPool,
    pub jwt_secret: &'a str,
    pub email: &'a str,
    pub password: &'a str,
    pub totp_code: Option<&'a str>,
    pub ip: &'a str,
    pub device: &'a DeviceInfo,
}

/// Full login-with-password orchestration.
///
/// Rate-limits, looks up user, verifies password, checks TOTP, and creates
/// an authenticated session. The caller (server_fn) applies HTTP cookies from
/// the returned `AuthenticatedSession`.
pub async fn login_with_password_service(
    params: LoginWithPasswordParams<'_>,
) -> krausen_core::Result<LoginServiceResult> {
    let LoginWithPasswordParams {
        db,
        kv,
        jwt_secret,
        email,
        password,
        totp_code,
        ip,
        device,
    } = params;
    // Rate limit
    let rate = crate::rate_limiter::check_rate_limit(kv, ip, "login", None).await?;
    if !rate.allowed {
        return Ok(LoginServiceResult::RateLimited {
            retry_after_secs: rate.retry_after_secs,
        });
    }

    // Look up user by email
    let user = match crate::user_service::get_user_by_email(db, email).await? {
        Some(u) => u,
        None => return Ok(LoginServiceResult::InvalidCredentials),
    };

    // Get password auth method
    let password_method =
        match crate::user_service::get_auth_method(db, &user.user_id, "password").await? {
            Some(m) => m,
            None => return Ok(LoginServiceResult::InvalidCredentials),
        };

    // Extract hash
    let hash = password_method
        .auth_data
        .get("hash")
        .and_then(|v| v.as_str())
        .ok_or_else(|| krausen_core::Error::Internal("Password auth method missing hash".into()))?;

    // Verify password
    if !crate::password::verify_password(password, hash)
        .map_err(|e| krausen_core::Error::Internal(format!("Password verification error: {e}")))?
    {
        return Ok(LoginServiceResult::InvalidCredentials);
    }

    // Check email verification before TOTP (don't leak TOTP status for unverified accounts)
    if !user.verified {
        return Ok(LoginServiceResult::VerificationRequired {
            email: user.email.clone(),
        });
    }

    // Check TOTP
    let totp_method = crate::user_service::get_auth_method(db, &user.user_id, "totp").await?;
    if let Some(totp_method) = totp_method
        && totp_method.active
    {
        match totp_code {
            None => {
                return Ok(LoginServiceResult::TwoFactorRequired {
                    email: user.email.clone(),
                });
            }
            Some(code) => {
                let secret = totp_method
                    .auth_data
                    .get("secret")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        krausen_core::Error::Internal(
                            "TOTP auth method is missing its secret".into(),
                        )
                    })?;

                if !crate::totp::verify_code(secret, code) {
                    return Ok(LoginServiceResult::InvalidCredentials);
                }

                // Best-effort: record that the second factor was used.
                let _ = crate::user_service::touch_auth_method(db, &user.user_id, "totp").await;
            }
        }
    }

    // Create authenticated session
    let sess = create_authenticated_session(db, kv, jwt_secret, &user, device).await?;

    // Touch last_used on password auth method (best-effort)
    let _ = crate::user_service::touch_auth_method(db, &user.user_id, "password").await;

    Ok(LoginServiceResult::Success(Box::new(sess)))
}

// ---------------------------------------------------------------------------
// Signup
// ---------------------------------------------------------------------------

/// Outcome of `signup_start_service`.
pub enum SignupStartServiceResult {
    /// Self-hosted SMTP-less: account created, cookies should be set from `session`.
    AccountCreated(Box<AuthenticatedSession>),
    /// SaaS flow: verification email sent.
    VerificationRequired,
    /// Rate limited.
    RateLimited { retry_after_secs: u64 },
    /// Non-fatal error (validation, registration closed, etc.).
    Error { message: String },
}

/// Parameters for `signup_start_service`.
pub struct SignupStartParams<'a> {
    pub db: &'a DbPool,
    pub kv: &'a KVPool,
    pub jwt_secret: &'a str,
    pub email: &'a str,
    pub name: Option<&'a str>,
    pub password: Option<&'a str>,
    pub ip: &'a str,
    pub device: &'a DeviceInfo,
    pub self_hosted: bool,
    pub smtp_configured: bool,
    pub frontend_url: &'a str,
    pub config: Option<&'a krausen_core::Config>,
}

/// Step one of signup: take an email address and send a link to it.
///
/// **This writes no `users` row, and must not start doing so.** Submitting an
/// address is a claim; the account is created once the emailed token comes back
/// and proves whoever sent it can read that mailbox. That happens in
/// [`establish_verified_account`], called from [`signup_complete_service`] and
/// [`passkey_signup_complete_service`]. There is no unverified-user state, and
/// nothing downstream should have to ask whether a row is "real yet".
///
/// Without a mailer, remote signup is unavailable; a local operator initializes
/// the first owner instead.
pub async fn signup_start_service(
    params: SignupStartParams<'_>,
) -> krausen_core::Result<SignupStartServiceResult> {
    let SignupStartParams {
        db,
        kv,
        email,
        ip,
        self_hosted,
        smtp_configured,
        frontend_url,
        ..
    } = params;
    // Rate limit
    let rate = crate::rate_limiter::check_rate_limit(kv, ip, "signup", None).await?;
    if !rate.allowed {
        return Ok(SignupStartServiceResult::RateLimited {
            retry_after_secs: rate.retry_after_secs,
        });
    }

    let smtp_less_self_hosted = self_hosted && !smtp_configured;

    // An email address by itself is not proof of an invitation. First-owner
    // setup is a local operator action; invitation proof must be implemented
    // before SMTP-less remote signup can be enabled.
    if smtp_less_self_hosted {
        return Ok(SignupStartServiceResult::Error {
            message: "Registration requires an administrator-provided proof; initialize the first owner locally.".into(),
        });
    }

    // Identical response for known and unknown accounts avoids enumeration.
    send_signup_link(db, email, frontend_url).await?;
    Ok(SignupStartServiceResult::VerificationRequired)
}

/// Mint a signup link and email it. Writes no `users` row.
///
/// The account is created when the emailed token is redeemed, by
/// [`establish_verified_account`]. Submitting an email address is a claim, not
/// an account: until someone proves they can read that mailbox there is
/// nothing to store, and a half-made row is a state the rest of the system has
/// to keep asking about.
///
/// Called for an unknown address and for one that already has an account — the
/// return value is identical either way, so the response cannot be used to
/// discover who has signed up. The mailbox, which only its owner can read, is
/// what tells the two apart.
async fn send_signup_link(
    db: &DbPool,
    email: &str,
    frontend_url: &str,
) -> krausen_core::Result<()> {
    let raw_token =
        crate::token_service::create_verification_token(db, email, "email_verification").await?;
    let signup_url = format!(
        "{}/signup/complete?token={raw_token}",
        frontend_url.trim_end_matches('/')
    );
    tracing::info!("Password signup link for {email}: {signup_url}");

    spawn_verification_email(email.to_string(), String::new(), signup_url);
    Ok(())
}

// ---------------------------------------------------------------------------
// Account establishment
// ---------------------------------------------------------------------------

/// Parameters for [`establish_verified_account`].
struct EstablishVerifiedAccountParams<'a> {
    db: &'a DbPool,
    email: &'a str,
    /// Display name collected by the redeeming step.
    name: Option<&'a str>,
    marketing_consent: bool,
    config: Option<&'a krausen_core::Config>,
    slack_feedback_webhook_url: Option<&'a str>,
    support_email: &'a str,
}

/// Create-or-adopt a verified `users` row, record terms, and place the user in
/// a workspace.
///
/// This is the single point where a signup becomes an account. No signup path
/// writes a `users` row before its verification token is redeemed, so every
/// redeeming step — [`signup_complete_service`] and
/// [`passkey_signup_complete_service`] — has to create the account rather than
/// find one an earlier step left behind.
///
/// - No row for `email`: create one, already verified, and fire the admin
///   notification — this is a genuinely new account.
/// - A row already exists: adopt it. Mark it verified and carry on, with no
///   admin notification, because the account is not new. This is reachable
///   rather than exotic — someone can ask for a signup link, sign in with
///   Google before clicking it, then click it. The token proves they control
///   the mailbox, so finishing password or passkey setup on that same row is
///   the right answer, not a collision.
///
/// Returns the row as it stands afterwards.
async fn establish_verified_account(
    params: EstablishVerifiedAccountParams<'_>,
) -> krausen_core::Result<krausen_core::models::User> {
    let EstablishVerifiedAccountParams {
        db,
        email,
        name,
        marketing_consent,
        config,
        slack_feedback_webhook_url,
        support_email,
    } = params;

    let existing = crate::user_service::get_user_by_email(db, email).await?;
    let (user_id, is_new_account) = match existing {
        None => {
            let user = crate::user_service::create_user(db, email, name, true).await?;
            (user.user_id, true)
        }
        Some(user) => {
            crate::user_service::mark_user_verified(db, email).await?;
            (user.user_id, false)
        }
    };

    if let Some(name) = name {
        crate::user_service::update_user_name(db, &user_id, name).await?;
    }

    crate::user_service::update_terms_acceptance(
        db,
        &user_id,
        krausen_core::TERMS_VERSION,
        marketing_consent,
    )
    .await?;

    if marketing_consent {
        crate::user_service::update_extra_metadata(
            db,
            &user_id,
            &serde_json::json!({"marketing_consent": true}),
        )
        .await?;
    }

    place_user_in_workspace(db, &user_id, name, email, config).await?;

    if is_new_account {
        // Fire-and-forget, and only for a new account: the adopt branch already
        // had one, so there is nothing to announce. This sits here rather than
        // at signup-start because there it could report a user who never came
        // into being.
        let notify_webhook = slack_feedback_webhook_url.map(|s| s.to_string());
        let notify_support = support_email.to_string();
        let notify_email = email.to_string();
        let notify_name = name.unwrap_or_default().to_string();
        let notify_user_id = user_id.clone();
        tokio::spawn(async move {
            crate::notifications::notify_signup(
                notify_webhook.as_deref(),
                &notify_support,
                &notify_email,
                &notify_name,
                &notify_user_id,
            )
            .await;
        });
    }

    crate::user_service::get_user_by_email(db, email)
        .await?
        .ok_or_else(|| {
            krausen_core::Error::Internal("user vanished after being established".into())
        })
}

/// Give a user somewhere to work: join the workspace they were invited to, or
/// make them one of their own.
///
/// Does nothing when they already have a workspace. The adopt branch of
/// [`establish_verified_account`] can hand back a row that already has one, and
/// `create_workspace_for_user` is not idempotent — it inserts a fresh
/// `workspaces` row and repoints `last_workspace_id` every time it is called.
async fn place_user_in_workspace(
    db: &DbPool,
    user_id: &str,
    user_name: Option<&str>,
    email: &str,
    config: Option<&krausen_core::Config>,
) -> krausen_core::Result<()> {
    if crate::user_service::get_user_workspace_context(db, user_id)
        .await?
        .is_some()
    {
        return Ok(());
    }

    // A signup email verification does not authorize an invitation response.
    // The recipient must complete the action-bound invitation workflow separately.
    crate::user_service::create_workspace_for_user(db, user_id, user_name, email, config).await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Signup complete (email verification token flow)
// ---------------------------------------------------------------------------

/// Outcome of `signup_complete_service`.
pub enum SignupCompleteServiceResult {
    /// Validation error (terms not accepted, bad password, etc.).
    Error { message: String },
    /// Invalid or expired signup token.
    InvalidToken,
    /// Account created and authenticated — server_fn should set cookies.
    Success(Box<AuthenticatedSession>),
}

/// Parameters for `signup_complete_service`.
pub struct SignupCompleteParams<'a> {
    pub db: &'a DbPool,
    pub kv: &'a KVPool,
    pub jwt_secret: &'a str,
    pub token: &'a str,
    pub name: &'a str,
    pub password: &'a str,
    pub terms_accepted: bool,
    pub marketing_consent: bool,
    pub device: &'a DeviceInfo,
    pub config: Option<&'a krausen_core::Config>,
    pub slack_feedback_webhook_url: Option<&'a str>,
    pub support_email: &'a str,
}

/// Full signup-complete orchestration (email verification token flow).
pub async fn signup_complete_service(
    params: SignupCompleteParams<'_>,
) -> krausen_core::Result<SignupCompleteServiceResult> {
    let SignupCompleteParams {
        db,
        kv,
        jwt_secret,
        token,
        name,
        password,
        terms_accepted,
        marketing_consent,
        device,
        config,
        slack_feedback_webhook_url,
        support_email,
    } = params;
    if !terms_accepted {
        return Ok(SignupCompleteServiceResult::Error {
            message:
                "You must accept the Terms of Service and Privacy Policy to create an account."
                    .to_string(),
        });
    }
    if password.len() < 8 {
        return Ok(SignupCompleteServiceResult::Error {
            message: "Password must be at least 8 characters".to_string(),
        });
    }
    let name = name.trim().to_string();
    if name.is_empty() {
        return Ok(SignupCompleteServiceResult::Error {
            message: "Name is required".to_string(),
        });
    }

    // Verify email verification token
    let email =
        crate::token_service::verify_verification_token(db, token, "email_verification").await?;
    let Some(email) = email else {
        return Ok(SignupCompleteServiceResult::InvalidToken);
    };

    // Hash first, so a bad password fails before anything is written.
    let hash = crate::password::hash_password(password)
        .map_err(|e| krausen_core::Error::Internal(format!("Failed to hash password: {e}")))?;
    let auth_data = serde_json::json!({"hash": hash});

    // The token proved control of the mailbox, so this is where the account
    // comes into being — signup-start deliberately left no row behind.
    let user = establish_verified_account(EstablishVerifiedAccountParams {
        db,
        email: &email,
        name: Some(&name),
        marketing_consent,
        config,
        slack_feedback_webhook_url,
        support_email,
    })
    .await?;

    crate::user_service::upsert_auth_method(db, &user.user_id, "password", &auth_data).await?;

    let sess = create_authenticated_session(db, kv, jwt_secret, &user, device).await?;
    Ok(SignupCompleteServiceResult::Success(Box::new(sess)))
}

// ---------------------------------------------------------------------------
// Google OAuth callback
// ---------------------------------------------------------------------------

/// Outcome of `google_oauth_callback_service`.
pub enum GoogleOAuthServiceResult {
    /// New user or user needing terms — redirect to welcome page.
    PendingTerms { redirect_url: String },
    /// Existing user logged in — server_fn should set cookies.
    Success {
        session: Box<AuthenticatedSession>,
        oauth_continue: Option<String>,
    },
    /// Rate limited.
    RateLimited { retry_after_secs: u64 },
}

/// Parameters for `google_oauth_callback_service`.
pub struct GoogleOAuthCallbackParams<'a> {
    pub db: &'a DbPool,
    pub kv: &'a KVPool,
    pub jwt_secret: &'a str,
    pub code: &'a str,
    pub state: Option<&'a str>,
    pub ip: &'a str,
    pub device: &'a DeviceInfo,
    pub client_id: &'a str,
    pub client_secret: &'a str,
    pub frontend_url: &'a str,
    pub encryption_key: &'a [u8; 32],
    pub config: Option<&'a krausen_core::Config>,
}

/// Full Google OAuth callback orchestration.
pub async fn google_oauth_callback_service(
    params: GoogleOAuthCallbackParams<'_>,
) -> krausen_core::Result<GoogleOAuthServiceResult> {
    let GoogleOAuthCallbackParams {
        db,
        kv,
        jwt_secret,
        code,
        state,
        ip,
        device,
        client_id,
        client_secret,
        frontend_url,
        encryption_key,
        config,
    } = params;
    // Rate limit
    let rate = crate::rate_limiter::check_rate_limit(kv, ip, "login", None).await?;
    if !rate.allowed {
        return Ok(GoogleOAuthServiceResult::RateLimited {
            retry_after_secs: rate.retry_after_secs,
        });
    }

    // Verify CSRF state (optional)
    let mut oauth_continue = None;
    if let Some(csrf_state) = state {
        let state_data = crate::redis_ops::verify_oauth_state(kv, "google", csrf_state).await?;
        if let Some(state_data) = state_data {
            oauth_continue = state_data
                .get("oauth_continue")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
        }
    }

    // Exchange the authorization code for tokens.
    let redirect_uri = format!(
        "{}/auth/google/callback",
        frontend_url.trim_end_matches('/')
    );
    let token_data = crate::google_oauth::exchange_code_for_tokens(
        client_id,
        client_secret,
        code,
        &redirect_uri,
    )
    .await
    .map_err(|e| {
        krausen_core::Error::Internal(format!(
            "Failed to exchange Google OAuth code for tokens: {e}"
        ))
    })?;

    // Identify the user.
    let user_info = crate::google_oauth::get_user_info(&token_data.access_token)
        .await
        .map_err(|e| {
            krausen_core::Error::Internal(format!("Failed to get user info from Google: {e}"))
        })?;
    let email = user_info.email.to_lowercase();

    let existing_user = crate::user_service::get_user_by_email(db, &email).await?;

    match existing_user {
        None => {
            // New user — stash the profile and send them through terms acceptance.
            let temp_token = crate::redis_ops::generate_token();
            let signup_data = serde_json::json!({
                "email": email,
                "name": user_info.name.clone().unwrap_or_default(),
                "oauth_data": {
                    "google_id": user_info.id,
                    "oauth_provider": "google",
                    "picture": user_info.picture,
                }
            });
            crate::redis_ops::store_pending_signup(kv, &temp_token, &signup_data).await?;
            let redirect_url = format!(
                "{}/welcome?temp_token={temp_token}",
                frontend_url.trim_end_matches('/')
            );
            Ok(GoogleOAuthServiceResult::PendingTerms { redirect_url })
        }
        Some(user) if user.terms_accepted_at.is_none() => {
            // Known user who has not accepted terms yet.
            let temp_token = crate::redis_ops::generate_token();
            let terms_data = serde_json::json!({
                "user_id": user.user_id,
                "email": email,
            });
            crate::redis_ops::store_pending_terms(kv, &temp_token, &terms_data).await?;
            let redirect_url = format!(
                "{}/welcome?temp_token={temp_token}&existing_user=true",
                frontend_url.trim_end_matches('/')
            );
            Ok(GoogleOAuthServiceResult::PendingTerms { redirect_url })
        }
        Some(user) => {
            // Normal login.
            ensure_google_oauth_auth_method(db, &user.user_id).await?;
            ensure_user_has_workspace(db, &user.user_id, user.name.as_deref(), &email, config)
                .await?;
            update_google_oauth_data(db, &user, &user_info, encryption_key).await?;

            let sess = create_authenticated_session(db, kv, jwt_secret, &user, device).await?;
            Ok(GoogleOAuthServiceResult::Success {
                session: Box::new(sess),
                oauth_continue,
            })
        }
    }
}

/// Ensure a `google_oauth` auth method row exists for the user (idempotent).
async fn ensure_google_oauth_auth_method(db: &DbPool, user_id: &str) -> krausen_core::Result<()> {
    if crate::user_service::get_auth_method(db, user_id, "google_oauth")
        .await?
        .is_none()
    {
        let auth_data = serde_json::json!({
            "linked_at": chrono::Utc::now().to_rfc3339(),
        });
        crate::user_service::upsert_auth_method(db, user_id, "google_oauth", &auth_data).await?;
    }
    Ok(())
}

/// Ensure the user belongs to at least one workspace; create one if not.
async fn ensure_user_has_workspace(
    db: &DbPool,
    user_id: &str,
    user_name: Option<&str>,
    email: &str,
    config: Option<&krausen_core::Config>,
) -> krausen_core::Result<()> {
    if crate::user_service::get_user_workspace_context(db, user_id)
        .await?
        .is_none()
    {
        crate::user_service::create_workspace_for_user(db, user_id, user_name, email, config)
            .await?;
    }
    Ok(())
}

/// Refresh the stored Google profile data after a successful sign-in.
async fn update_google_oauth_data(
    db: &DbPool,
    user: &krausen_core::models::User,
    user_info: &crate::google_oauth::GoogleUserInfo,
    encryption_key: &[u8; 32],
) -> krausen_core::Result<()> {
    let updated_oauth = crate::google_oauth::OAuthData {
        google_id: Some(user_info.id.clone()),
        oauth_provider: Some("google".to_string()),
        picture: user_info.picture.clone(),
        last_oauth_login: Some(chrono::Utc::now().to_rfc3339()),
    };

    let encrypted = crate::google_oauth::build_oauth_data(&updated_oauth, encryption_key)
        .map_err(|e| krausen_core::Error::Internal(format!("Failed to build OAuth data: {e}")))?;
    crate::user_service::update_user_oauth_data(db, &user.user_id, Some(&encrypted)).await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Account recovery
// ---------------------------------------------------------------------------

/// Outcome of `recovery_verify_service`.
pub enum RecoveryVerifyServiceResult {
    /// Token verified — recovery session created.
    Success {
        recovery_session_id: String,
        has_passkeys: bool,
    },
    /// Invalid or expired token.
    InvalidToken,
    /// Account is not verified.
    AccountNotVerified,
}

/// Verify a recovery token and create a short-lived recovery session.
pub async fn recovery_verify_service(
    db: &DbPool,
    kv: &KVPool,
    token: &str,
) -> krausen_core::Result<RecoveryVerifyServiceResult> {
    let email =
        crate::token_service::verify_verification_token(db, token, "account_recovery").await?;
    let Some(email) = email else {
        return Ok(RecoveryVerifyServiceResult::InvalidToken);
    };

    let user = crate::user_service::get_user_by_email(db, &email)
        .await?
        .ok_or_else(|| krausen_core::Error::Internal("User not found for recovery token".into()))?;

    if !user.verified {
        return Ok(RecoveryVerifyServiceResult::AccountNotVerified);
    }

    let creds = crate::user_service::get_passkey_credentials(db, &user.user_id).await?;
    let has_passkeys = !creds.is_empty();

    let recovery_session_id = crate::redis_ops::generate_token();
    crate::redis_ops::store_recovery_session(kv, &recovery_session_id, &user.user_id).await?;

    Ok(RecoveryVerifyServiceResult::Success {
        recovery_session_id,
        has_passkeys,
    })
}

/// Outcome of `recovery_set_password_service`.
pub enum RecoverySetPasswordServiceResult {
    /// Password changed and user logged in — server_fn should set cookies.
    Success(Box<AuthenticatedSession>),
    /// Password validation failed.
    Error { message: String },
    /// Invalid or expired recovery session.
    InvalidSession,
}

/// Set a new password using a recovery session, completing the recovery flow.
pub async fn recovery_set_password_service(
    db: &DbPool,
    kv: &KVPool,
    jwt_secret: &str,
    recovery_session_id: &str,
    new_password: &str,
    device: &DeviceInfo,
) -> krausen_core::Result<RecoverySetPasswordServiceResult> {
    if new_password.len() < 8 {
        return Ok(RecoverySetPasswordServiceResult::Error {
            message: "Password must be at least 8 characters".into(),
        });
    }

    // Peek recovery session (non-destructive — keeps session alive if validation fails)
    let user_id = crate::redis_ops::peek_recovery_session(kv, recovery_session_id).await?;
    let Some(user_id) = user_id else {
        return Ok(RecoverySetPasswordServiceResult::InvalidSession);
    };

    let user = crate::user_service::get_user_by_id(db, &user_id)
        .await?
        .ok_or_else(|| {
            krausen_core::Error::Internal("User not found for recovery session".into())
        })?;

    // Require new password to differ from existing (if any)
    if let Some(existing) = crate::user_service::get_auth_method(db, &user_id, "password").await?
        && let Some(existing_hash) = existing.auth_data.get("hash").and_then(|v| v.as_str())
    {
        let same = crate::password::verify_password(new_password, existing_hash).map_err(|e| {
            krausen_core::Error::Internal(format!("Password verification error: {e}"))
        })?;
        if same {
            return Ok(RecoverySetPasswordServiceResult::Error {
                message: "New password must be different from your current password.".into(),
            });
        }
    }

    // Hash and store new password
    let hash = crate::password::hash_password(new_password)
        .map_err(|e| krausen_core::Error::Internal(format!("Failed to hash password: {e}")))?;
    crate::user_service::upsert_auth_method(
        db,
        &user_id,
        "password",
        &serde_json::json!({"hash": hash}),
    )
    .await?;

    // Consume recovery session
    crate::redis_ops::delete_recovery_session(kv, recovery_session_id).await?;

    // Disable TOTP — only after password successfully changed
    let totp_disabled = crate::user_service::remove_auth_method(db, &user_id, "totp").await?;
    if totp_disabled {
        tracing::info!(user_id = %user_id, "TOTP disabled during account recovery");
    }

    let sess = create_authenticated_session(db, kv, jwt_secret, &user, device).await?;
    Ok(RecoverySetPasswordServiceResult::Success(Box::new(sess)))
}

// ---------------------------------------------------------------------------
// Passkey login complete
// ---------------------------------------------------------------------------

/// Outcome of `passkey_login_complete_service`.
pub enum PasskeyLoginServiceResult {
    /// Authenticated successfully — server_fn should set cookies.
    Success(Box<AuthenticatedSession>),
    /// Challenge not found or expired.
    InvalidChallenge,
    /// User not found for credential.
    InvalidCredentials,
    /// Email not verified.
    VerificationRequired { email: String },
    /// Rate limited.
    RateLimited { retry_after_secs: u64 },
    /// WebAuthn assertion verification failed.
    AuthFailed,
}

/// Parameters for `passkey_login_complete_service`.
pub struct PasskeyLoginCompleteParams<'a> {
    pub db: &'a DbPool,
    pub kv: &'a KVPool,
    pub jwt_secret: &'a str,
    pub webauthn: &'a webauthn_rs::Webauthn,
    pub challenge_id: &'a str,
    pub assertion_json: &'a str,
    pub ip: &'a str,
    pub device: &'a DeviceInfo,
}

/// Full passkey-login-complete orchestration.
pub async fn passkey_login_complete_service(
    params: PasskeyLoginCompleteParams<'_>,
) -> krausen_core::Result<PasskeyLoginServiceResult> {
    let PasskeyLoginCompleteParams {
        db,
        kv,
        jwt_secret,
        webauthn,
        challenge_id,
        assertion_json,
        ip,
        device,
    } = params;
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    use webauthn_rs::prelude::*;

    // Rate limit
    let rate = crate::rate_limiter::check_rate_limit(kv, ip, "login", None).await?;
    if !rate.allowed {
        return Ok(PasskeyLoginServiceResult::RateLimited {
            retry_after_secs: rate.retry_after_secs,
        });
    }

    // Parse assertion
    let credential: PublicKeyCredential = serde_json::from_str(assertion_json)
        .map_err(|e| krausen_core::Error::Internal(format!("Invalid assertion JSON: {e}")))?;

    // Get and delete challenge (prevent replay)
    let challenge_data = crate::redis_ops::get_webauthn_challenge(kv, challenge_id).await?;
    let Some(challenge_data) = challenge_data else {
        return Ok(PasskeyLoginServiceResult::InvalidChallenge);
    };
    crate::redis_ops::delete_webauthn_challenge(kv, challenge_id).await?;

    // Find user by credential ID
    let cred_id_bytes: &[u8] = credential.raw_id.as_ref();
    let credential_id_b64 = URL_SAFE_NO_PAD.encode(cred_id_bytes);
    let user = crate::user_service::find_user_by_credential_id(db, &credential_id_b64).await?;
    let Some(user) = user else {
        return Ok(PasskeyLoginServiceResult::InvalidCredentials);
    };

    if !user.verified {
        return Ok(PasskeyLoginServiceResult::VerificationRequired {
            email: user.email.clone(),
        });
    }

    // Verify assertion (discoverable or standard flow)
    let is_discoverable = challenge_data["discoverable"].as_bool().unwrap_or(false);
    let passkeys = get_passkeys_for_user(db, &user.user_id).await?;

    if is_discoverable {
        let disc_state: DiscoverableAuthentication =
            serde_json::from_value(challenge_data["discoverable_state"].clone()).map_err(|e| {
                krausen_core::Error::Internal(format!("Deserialize discoverable state: {e}"))
            })?;
        if passkeys.is_empty() {
            return Ok(PasskeyLoginServiceResult::InvalidCredentials);
        }
        let auth_result = crate::webauthn::finish_discoverable_authentication(
            webauthn,
            &credential,
            disc_state,
            &passkeys,
        )
        .map_err(|e| {
            tracing::warn!(error = %e, "Passkey discoverable auth failed");
            krausen_core::Error::Internal("Authentication failed".into())
        });
        match auth_result {
            Ok(auth_result) => {
                update_passkey_after_auth_inner(
                    db,
                    &user.user_id,
                    &credential_id_b64,
                    cred_id_bytes,
                    &passkeys,
                    &auth_result,
                )
                .await;
            }
            Err(_) => return Ok(PasskeyLoginServiceResult::AuthFailed),
        }
    } else {
        let auth_state: PasskeyAuthentication = serde_json::from_value(
            challenge_data["authentication_state"].clone(),
        )
        .map_err(|e| krausen_core::Error::Internal(format!("Deserialize auth state: {e}")))?;
        let auth_result =
            crate::webauthn::finish_authentication(webauthn, &credential, &auth_state).map_err(
                |e| {
                    tracing::warn!(error = %e, "Passkey auth failed");
                    krausen_core::Error::Internal("Authentication failed".into())
                },
            );
        match auth_result {
            Ok(auth_result) => {
                update_passkey_after_auth_inner(
                    db,
                    &user.user_id,
                    &credential_id_b64,
                    cred_id_bytes,
                    &passkeys,
                    &auth_result,
                )
                .await;
            }
            Err(_) => return Ok(PasskeyLoginServiceResult::AuthFailed),
        }
    }

    // Touch last_used on webauthn auth method (best-effort)
    let _ = crate::user_service::touch_auth_method(db, &user.user_id, "webauthn").await;

    let sess = create_authenticated_session(db, kv, jwt_secret, &user, device).await?;
    Ok(PasskeyLoginServiceResult::Success(Box::new(sess)))
}

// ---------------------------------------------------------------------------
// Passkey register start
// ---------------------------------------------------------------------------

/// Outcome of `passkey_register_start_service`.
pub enum PasskeyRegisterStartServiceResult {
    /// Challenge generated — server_fn returns to client.
    Success {
        challenge_id: String,
        creation_challenge: String,
    },
    /// Rate limited.
    RateLimited { retry_after_secs: u64 },
    /// Unverified email — must verify first.
    UnverifiedEmail,
    /// SaaS: verification email sent — user must verify before proceeding.
    VerificationEmailSent,
}

/// Parameters for `passkey_register_start_service`.
pub struct PasskeyRegisterStartParams<'a> {
    pub db: &'a DbPool,
    pub kv: &'a KVPool,
    pub webauthn: &'a webauthn_rs::Webauthn,
    pub email: &'a str,
    pub name: &'a str,
    pub device_name: &'a str,
    pub ip: &'a str,
    pub self_hosted: bool,
    pub smtp_configured: bool,
    pub frontend_url: &'a str,
}

/// Full passkey-register-start orchestration.
pub async fn passkey_register_start_service(
    params: PasskeyRegisterStartParams<'_>,
) -> krausen_core::Result<PasskeyRegisterStartServiceResult> {
    let _ = params;
    Err(krausen_core::Error::ServiceUnavailable(
        "Public passkey enrollment is unavailable until account ownership can be verified".into(),
    ))
}

// ---------------------------------------------------------------------------
// Passkey register complete
// ---------------------------------------------------------------------------

/// Full passkey-register-complete orchestration.
///
/// Returns an `AuthenticatedSession` on success (auto-login after registration).
pub async fn passkey_register_complete_service(
    db: &DbPool,
    kv: &KVPool,
    jwt_secret: &str,
    webauthn: &webauthn_rs::Webauthn,
    challenge_id: &str,
    credential_json: &str,
    device: &DeviceInfo,
) -> krausen_core::Result<AuthenticatedSession> {
    let _ = (
        db,
        kv,
        jwt_secret,
        webauthn,
        challenge_id,
        credential_json,
        device,
    );
    Err(krausen_core::Error::ServiceUnavailable(
        "Public passkey enrollment is unavailable until account ownership can be verified".into(),
    ))
}

// ---------------------------------------------------------------------------
// Passkey signup complete
// ---------------------------------------------------------------------------

/// Parameters for `passkey_signup_complete_service`.
pub struct PasskeySignupCompleteParams<'a> {
    pub db: &'a DbPool,
    pub kv: &'a KVPool,
    pub webauthn: &'a webauthn_rs::Webauthn,
    pub token: &'a str,
    pub name: &'a str,
    pub terms_accepted: bool,
    pub marketing_consent: bool,
    pub config: Option<&'a krausen_core::Config>,
    pub slack_feedback_webhook_url: Option<&'a str>,
    pub support_email: &'a str,
}

/// Full passkey-signup-complete orchestration.
///
/// Verifies the email verification token, sets up the user account, and
/// generates a WebAuthn registration challenge. Returns the challenge for the
/// client to complete via `passkey_register_complete`.
pub async fn passkey_signup_complete_service(
    params: PasskeySignupCompleteParams<'_>,
) -> krausen_core::Result<Result<(String, String), String>> {
    let PasskeySignupCompleteParams {
        db,
        kv,
        webauthn,
        token,
        name,
        terms_accepted,
        marketing_consent,
        config,
        slack_feedback_webhook_url,
        support_email,
    } = params;
    // Returns Ok((challenge_id, creation_challenge)) or Err(message)

    if !terms_accepted {
        return Ok(Err(
            "You must accept the Terms of Service and Privacy Policy to create an account."
                .to_string(),
        ));
    }
    let name = name.trim().to_string();
    if name.is_empty() {
        return Ok(Err("Name is required".to_string()));
    }

    // Verify token
    let email =
        crate::token_service::verify_verification_token(db, token, "email_verification").await?;
    let Some(email) = email else {
        return Ok(Err(
            "Invalid or expired signup link. Please request a new one.".to_string(),
        ));
    };

    // The token proved control of the mailbox, so this is where the account
    // comes into being — passkey signup-start deliberately left no row behind.
    let user = establish_verified_account(EstablishVerifiedAccountParams {
        db,
        email: &email,
        name: Some(&name),
        marketing_consent,
        config,
        slack_feedback_webhook_url,
        support_email,
    })
    .await?;

    // Generate WebAuthn registration challenge
    let user_unique_id = webauthn_user_id(&email);
    let creds = crate::user_service::get_passkey_credentials(db, &user.user_id).await?;
    let exclude_ids = build_exclude_ids(&creds);
    let exclude_opt = if exclude_ids.is_empty() {
        None
    } else {
        Some(exclude_ids)
    };

    let (ccr, reg_state) =
        crate::webauthn::start_registration(webauthn, user_unique_id, &email, &name, exclude_opt)
            .map_err(|e| krausen_core::Error::Internal(e.to_string()))?;

    let challenge_id = crate::redis_ops::generate_token();
    let reg_state_json = serde_json::to_value(&reg_state)
        .map_err(|e| krausen_core::Error::Internal(format!("Serialize reg state: {e}")))?;
    let challenge_data = serde_json::json!({
        "registration_state": reg_state_json,
        "email": email,
        "user_name": &name,
        "user_id": user.user_id,
        "device_name": "Unknown Device",
        "is_signup": true,
    });
    crate::redis_ops::store_webauthn_challenge(kv, &challenge_id, &challenge_data).await?;

    let creation_challenge = serde_json::to_string(&ccr)
        .map_err(|e| krausen_core::Error::Internal(format!("Serialize creation challenge: {e}")))?;

    tracing::info!(
        email = %email,
        user_id = %user.user_id,
        "Passkey signup token verified, WebAuthn challenge generated"
    );
    Ok(Ok((challenge_id, creation_challenge)))
}

// ---------------------------------------------------------------------------
// Passkey recovery verify
// ---------------------------------------------------------------------------

/// Full passkey-recovery-verify orchestration.
///
/// Verifies the recovery token and generates a WebAuthn registration challenge
/// for replacing the user's passkey. Returns `(challenge_id, creation_challenge,
/// email)` on success, or an error message on failure.
pub async fn passkey_recovery_verify_service(
    db: &DbPool,
    kv: &KVPool,
    webauthn: &webauthn_rs::Webauthn,
    token: &str,
) -> krausen_core::Result<Result<(String, String, String), String>> {
    // Returns Ok((challenge_id, creation_challenge, email)) or Err(message)

    let email = crate::token_service::verify_verification_token(db, token, "recovery").await?;
    let Some(email) = email else {
        return Ok(Err(
            "Invalid or expired recovery link. Please request a new one.".to_string(),
        ));
    };

    let user = crate::user_service::get_user_by_email(db, &email)
        .await?
        .ok_or_else(|| krausen_core::Error::Internal("User not found for recovery token".into()))?;

    let user_unique_id = webauthn_user_id(&email);
    let display_name = user.name.as_deref().unwrap_or(&email);

    let creds = crate::user_service::get_passkey_credentials(db, &user.user_id).await?;
    let exclude_ids = build_exclude_ids(&creds);
    let exclude_opt = if exclude_ids.is_empty() {
        None
    } else {
        Some(exclude_ids)
    };

    let (ccr, reg_state) = crate::webauthn::start_registration(
        webauthn,
        user_unique_id,
        &email,
        display_name,
        exclude_opt,
    )
    .map_err(|e| krausen_core::Error::Internal(e.to_string()))?;

    let challenge_id = crate::redis_ops::generate_token();
    let reg_state_json = serde_json::to_value(&reg_state)
        .map_err(|e| krausen_core::Error::Internal(format!("Serialize reg state: {e}")))?;
    let challenge_data = serde_json::json!({
        "registration_state": reg_state_json,
        "email": email,
        "user_name": display_name,
        "user_id": user.user_id,
        "device_name": "Unknown Device",
        "is_signup": false,
    });
    crate::redis_ops::store_webauthn_challenge(kv, &challenge_id, &challenge_data).await?;

    let creation_challenge = serde_json::to_string(&ccr)
        .map_err(|e| krausen_core::Error::Internal(format!("Serialize creation challenge: {e}")))?;

    tracing::info!(
        email = %email,
        user_id = %user.user_id,
        "Passkey recovery token verified, WebAuthn challenge generated"
    );
    Ok(Ok((challenge_id, creation_challenge, email)))
}

// ---------------------------------------------------------------------------
// Rate limit helper (exposed for server_fn use in resend_verification /
// recovery_start which only need the rate-limit check and some trivial work)
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Generate a WebAuthn user handle from email (deterministic, matching the server route).
///
/// `sha256(email)[:16]` interpreted as a UUID.
pub fn webauthn_user_id(email: &str) -> webauthn_rs::prelude::Uuid {
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();
    hasher.update(email.as_bytes());
    let hash = hasher.finalize();
    let bytes: [u8; 16] = hash[..16].try_into().expect("16 bytes");
    webauthn_rs::prelude::Uuid::from_bytes(bytes)
}

/// Retrieve `Passkey` objects from stored credentials for a user.
pub async fn get_passkeys_for_user(
    db: &DbPool,
    user_id: &str,
) -> krausen_core::Result<Vec<webauthn_rs::prelude::Passkey>> {
    let creds = crate::user_service::get_passkey_credentials(db, user_id).await?;
    let mut passkeys = Vec::new();
    for (_cred_id, cred_data) in &creds {
        if let Some(passkey_json) = cred_data.get("passkey")
            && let Ok(passkey) =
                serde_json::from_value::<webauthn_rs::prelude::Passkey>(passkey_json.clone())
        {
            passkeys.push(passkey);
        }
    }
    Ok(passkeys)
}

/// Build a list of `CredentialID` values from stored credentials to exclude during registration.
fn build_exclude_ids(
    creds: &serde_json::Map<String, serde_json::Value>,
) -> Vec<webauthn_rs::prelude::CredentialID> {
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    use webauthn_rs::prelude::CredentialID;

    let mut ids = Vec::new();
    for (cred_id_b64, _) in creds {
        if let Ok(bytes) = URL_SAFE_NO_PAD.decode(cred_id_b64) {
            ids.push(CredentialID::from(bytes));
        }
    }
    ids
}

/// Update passkey credential usage after successful authentication (best-effort, fire-and-forget).
pub async fn update_passkey_after_auth_inner(
    db: &DbPool,
    user_id: &str,
    credential_id_b64: &str,
    cred_id_bytes: &[u8],
    passkeys: &[webauthn_rs::prelude::Passkey],
    auth_result: &webauthn_rs::prelude::AuthenticationResult,
) {
    let updated_passkey = passkeys.iter().find(|pk| {
        let pk_cred_id: &[u8] = pk.cred_id().as_ref();
        pk_cred_id == cred_id_bytes
    });

    if let Some(pk) = updated_passkey {
        let mut updated_pk = pk.clone();
        updated_pk.update_credential(auth_result);
        let updated_json = match serde_json::to_value(&updated_pk) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(
                    user_id = %user_id,
                    credential_id = %credential_id_b64,
                    error = %e,
                    "Failed to serialize updated passkey — skipping counter update"
                );
                return;
            }
        };
        if let Err(e) = crate::user_service::update_credential_usage(
            db,
            user_id,
            credential_id_b64,
            auth_result.counter(),
            &updated_json,
        )
        .await
        {
            tracing::warn!(
                user_id = %user_id,
                credential_id = %credential_id_b64,
                error = %e,
                "Failed to update credential usage after passkey auth"
            );
        }
    }
}

/// Result of attempting to resend a verification email.
pub struct ResendVerificationResult {
    pub should_send: bool,
    pub user_name: String,
    pub raw_token: String,
}

/// Issue another signup link for an address. Returns `None` only when rate
/// limited.
///
/// "Send it again" is the same operation as "send it" — it mints a fresh
/// [`signup_start_service`] token and nothing else. In particular it does not
/// look for an unverified `users` row to attach to, because no signup writes
/// one; it used to, which made this permanently return `None`.
///
/// No lookup means no way to tell a registered address from an unregistered
/// one by watching what comes back.
pub async fn resend_verification_service(
    db: &krausen_core::DbPool,
    kv: &KVPool,
    ip: &str,
    email: &str,
) -> krausen_core::Result<Option<ResendVerificationResult>> {
    let rate = crate::rate_limiter::check_rate_limit(kv, ip, "register", None).await?;
    if !rate.allowed {
        return Ok(None);
    }

    let raw_token =
        crate::token_service::create_verification_token(db, email, "email_verification").await?;

    Ok(Some(ResendVerificationResult {
        should_send: true,
        user_name: String::new(),
        raw_token,
    }))
}

/// Result of attempting to start account recovery.
pub struct RecoveryStartResult {
    pub user_name: String,
    pub raw_token: String,
}

/// Check rate limit, look up the verified user, and create a recovery token
/// in one service call. Returns `None` if rate-limited or user not found/unverified.
pub async fn recovery_start_service(
    db: &krausen_core::DbPool,
    kv: &KVPool,
    ip: &str,
    email: &str,
) -> krausen_core::Result<Option<RecoveryStartResult>> {
    let rate = crate::rate_limiter::check_rate_limit(kv, ip, "register", None).await?;
    if !rate.allowed {
        return Err(krausen_core::Error::BadRequest(format!(
            "Rate limited. Try again in {} seconds",
            rate.retry_after_secs
        )));
    }

    let user = crate::user_service::get_user_by_email(db, email).await?;
    let Some(user) = user else { return Ok(None) };
    if !user.verified {
        return Ok(None);
    }

    let raw_token = crate::token_service::create_verification_token_with_expiry(
        db,
        email,
        "account_recovery",
        Some(0.25),
    )
    .await?;

    Ok(Some(RecoveryStartResult {
        user_name: user.name.unwrap_or_default(),
        raw_token,
    }))
}

/// Send a verification email in a background task (fire-and-forget).
fn spawn_verification_email(email: String, name: String, url: String) {
    tokio::spawn(async move {
        let email_svc = crate::email_service::EmailService::from_env();
        let sent = email_svc.send_verification_email(&email, &name, &url).await;
        if sent {
            tracing::info!("Verification email sent to {email}");
        } else {
            tracing::warn!("Failed to send verification email to {email}");
        }
    });
}
