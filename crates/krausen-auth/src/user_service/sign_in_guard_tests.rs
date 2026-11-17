// SPDX-License-Identifier: AGPL-3.0-or-later

//! `remove_auth_method`'s last-sign-in-method guard (KYO-701) and the
//! `google_is_only_sign_in_method` hint, ported from Kyomi's `user_service.rs`
//! test module and run on SQLite and PostgreSQL.
//!
//! Exercises [`guard_last_sign_in_method`] through the public
//! `remove_auth_method` entry point rather than calling the private helper
//! directly, so these tests keep pinning the same contract callers rely on.
//! An inactive row is produced the way production produces one: by removing a
//! method that had a sign-in sibling, never by writing the table directly.

use krausen_core::DbPool;

use super::*;
use crate::auth_service::signup_tests::{both_backends, email, postgres_db, sqlite_db};

async fn new_user(db: &DbPool, tag: &str) -> String {
    create_user(db, &email(tag), Some("Brewer"), true)
        .await
        .expect("create user")
        .user_id
}

async fn seed(db: &DbPool, user_id: &str, auth_type: &str) {
    upsert_auth_method(db, user_id, auth_type, &serde_json::json!({}))
        .await
        .expect("seed auth method");
}

async fn google_only_hint_uses_active_sign_in_methods(db: &DbPool) {
    let hint_user = new_user(db, "hint-user").await;
    let other_user = new_user(db, "other-user").await;
    assert!(!google_is_only_sign_in_method(db, &hint_user).await.unwrap());
    seed(db, &hint_user, "google_oauth").await;
    seed(db, &hint_user, "totp").await;
    // An inactive password row: added, then removed while google_oauth still
    // stood beside it (so the guard allows the removal).
    seed(db, &hint_user, "password").await;
    remove_auth_method(db, &hint_user, "password").await.unwrap();
    seed(db, &other_user, "password").await;
    assert!(google_is_only_sign_in_method(db, &hint_user).await.unwrap());
    for auth_type in ["password", "webauthn"] {
        upsert_auth_method(db, &hint_user, auth_type, &serde_json::json!({}))
            .await
            .unwrap();
        assert!(!google_is_only_sign_in_method(db, &hint_user).await.unwrap());
        remove_auth_method(db, &hint_user, auth_type).await.unwrap();
        assert!(google_is_only_sign_in_method(db, &hint_user).await.unwrap());
    }
}

async fn removing_the_only_sign_in_method_is_refused_and_leaves_it_active(db: &DbPool) {
    let user_id = new_user(db, "only-method").await;
    seed(db, &user_id, "google_oauth").await;

    let result = remove_auth_method(db, &user_id, "google_oauth").await;

    assert!(
        matches!(result, Err(krausen_core::Error::Conflict(_))),
        "removing the only sign-in method must be refused with Conflict, got {result:?}"
    );

    let still_active = get_auth_method(db, &user_id, "google_oauth").await.unwrap();
    assert!(
        still_active.is_some(),
        "the row must remain active after a refused removal"
    );
}

async fn removing_google_oauth_with_a_password_present_succeeds(db: &DbPool) {
    let user_id = new_user(db, "google-and-password").await;
    seed(db, &user_id, "google_oauth").await;
    upsert_auth_method(db, &user_id, "password", &serde_json::json!({"hash": "x"}))
        .await
        .unwrap();

    let result = remove_auth_method(db, &user_id, "google_oauth").await;

    assert!(result.unwrap(), "removal must succeed when a password also exists");
}

async fn removing_google_oauth_with_a_passkey_present_succeeds(db: &DbPool) {
    let user_id = new_user(db, "google-and-passkey").await;
    seed(db, &user_id, "google_oauth").await;
    upsert_auth_method(db, &user_id, "webauthn", &serde_json::json!({"credentials": {}}))
        .await
        .unwrap();

    let result = remove_auth_method(db, &user_id, "google_oauth").await;

    assert!(result.unwrap(), "removal must succeed when a passkey also exists");
}

/// Proves the sign-in-type distinction is load-bearing: totp is the only active
/// row here, but it isn't a sign-in method, so removing it must never be
/// refused.
async fn removing_totp_as_the_only_active_row_is_not_blocked(db: &DbPool) {
    let user_id = new_user(db, "only-totp").await;
    upsert_auth_method(db, &user_id, "totp", &serde_json::json!({"secret": "x"}))
        .await
        .unwrap();

    let result = remove_auth_method(db, &user_id, "totp").await;

    assert!(
        result.unwrap(),
        "totp is a second factor, not a sign-in method, and must never be blocked"
    );
}

/// The case a naive "count every active row" guard gets wrong: google_oauth +
/// totp is 2 active rows, but totp alone can never authenticate this user, so
/// removing google_oauth must still be refused.
async fn removing_google_oauth_with_only_totp_present_is_refused(db: &DbPool) {
    let user_id = new_user(db, "google-and-totp").await;
    seed(db, &user_id, "google_oauth").await;
    upsert_auth_method(db, &user_id, "totp", &serde_json::json!({"secret": "x"}))
        .await
        .unwrap();

    let result = remove_auth_method(db, &user_id, "google_oauth").await;

    assert!(
        matches!(result, Err(krausen_core::Error::Conflict(_))),
        "totp cannot sign the user in on its own, so google_oauth is still the last \
         sign-in method; got {result:?}"
    );

    let still_active = get_auth_method(db, &user_id, "google_oauth").await.unwrap();
    assert!(still_active.is_some());
}

async fn an_inactive_sibling_row_does_not_count_toward_the_total(db: &DbPool) {
    let user_id = new_user(db, "inactive-sibling").await;
    seed(db, &user_id, "google_oauth").await;
    // A previously-removed password method: present in the table, but
    // inactive, so it must not save google_oauth from being "the last".
    seed(db, &user_id, "password").await;
    remove_auth_method(db, &user_id, "password").await.unwrap();

    let result = remove_auth_method(db, &user_id, "google_oauth").await;

    assert!(
        matches!(result, Err(krausen_core::Error::Conflict(_))),
        "an inactive sibling row must not count as a sign-in method; got {result:?}"
    );
}

/// `notify_existing_verified_account` (signup-start goes through it) reads the
/// account's active auth methods via `list_active_auth_types` to tell the owner
/// how they can sign in. Assert that query returns exactly the seeded, active
/// methods: not a hardcoded guess, and not methods that were removed.
async fn list_active_auth_types_reflects_seeded_and_removed_methods(db: &DbPool) {
    let user_id = new_user(db, "signup-start-auth-methods").await;

    upsert_auth_method(db, &user_id, "password", &serde_json::json!({"hash": "test-hash"}))
        .await
        .expect("seed password auth method");
    upsert_auth_method(
        db,
        &user_id,
        "google_oauth",
        &serde_json::json!({"google_id": "test-google-id"}),
    )
    .await
    .expect("seed google_oauth auth method");

    let methods = list_active_auth_types(db, &user_id)
        .await
        .expect("list_active_auth_types should not error");
    assert_eq!(
        methods,
        vec!["google_oauth".to_string(), "password".to_string()],
        "must reflect exactly the seeded active auth methods"
    );

    remove_auth_method(db, &user_id, "google_oauth")
        .await
        .expect("remove google_oauth auth method");

    let methods_after_removal = list_active_auth_types(db, &user_id)
        .await
        .expect("list_active_auth_types should not error");
    assert_eq!(
        methods_after_removal,
        vec!["password".to_string()],
        "a removed (inactive) auth method must not be reported"
    );
}

both_backends!(
    google_only_hint_uses_active_sign_in_methods,
    removing_the_only_sign_in_method_is_refused_and_leaves_it_active,
    removing_google_oauth_with_a_password_present_succeeds,
    removing_google_oauth_with_a_passkey_present_succeeds,
    removing_totp_as_the_only_active_row_is_not_blocked,
    removing_google_oauth_with_only_totp_present_is_refused,
    an_inactive_sibling_row_does_not_count_toward_the_total,
    list_active_auth_types_reflects_seeded_and_removed_methods,
);
