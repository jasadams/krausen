// SPDX-License-Identifier: AGPL-3.0-or-later

//! WebAuthn challenge purpose binding for the authenticated add-device flow
//! (KYO-279), ported from Kyomi's `security_service.rs` test module and run on
//! SQLite and PostgreSQL.
//!
//! `complete_passkey_registration` is the authenticated add-device consumer.
//! The credential fixture is a structurally valid capture from webauthn-rs-core
//! (real JSON shape, cryptographically inert against the test RP), so a
//! challenge that clears both the purpose gate and the user-match check always
//! fails *later*, at real WebAuthn verification, which is exactly what proves
//! those gates, specifically, let it through.

use krausen_core::{DbPool, KVPool};
use url::Url;

use super::*;
use crate::auth_service::passkey_tests::REGISTER_CREDENTIAL_JSON;
use crate::auth_service::signup_tests::{both_backends, email, postgres_db, sqlite_db};
use crate::webauthn_challenge_purpose as purpose;

fn test_webauthn() -> Webauthn {
    crate::webauthn::build_webauthn(
        "localhost",
        "Krausen Test",
        &Url::parse("http://localhost:8080").unwrap(),
    )
    .expect("build webauthn")
}

fn test_credential() -> RegisterPublicKeyCredential {
    serde_json::from_str(REGISTER_CREDENTIAL_JSON).expect("parse fixture credential")
}

fn unique_user_id(tag: &str) -> String {
    format!("{tag}-{}", uuid::Uuid::new_v4().simple())
}

async fn complete_with(
    db: &DbPool,
    purpose_field: Option<&str>,
    challenge_user_id: &str,
    authenticated_user_id: &str,
    include_registration_state: bool,
) -> krausen_core::Result<String> {
    let kv: KVPool = krausen_core::kv_store_memory::InMemoryKVStore::new_pool();
    let webauthn = test_webauthn();

    let challenge_email = email("add-device");
    let challenge_id = unique_user_id("add-device-challenge");
    let mut challenge_data = serde_json::json!({
        "email": challenge_email,
        "user_id": challenge_user_id,
        "device_name": "Test Device",
    });
    if include_registration_state {
        let (_ccr, reg_state) = crate::webauthn::start_registration(
            &webauthn,
            Uuid::new_v4(),
            &challenge_email,
            "User",
            None,
        )
        .expect("start registration");
        challenge_data["registration_state"] =
            serde_json::to_value(&reg_state).expect("serialize reg state");
    }
    if let Some(p) = purpose_field {
        challenge_data["purpose"] = serde_json::json!(p);
    }
    crate::redis_ops::store_webauthn_challenge(&kv, &challenge_id, &challenge_data)
        .await
        .expect("store challenge");

    let credential = test_credential();
    complete_passkey_registration(
        db,
        &kv,
        &webauthn,
        authenticated_user_id,
        &challenge_id,
        &credential,
    )
    .await
}

fn assert_invalid_or_expired_challenge(result: krausen_core::Result<String>) {
    match result {
        Err(krausen_core::Error::Internal(msg)) => {
            assert_eq!(msg, "Invalid or expired challenge");
        }
        other => panic!(
            "expected Error::Internal(\"Invalid or expired challenge\"), got {other:?}"
        ),
    }
}

async fn rejects_challenge_minted_for_login(db: &DbPool) {
    let user_id = unique_user_id("u");
    let result = complete_with(db, Some(purpose::PASSKEY_LOGIN), &user_id, &user_id, false).await;
    assert_invalid_or_expired_challenge(result);
}

async fn rejects_challenge_minted_for_signup(db: &DbPool) {
    let user_id = unique_user_id("u");
    let result = complete_with(db, Some(purpose::PASSKEY_SIGNUP), &user_id, &user_id, false).await;
    assert_invalid_or_expired_challenge(result);
}

async fn rejects_challenge_minted_for_recovery(db: &DbPool) {
    let user_id = unique_user_id("u");
    let result =
        complete_with(db, Some(purpose::PASSKEY_RECOVERY), &user_id, &user_id, false).await;
    assert_invalid_or_expired_challenge(result);
}

async fn rejects_missing_purpose(db: &DbPool) {
    let user_id = unique_user_id("u");
    let result = complete_with(db, None, &user_id, &user_id, false).await;
    assert_invalid_or_expired_challenge(result);
}

async fn rejects_unknown_purpose(db: &DbPool) {
    let user_id = unique_user_id("u");
    let result = complete_with(db, Some("bogus"), &user_id, &user_id, false).await;
    assert_invalid_or_expired_challenge(result);
}

/// Correct purpose (and matching user_id) clears both gates; verification then
/// fails for an unrelated reason (the fixture credential doesn't match this
/// reg_state): a *different* error than either gate's rejection, proving
/// neither gate blocked it.
async fn accepts_its_own_purpose(db: &DbPool) {
    let user_id = unique_user_id("u");
    let result = complete_with(
        db,
        Some(purpose::PASSKEY_ADD_DEVICE),
        &user_id,
        &user_id,
        true,
    )
    .await;
    let err = result.expect_err("fixture credential must fail real verification");
    let msg = err.to_string();
    assert_ne!(msg, "internal: Invalid or expired challenge");
    assert_ne!(msg, "internal: Challenge does not match authenticated user");
    assert!(
        msg.contains("registration failed"),
        "expected a WebAuthn verification failure, got: {msg}"
    );
}

/// The pre-existing ownership check (KYO-279 keeps this, purpose is additive)
/// must still reject when the purpose is correct but the challenge belongs to a
/// different user than the authenticated caller.
async fn still_rejects_user_id_mismatch_with_correct_purpose(db: &DbPool) {
    let result = complete_with(
        db,
        Some(purpose::PASSKEY_ADD_DEVICE),
        &unique_user_id("victim-user-id"),
        &unique_user_id("attacker-user-id"),
        false,
    )
    .await;
    match result {
        Err(krausen_core::Error::Internal(msg)) => {
            assert_eq!(msg, "Challenge does not match authenticated user");
        }
        other => panic!(
            "expected Error::Internal(\"Challenge does not match authenticated user\"), got {other:?}"
        ),
    }
}

both_backends!(
    rejects_challenge_minted_for_login,
    rejects_challenge_minted_for_signup,
    rejects_challenge_minted_for_recovery,
    rejects_missing_purpose,
    rejects_unknown_purpose,
    accepts_its_own_purpose,
    still_rejects_user_id_mismatch_with_correct_purpose,
);
