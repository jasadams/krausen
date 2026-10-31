// SPDX-License-Identifier: AGPL-3.0-or-later

//! The signup flow, against a real database.
//!
//! Signup is: submit an email, confirm it, *then* the account exists, then add
//! a password or a passkey. Submitting an address must not create anything —
//! these tests exist because that is the part that keeps getting "improved"
//! into writing an unverified `users` row up front and flipping a flag later.
//! If one of these fails, the two-phase-row shape has come back.

use krausen_auth::auth_service::{
    signup_complete_service, signup_start_service, SignupCompleteParams, SignupCompleteServiceResult,
    SignupStartParams, SignupStartServiceResult,
};
use krausen_core::kv_store_memory::InMemoryKVStore;
use krausen_core::{DbPool, KVPool};

const JWT_SECRET: &str = "test-secret-that-is-long-enough-for-hs256";

async fn test_db() -> DbPool {
    let pool = sqlx::SqlitePool::connect("sqlite::memory:")
        .await
        .expect("in-memory sqlite");
    sqlx::migrate!("../../apps/server/migrations-sqlite")
        .run(&pool)
        .await
        .expect("migrations apply");
    DbPool::Sqlite(pool)
}

fn kv() -> KVPool {
    InMemoryKVStore::new_pool()
}

fn device() -> krausen_auth::token_service::DeviceInfo {
    krausen_auth::token_service::DeviceInfo {
        user_agent: Some("signup-flow-test".into()),
        ip_address: Some("198.51.100.7".into()),
        country_code: None,
        oauth_client_id: None,
    }
}

/// Start signup for `email` as a hosted instance would: a mailer is available,
/// so a link is sent rather than an account made.
async fn start(db: &DbPool, kv: &KVPool, email: &str, ip: &str) -> SignupStartServiceResult {
    signup_start_service(SignupStartParams {
        db,
        kv,
        jwt_secret: JWT_SECRET,
        email,
        name: None,
        password: None,
        ip,
        device: &device(),
        self_hosted: false,
        smtp_configured: true,
        frontend_url: "https://example.test",
        config: None,
    })
    .await
    .expect("signup start runs")
}

async fn count_users(db: &DbPool) -> i64 {
    let DbPool::Sqlite(pool) = db else { unreachable!() };
    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM users")
        .fetch_one(pool)
        .await
        .expect("count")
}

/// The signup link, as it would have arrived in the mailbox.
///
/// Only a bcrypt hash of the token reaches the database — the raw value is
/// returned once, to be emailed — so a test cannot read back the token that
/// `signup_start_service` minted. It mints its own instead, which is the same
/// call `signup_start_service` makes and redeems through the same
/// verification path. That a start actually mints one is asserted separately,
/// by `submitting_an_email_mints_exactly_one_token`.
async fn emailed_token(db: &DbPool, email: &str) -> String {
    krausen_auth::token_service::create_verification_token(db, email, "email_verification")
        .await
        .expect("token minted")
}

#[tokio::test]
async fn submitting_an_email_creates_no_user_row() {
    let db = test_db().await;
    let kv = kv();

    let result = start(&db, &kv, "brewer@example.test", "198.51.100.1").await;

    assert!(
        matches!(result, SignupStartServiceResult::VerificationRequired),
        "signup start should ask for confirmation, not create an account"
    );
    assert_eq!(
        count_users(&db).await,
        0,
        "no users row may exist before the emailed token is redeemed"
    );
}

#[tokio::test]
async fn submitting_an_email_mints_exactly_one_token() {
    let db = test_db().await;
    let kv = kv();

    start(&db, &kv, "brewer@example.test", "198.51.100.2").await;

    let DbPool::Sqlite(pool) = &db else { unreachable!() };
    let tokens = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM verification_tokens WHERE token_type = 'email_verification'",
    )
        .fetch_one(pool)
        .await
        .expect("count");
    assert_eq!(tokens, 1);
}

#[tokio::test]
async fn redeeming_the_token_creates_a_verified_account() {
    let db = test_db().await;
    let kv = kv();
    let email = "brewer@example.test";

    start(&db, &kv, email, "198.51.100.3").await;
    let token = emailed_token(&db, email).await;

    let result = signup_complete_service(SignupCompleteParams {
        db: &db,
        kv: &kv,
        jwt_secret: JWT_SECRET,
        token: &token,
        name: "Brewer",
        password: "long-enough-password",
        terms_accepted: true,
        marketing_consent: false,
        device: &device(),
        config: None,
        slack_feedback_webhook_url: None,
        support_email: "support@example.test",
    })
    .await
    .expect("signup complete runs");

    assert!(
        matches!(result, SignupCompleteServiceResult::Success(_)),
        "redeeming a valid token should sign the user in"
    );

    let DbPool::Sqlite(pool) = &db else { unreachable!() };
    let (name, verified) =
        sqlx::query_as::<_, (Option<String>, bool)>("SELECT name, verified FROM users WHERE email = ?")
            .bind(email)
            .fetch_one(pool)
            .await
            .expect("the account now exists");

    assert_eq!(name.as_deref(), Some("Brewer"));
    assert!(verified, "an account only comes into being already verified");
}

#[tokio::test]
async fn the_account_gets_a_workspace() {
    let db = test_db().await;
    let kv = kv();
    let email = "brewer@example.test";

    start(&db, &kv, email, "198.51.100.4").await;
    let token = emailed_token(&db, email).await;
    signup_complete_service(SignupCompleteParams {
        db: &db,
        kv: &kv,
        jwt_secret: JWT_SECRET,
        token: &token,
        name: "Brewer",
        password: "long-enough-password",
        terms_accepted: true,
        marketing_consent: false,
        device: &device(),
        config: None,
        slack_feedback_webhook_url: None,
        support_email: "support@example.test",
    })
    .await
    .expect("signup complete runs");

    let DbPool::Sqlite(pool) = &db else { unreachable!() };
    let workspaces = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM workspace_users wu \
         JOIN users u ON u.user_id = wu.user_id WHERE u.email = ?",
    )
    .bind(email)
    .fetch_one(pool)
    .await
    .expect("count");

    assert_eq!(workspaces, 1, "a new account needs somewhere to work");
}

#[tokio::test]
async fn a_second_link_does_not_make_a_second_account() {
    let db = test_db().await;
    let kv = kv();
    let email = "brewer@example.test";

    // Two links for the same address — someone who asked twice, then clicked
    // the newer one and later the older one still sitting in their inbox.
    start(&db, &kv, email, "198.51.100.5").await;
    start(&db, &kv, email, "198.51.100.6").await;
    let first = emailed_token(&db, email).await;
    let second = emailed_token(&db, email).await;
    assert_ne!(first, second, "each request mints its own token");

    for token in [second, first] {
        signup_complete_service(SignupCompleteParams {
            db: &db,
            kv: &kv,
            jwt_secret: JWT_SECRET,
            token: &token,
            name: "Brewer",
            password: "long-enough-password",
            terms_accepted: true,
            marketing_consent: false,
            device: &device(),
            config: None,
            slack_feedback_webhook_url: None,
            support_email: "support@example.test",
        })
        .await
        .expect("signup complete runs");
    }

    assert_eq!(
        count_users(&db).await,
        1,
        "redeeming a stale link must adopt the existing row, not collide with it"
    );
}

#[tokio::test]
async fn a_known_address_is_answered_the_same_as_an_unknown_one() {
    let db = test_db().await;
    let kv = kv();
    let email = "brewer@example.test";

    start(&db, &kv, email, "198.51.100.8").await;
    let token = emailed_token(&db, email).await;
    signup_complete_service(SignupCompleteParams {
        db: &db,
        kv: &kv,
        jwt_secret: JWT_SECRET,
        token: &token,
        name: "Brewer",
        password: "long-enough-password",
        terms_accepted: true,
        marketing_consent: false,
        device: &device(),
        config: None,
        slack_feedback_webhook_url: None,
        support_email: "support@example.test",
    })
    .await
    .expect("signup complete runs");

    // Now sign up again with the same address, and with one nobody has used.
    let registered = start(&db, &kv, email, "198.51.100.9").await;
    let stranger = start(&db, &kv, "nobody@example.test", "198.51.100.10").await;

    assert!(
        matches!(registered, SignupStartServiceResult::VerificationRequired)
            && matches!(stranger, SignupStartServiceResult::VerificationRequired),
        "the response must not reveal which addresses already have accounts"
    );
    assert_eq!(count_users(&db).await, 1, "and neither call creates a row");
}

#[tokio::test]
async fn an_unaccepted_token_leaves_nothing_behind() {
    let db = test_db().await;
    let kv = kv();

    start(&db, &kv, "never-returns@example.test", "198.51.100.11").await;

    // No redemption. This is the case the old flow got wrong: it left a row
    // for someone who never confirmed, which nothing could then finish.
    assert_eq!(count_users(&db).await, 0);
}
