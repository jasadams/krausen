// SPDX-License-Identifier: AGPL-3.0-or-later

//! Passkey login, passkey recovery, account recovery and recovery-start tests,
//! ported from Kyomi's `auth_service.rs` test module and run on SQLite and
//! PostgreSQL.
//!
//! Every scenario is an `async fn(&DbPool)`; `both_backends!` registers it
//! twice, as `<scenario>::sqlite` and `<scenario>::postgres`. Each test gets
//! its own database, and every email address, user id, challenge id and
//! refresh token is made unique per test run.
//!
//! The WebAuthn fixtures are structurally valid captures from
//! `webauthn-rs-core`'s own test fixtures: real JSON shape, cryptographically
//! inert against this test RP. A request that clears the gate under test
//! therefore always fails *later*, in real WebAuthn verification, which is
//! what proves the gate specifically did not block it.

use axum::extract::FromRequestParts;
use krausen_core::{DbPool, KVPool};
use krausen_test_tracing::capture_tracing;
use tracing::Level;
use url::Url;
use webauthn_rs::prelude::Uuid;

use super::*;
use crate::auth_service::signup_tests::{
    ALL_VERIFICATION_TOKEN_TYPES, TOKEN_TYPE_PASSKEY_RECOVERY, TOKEN_TYPE_PASSKEY_SIGNUP,
    TOKEN_TYPE_PASSWORD_RECOVERY, TOKEN_TYPE_PASSWORD_SIGNUP, both_backends, email, postgres_db,
    sqlite_db,
};
use crate::webauthn_challenge_purpose as purpose;

const SECRET: &str = "test-secret-at-least-32-bytes-long";

fn test_webauthn() -> webauthn_rs::Webauthn {
    crate::webauthn::build_webauthn(
        "localhost",
        "Krausen Test",
        &Url::parse("http://localhost:8080").unwrap(),
    )
    .expect("build webauthn")
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

/// A unique id per call (challenge ids, recovery session ids, raw tokens).
fn unique(tag: &str) -> String {
    format!("{tag}-{}", uuid::Uuid::new_v4().simple())
}

/// A structurally-valid WebAuthn authentication assertion, captured from
/// `webauthn-rs-core`'s own `test_authentication` fixture. Deserializes
/// cleanly into `PublicKeyCredential` but does not correspond to any
/// challenge our test RP config ever issued.
pub(crate) const LOGIN_ASSERTION_JSON: &str = r#"
{
    "id":"at-FfKGsOI21EhtCu7Vx-7t7FKkpUOyKXIkEBBD_vC-eym_AdW6Y9V8WyKxHmii11EBQEe7uFQ0bkYwb0GWmUQ",
    "rawId":"at-FfKGsOI21EhtCu7Vx-7t7FKkpUOyKXIkEBBD_vC-eym_AdW6Y9V8WyKxHmii11EBQEe7uFQ0bkYwb0GWmUQ",
    "response":{
        "authenticatorData":"SZYN5YgOjGh0NBcPZHZgW4_krrmihjLHmVzzuoMdl2MBAAAAFA",
        "clientDataJSON":"eyJjaGFsbGVuZ2UiOiJXZ1h6X2tUdjNXVVUxa3c4aG0tT0dvR1M0WkNIWF8zYkVxSEgyUHZWcDhNIiwiY2xpZW50RXh0ZW5zaW9ucyI6e30sImhhc2hBbGdvcml0aG0iOiJTSEEtMjU2Iiwib3JpZ2luIjoiaHR0cDovL2xvY2FsaG9zdDo4MDgwIiwidHlwZSI6IndlYmF1dGhuLmdldCJ9",
        "signature":"MEYCIQDmLVOqv85cdRup4Fr8Pf9zC4AWO-XKBJqa8xPwYFCCMAIhAOiExLoyes0xipmUmq0BVlqJaCKLn_MFKG9GIDsCGq_-",
        "userHandle":null
    },
    "type":"public-key"
}
"#;

/// A structurally-valid WebAuthn registration credential, captured from
/// `webauthn-rs-core`'s own `test_registration_yk` fixture (a real Yubico
/// 5 response to an unrelated challenge/RP). Deserializes cleanly into
/// `RegisterPublicKeyCredential` but will always fail verification
/// against any `PasskeyRegistration` state our tests generate.
pub(crate) const REGISTER_CREDENTIAL_JSON: &str = r#"
{
    "id":"0xYE4bQ_HZM51-XYwp7WHJu8RfeA2Oz3_9HnNIZAKqRTz9gsUlF3QO7EqcJ0pgLSwDcq6cL1_aQpTtKLeGu6Ig",
    "rawId":"0xYE4bQ_HZM51-XYwp7WHJu8RfeA2Oz3_9HnNIZAKqRTz9gsUlF3QO7EqcJ0pgLSwDcq6cL1_aQpTtKLeGu6Ig",
    "response":{
         "attestationObject":"o2NmbXRoZmlkby11MmZnYXR0U3RtdKJjc2lnWEcwRQIhALjRb43YFcbJ3V9WiYPpIrZkhgzAM6KTR8KIjwCXejBCAiAO5Lvp1VW4dYBhBDv7HZIrxZb1SwKKYOLfFRXykRxMqGN4NWOBWQLBMIICvTCCAaWgAwIBAgIEGKxGwDANBgkqhkiG9w0BAQsFADAuMSwwKgYDVQQDEyNZdWJpY28gVTJGIFJvb3QgQ0EgU2VyaWFsIDQ1NzIwMDYzMTAgFw0xNDA4MDEwMDAwMDBaGA8yMDUwMDkwNDAwMDAwMFowbjELMAkGA1UEBhMCU0UxEjAQBgNVBAoMCVl1YmljbyBBQjEiMCAGA1UECwwZQXV0aGVudGljYXRvciBBdHRlc3RhdGlvbjEnMCUGA1UEAwweWXViaWNvIFUyRiBFRSBTZXJpYWwgNDEzOTQzNDg4MFkwEwYHKoZIzj0CAQYIKoZIzj0DAQcDQgAEeeo7LHxJcBBiIwzSP-tg5SkxcdSD8QC-hZ1rD4OXAwG1Rs3Ubs_K4-PzD4Hp7WK9Jo1MHr03s7y-kqjCrutOOqNsMGowIgYJKwYBBAGCxAoCBBUxLjMuNi4xLjQuMS40MTQ4Mi4xLjcwEwYLKwYBBAGC5RwCAQEEBAMCBSAwIQYLKwYBBAGC5RwBAQQEEgQQy2lIHo_3QDmT7AonKaFUqDAMBgNVHRMBAf8EAjAAMA0GCSqGSIb3DQEBCwUAA4IBAQCXnQOX2GD4LuFdMRx5brr7Ivqn4ITZurTGG7tX8-a0wYpIN7hcPE7b5IND9Nal2bHO2orh_tSRKSFzBY5e4cvda9rAdVfGoOjTaCW6FZ5_ta2M2vgEhoz5Do8fiuoXwBa1XCp61JfIlPtx11PXm5pIS2w3bXI7mY0uHUMGvxAzta74zKXLslaLaSQibSKjWKt9h-SsXy4JGqcVefOlaQlJfXL1Tga6wcO0QTu6Xq-Uw7ZPNPnrpBrLauKDd202RlN4SP7ohL3d9bG6V5hUz_3OusNEBZUn5W3VmPj1ZnFavkMB3RkRMOa58MZAORJT4imAPzrvJ0vtv94_y71C6tZ5aGF1dGhEYXRhWMQSyhe0mvIolDbzA-AWYDCiHlJdJm4gkmdDOAGo_UBxoEEAAAAAAAAAAAAAAAAAAAAAAAAAAABA0xYE4bQ_HZM51-XYwp7WHJu8RfeA2Oz3_9HnNIZAKqRTz9gsUlF3QO7EqcJ0pgLSwDcq6cL1_aQpTtKLeGu6IqUBAgMmIAEhWCCe1KvqpcVWN416_QZc8vJynt3uo3_WeJ2R4uj6kJbaiiJYIDC5ssxxummKviGgLoP9ZLFb836A9XfRO7op18QY3i5m",
         "clientDataJSON":"eyJjaGFsbGVuZ2UiOiJBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBIiwiY2xpZW50RXh0ZW5zaW9ucyI6e30sImhhc2hBbGdvcml0aG0iOiJTSEEtMjU2Iiwib3JpZ2luIjoiaHR0cDovLzEyNy4wLjAuMTo4MDgwIiwidHlwZSI6IndlYmF1dGhuLmNyZWF0ZSJ9"
    },
    "type":"public-key"}
"#;

// ---------------------------------------------------------------------------
// Passkey login: challenge purpose gate (KYO-279)
// ---------------------------------------------------------------------------

async fn login_complete_with_purpose(
    db: &DbPool,
    purpose_field: Option<&str>,
) -> PasskeyLoginServiceResult {
    let kv = test_kv();
    let webauthn = test_webauthn();
    let device = test_device();

    let challenge_id = unique("login-challenge");
    let mut challenge_data = serde_json::json!({});
    if let Some(p) = purpose_field {
        challenge_data["purpose"] = serde_json::json!(p);
    }
    crate::redis_ops::store_webauthn_challenge(&kv, &challenge_id, &challenge_data)
        .await
        .expect("store challenge");

    passkey_login_complete_service(PasskeyLoginCompleteParams {
        db,
        kv: &kv,
        jwt_secret: "test-secret",
        webauthn: &webauthn,
        challenge_id: &challenge_id,
        assertion_json: LOGIN_ASSERTION_JSON,
        ip: "127.0.0.1",
        device: &device,
    })
    .await
    .expect("service call should not error")
}

async fn login_complete_rejects_challenge_minted_for_signup(db: &DbPool) {
    let result = login_complete_with_purpose(db, Some(purpose::PASSKEY_SIGNUP)).await;
    assert!(matches!(result, PasskeyLoginServiceResult::InvalidChallenge));
}

async fn login_complete_rejects_challenge_minted_for_recovery(db: &DbPool) {
    let result = login_complete_with_purpose(db, Some(purpose::PASSKEY_RECOVERY)).await;
    assert!(matches!(result, PasskeyLoginServiceResult::InvalidChallenge));
}

async fn login_complete_rejects_challenge_minted_for_add_device(db: &DbPool) {
    let result = login_complete_with_purpose(db, Some(purpose::PASSKEY_ADD_DEVICE)).await;
    assert!(matches!(result, PasskeyLoginServiceResult::InvalidChallenge));
}

async fn login_complete_rejects_missing_purpose(db: &DbPool) {
    let result = login_complete_with_purpose(db, None).await;
    assert!(matches!(result, PasskeyLoginServiceResult::InvalidChallenge));
}

async fn login_complete_rejects_unknown_purpose(db: &DbPool) {
    let result = login_complete_with_purpose(db, Some("bogus")).await;
    assert!(matches!(result, PasskeyLoginServiceResult::InvalidChallenge));
}

/// Correct purpose clears the gate; since the (empty) test DB has no user with
/// this fixture's credential id, the function proceeds to a *different*, later
/// rejection (`InvalidCredentials`) rather than `InvalidChallenge`, proving the
/// purpose check specifically did not block it.
async fn login_complete_accepts_its_own_purpose(db: &DbPool) {
    let result = login_complete_with_purpose(db, Some(purpose::PASSKEY_LOGIN)).await;
    assert!(matches!(
        result,
        PasskeyLoginServiceResult::InvalidCredentials
    ));
}

/// Assert a purpose-gate rejection: the exact `Error::Internal` message the
/// "challenge not found" path also uses, so there is no distinguishing oracle.
fn assert_invalid_or_expired_challenge(result: krausen_core::Result<AuthenticatedSession>) {
    match result {
        Err(krausen_core::Error::Internal(msg)) => {
            assert_eq!(msg, "Invalid or expired challenge");
        }
        Err(other) => panic!(
            "expected Error::Internal(\"Invalid or expired challenge\"), got Err({other})"
        ),
        Ok(_) => panic!("expected Error::Internal(\"Invalid or expired challenge\"), got Ok"),
    }
}

// ---------------------------------------------------------------------------
// Recovery completion session binding (KYO-284)
// ---------------------------------------------------------------------------
//
// `passkey_recovery_complete_service` requires, in order: (1) a
// recovery_session cookie value at all, (2) it validates as a JWT,
// (3) scope == "passkey_recovery", (4) challenge purpose == PASSKEY_RECOVERY,
// (5) challenge email == JWT email, (6) the user still exists. Every rejection
// must be indistinguishable from the "challenge not found" case; that equality
// *is* the security property, so every test reuses
// `assert_invalid_or_expired_challenge`.

struct RecoveryFixture {
    kv: KVPool,
    webauthn: webauthn_rs::Webauthn,
    device: DeviceInfo,
    user_id: String,
    email: String,
}

async fn recovery_fixture(db: &DbPool, tag: &str) -> RecoveryFixture {
    let email = email(tag);
    let user = crate::user_service::create_user(db, &email, Some("Victim"), true)
        .await
        .expect("create user");
    RecoveryFixture {
        kv: test_kv(),
        webauthn: test_webauthn(),
        device: test_device(),
        user_id: user.user_id,
        email,
    }
}

/// Mint a `recovery_session`-shaped JWT with the given claims, matching the
/// shape `passkey_recovery_verify_service` mints in production. Signed with
/// `"test-secret"` (what every test's `jwt_secret` param uses) and a 15-minute
/// expiry, matching production.
fn mint_recovery_jwt(user_id: &str, email: &str, scope: &str) -> String {
    mint_recovery_jwt_with(user_id, email, scope, "test-secret", 15)
}

/// Same as `mint_recovery_jwt`, with the signing secret and expiry (minutes
/// from now; negative mints an already-expired token) exposed.
fn mint_recovery_jwt_with(
    user_id: &str,
    email: &str,
    scope: &str,
    secret: &str,
    expires_minutes: i64,
) -> String {
    let mut extra = std::collections::HashMap::new();
    extra.insert("user_id".into(), serde_json::json!(user_id));
    extra.insert("email".into(), serde_json::json!(email));
    extra.insert("scope".into(), serde_json::json!(scope));
    crate::jwt::create_access_token_str(user_id, secret, expires_minutes, extra)
        .expect("mint recovery jwt")
}

async fn store_recovery_challenge(
    fixture: &RecoveryFixture,
    challenge_id: &str,
    challenge_email: &str,
    purpose_field: Option<&str>,
    include_registration_state: bool,
) {
    let mut challenge_data = serde_json::json!({
        "email": challenge_email,
        "user_id": fixture.user_id,
        "device_name": "Recovery Device",
    });
    if include_registration_state {
        let (_ccr, reg_state) = crate::webauthn::start_registration(
            &fixture.webauthn,
            Uuid::new_v4(),
            challenge_email,
            "Victim",
            None,
        )
        .expect("start registration");
        challenge_data["registration_state"] =
            serde_json::to_value(&reg_state).expect("serialize reg state");
    }
    if let Some(p) = purpose_field {
        challenge_data["purpose"] = serde_json::json!(p);
    }
    crate::redis_ops::store_webauthn_challenge(&fixture.kv, challenge_id, &challenge_data)
        .await
        .expect("store challenge");
}

async fn complete_recovery(
    db: &DbPool,
    fixture: &RecoveryFixture,
    challenge_id: &str,
    recovery_session_token: Option<&str>,
) -> krausen_core::Result<AuthenticatedSession> {
    passkey_recovery_complete_service(PasskeyRecoveryCompleteParams {
        db,
        kv: &fixture.kv,
        jwt_secret: "test-secret",
        webauthn: &fixture.webauthn,
        challenge_id,
        credential_json: REGISTER_CREDENTIAL_JSON,
        device: &fixture.device,
        recovery_session_token,
    })
    .await
}

/// Also stands in for the wire-level `recovery_register_requires_recovery_session`
/// contract test: `passkey_recovery_complete` (the server fn) passes its
/// `recovery_session` cookie straight through to this service, so this covers
/// the same gate. "Rejected" is the property that survives, not a status code.
async fn recovery_complete_rejects_missing_recovery_session(db: &DbPool) {
    let fixture = recovery_fixture(db, "recovery-missing-session").await;
    let challenge_id = unique("recovery-chal-missing-session");
    store_recovery_challenge(
        &fixture,
        &challenge_id,
        &fixture.email,
        Some(purpose::PASSKEY_RECOVERY),
        false,
    )
    .await;

    let result = complete_recovery(db, &fixture, &challenge_id, None).await;

    assert_invalid_or_expired_challenge(result);
}

async fn recovery_complete_rejects_expired_recovery_session(db: &DbPool) {
    let fixture = recovery_fixture(db, "recovery-expired-session").await;
    let challenge_id = unique("recovery-chal-expired-session");
    store_recovery_challenge(
        &fixture,
        &challenge_id,
        &fixture.email,
        Some(purpose::PASSKEY_RECOVERY),
        false,
    )
    .await;
    // Expired 5 minutes ago: well past jsonwebtoken's default leeway.
    let token = mint_recovery_jwt_with(
        &fixture.user_id,
        &fixture.email,
        "passkey_recovery",
        "test-secret",
        -5,
    );

    let result = complete_recovery(db, &fixture, &challenge_id, Some(&token)).await;

    assert_invalid_or_expired_challenge(result);
}

async fn recovery_complete_rejects_recovery_session_signed_with_wrong_secret(db: &DbPool) {
    let fixture = recovery_fixture(db, "recovery-wrong-secret").await;
    let challenge_id = unique("recovery-chal-wrong-secret");
    store_recovery_challenge(
        &fixture,
        &challenge_id,
        &fixture.email,
        Some(purpose::PASSKEY_RECOVERY),
        false,
    )
    .await;
    // Signed with a secret other than the one the service validates against,
    // e.g. a forged token, or a stale one from before a secret rotation.
    let token = mint_recovery_jwt_with(
        &fixture.user_id,
        &fixture.email,
        "passkey_recovery",
        "a-completely-different-secret",
        15,
    );

    let result = complete_recovery(db, &fixture, &challenge_id, Some(&token)).await;

    assert_invalid_or_expired_challenge(result);
}

async fn recovery_complete_rejects_wrong_scope(db: &DbPool) {
    let fixture = recovery_fixture(db, "recovery-wrong-scope").await;
    let challenge_id = unique("recovery-chal-wrong-scope");
    store_recovery_challenge(
        &fixture,
        &challenge_id,
        &fixture.email,
        Some(purpose::PASSKEY_RECOVERY),
        false,
    )
    .await;
    // Same user_id/email as the challenge, but minted for a different purpose
    // entirely, e.g. a stray access-token-shaped cookie.
    let token = mint_recovery_jwt(&fixture.user_id, &fixture.email, "access");

    let result = complete_recovery(db, &fixture, &challenge_id, Some(&token)).await;

    assert_invalid_or_expired_challenge(result);
}

async fn recovery_complete_rejects_email_mismatch(db: &DbPool) {
    let fixture = recovery_fixture(db, "recovery-victim").await;
    let challenge_id = unique("recovery-chal-email-mismatch");
    // Challenge was minted for a *different* email than the recovery session
    // JWT claims: the session belongs to one account but the challenge_id
    // belongs to another.
    store_recovery_challenge(
        &fixture,
        &challenge_id,
        &email("someone-else"),
        Some(purpose::PASSKEY_RECOVERY),
        false,
    )
    .await;
    let token = mint_recovery_jwt(&fixture.user_id, &fixture.email, "passkey_recovery");

    let result = complete_recovery(db, &fixture, &challenge_id, Some(&token)).await;

    assert_invalid_or_expired_challenge(result);
}

async fn recovery_complete_rejects_challenge_minted_for_a_different_flow(db: &DbPool) {
    let fixture = recovery_fixture(db, "recovery-wrong-purpose").await;
    let challenge_id = unique("recovery-chal-wrong-purpose");
    // Challenge purpose is signup, not recovery: a signup challenge replayed
    // against the recovery-completion path.
    store_recovery_challenge(
        &fixture,
        &challenge_id,
        &fixture.email,
        Some(purpose::PASSKEY_SIGNUP),
        false,
    )
    .await;
    let token = mint_recovery_jwt(&fixture.user_id, &fixture.email, "passkey_recovery");

    let result = complete_recovery(db, &fixture, &challenge_id, Some(&token)).await;

    assert_invalid_or_expired_challenge(result);
}

/// Baseline for the equality property under test: a fully valid recovery
/// session presented against a challenge_id that was never stored must produce
/// the exact same rejection as every binding failure above, proving none of
/// them are distinguishable oracles.
async fn recovery_complete_rejects_nonexistent_challenge_with_identical_error(db: &DbPool) {
    let fixture = recovery_fixture(db, "recovery-no-such-challenge").await;
    let token = mint_recovery_jwt(&fixture.user_id, &fixture.email, "passkey_recovery");

    let result = complete_recovery(db, &fixture, &unique("does-not-exist"), Some(&token)).await;

    assert_invalid_or_expired_challenge(result);
}

/// All six checks pass: cookie present, valid JWT, correct scope, correct
/// purpose, matching email, user exists. Execution must reach real WebAuthn
/// verification and fail *there* (the fixture credential is cryptographically
/// inert against our test RP) rather than at the binding gate.
async fn recovery_complete_accepts_valid_binding_and_reaches_webauthn_verification(db: &DbPool) {
    let fixture = recovery_fixture(db, "recovery-happy-path").await;
    let challenge_id = unique("recovery-chal-happy-path");
    store_recovery_challenge(
        &fixture,
        &challenge_id,
        &fixture.email,
        Some(purpose::PASSKEY_RECOVERY),
        true,
    )
    .await;
    let token = mint_recovery_jwt(&fixture.user_id, &fixture.email, "passkey_recovery");

    let result = complete_recovery(db, &fixture, &challenge_id, Some(&token)).await;

    let err = match result {
        Err(e) => e,
        Ok(_) => panic!("fixture credential must fail real verification"),
    };
    let msg = err.to_string();
    assert_ne!(msg, "internal: Invalid or expired challenge");
    assert!(
        msg.contains("registration failed"),
        "expected a WebAuthn verification failure, got: {msg}"
    );
}

// ---------------------------------------------------------------------------
// Session revocation on recovery (KYO-287, KYO-339, KYO-341)
// ---------------------------------------------------------------------------
//
// `passkey_recovery_complete_service` can't be driven to a successful `Ok(_)`
// here at all: every fixture credential is cryptographically inert against the
// fresh `PasskeyRegistration` state a test generates, so
// `verify_and_store_passkey` always errors first. The passkey tests therefore
// exercise `revoke_sessions_and_mint_recovery_session` directly, the exact
// function `passkey_recovery_complete_service` calls once
// `verify_and_store_passkey` succeeds. The password flow has no such ceremony,
// so its tests drive the real `recovery_set_password_service` end to end.

async fn authenticate(db: &DbPool, token: &str) -> krausen_core::Result<crate::middleware::AuthUser> {
    let state = crate::middleware::AuthState {
        jwt_secret: SECRET.into(),
        db: db.clone(),
    };
    let (mut parts, _) = axum::http::Request::builder()
        .uri("/x")
        .header("authorization", format!("Bearer {token}"))
        .body(())
        .unwrap()
        .into_parts();
    crate::middleware::AuthUser::from_request_parts(&mut parts, &state).await
}

/// Assert the extractor refuses `token` because of the cutoff alone.
async fn assert_refused_by_cutoff(db: &DbPool, token: &str) {
    match authenticate(db, token).await {
        Err(krausen_core::Error::Unauthorized(msg)) => assert!(
            msg.contains("Session revoked"),
            "expected the cutoff to refuse the token, got: {msg}"
        ),
        Err(other) => panic!("expected the cutoff to refuse the token, got: {other:?}"),
        Ok(_) => panic!("token issued before the cutoff must be refused"),
    }
}

async fn session_cutoff_both_recovery_flows_replace_session_immediately(db: &DbPool) {
    let kv = test_kv();
    let device = test_device();
    let user = crate::user_service::create_user(
        db,
        &email("cutoff-recovery"),
        Some("Test User"),
        true,
    )
    .await
    .unwrap();
    let original = create_authenticated_session(db, &kv, SECRET, &user, &device)
        .await
        .unwrap();
    // Exact production post-WebAuthn orchestration; inert test passkeys cannot
    // complete signature verification.
    let passkey =
        revoke_sessions_and_mint_recovery_session(db, &kv, SECRET, &user, &device, "passkey")
            .await
            .unwrap();
    authenticate(db, &passkey.access_token).await.unwrap();
    assert_refused_by_cutoff(db, &original.access_token).await;
    crate::token_refresh::refresh_tokens(db, SECRET, &passkey.refresh_token, &device)
        .await
        .unwrap();

    let recovery_session_id = unique("cutoff-password-recovery");
    crate::redis_ops::store_recovery_session(&kv, &recovery_session_id, &user.user_id)
        .await
        .unwrap();
    let result = recovery_set_password_service(
        db,
        &kv,
        SECRET,
        &recovery_session_id,
        "new-test-password-123",
        &device,
    )
    .await
    .unwrap();
    let RecoverySetPasswordServiceResult::Success(password) = result else {
        panic!("password recovery succeeds");
    };
    assert_refused_by_cutoff(db, &passkey.access_token).await;
    authenticate(db, &password.access_token).await.unwrap();
    assert!(
        crate::token_refresh::refresh_tokens(db, SECRET, &passkey.refresh_token, &device)
            .await
            .is_err()
    );
    crate::token_refresh::refresh_tokens(db, SECRET, &password.refresh_token, &device)
        .await
        .unwrap();
}

/// Seed a refresh token through the production session-token path, as if the
/// (possibly stolen) authenticator had logged in before recovery.
async fn seed_pre_existing_refresh_token(db: &DbPool, user_id: &str, device: &DeviceInfo) -> String {
    let raw = unique("rt_pre-existing");
    let hash = crate::token_service::hash_refresh_token(&raw);
    crate::token_service::store_session_refresh_token(
        db,
        user_id,
        &hash,
        chrono::Utc::now() + chrono::Duration::days(7),
        device,
        &unique("fam_pre-existing"),
        chrono::Utc::now().timestamp_micros(),
    )
    .await
    .expect("seed pre-existing refresh token");
    hash
}

/// `(is_active, revoked)` flags (1 or 0) for the refresh token matching
/// `token_hash`. The columns are booleans/timestamps on PostgreSQL and
/// integers/text on SQLite, so both are normalised to integers in SQL.
async fn refresh_token_state(db: &DbPool, token_hash: &str) -> (i64, i64) {
    #[derive(sqlx::FromRow)]
    struct Row {
        is_active: i64,
        revoked: i64,
    }
    let row: Row = krausen_core::db_fetch_one!(
        db,
        Row,
        "SELECT CAST(CASE WHEN is_active THEN 1 ELSE 0 END AS BIGINT) AS is_active, \
                CAST(CASE WHEN revoked_at IS NULL THEN 0 ELSE 1 END AS BIGINT) AS revoked \
         FROM refresh_tokens WHERE token_hash = $1",
        token_hash
    )
    .expect("refresh token row must exist");
    (row.is_active, row.revoked)
}

async fn passkey_recovery_revokes_pre_existing_refresh_tokens(db: &DbPool) {
    let kv = test_kv();
    let device = test_device();
    let user = crate::user_service::create_user(
        db,
        &email("recovery-revoke-old"),
        Some("Test User"),
        true,
    )
    .await
    .expect("create user");
    let pre_existing_hash = seed_pre_existing_refresh_token(db, &user.user_id, &device).await;

    revoke_sessions_and_mint_recovery_session(db, &kv, "test-secret", &user, &device, "passkey")
        .await
        .expect("revoke + mint should not error");

    let (is_active, revoked) = refresh_token_state(db, &pre_existing_hash).await;
    assert_eq!(
        is_active, 0,
        "pre-existing refresh token must be revoked (is_active) after recovery"
    );
    assert_eq!(
        revoked, 1,
        "pre-existing refresh token must have revoked_at set after recovery"
    );
}

async fn passkey_recovery_leaves_the_newly_minted_refresh_token_active(db: &DbPool) {
    let kv = test_kv();
    let device = test_device();
    let user = crate::user_service::create_user(
        db,
        &email("recovery-revoke-new"),
        Some("Test User"),
        true,
    )
    .await
    .expect("create user");

    let sess = revoke_sessions_and_mint_recovery_session(
        db,
        &kv,
        "test-secret",
        &user,
        &device,
        "passkey",
    )
    .await
    .expect("revoke + mint should not error");

    let new_hash = crate::token_service::hash_refresh_token(&sess.refresh_token);
    let (is_active, revoked) = refresh_token_state(db, &new_hash).await;
    assert_eq!(
        is_active, 1,
        "the refresh token minted by recovery itself must still be active: \
         revoking after minting would log the user out of the session they \
         just established"
    );
    assert_eq!(
        revoked, 0,
        "the refresh token minted by recovery itself must not be revoked"
    );
}

async fn password_recovery_revokes_pre_existing_refresh_tokens(db: &DbPool) {
    let kv = test_kv();
    let device = test_device();
    let user = crate::user_service::create_user(
        db,
        &email("password-recovery-revoke-old"),
        Some("Test User"),
        true,
    )
    .await
    .expect("create user");
    let pre_existing_hash = seed_pre_existing_refresh_token(db, &user.user_id, &device).await;

    let recovery_session_id = unique("recovery-session-revoke-old");
    crate::redis_ops::store_recovery_session(&kv, &recovery_session_id, &user.user_id)
        .await
        .expect("seed recovery session");

    let result = recovery_set_password_service(
        db,
        &kv,
        "test-secret",
        &recovery_session_id,
        "a-brand-new-password-123",
        &device,
    )
    .await
    .expect("recovery_set_password_service should not error");

    assert!(
        matches!(result, RecoverySetPasswordServiceResult::Success(_)),
        "expected Success, got a different result variant"
    );

    let (is_active, revoked) = refresh_token_state(db, &pre_existing_hash).await;
    assert_eq!(
        is_active, 0,
        "pre-existing refresh token must be revoked (is_active) after password recovery"
    );
    assert_eq!(
        revoked, 1,
        "pre-existing refresh token must have revoked_at set after password recovery"
    );
}

async fn password_recovery_leaves_the_newly_minted_refresh_token_active(db: &DbPool) {
    let kv = test_kv();
    let device = test_device();
    let user = crate::user_service::create_user(
        db,
        &email("password-recovery-revoke-new"),
        Some("Test User"),
        true,
    )
    .await
    .expect("create user");

    let recovery_session_id = unique("recovery-session-revoke-new");
    crate::redis_ops::store_recovery_session(&kv, &recovery_session_id, &user.user_id)
        .await
        .expect("seed recovery session");

    let result = recovery_set_password_service(
        db,
        &kv,
        "test-secret",
        &recovery_session_id,
        "a-brand-new-password-123",
        &device,
    )
    .await
    .expect("recovery_set_password_service should not error");

    let sess = match result {
        RecoverySetPasswordServiceResult::Success(sess) => sess,
        _ => panic!("expected Success, got a different result variant"),
    };

    let new_hash = crate::token_service::hash_refresh_token(&sess.refresh_token);
    let (is_active, revoked) = refresh_token_state(db, &new_hash).await;
    assert_eq!(
        is_active, 1,
        "the refresh token minted by password recovery itself must still be active: \
         revoking after minting would log the user out of the session they \
         just established"
    );
    assert_eq!(
        revoked, 0,
        "the refresh token minted by password recovery itself must not be revoked"
    );
}

// ---------------------------------------------------------------------------
// Verification-token type binding (KYO-282)
// ---------------------------------------------------------------------------
//
// `token_service::verify_verification_token` filters strictly on `token_type`:
// a mismatch is indistinguishable from "no such token". Each test mints a token
// with the exact type string that flow's real minting call site uses, drives
// the real verifying service and asserts it accepts its own type and rejects
// every other flow's type.

async fn mint_token(db: &DbPool, email: &str, token_type: &str) -> String {
    crate::token_service::create_verification_token(db, email, token_type)
        .await
        .expect("mint verification token")
}

async fn password_recovery_verify_only_accepts_its_own_token_type(db: &DbPool) {
    for &token_type in &ALL_VERIFICATION_TOKEN_TYPES {
        let kv = test_kv();
        let email = email("recovery-binding");

        crate::user_service::create_user(db, &email, Some("Test User"), true)
            .await
            .expect("create user");

        let token = mint_token(db, &email, token_type).await;

        let result = recovery_verify_service(db, &kv, &token)
            .await
            .expect("service call should not error");

        if token_type == TOKEN_TYPE_PASSWORD_RECOVERY {
            assert!(
                matches!(result, RecoveryVerifyServiceResult::Success { .. }),
                "password recovery must accept its own token type ({token_type})"
            );
        } else {
            assert!(
                matches!(result, RecoveryVerifyServiceResult::InvalidToken),
                "password recovery must reject token type {token_type}"
            );
        }
    }
}

async fn passkey_recovery_verify_only_accepts_its_own_token_type(db: &DbPool) {
    for &token_type in &ALL_VERIFICATION_TOKEN_TYPES {
        let kv = test_kv();
        let webauthn = test_webauthn();
        let email = email("passkey-recovery-binding");

        crate::user_service::create_user(db, &email, Some("Test User"), true)
            .await
            .expect("create user");

        let token = mint_token(db, &email, token_type).await;

        let result = passkey_recovery_verify_service(db, &kv, "test-secret", &webauthn, &token)
            .await
            .expect("service call should not error");

        if token_type == TOKEN_TYPE_PASSKEY_RECOVERY {
            assert!(
                matches!(result, Ok(PasskeyRecoveryVerifySuccess { .. })),
                "passkey recovery must accept its own token type ({token_type}), got {result:?}"
            );
        } else {
            assert!(
                matches!(&result, Err(msg) if msg.contains("Invalid or expired")),
                "passkey recovery must reject token type {token_type}, got {result:?}"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// passkey_recovery_start_service (KYO-285)
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

/// The response for an unknown email, an existing-but-unverified email, and an
/// existing-verified email must be indistinguishable; otherwise the endpoint is
/// an account-enumeration oracle.
///
/// Also stands in for the wire-level
/// `recovery_request_response_is_identical_for_different_unknown_emails`
/// contract test: `passkey_recovery_start` delegates to this service with no
/// branching on the result and returns no per-account body, so every account
/// state mapping to the same `Ok(())` here is the whole of that contract.
async fn passkey_recovery_start_indistinguishable_across_account_states(db: &DbPool) {
    let kv = test_kv();

    let unknown_email = email("passkey-recovery-unknown");
    let unverified_email = email("passkey-recovery-unverified");
    let verified_email = email("passkey-recovery-verified");

    crate::user_service::create_user(db, &unverified_email, Some("Unverified User"), false)
        .await
        .expect("create unverified user");
    crate::user_service::create_user(db, &verified_email, Some("Verified User"), true)
        .await
        .expect("create verified user");

    for email in [&unknown_email, &unverified_email, &verified_email] {
        let result = passkey_recovery_start_service(
            db,
            &kv,
            "127.0.0.1",
            email,
            "https://app.example.com",
            false,
        )
        .await;

        assert!(
            matches!(result, Ok(())),
            "expected Ok(()) for {email}: unknown/unverified/verified account \
             states must return the same result (email enumeration guard), got {result:?}"
        );
    }
}

/// A verified user's request must mint a token of type `"passkey_recovery"`
/// and, critically, *not* `"account_recovery"`, which is the exact KYO-285 bug
/// (`/auth/recover-passkey` was wired to the account/password recovery flow).
async fn passkey_recovery_start_mints_passkey_recovery_token_for_verified_user(db: &DbPool) {
    let kv = test_kv();
    let email = email("passkey-recovery-mint");

    crate::user_service::create_user(db, &email, Some("Verified User"), true)
        .await
        .expect("create verified user");

    passkey_recovery_start_service(db, &kv, "127.0.0.1", &email, "https://app.example.com", false)
        .await
        .expect("service call should not error");

    assert_eq!(
        count_tokens_of_type(db, &email, "passkey_recovery").await,
        1,
        "a verified user's passkey recovery request must mint a \"passkey_recovery\" token"
    );
    assert_eq!(
        count_tokens_of_type(db, &email, "account_recovery").await,
        0,
        "passkey recovery must never mint an \"account_recovery\" token: that's the KYO-285 bug"
    );
}

async fn passkey_recovery_start_mints_no_token_for_unverified_user(db: &DbPool) {
    let kv = test_kv();
    let email = email("passkey-recovery-unverified-mint");

    crate::user_service::create_user(db, &email, Some("Unverified User"), false)
        .await
        .expect("create unverified user");

    passkey_recovery_start_service(db, &kv, "127.0.0.1", &email, "https://app.example.com", false)
        .await
        .expect("service call should not error");

    assert_eq!(
        count_tokens_of_type(db, &email, "passkey_recovery").await,
        0,
        "an unverified user's passkey recovery request must not mint a token"
    );
}

/// The actual KYO-285 bug: completing the *wrong* recovery flow (account
/// recovery) adds a password auth method and removes totp, silently
/// downgrading a passkey-only user's account security. Assert directly that
/// starting a *passkey* recovery leaves both auth methods exactly as they were.
async fn passkey_recovery_start_leaves_password_and_totp_auth_methods_untouched(db: &DbPool) {
    let kv = test_kv();
    let email = email("passkey-recovery-auth-methods");

    let user = crate::user_service::create_user(db, &email, Some("Verified User"), true)
        .await
        .expect("create verified user");

    crate::user_service::upsert_auth_method(
        db,
        &user.user_id,
        "totp",
        &serde_json::json!({"secret": "test-secret"}),
    )
    .await
    .expect("seed totp auth method");

    let password_before = crate::user_service::get_auth_method(db, &user.user_id, "password")
        .await
        .expect("get_auth_method password");
    let totp_before = crate::user_service::get_auth_method(db, &user.user_id, "totp")
        .await
        .expect("get_auth_method totp");
    assert!(
        password_before.is_none(),
        "sanity check: no password auth method before the call"
    );
    assert!(
        totp_before.is_some(),
        "sanity check: totp auth method exists before the call"
    );

    passkey_recovery_start_service(db, &kv, "127.0.0.1", &email, "https://app.example.com", false)
        .await
        .expect("service call should not error");

    let password_after = crate::user_service::get_auth_method(db, &user.user_id, "password")
        .await
        .expect("get_auth_method password");
    let totp_after = crate::user_service::get_auth_method(db, &user.user_id, "totp")
        .await
        .expect("get_auth_method totp");

    assert!(
        password_after.is_none(),
        "passkey recovery must never add a password auth method (KYO-285)"
    );
    assert!(
        totp_after.is_some(),
        "passkey recovery must never remove the totp auth method (KYO-285)"
    );
}

// ---------------------------------------------------------------------------
// `lookup_recovery_user`: enumeration-resistant DB-error handling (KYO-215)
// ---------------------------------------------------------------------------

/// Close whichever pool backs `db`, so every later query fails.
async fn close_pool(db: &DbPool) {
    match db {
        DbPool::Sqlite(pool) => pool.close().await,
        DbPool::Postgres(pool) => pool.close().await,
    }
}

/// `lookup_recovery_user` must return `None` when the database lookup itself
/// fails (not merely when no user exists), and it must say so via an
/// `error!`-level log rather than swallowing the error, since a DB outage on
/// the recovery path would otherwise silently return success to every caller.
async fn lookup_recovery_user_db_error_returns_none_and_logs_error(db: &DbPool) {
    close_pool(db).await;

    let email = email("db-error-recovery-test");
    let logs = capture_tracing();

    let result = lookup_recovery_user(db, &email, "passkey_recovery").await;

    assert!(
        result.is_none(),
        "a DB error must be reported as no user, not surfaced to the caller"
    );

    let error_events = logs.events_at(Level::ERROR);
    assert!(
        !error_events.is_empty(),
        "a DB error during recovery must emit an error!-level log; captured: {:?}",
        logs.events()
    );
    assert!(
        error_events
            .iter()
            .any(|(_, message)| message.contains("user lookup failed")),
        "expected an error log describing the lookup failure; captured: {:?}",
        logs.events()
    );
    for (_, message) in &error_events {
        assert!(
            !message.contains(&email),
            "recovery error logs must never contain the requester's email \
             (that would reintroduce the enumeration leak through logs); got: {message}"
        );
    }
}

/// The absent-user case must NOT log an error; only the DB-failure case should.
/// Without this test, a broad "always log an error" change would pass the test
/// above while defeating the point of the fix: the two enumeration-resistant
/// outcomes (no such user vs. DB down) must stay distinguishable in the logs.
async fn lookup_recovery_user_absent_user_returns_none_without_error_log(db: &DbPool) {
    let email = email("absent-recovery-test");
    let logs = capture_tracing();

    let result = lookup_recovery_user(db, &email, "passkey_recovery").await;

    assert!(result.is_none(), "no such user should look up to None");

    let error_events = logs.events_at(Level::ERROR);
    assert!(
        error_events.is_empty(),
        "a merely-absent user must not log an error, only a DB failure should; captured: {:?}",
        logs.events()
    );
}

// ---------------------------------------------------------------------------
// `recovery_start_service`: enumeration resistance (KYO-291)
// ---------------------------------------------------------------------------
//
// `recovery_start_service` used to propagate a DB-lookup error and a
// token-mint failure to its caller, observably distinct from the `Ok(None)` an
// unregistered email produces. Both are now folded into `Ok(None)`. These tests
// assert that absent, unverified, lookup-error and token-mint-error account
// states are all indistinguishable to the caller.

/// Absent and present-but-unverified accounts must both resolve to `Ok(None)`.
async fn recovery_start_service_returns_none_for_absent_and_unverified_users(db: &DbPool) {
    let kv = test_kv();

    let unknown_email = email("recovery-start-unknown");
    let unverified_email = email("recovery-start-unverified");

    crate::user_service::create_user(db, &unverified_email, Some("Unverified User"), false)
        .await
        .expect("create unverified user");

    for email in [&unknown_email, &unverified_email] {
        let result = recovery_start_service(db, &kv, "127.0.0.1", email)
            .await
            .expect("service call should not error");

        assert!(
            result.is_none(),
            "expected Ok(None) for {email} (unknown or unverified account), got Ok(Some(_))"
        );
    }
}

/// The KYO-291 regression test: a DB error during the user lookup must resolve
/// to `Ok(None)`, exactly like an absent user, not propagate as `Err`. It must
/// still log at `error!`, tagged with the `account_recovery` flow.
async fn recovery_start_service_lookup_error_returns_none_and_logs_error(db: &DbPool) {
    close_pool(db).await;
    let kv = test_kv();

    let email = email("recovery-start-db-error");
    let logs = capture_tracing();

    let result = recovery_start_service(db, &kv, "127.0.0.1", &email)
        .await
        .expect("a lookup failure must resolve to Ok(None), not Err: that's the leak");

    assert!(
        result.is_none(),
        "a DB error during account recovery must be reported as no user, not surfaced \
         to the caller as an error (KYO-291 enumeration leak)"
    );

    let error_events = logs.events_at(Level::ERROR);
    assert!(
        !error_events.is_empty(),
        "a DB error during account recovery must emit an error!-level log; captured: {:?}",
        logs.events()
    );
    assert!(
        error_events
            .iter()
            .any(|(_, message)| message.contains("user lookup failed")),
        "expected an error log describing the lookup failure; captured: {:?}",
        logs.events()
    );
    for (_, message) in &error_events {
        assert!(
            !message.contains(&email),
            "recovery error logs must never contain the requester's email \
             (that would reintroduce the enumeration leak through logs); got: {message}"
        );
    }
}

/// The second KYO-291 leak: a token-mint failure (after a verified user is
/// found) must also resolve to `Ok(None)` rather than `Err`, and log at
/// `error!`. Forces the failure by dropping `verification_tokens` after the
/// user lookup has already succeeded, so the INSERT inside
/// `create_verification_token_with_expiry` fails while the preceding SELECT
/// still works.
async fn recovery_start_service_token_mint_error_returns_none_and_logs_error(db: &DbPool) {
    let kv = test_kv();

    let email = email("recovery-start-token-mint-error");
    crate::user_service::create_user(db, &email, Some("Verified User"), true)
        .await
        .expect("create verified user");

    krausen_core::db_execute!(db, "DROP TABLE verification_tokens")
        .expect("drop verification_tokens table");

    let logs = capture_tracing();

    let result = recovery_start_service(db, &kv, "127.0.0.1", &email)
        .await
        .expect("a token-mint failure must resolve to Ok(None), not Err: that's the leak");

    assert!(
        result.is_none(),
        "a token-mint failure for a verified user must be reported as no user, not \
         surfaced to the caller as an error (KYO-291 enumeration leak)"
    );

    let error_events = logs.events_at(Level::ERROR);
    assert!(
        !error_events.is_empty(),
        "a token-mint failure during account recovery must emit an error!-level log; \
         captured: {:?}",
        logs.events()
    );
    assert!(
        error_events
            .iter()
            .any(|(_, message)| message.contains("token creation failed")),
        "expected an error log describing the token-creation failure; captured: {:?}",
        logs.events()
    );
}

/// The client-visible classification, `Ok` vs `Err`, must be identical whether
/// or not an account exists. `recovery_start_service` folds "no account",
/// "unverified account" and "token-mint failure" into `Ok(None)`; a verified
/// account is `Ok(Some(_))`. Both are `Ok`, so the caller takes the identical
/// `Ok` branch regardless of account state.
async fn recovery_start_service_classification_is_ok_for_all_account_states(db: &DbPool) {
    let kv = test_kv();

    let unknown_email = email("recovery-start-classification-unknown");
    let unverified_email = email("recovery-start-classification-unverified");
    let verified_email = email("recovery-start-classification-verified");

    crate::user_service::create_user(db, &unverified_email, Some("Unverified User"), false)
        .await
        .expect("create unverified user");
    crate::user_service::create_user(db, &verified_email, Some("Verified User"), true)
        .await
        .expect("create verified user");

    for (email, expect_some) in [
        (&unknown_email, false),
        (&unverified_email, false),
        (&verified_email, true),
    ] {
        let result = recovery_start_service(db, &kv, "127.0.0.1", email)
            .await
            .unwrap_or_else(|e| {
                panic!("expected Ok for {email} (enumeration guard), got Err: {e}")
            });

        assert_eq!(
            result.is_some(),
            expect_some,
            "unexpected Some/None for {email} (RecoveryStartResult has no Debug impl \
             to print the value directly)"
        );
    }
}

/// The one `Err` `recovery_start_service` returns: rate limiting, checked
/// before any account lookup so it leaks nothing about a specific email. Pins
/// the message shape (and that a retry-after seconds count is embedded in it).
async fn recovery_start_service_rate_limited_returns_err_with_retry_after_seconds(db: &DbPool) {
    let kv = test_kv();
    let ip = "203.0.113.5";
    let email = email("recovery-start-rate-limited");

    // The `register` bucket's ip_capacity: exhaust it so the next call is denied.
    let capacity = krausen_core::constants::get().rate_limits.register.ip_capacity;
    for i in 0..capacity {
        recovery_start_service(db, &kv, ip, &email)
            .await
            .unwrap_or_else(|e| panic!("call {i} should be within the rate limit, got Err: {e}"));
    }

    // `Option<RecoveryStartResult>` has no `Debug`, so `.expect_err()` is not
    // available: match directly.
    let over_quota_call = recovery_start_service(db, &kv, ip, &email).await;

    let Err(err) = over_quota_call else {
        panic!(
            "the call past the register bucket's ip_capacity window must be \
             rate limited, got Ok"
        );
    };

    match err {
        krausen_core::Error::BadRequest(msg) => {
            assert!(
                msg.contains("Rate limited") && msg.contains("Try again in"),
                "expected a retry-after message, got: {msg}"
            );
            assert!(
                msg.chars().any(|c| c.is_ascii_digit()),
                "expected the retry-after seconds count embedded in the message; got: {msg}"
            );
        }
        other => panic!("expected Error::BadRequest for rate limiting, got: {other:?}"),
    }
}

/// The one `Err` `passkey_recovery_start_service` returns: rate limiting on the
/// `passkey_recovery` bucket (mapped to the `login` bucket config in
/// `rate_limiter.rs`), checked before any account lookup. Pins the
/// `TooManyRequests` shape: message plus a nonzero `retry_after_secs`.
async fn passkey_recovery_start_service_rate_limited_returns_err_with_retry_after_seconds(
    db: &DbPool,
) {
    let kv = test_kv();
    let ip = "203.0.113.6";
    let email = email("passkey-recovery-rate-limited");
    let frontend_url = "https://app.example.com";

    // rate_limiter.rs maps "passkey_recovery" to the `login` bucket config:
    // exhaust its ip_capacity so the next call is denied.
    let capacity = krausen_core::constants::get().rate_limits.login.ip_capacity;
    for i in 0..capacity {
        passkey_recovery_start_service(db, &kv, ip, &email, frontend_url, false)
            .await
            .unwrap_or_else(|e| panic!("call {i} should be within the rate limit, got Err: {e}"));
    }

    let err = passkey_recovery_start_service(db, &kv, ip, &email, frontend_url, false)
        .await
        .expect_err(
            "the call past the login bucket's ip_capacity window must be rate limited",
        );

    match err {
        krausen_core::Error::TooManyRequests(msg, retry_after_secs) => {
            assert!(
                msg.contains("Rate limited") && msg.contains("Try again in"),
                "expected a retry-after message, got: {msg}"
            );
            assert!(
                retry_after_secs > 0,
                "expected a nonzero retry-after seconds count, got {retry_after_secs}"
            );
        }
        other => panic!("expected Error::TooManyRequests for rate limiting, got: {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Mint half of purpose binding: the stored challenge carries its flow's purpose
// ---------------------------------------------------------------------------
//
// Every consumer test above hand-builds its challenge, so a mint site that
// dropped or mistyped `purpose` would fail none of them. These drive the real
// minting services and read the stored blob back.

async fn stored_purpose(kv: &KVPool, challenge_id: &str) -> Option<String> {
    crate::redis_ops::get_webauthn_challenge(kv, challenge_id)
        .await
        .expect("read challenge")
        .expect("challenge was stored")
        .get("purpose")
        .and_then(|v| v.as_str())
        .map(str::to_string)
}

/// `passkey_recovery_verify_service` stores a registration challenge bound to
/// the passkey-recovery purpose.
async fn passkey_recovery_verify_mints_the_recovery_purpose(db: &DbPool) {
    let kv = test_kv();
    let webauthn = test_webauthn();
    let email = email("mint-recovery");
    crate::user_service::create_user(db, &email, Some("Test User"), true)
        .await
        .expect("create user");
    let token = mint_token(db, &email, TOKEN_TYPE_PASSKEY_RECOVERY).await;

    let success = passkey_recovery_verify_service(db, &kv, "test-secret", &webauthn, &token)
        .await
        .expect("service call should not error")
        .expect("valid token");

    assert_eq!(
        stored_purpose(&kv, &success.challenge_id).await.as_deref(),
        Some(purpose::PASSKEY_RECOVERY)
    );
}

/// The authenticated add-device start stores a challenge bound to the
/// add-device purpose.
async fn start_passkey_registration_mints_the_add_device_purpose(db: &DbPool) {
    let kv = test_kv();
    let webauthn = test_webauthn();
    let email = email("mint-add-device");
    let user = crate::user_service::create_user(db, &email, Some("Test User"), true)
        .await
        .expect("create user");

    let (_ccr, challenge_id) = crate::security_service::start_passkey_registration(
        db,
        &kv,
        &webauthn,
        &user.user_id,
        "Laptop",
    )
    .await
    .expect("start registration");

    assert_eq!(
        stored_purpose(&kv, &challenge_id).await.as_deref(),
        Some(purpose::PASSKEY_ADD_DEVICE)
    );
}

both_backends!(
    login_complete_rejects_challenge_minted_for_signup,
    login_complete_rejects_challenge_minted_for_recovery,
    login_complete_rejects_challenge_minted_for_add_device,
    login_complete_rejects_missing_purpose,
    login_complete_rejects_unknown_purpose,
    login_complete_accepts_its_own_purpose,
    recovery_complete_rejects_missing_recovery_session,
    recovery_complete_rejects_expired_recovery_session,
    recovery_complete_rejects_recovery_session_signed_with_wrong_secret,
    recovery_complete_rejects_wrong_scope,
    recovery_complete_rejects_email_mismatch,
    recovery_complete_rejects_challenge_minted_for_a_different_flow,
    recovery_complete_rejects_nonexistent_challenge_with_identical_error,
    recovery_complete_accepts_valid_binding_and_reaches_webauthn_verification,
    session_cutoff_both_recovery_flows_replace_session_immediately,
    passkey_recovery_revokes_pre_existing_refresh_tokens,
    passkey_recovery_leaves_the_newly_minted_refresh_token_active,
    password_recovery_revokes_pre_existing_refresh_tokens,
    password_recovery_leaves_the_newly_minted_refresh_token_active,
    password_recovery_verify_only_accepts_its_own_token_type,
    passkey_recovery_verify_only_accepts_its_own_token_type,
    passkey_recovery_start_indistinguishable_across_account_states,
    passkey_recovery_start_mints_passkey_recovery_token_for_verified_user,
    passkey_recovery_start_mints_no_token_for_unverified_user,
    passkey_recovery_start_leaves_password_and_totp_auth_methods_untouched,
    lookup_recovery_user_db_error_returns_none_and_logs_error,
    lookup_recovery_user_absent_user_returns_none_without_error_log,
    recovery_start_service_returns_none_for_absent_and_unverified_users,
    recovery_start_service_lookup_error_returns_none_and_logs_error,
    recovery_start_service_token_mint_error_returns_none_and_logs_error,
    recovery_start_service_classification_is_ok_for_all_account_states,
    recovery_start_service_rate_limited_returns_err_with_retry_after_seconds,
    passkey_recovery_start_service_rate_limited_returns_err_with_retry_after_seconds,
    passkey_recovery_verify_mints_the_recovery_purpose,
    start_passkey_registration_mints_the_add_device_purpose,
);
