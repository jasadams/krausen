// SPDX-License-Identifier: AGPL-3.0-or-later

//! Signup, verification-email and existing-account-notice tests, ported from
//! Kyomi's `auth_service.rs` test module and run on SQLite and PostgreSQL.
//!
//! Every scenario is an `async fn(&DbPool)`; `both_backends!` registers it
//! twice, as `<scenario>::sqlite` and `<scenario>::postgres`.
//!
//! Isolation: each test gets its own database (an in-memory SQLite database,
//! or a freshly created PostgreSQL database that is dropped afterwards).
//! Several scenarios assert on "is there any user yet" (the first-user rule
//! for SMTP-less self-hosted signup), which a shared PostgreSQL database
//! could not answer truthfully. Every email address is additionally made
//! unique per test run by `email()`.

use std::ops::Deref;

use krausen_core::{DbPool, KVPool};
use krausen_test_tracing::capture_tracing;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use tracing::Level;

use super::*;
use crate::token_service::DeviceInfo;

// ---------------------------------------------------------------------------
// Backend harness
// ---------------------------------------------------------------------------

/// A per-test database. Derefs to the `DbPool` the services take. A
/// PostgreSQL database created for the test is dropped when this is dropped.
pub(crate) struct TestDb {
    pool: DbPool,
    postgres_cleanup: Option<(PgConnectOptions, String)>,
}

impl Deref for TestDb {
    type Target = DbPool;
    fn deref(&self) -> &DbPool {
        &self.pool
    }
}

impl Drop for TestDb {
    fn drop(&mut self) {
        let Some((admin, name)) = self.postgres_cleanup.take() else {
            return;
        };
        // Drop runs inside a runtime thread, so run the async cleanup on a
        // dedicated thread with its own runtime. `WITH (FORCE)` disconnects
        // this test's still-open pool connections.
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("cleanup runtime");
            rt.block_on(async {
                let pool = PgPoolOptions::new()
                    .max_connections(1)
                    .connect_with(admin)
                    .await
                    .expect("connect to drop test database");
                sqlx::query(&format!("DROP DATABASE IF EXISTS \"{name}\" WITH (FORCE)"))
                    .execute(&pool)
                    .await
                    .expect("drop test database");
            });
        })
        .join()
        .expect("cleanup thread");
    }
}

pub(crate) async fn sqlite_db() -> TestDb {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("open in-memory sqlite");
    sqlx::migrate!("../../apps/server/migrations-sqlite")
        .run(&pool)
        .await
        .expect("sqlite migrations");
    TestDb {
        pool: DbPool::Sqlite(pool),
        postgres_cleanup: None,
    }
}

pub(crate) async fn postgres_db() -> TestDb {
    let url =
        std::env::var("KRAUSEN_TEST_POSTGRES_URL").expect("disposable PostgreSQL URL required");
    let admin: PgConnectOptions = url.parse().expect("parse KRAUSEN_TEST_POSTGRES_URL");
    let name = format!("krausen_signup_{}", uuid::Uuid::new_v4().simple());
    let admin_pool = PgPoolOptions::new()
        .max_connections(1)
        .connect_with(admin.clone())
        .await
        .expect("connect to PostgreSQL");
    sqlx::query(&format!("CREATE DATABASE \"{name}\""))
        .execute(&admin_pool)
        .await
        .expect("create per-test database");
    admin_pool.close().await;

    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect_with(admin.clone().database(&name))
        .await
        .expect("connect to per-test database");
    sqlx::migrate!("../../apps/server/migrations")
        .run(&pool)
        .await
        .expect("postgres migrations");
    TestDb {
        pool: DbPool::Postgres(pool),
        postgres_cleanup: Some((admin, name)),
    }
}

/// Register each `async fn(&DbPool)` scenario as a SQLite and a PostgreSQL test.
macro_rules! both_backends {
    ($($scenario:ident),+ $(,)?) => {
        $(
            mod $scenario {
                #[tokio::test]
                async fn sqlite() {
                    let db = super::sqlite_db().await;
                    super::$scenario(&db).await;
                }

                #[tokio::test]
                async fn postgres() {
                    let db = super::postgres_db().await;
                    super::$scenario(&db).await;
                }
            }
        )+
    };
}
pub(crate) use both_backends;

/// A unique address per call, so no two tests (or two runs) ever share one.
pub(crate) fn email(tag: &str) -> String {
    format!("{tag}-{}@example.com", uuid::Uuid::new_v4().simple())
}

fn test_device() -> DeviceInfo {
    DeviceInfo {
        user_agent: None,
        ip_address: None,
        country_code: None,
        oauth_client_id: None,
    }
}

fn test_kv() -> KVPool {
    krausen_core::kv_store_memory::InMemoryKVStore::new_pool()
}

// ---------------------------------------------------------------------------
// Query helpers
// ---------------------------------------------------------------------------

async fn count_tokens_of_type(db: &DbPool, email: &str, token_type: &str) -> i64 {
    krausen_core::db_fetch_scalar!(
        db,
        i64,
        "SELECT COUNT(*) FROM verification_tokens WHERE email = $1 AND token_type = $2",
        email,
        token_type
    )
    .expect("count verification_tokens")
}

async fn count_all_tokens(db: &DbPool, email: &str) -> i64 {
    krausen_core::db_fetch_scalar!(
        db,
        i64,
        "SELECT COUNT(*) FROM verification_tokens WHERE email = $1",
        email
    )
    .expect("count all verification_tokens")
}

async fn count_users(db: &DbPool, email: &str) -> i64 {
    krausen_core::db_fetch_scalar!(
        db,
        i64,
        "SELECT COUNT(*) FROM users WHERE email = $1",
        email
    )
    .expect("count users")
}

async fn count_workspaces_owned_by(db: &DbPool, user_id: &str) -> i64 {
    krausen_core::db_fetch_scalar!(
        db,
        i64,
        "SELECT COUNT(*) FROM workspaces WHERE owner_user_id = $1",
        user_id
    )
    .expect("count workspaces")
}

async fn count_memberships(db: &DbPool, workspace_id: &str, user_id: &str) -> i64 {
    krausen_core::db_fetch_scalar!(
        db,
        i64,
        "SELECT COUNT(*) FROM workspace_users WHERE workspace_id = $1 AND user_id = $2",
        workspace_id,
        user_id
    )
    .expect("count workspace_users")
}

async fn mint_token(db: &DbPool, email: &str, token_type: &str) -> String {
    crate::token_service::create_verification_token(db, email, token_type)
        .await
        .expect("mint verification token")
}

/// Recover the raw token from the operator log line
/// `mint_and_send_verification_email` writes for `email`.
fn logged_tokens(logs: &krausen_test_tracing::TracingCapture, email: &str) -> Vec<String> {
    logs.events_at(Level::INFO)
        .iter()
        .filter_map(|(_, msg)| {
            if msg.contains("Verification link (email_verification)") && msg.contains(email) {
                msg.split("token=")
                    .nth(1)
                    .and_then(|rest| rest.split(' ').next())
                    .map(str::to_string)
            } else {
                None
            }
        })
        .collect()
}

fn password_start_params<'a>(
    db: &'a DbPool,
    kv: &'a KVPool,
    email: &'a str,
    device: &'a DeviceInfo,
) -> SignupStartParams<'a> {
    SignupStartParams {
        db,
        kv,
        jwt_secret: "test-secret",
        email,
        name: None,
        password: Some("correct-horse-battery-staple"),
        ip: "127.0.0.1",
        device,
        self_hosted: false,
        smtp_configured: true,
        frontend_url: "https://app.example.com",
        config: None,
    }
}

fn smtp_less_params<'a>(
    db: &'a DbPool,
    kv: &'a KVPool,
    email: &'a str,
    name: Option<&'a str>,
    device: &'a DeviceInfo,
) -> SignupStartParams<'a> {
    SignupStartParams {
        db,
        kv,
        jwt_secret: "test-secret",
        email,
        name,
        password: Some("correct-horse-battery-staple"),
        ip: "127.0.0.1",
        device,
        self_hosted: true,
        smtp_configured: false,
        frontend_url: "http://localhost:3000",
        config: None,
    }
}

fn signup_verify_params<'a>(
    db: &'a DbPool,
    kv: &'a KVPool,
    token: &'a str,
    device: &'a DeviceInfo,
    terms_accepted: bool,
) -> SignupVerifyParams<'a> {
    SignupVerifyParams {
        db,
        kv,
        jwt_secret: "test-secret",
        token,
        name: None,
        terms_accepted,
        marketing_consent: false,
        device,
        config: None,
        slack_feedback_webhook_url: None,
        support_email: "support@example.com",
    }
}

// ---------------------------------------------------------------------------
// Token-type binding
// ---------------------------------------------------------------------------

pub(crate) const TOKEN_TYPE_PASSWORD_SIGNUP: &str = "email_verification";
pub(crate) const TOKEN_TYPE_PASSWORD_RECOVERY: &str = "account_recovery";
pub(crate) const TOKEN_TYPE_PASSKEY_SIGNUP: &str = "signup";
pub(crate) const TOKEN_TYPE_PASSKEY_RECOVERY: &str = "passkey_recovery";

pub(crate) const ALL_VERIFICATION_TOKEN_TYPES: [&str; 4] = [
    TOKEN_TYPE_PASSWORD_SIGNUP,
    TOKEN_TYPE_PASSWORD_RECOVERY,
    TOKEN_TYPE_PASSKEY_SIGNUP,
    TOKEN_TYPE_PASSKEY_RECOVERY,
];

async fn signup_verify_only_accepts_its_own_token_type(db: &DbPool) {
    for &token_type in &ALL_VERIFICATION_TOKEN_TYPES {
        let kv = test_kv();
        let device = test_device();
        let email = email("signup-binding");

        crate::user_service::create_user(db, &email, None, false)
            .await
            .expect("create user");

        let token = mint_token(db, &email, token_type).await;

        let result = signup_verify_service(signup_verify_params(db, &kv, &token, &device, true))
            .await
            .expect("service call should not error");

        if token_type == TOKEN_TYPE_PASSWORD_SIGNUP {
            assert!(
                matches!(result, SignupVerifyServiceResult::Success(_)),
                "signup verify must accept its own token type ({token_type})"
            );
        } else {
            assert!(
                matches!(result, SignupVerifyServiceResult::InvalidToken),
                "signup verify must reject token type {token_type}"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// signup_start_service
// ---------------------------------------------------------------------------

/// The response for a brand-new email, an existing-but-unverified email, and
/// an existing-verified email must be indistinguishable (enumeration guard).
async fn signup_start_service_indistinguishable_across_account_states(db: &DbPool) {
    let kv = test_kv();
    let device = test_device();

    let new_email = email("signup-start-new");
    let unverified_email = email("signup-start-unverified");
    let verified_email = email("signup-start-verified");

    crate::user_service::create_user(db, &unverified_email, None, false)
        .await
        .expect("create unverified user");
    crate::user_service::create_user(db, &verified_email, Some("Verified User"), true)
        .await
        .expect("create verified user");

    for email in [&new_email, &unverified_email, &verified_email] {
        let result = signup_start_service(password_start_params(db, &kv, email, &device))
            .await
            .expect("service call should not error");

        assert!(
            matches!(result, SignupStartServiceResult::VerificationRequired),
            "expected VerificationRequired for {email} — new/unverified/verified account \
             states must return the same result (email enumeration guard)"
        );
    }
}

/// A self-hosted instance whose operator set `SMTP_HOST` and `SMTP_USER` but
/// forgot `SMTP_PASSWORD` must take the SMTP-less signup path and actually
/// create the account. The flag is computed through the same predicate
/// `Config::from_env` uses, rather than by mutating process env.
async fn smtp_less_self_hosted_signup_creates_the_account_when_the_password_is_missing(
    db: &DbPool,
) {
    let kv = test_kv();
    let device = test_device();
    let email = email("smtp-less-first-operator");

    let smtp_configured = krausen_core::config::SmtpSettings::classify(
        Some("smtp.example.com"),
        Some("mailer@example.com"),
        None,
    )
    .can_send();
    assert!(
        !smtp_configured,
        "SMTP_HOST + SMTP_USER with no SMTP_PASSWORD cannot send mail, so the config \
         flag this signup branches on must be false"
    );

    let result = signup_start_service(SignupStartParams {
        db,
        kv: &kv,
        jwt_secret: "test-secret",
        email: &email,
        name: Some("First Operator"),
        password: Some("correct-horse-battery-staple"),
        ip: "127.0.0.1",
        device: &device,
        self_hosted: true,
        smtp_configured,
        frontend_url: "http://localhost:3000",
        config: None,
    })
    .await
    .expect("service call should not error");

    match result {
        SignupStartServiceResult::AccountCreated(_) => {}
        SignupStartServiceResult::VerificationRequired => panic!(
            "signup took the SaaS verification-email branch on a self-hosted instance \
             whose mailer cannot send — the email is never delivered and no account is \
             ever created"
        ),
        SignupStartServiceResult::RateLimited { retry_after_secs } => {
            panic!("unexpectedly rate limited for {retry_after_secs}s")
        }
        SignupStartServiceResult::Error { message } => {
            panic!("SMTP-less signup rejected the first account: {message}")
        }
    }

    let user = crate::user_service::get_user_by_email(db, &email)
        .await
        .expect("lookup user")
        .expect("the SMTP-less signup path must have created a users row");
    assert!(
        user.verified,
        "an SMTP-less signup has no email to verify with, so the account must be \
         created already verified — otherwise the operator can never log in"
    );
}

/// SMTP-less self-hosted: the first user registers (AccountCreated).
async fn smtp_less_self_hosted_first_user_can_register(db: &DbPool) {
    let kv = test_kv();
    let device = test_device();
    let email = email("smtp-less-first");

    assert!(
        !crate::user_service::has_any_users(db)
            .await
            .expect("has_any_users"),
        "sanity check: the database starts with no users"
    );

    let result = signup_start_service(smtp_less_params(db, &kv, &email, Some("First"), &device))
        .await
        .expect("service call should not error");

    let session = match result {
        SignupStartServiceResult::AccountCreated(session) => session,
        SignupStartServiceResult::Error { message } => {
            panic!("the first user must be allowed to register, got Error({message})")
        }
        _ => panic!("expected AccountCreated for the first SMTP-less self-hosted user"),
    };
    assert_eq!(session.user.email, email);

    let user = crate::user_service::get_user_by_email(db, &email)
        .await
        .expect("lookup user")
        .expect("user created");
    assert!(user.verified);
    assert_eq!(
        count_workspaces_owned_by(db, &user.user_id).await,
        1,
        "the first user gets a personal workspace"
    );
}

/// SMTP-less self-hosted: a non-first user with a pending, unexpired
/// invitation for their address can register and joins the inviting workspace.
async fn smtp_less_self_hosted_invited_user_joins_the_inviting_workspace(db: &DbPool) {
    let kv = test_kv();
    let device = test_device();
    let owner_email = email("smtp-less-owner");
    let invited_email = email("smtp-less-invited");

    let first = signup_start_service(smtp_less_params(db, &kv, &owner_email, Some("Owner"), &device))
        .await
        .expect("service call should not error");
    assert!(matches!(first, SignupStartServiceResult::AccountCreated(_)));
    let owner = crate::user_service::get_user_by_email(db, &owner_email)
        .await
        .expect("lookup owner")
        .expect("owner exists");
    let workspace_id = crate::user_service::get_user_workspace_context(db, &owner.user_id)
        .await
        .expect("owner workspace context")
        .expect("owner has a workspace")
        .0
        .workspace_id;

    let invitation_id = format!("inv-{}", uuid::Uuid::new_v4());
    crate::workspace_service::create_invitation(
        db,
        &invitation_id,
        &workspace_id,
        &invited_email,
        "workspace_user",
        &owner.user_id,
        chrono::Utc::now() + chrono::Duration::days(7),
    )
    .await
    .expect("create invitation");

    let result =
        signup_start_service(smtp_less_params(db, &kv, &invited_email, Some("Invited"), &device))
            .await
            .expect("service call should not error");
    match result {
        SignupStartServiceResult::AccountCreated(_) => {}
        SignupStartServiceResult::Error { message } => {
            panic!("an invited user must be allowed to register, got Error({message})")
        }
        _ => panic!("expected AccountCreated for an invited SMTP-less self-hosted user"),
    }

    let invited = crate::user_service::get_user_by_email(db, &invited_email)
        .await
        .expect("lookup invited user")
        .expect("invited user created");
    assert_eq!(
        count_memberships(db, &workspace_id, &invited.user_id).await,
        1,
        "the invited user must be a member of the inviting workspace"
    );
    assert_eq!(
        invited.last_workspace_id.as_deref(),
        Some(workspace_id.as_str()),
        "the invited user's current workspace must be the inviting workspace"
    );
    assert_eq!(
        count_workspaces_owned_by(db, &invited.user_id).await,
        0,
        "an invited user joins the inviting workspace instead of getting their own"
    );
    let invitation = crate::workspace_service::get_invitation(db, &invitation_id)
        .await
        .expect("lookup invitation")
        .expect("invitation exists");
    assert_eq!(invitation.status.as_ref(), "accepted");
}

/// SMTP-less self-hosted: a non-first, non-invited address is refused.
async fn smtp_less_self_hosted_uninvited_user_is_refused_once_a_user_exists(db: &DbPool) {
    let kv = test_kv();
    let device = test_device();
    let owner_email = email("smtp-less-closed-owner");
    let stranger_email = email("smtp-less-closed-stranger");

    let first = signup_start_service(smtp_less_params(db, &kv, &owner_email, Some("Owner"), &device))
        .await
        .expect("service call should not error");
    assert!(matches!(first, SignupStartServiceResult::AccountCreated(_)));

    let result =
        signup_start_service(smtp_less_params(db, &kv, &stranger_email, Some("Stranger"), &device))
            .await
            .expect("service call should not error");
    match result {
        SignupStartServiceResult::Error { message } => assert_eq!(
            message,
            "Registration is closed. Ask your administrator to invite you."
        ),
        SignupStartServiceResult::AccountCreated(_) => {
            panic!("an uninvited user must not be able to register once a user exists")
        }
        _ => panic!("expected the registration-closed Error"),
    }
    assert_eq!(
        count_users(db, &stranger_email).await,
        0,
        "a refused signup must create no users row"
    );
}

/// A verified user's email must never mint a new "email_verification" token
/// when they attempt to sign up again — there is nothing to verify.
async fn signup_start_service_mints_no_token_for_verified_user(db: &DbPool) {
    let kv = test_kv();
    let device = test_device();
    let email = email("signup-start-verified-notoken");

    crate::user_service::create_user(db, &email, Some("Existing User"), true)
        .await
        .expect("create verified user");

    assert_eq!(
        count_tokens_of_type(db, &email, "email_verification").await,
        0,
        "sanity check: no verification tokens exist before the call"
    );

    let result = signup_start_service(password_start_params(db, &kv, &email, &device))
        .await
        .expect("service call should not error");

    assert!(matches!(
        result,
        SignupStartServiceResult::VerificationRequired
    ));
    assert_eq!(
        count_tokens_of_type(db, &email, "email_verification").await,
        0,
        "a verified account's email must never mint a new verification token"
    );
}

/// Security regression: signup-start for an already-verified account that has
/// a password and TOTP must mint no verification token of any type, so no
/// emailed signup link can ever overwrite that account's password or sign in
/// without the TOTP step. Covers both the SaaS path and the SMTP-less
/// self-hosted path. The previous behaviour minted a token for an existing
/// verified user and mailed a link that redeemed to a signed-in session.
async fn signup_start_service_cannot_take_over_a_verified_account_with_password_and_totp(
    db: &DbPool,
) {
    let kv = test_kv();
    let device = test_device();
    let victim_email = email("signup-takeover-victim");

    let victim = crate::user_service::create_user(db, &victim_email, Some("Victim"), true)
        .await
        .expect("create verified user");
    let original_hash = crate::password::hash_password("the-victims-own-password-1")
        .expect("hash original password");
    crate::user_service::upsert_auth_method(
        db,
        &victim.user_id,
        "password",
        &serde_json::json!({"hash": original_hash}),
    )
    .await
    .expect("seed password auth method");
    crate::user_service::upsert_auth_method(
        db,
        &victim.user_id,
        "totp",
        &serde_json::json!({"secret": "seeded-totp-secret"}),
    )
    .await
    .expect("seed totp auth method");
    assert!(
        crate::user_service::has_totp_enabled(db, &victim.user_id)
            .await
            .expect("has_totp_enabled")
    );

    let saas = SignupStartParams {
        password: Some("attacker-chosen-password-9"),
        ..password_start_params(db, &kv, &victim_email, &device)
    };
    let smtp_less = SignupStartParams {
        password: Some("attacker-chosen-password-9"),
        ..smtp_less_params(db, &kv, &victim_email, Some("Attacker"), &device)
    };

    for (label, params) in [("SaaS", saas), ("SMTP-less self-hosted", smtp_less)] {
        let result = signup_start_service(params)
            .await
            .expect("service call should not error");
        assert!(
            matches!(result, SignupStartServiceResult::VerificationRequired),
            "{label}: signup for an existing verified account must return the generic \
             VerificationRequired result, never a session (AccountCreated) or a distinct error"
        );
        assert_eq!(
            count_all_tokens(db, &victim_email).await,
            0,
            "{label}: signup for an existing verified account must mint no verification \
             token of any type, otherwise its link could take the account over"
        );
    }

    let stored = crate::user_service::get_auth_method(db, &victim.user_id, "password")
        .await
        .expect("get password auth method")
        .expect("password auth method still present");
    assert_eq!(
        stored.auth_data,
        serde_json::json!({"hash": original_hash}),
        "the victim's password must be untouched by an attacker's signup attempt"
    );
    assert!(
        crate::user_service::has_totp_enabled(db, &victim.user_id)
            .await
            .expect("has_totp_enabled"),
        "TOTP must still be enabled"
    );
    assert_eq!(count_users(db, &victim_email).await, 1);
}

/// Wiring test: the verified-user arm actually reaches the existing-account
/// notice, proven by its operator log line.
async fn signup_start_service_for_verified_user_logs_existing_account_notice(db: &DbPool) {
    let kv = test_kv();
    let device = test_device();
    let email = email("signup-start-verified-logged");

    let user = crate::user_service::create_user(db, &email, Some("Existing User"), true)
        .await
        .expect("create verified user");
    crate::user_service::upsert_auth_method(
        db,
        &user.user_id,
        "password",
        &serde_json::json!({"hash": "test-hash"}),
    )
    .await
    .expect("seed password auth method");

    let logs = capture_tracing();

    let result = signup_start_service(password_start_params(db, &kv, &email, &device))
        .await
        .expect("service call should not error");

    assert!(matches!(
        result,
        SignupStartServiceResult::VerificationRequired
    ));

    let info_events = logs.events_at(Level::INFO);
    assert!(
        info_events
            .iter()
            .any(|(_, msg)| msg.contains("existing-account notice") && msg.contains(&email)),
        "verified-user signup must log that an existing-account notice was sent \
         instead of staying silent; captured: {:?}",
        logs.events()
    );
}

/// Starting signup for a brand-new address mints and emails a token without
/// ever writing a `users` row.
async fn signup_start_service_creates_no_users_row_for_new_address(db: &DbPool) {
    let kv = test_kv();
    let device = test_device();
    let email = email("signup-start-no-row");

    let result = signup_start_service(password_start_params(db, &kv, &email, &device))
        .await
        .expect("service call should not error");

    assert!(matches!(
        result,
        SignupStartServiceResult::VerificationRequired
    ));
    assert_eq!(
        count_users(db, &email).await,
        0,
        "signup-start must never write a users row for a new address"
    );
    assert_eq!(
        count_tokens_of_type(db, &email, "email_verification").await,
        1,
        "a verification token must still be minted even though no row is created"
    );
}

// ---------------------------------------------------------------------------
// resend_verification_service
// ---------------------------------------------------------------------------

/// Resending during the pre-redemption window (no `users` row yet) must mint
/// and email a second, distinct token rather than silently doing nothing.
async fn resend_verification_mints_a_token_for_pending_signup_and_creates_no_user_row(
    db: &DbPool,
) {
    let kv = test_kv();
    let device = test_device();
    let email = email("resend-pending-signup");

    let logs = capture_tracing();

    let start_result = signup_start_service(password_start_params(db, &kv, &email, &device))
        .await
        .expect("service call should not error");
    assert!(matches!(
        start_result,
        SignupStartServiceResult::VerificationRequired
    ));

    resend_verification_service(db, &kv, "https://app.example.com", "127.0.0.1", &email, false)
        .await
        .expect("resend should not error");

    assert_eq!(
        count_users(db, &email).await,
        0,
        "resending during the pre-redemption window must not create a users row"
    );
    assert_eq!(
        count_tokens_of_type(db, &email, "email_verification").await,
        2,
        "signup-start and resend must each mint their own token"
    );

    let tokens = logged_tokens(&logs, &email);
    assert_eq!(
        tokens.len(),
        2,
        "expected exactly two minted-token log lines (signup-start + resend); captured: {:?}",
        logs.events()
    );
    assert_ne!(
        tokens[0], tokens[1],
        "resend must mint a fresh token, not reuse the original"
    );
}

/// Resending for an address that already has a verified account mints no token.
async fn resend_verification_mints_no_token_for_verified_user(db: &DbPool) {
    let kv = test_kv();
    let email = email("resend-verified-notoken");

    crate::user_service::create_user(db, &email, Some("Existing User"), true)
        .await
        .expect("create verified user");

    resend_verification_service(db, &kv, "https://app.example.com", "127.0.0.1", &email, false)
        .await
        .expect("resend should not error");

    assert_eq!(
        count_tokens_of_type(db, &email, "email_verification").await,
        0,
        "resending for a verified account must never mint a new verification token"
    );
}

// ---------------------------------------------------------------------------
// signup_verify_service / establish_verified_account
// ---------------------------------------------------------------------------

/// A valid redemption creates the account verified, records terms acceptance,
/// ensures a workspace, and returns an authenticated session.
async fn signup_verify_service_establishes_new_verified_account_with_workspace_and_terms(
    db: &DbPool,
) {
    let kv = test_kv();
    let device = test_device();
    let email = email("signup-verify-new");

    let token = mint_token(db, &email, "email_verification").await;

    let result = signup_verify_service(signup_verify_params(db, &kv, &token, &device, true))
        .await
        .expect("service call should not error");

    let session = match result {
        SignupVerifyServiceResult::Success(session) => session,
        SignupVerifyServiceResult::InvalidToken => panic!("expected Success, got InvalidToken"),
        SignupVerifyServiceResult::Error { message } => {
            panic!("expected Success, got Error({message})")
        }
    };
    assert_eq!(session.user.email, email);

    let user = crate::user_service::get_user_by_email(db, &email)
        .await
        .expect("lookup user")
        .expect("user must have been created");
    assert!(
        user.verified,
        "establish_verified_account must leave the row verified"
    );
    assert!(
        user.terms_accepted_at.is_some(),
        "terms acceptance must be recorded"
    );

    let ws_ctx = crate::user_service::get_user_workspace_context(db, &user.user_id)
        .await
        .expect("lookup workspace context");
    assert!(
        ws_ctx.is_some(),
        "establish_verified_account must ensure a workspace exists"
    );
}

/// Minting alone creates no account; the first redemption succeeds and the
/// same token can never be redeemed again.
async fn signup_verify_service_is_single_use_and_minting_alone_creates_no_user(db: &DbPool) {
    let kv = test_kv();
    let device = test_device();
    let email = email("signup-verify-mail-scanner");

    let token = mint_token(db, &email, "email_verification").await;

    assert_eq!(
        count_users(db, &email).await,
        0,
        "minting a verification token must not create a user"
    );

    let first = signup_verify_service(signup_verify_params(db, &kv, &token, &device, true))
        .await
        .expect("service call should not error");
    assert!(matches!(first, SignupVerifyServiceResult::Success(_)));
    assert_eq!(count_users(db, &email).await, 1);

    let second = signup_verify_service(signup_verify_params(db, &kv, &token, &device, true))
        .await
        .expect("service call should not error");
    assert!(
        matches!(second, SignupVerifyServiceResult::InvalidToken),
        "a redeemed signup token must be single-use"
    );
    assert_eq!(
        count_users(db, &email).await,
        1,
        "a rejected re-redemption must not create a second user"
    );
}

/// Terms rejection creates nothing and leaves the token usable.
async fn signup_verify_service_rejects_missing_terms_and_leaves_token_usable(db: &DbPool) {
    let kv = test_kv();
    let device = test_device();
    let email = email("signup-verify-no-terms");

    let token = mint_token(db, &email, "email_verification").await;

    let rejected = signup_verify_service(signup_verify_params(db, &kv, &token, &device, false))
        .await
        .expect("service call should not error");
    assert!(matches!(rejected, SignupVerifyServiceResult::Error { .. }));
    assert_eq!(
        count_users(db, &email).await,
        0,
        "a terms-rejected call must create nothing"
    );

    let accepted = signup_verify_service(signup_verify_params(db, &kv, &token, &device, true))
        .await
        .expect("service call should not error");
    assert!(
        matches!(accepted, SignupVerifyServiceResult::Success(_)),
        "the token must still be usable after an earlier terms-rejected call"
    );
}

/// Redeeming adopts an existing row rather than failing on the unique email.
async fn signup_verify_service_adopts_existing_row_instead_of_erroring_on_unique_constraint(
    db: &DbPool,
) {
    let kv = test_kv();
    let device = test_device();
    let email = email("signup-verify-adopt");

    let existing = crate::user_service::create_user(db, &email, Some("Existing Name"), false)
        .await
        .expect("seed pre-existing row");

    let token = mint_token(db, &email, "email_verification").await;

    let result = signup_verify_service(signup_verify_params(db, &kv, &token, &device, true))
        .await
        .expect("service call must not error — adopting must not hit the unique constraint");

    assert!(matches!(result, SignupVerifyServiceResult::Success(_)));
    assert_eq!(
        count_users(db, &email).await,
        1,
        "adopting an existing row must not create a second one"
    );

    let user = crate::user_service::get_user_by_email(db, &email)
        .await
        .expect("lookup user")
        .expect("user still exists");
    assert_eq!(
        user.user_id, existing.user_id,
        "the adopted row must be the same one that pre-existed, not a new one"
    );
    assert!(user.verified, "the adopted row must end up verified");
}

/// The adopt branch must not create a second workspace or repoint
/// `last_workspace_id` for a user who already has one.
async fn signup_verify_service_does_not_duplicate_workspace_on_adopt(db: &DbPool) {
    let kv = test_kv();
    let device = test_device();
    let email = email("signup-verify-adopt-workspace");

    let user = crate::user_service::create_user(db, &email, Some("Existing Name"), true)
        .await
        .expect("seed pre-existing verified user");
    let original_workspace_id = crate::user_service::create_workspace_for_user(
        db,
        &user.user_id,
        Some("Existing Name"),
        &email,
        None,
    )
    .await
    .expect("seed pre-existing workspace");

    let token = mint_token(db, &email, "email_verification").await;

    let result = signup_verify_service(signup_verify_params(db, &kv, &token, &device, true))
        .await
        .expect("service call should not error");
    assert!(matches!(result, SignupVerifyServiceResult::Success(_)));

    assert_eq!(
        count_workspaces_owned_by(db, &user.user_id).await,
        1,
        "redeeming a token for an already-onboarded user must not create a second workspace"
    );

    let refreshed = crate::user_service::get_user_by_id(db, &user.user_id)
        .await
        .expect("lookup user")
        .expect("user still exists");
    assert_eq!(
        refreshed.last_workspace_id.as_deref(),
        Some(original_workspace_id.as_str()),
        "the adopt path must not repoint last_workspace_id at a newly created workspace"
    );
}

/// A token minted by `signup_start_service` is redeemable end-to-end.
async fn signup_verify_service_still_works_end_to_end_after_signup_start(db: &DbPool) {
    let kv = test_kv();
    let device = test_device();
    let email = email("signup-verify-e2e");

    let logs = capture_tracing();
    let start_result = signup_start_service(password_start_params(db, &kv, &email, &device))
        .await
        .expect("service call should not error");
    assert!(matches!(
        start_result,
        SignupStartServiceResult::VerificationRequired
    ));
    assert_eq!(
        count_users(db, &email).await,
        0,
        "sanity check: signup-start must not have created a row"
    );

    let tokens = logged_tokens(&logs, &email);
    let token = tokens
        .first()
        .expect("mint_and_send_verification_email must log the token URL");

    let verify_result = signup_verify_service(signup_verify_params(db, &kv, token, &device, true))
        .await
        .expect("service call should not error");

    match verify_result {
        SignupVerifyServiceResult::Success(_) => {}
        SignupVerifyServiceResult::InvalidToken => {
            panic!(
                "expected Success, got InvalidToken — signup_verify_service can no \
                 longer redeem a token minted by signup_start_service"
            )
        }
        SignupVerifyServiceResult::Error { message } => {
            panic!("expected Success, got Error({message})")
        }
    }

    let user = crate::user_service::get_user_by_email(db, &email)
        .await
        .expect("lookup user")
        .expect("user must have been created");
    assert!(user.verified);
}

/// `list_active_auth_types` returns exactly the seeded, active methods.
async fn list_active_auth_types_reflects_seeded_and_removed_methods(db: &DbPool) {
    let email = email("signup-start-auth-methods");

    let user = crate::user_service::create_user(db, &email, Some("Verified User"), true)
        .await
        .expect("create verified user");

    crate::user_service::upsert_auth_method(
        db,
        &user.user_id,
        "password",
        &serde_json::json!({"hash": "test-hash"}),
    )
    .await
    .expect("seed password auth method");
    crate::user_service::upsert_auth_method(
        db,
        &user.user_id,
        "google_oauth",
        &serde_json::json!({"google_id": "test-google-id"}),
    )
    .await
    .expect("seed google_oauth auth method");

    let methods = crate::user_service::list_active_auth_types(db, &user.user_id)
        .await
        .expect("list_active_auth_types should not error");
    assert_eq!(
        methods,
        vec!["google_oauth".to_string(), "password".to_string()],
        "must reflect exactly the seeded active auth methods"
    );

    crate::user_service::remove_auth_method(db, &user.user_id, "google_oauth")
        .await
        .expect("remove google_oauth auth method");

    let methods_after_removal = crate::user_service::list_active_auth_types(db, &user.user_id)
        .await
        .expect("list_active_auth_types should not error");
    assert_eq!(
        methods_after_removal,
        vec!["password".to_string()],
        "a removed (inactive) auth method must not be reported"
    );
}

// ---------------------------------------------------------------------------
// Verification-email failure reporting
// ---------------------------------------------------------------------------

/// A send failure arrives at the caller as a named reason, for every kind.
#[tokio::test]
async fn verification_email_failure_is_returned_for_every_kind() {
    let svc = crate::email_service::EmailService::unconfigured_for_tests();

    for kind in [
        VerificationEmailKind::Verification,
        VerificationEmailKind::ExistingAccount {
            auth_methods: vec!["password".to_string()],
        },
    ] {
        let err = send_verification_email_of_kind(
            &svc,
            "user@example.com",
            "Jane",
            "https://app.example.com/signup/complete?token=abc",
            &kind,
        )
        .await
        .expect_err("an unconfigured SMTP service must not report a send as successful");

        assert!(
            matches!(err, crate::email_service::EmailSendError::NotConfigured),
            "{}: expected the NotConfigured reason to survive, got {err:?}",
            kind.as_str()
        );
    }
}

/// The failure is reported at `error` level with its reason, kind and recipient.
#[tokio::test]
async fn verification_email_failure_is_reported_with_its_reason() {
    let logs = capture_tracing();
    let svc = crate::email_service::EmailService::unconfigured_for_tests();
    let kind = VerificationEmailKind::Verification;

    let result = send_verification_email_of_kind(
        &svc,
        "user@example.com",
        "Jane",
        "https://app.example.com/signup/complete?token=abc",
        &kind,
    )
    .await;
    report_verification_email_outcome(&kind, "user@example.com", &result);

    let errors = logs.events_at(Level::ERROR);
    assert_eq!(
        errors.len(),
        1,
        "exactly one error must be reported for one failed send, got: {errors:?}"
    );
    let (_, rendered) = &errors[0];
    assert!(
        rendered.contains("Failed to send verification email"),
        "the message operators grep for must be present: {rendered}"
    );
    assert!(
        rendered.contains("SMTP_HOST"),
        "the reason must be in the log line, not collapsed away: {rendered}"
    );
    assert!(
        rendered.contains(r#"email_kind="verification""#),
        "the log must say which email failed: {rendered}"
    );
    assert!(
        rendered.contains("user@example.com"),
        "the log must say who it was for: {rendered}"
    );
}

/// A successful send must not report an error.
#[test]
fn verification_email_success_reports_no_error() {
    let logs = capture_tracing();
    report_verification_email_outcome(
        &VerificationEmailKind::Verification,
        "user@example.com",
        &Ok(()),
    );

    assert!(
        logs.events_at(Level::ERROR).is_empty(),
        "a successful send must not be reported as a failure: {:?}",
        logs.events_at(Level::ERROR)
    );
    assert!(
        logs.has_message_containing(Level::INFO, "Verification email sent"),
        "a successful send must still be reported: {:?}",
        logs.events()
    );
}

// ---------------------------------------------------------------------------
// may_log_live_credential_link / mint_and_send_verification_email
// ---------------------------------------------------------------------------

#[test]
fn may_log_live_credential_link_truth_table() {
    assert!(
        !may_log_live_credential_link(false, false),
        "SaaS production (neither self-hosted nor a dev build) must never log the live token"
    );
    assert!(
        may_log_live_credential_link(true, false),
        "self-hosted keeps the line regardless of build profile"
    );
    assert!(
        may_log_live_credential_link(false, true),
        "a dev build keeps the line regardless of self_hosted"
    );
    assert!(
        may_log_live_credential_link(true, true),
        "self-hosted dev build: both reasons apply, still must log"
    );
}

fn mint_params<'a>(
    db: &'a DbPool,
    email: &'a str,
    frontend_url: &'a str,
    self_hosted: bool,
    dev_mode: bool,
) -> MintVerificationEmailParams<'a> {
    MintVerificationEmailParams {
        db,
        email,
        name: "Jane",
        user_id: None,
        frontend_url,
        self_hosted,
        dev_mode,
        token_type: "email_verification",
        verification_path: "/signup/complete",
        expire_hours: None,
        email_kind: VerificationEmailKind::Verification,
    }
}

/// SaaS production (`self_hosted: false, dev_mode: false`) must never log the
/// live link in any event, while still minting the token.
async fn mint_and_send_verification_email_omits_the_link_in_saas_production(db: &DbPool) {
    let email = email("saas-production");
    let frontend_url = "https://app.example.com";

    let logs = capture_tracing();

    mint_and_send_verification_email(mint_params(db, &email, frontend_url, false, false))
        .await
        .expect("mint should not error");

    assert_eq!(
        count_tokens_of_type(db, &email, "email_verification").await,
        1,
        "the token must still be minted — only the log line is affected"
    );

    for (level, message) in logs.events() {
        assert!(
            !message.contains(frontend_url) && !message.contains("token="),
            "SaaS production (self_hosted: false, dev_mode: false) must never log the \
             live verification link in any event, got a {level:?} event: {message}"
        );
    }
}

/// `self_hosted: true` still logs the link.
async fn mint_and_send_verification_email_logs_the_link_when_self_hosted(db: &DbPool) {
    let email = email("self-hosted");
    let frontend_url = "https://app.example.com";

    let logs = capture_tracing();

    mint_and_send_verification_email(mint_params(db, &email, frontend_url, true, false))
        .await
        .expect("mint should not error");

    assert!(
        logs.events_at(Level::INFO)
            .iter()
            .any(|(_, msg)| msg.contains("Verification link (email_verification)")
                && msg.contains(frontend_url)
                && msg.contains(&email)),
        "a self-hosted deployment must still get the full link in its own logs; captured: {:?}",
        logs.events()
    );
}

/// `dev_mode: true` still logs the link in SaaS-mode config.
async fn mint_and_send_verification_email_logs_the_link_in_dev_mode(db: &DbPool) {
    let email = email("dev-mode");
    let frontend_url = "https://app.example.com";

    let logs = capture_tracing();

    mint_and_send_verification_email(mint_params(db, &email, frontend_url, false, true))
        .await
        .expect("mint should not error");

    assert!(
        logs.events_at(Level::INFO)
            .iter()
            .any(|(_, msg)| msg.contains("Verification link (email_verification)")
                && msg.contains(frontend_url)
                && msg.contains(&email)),
        "a locally run dev-server-profile build must still get the full link; captured: {:?}",
        logs.events()
    );
}

// ---------------------------------------------------------------------------
// workspace_service behaviour
// ---------------------------------------------------------------------------

/// Seed an owner with a workspace and a pending invitation for a new address.
async fn seed_invitation(db: &DbPool) -> (String, String, String, String) {
    let owner = crate::user_service::create_user(db, &email("ws-owner"), Some("Owner"), true)
        .await
        .expect("create owner");
    let workspace_id = crate::user_service::create_workspace_for_user(
        db,
        &owner.user_id,
        Some("Owner"),
        &owner.email,
        None,
    )
    .await
    .expect("create workspace");
    let invitee = crate::user_service::create_user(db, &email("ws-invitee"), Some("Invitee"), true)
        .await
        .expect("create invitee");
    let invitation_id = format!("inv-{}", uuid::Uuid::new_v4());
    crate::workspace_service::create_invitation(
        db,
        &invitation_id,
        &workspace_id,
        &invitee.email,
        "workspace_user",
        &owner.user_id,
        chrono::Utc::now() + chrono::Duration::days(7),
    )
    .await
    .expect("create invitation");
    (owner.user_id, workspace_id, invitee.user_id, invitation_id)
}

async fn accept_invitation_for_user_conflicts_the_second_time_and_adds_no_duplicate_membership(
    db: &DbPool,
) {
    let (_owner, workspace_id, invitee, invitation_id) = seed_invitation(db).await;

    crate::workspace_service::accept_invitation_for_user(db, &invitation_id, &invitee)
        .await
        .expect("first accept succeeds");
    assert_eq!(count_memberships(db, &workspace_id, &invitee).await, 1);

    let second =
        crate::workspace_service::accept_invitation_for_user(db, &invitation_id, &invitee).await;
    match second {
        Err(krausen_core::Error::Conflict(message)) => {
            assert_eq!(message, "Invitation already accepted")
        }
        other => panic!("expected Conflict(\"Invitation already accepted\"), got {other:?}"),
    }
    assert_eq!(
        count_memberships(db, &workspace_id, &invitee).await,
        1,
        "a second accept must not create a duplicate membership"
    );
}

async fn create_workspace_user_is_idempotent(db: &DbPool) {
    let owner = crate::user_service::create_user(db, &email("ws-idem-owner"), Some("Owner"), true)
        .await
        .expect("create owner");
    let workspace_id = crate::user_service::create_workspace_for_user(
        db,
        &owner.user_id,
        Some("Owner"),
        &owner.email,
        None,
    )
    .await
    .expect("create workspace");
    let member = crate::user_service::create_user(db, &email("ws-idem-member"), Some("Member"), true)
        .await
        .expect("create member");

    for _ in 0..2 {
        crate::workspace_service::create_workspace_user(
            db,
            &workspace_id,
            &member.user_id,
            "workspace_user",
        )
        .await
        .expect("create_workspace_user must not error on a repeat (ON CONFLICT DO NOTHING)");
    }
    assert_eq!(
        count_memberships(db, &workspace_id, &member.user_id).await,
        1,
        "repeated create_workspace_user must leave exactly one membership"
    );

    // The existing membership is left as it was, not overwritten.
    crate::workspace_service::create_workspace_user(
        db,
        &workspace_id,
        &owner.user_id,
        "workspace_user",
    )
    .await
    .expect("repeat for the owner must not error");
    let role: String = krausen_core::db_fetch_scalar!(
        db,
        String,
        "SELECT role FROM workspace_users WHERE workspace_id = $1 AND user_id = $2",
        &workspace_id,
        &owner.user_id
    )
    .expect("fetch owner role");
    assert_eq!(
        role, "workspace_admin",
        "ON CONFLICT DO NOTHING must not change an existing membership"
    );
}

/// Signup and resend each enforce the per-IP `register` quota: past it, signup
/// answers `RateLimited` and resend sends nothing and mints no token.
async fn signup_and_resend_are_rate_limited_per_ip(db: &DbPool) {
    let kv = krausen_core::kv_store_memory::InMemoryKVStore::new_pool();
    let device = test_device();
    let capacity = krausen_core::constants::get().rate_limits.register.ip_capacity;

    for n in 0..capacity {
        let e = email(&format!("rate-{n}"));
        let result = signup_start_service(password_start_params(db, &kv, &e, &device))
            .await
            .expect("service call should not error");
        assert!(
            matches!(result, SignupStartServiceResult::VerificationRequired),
            "request {n} is inside the quota"
        );
    }

    let over = email("rate-over");
    let result = signup_start_service(password_start_params(db, &kv, &over, &device))
        .await
        .expect("service call should not error");
    assert!(
        matches!(result, SignupStartServiceResult::RateLimited { .. }),
        "the request past the quota must be rate limited"
    );
    assert_eq!(count_tokens_of_type(db, &over, "email_verification").await, 0);

    // Resend has its own `register` bucket (as in Kyomi): use its quota, and the
    // request past it sends nothing and mints no token.
    for n in 0..capacity {
        let e = email(&format!("resend-{n}"));
        resend_verification_service(db, &kv, "https://app.example.com", "127.0.0.1", &e, false)
            .await
            .expect("resend inside the quota");
        assert_eq!(count_tokens_of_type(db, &e, "email_verification").await, 1);
    }
    let resend_email = email("rate-resend");
    resend_verification_service(
        db,
        &kv,
        "https://app.example.com",
        "127.0.0.1",
        &resend_email,
        false,
    )
    .await
    .expect("resend never errors on a rate limit");
    assert_eq!(
        count_tokens_of_type(db, &resend_email, "email_verification").await,
        0,
        "a rate-limited resend must mint no token"
    );
}

/// `get_pending_invitations_for_email` hides invitations whose expiry has
/// passed (an explicit owner exception to Kyomi, which lists them).
async fn pending_invitations_exclude_expired_ones(db: &DbPool) {
    let inviter_email = email("inviter");
    let inviter = crate::user_service::create_user(db, &inviter_email, Some("Inviter"), true)
        .await
        .expect("create inviter");
    let workspace_id =
        crate::user_service::create_workspace_for_user(db, &inviter.user_id, Some("Inviter"), &inviter_email, None)
            .await
            .expect("create workspace");
    let invited = email("invited");

    let live = format!("inv-live-{}", uuid::Uuid::new_v4());
    let stale = format!("inv-stale-{}", uuid::Uuid::new_v4());
    for (id, expires) in [
        (&live, chrono::Utc::now() + chrono::Duration::days(7)),
        (&stale, chrono::Utc::now() - chrono::Duration::days(1)),
    ] {
        crate::workspace_service::create_invitation(
            db,
            id,
            &workspace_id,
            &invited,
            "workspace_user",
            &inviter.user_id,
            expires,
        )
        .await
        .expect("create invitation");
    }

    let pending = crate::workspace_service::get_pending_invitations_for_email(db, &invited)
        .await
        .expect("list pending");
    let ids: Vec<&str> = pending.iter().map(|i| i.invitation_id.as_str()).collect();
    assert_eq!(ids, vec![live.as_str()], "only the unexpired invitation is pending");
}

both_backends!(
    signup_verify_only_accepts_its_own_token_type,
    signup_start_service_indistinguishable_across_account_states,
    smtp_less_self_hosted_signup_creates_the_account_when_the_password_is_missing,
    smtp_less_self_hosted_first_user_can_register,
    smtp_less_self_hosted_invited_user_joins_the_inviting_workspace,
    smtp_less_self_hosted_uninvited_user_is_refused_once_a_user_exists,
    signup_start_service_mints_no_token_for_verified_user,
    signup_start_service_cannot_take_over_a_verified_account_with_password_and_totp,
    signup_start_service_for_verified_user_logs_existing_account_notice,
    signup_start_service_creates_no_users_row_for_new_address,
    resend_verification_mints_a_token_for_pending_signup_and_creates_no_user_row,
    resend_verification_mints_no_token_for_verified_user,
    signup_verify_service_establishes_new_verified_account_with_workspace_and_terms,
    signup_verify_service_is_single_use_and_minting_alone_creates_no_user,
    signup_verify_service_rejects_missing_terms_and_leaves_token_usable,
    signup_verify_service_adopts_existing_row_instead_of_erroring_on_unique_constraint,
    signup_verify_service_does_not_duplicate_workspace_on_adopt,
    signup_verify_service_still_works_end_to_end_after_signup_start,
    list_active_auth_types_reflects_seeded_and_removed_methods,
    mint_and_send_verification_email_omits_the_link_in_saas_production,
    mint_and_send_verification_email_logs_the_link_when_self_hosted,
    mint_and_send_verification_email_logs_the_link_in_dev_mode,
    accept_invitation_for_user_conflicts_the_second_time_and_adds_no_duplicate_membership,
    create_workspace_user_is_idempotent,
    signup_and_resend_are_rate_limited_per_ip,
    pending_invitations_exclude_expired_ones,
);
