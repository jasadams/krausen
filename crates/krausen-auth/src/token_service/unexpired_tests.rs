// SPDX-License-Identifier: AGPL-3.0-or-later

//! Chronological-expiry regressions for `unexpired_token_sql`, ported from
//! Kyomi's `token_service.rs` tests (`chronological_expiry_regression`,
//! `chronological_verification_expiry_regression` and
//! `chronological_expiry_sqlite_exact_fractional_boundary`).
//!
//! Differences forced by Krausen: the two regression scenarios run on SQLite and
//! PostgreSQL through the shared both-backends harness instead of Kyomi's
//! separate `_sqlite`/`_postgres` wrappers; the harness gives each test its own
//! database, so Kyomi's trailing cleanup deletes are not needed; users are
//! created with `user_service::create_user` and addresses come from the
//! harness `email` helper; the exact-fractional-boundary test takes its own
//! SQLite database from the harness and seeds its user through
//! `create_user` rather than Kyomi's `seed_user`. Case tables and assertions are
//! unchanged.

use super::*;
use crate::{
    auth_service::signup_tests::{both_backends, email, postgres_db, sqlite_db},
    user_service,
};

fn test_device_info() -> DeviceInfo {
    DeviceInfo {
        user_agent: None,
        ip_address: None,
        country_code: None,
        oauth_client_id: None,
    }
}

async fn chronological_expiry_regression(db: &DbPool) {
    let _ = krausen_core::constants::load_with_fallback();
    let user = user_service::create_user(db, &email("chronological"), None, true)
        .await
        .unwrap();
    let device = test_device_info();
    // The live-path expired fixture shares today's date with the database clock.
    let now = Utc::now();
    let midnight = now.date_naive().and_hms_opt(0, 0, 0).unwrap().and_utc();
    let cases = [
        (midnight, false),
        (now - Duration::days(1), false),
        (now + Duration::days(1), true),
        (now + Duration::hours(1), true),
    ];
    let mut expected_sessions = 0;
    for (index, (expiry, valid)) in cases.into_iter().enumerate() {
        for sql_format in [false, true] {
            let raw = format!("rt_{}_{}_{}", user.user_id, index, sql_format);
            let family = generate_family_id();
            let id = store_refresh_token(
                db,
                &user.user_id,
                &hash_refresh_token(&raw),
                expiry,
                &device,
                &family,
            )
            .await
            .unwrap();
            if let DbPool::Sqlite(sq) = db {
                let text = if sql_format {
                    expiry.format("%Y-%m-%d %H:%M:%S%.3f").to_string()
                } else {
                    expiry.to_rfc3339()
                };
                sqlx::query("UPDATE refresh_tokens SET expires_at = $1 WHERE token_id = $2")
                    .bind(text)
                    .bind(&id)
                    .execute(sq)
                    .await
                    .unwrap();
            }
            assert_eq!(
                matches!(
                    verify_refresh_token(db, &raw).await.unwrap(),
                    RefreshTokenVerifyResult::Valid(_)
                ),
                valid,
                "case {index}, SQL format {sql_format}"
            );
            let listed = get_user_sessions(db, &user.user_id).await.unwrap();
            expected_sessions += usize::from(valid);
            assert_eq!(
                listed.len(),
                expected_sessions,
                "only unexpired current families are listed"
            );
            assert_eq!(listed.iter().any(|row| row.token_id == id), valid);
            let rotated = rotate_refresh_token(
                db,
                &id,
                &user.user_id,
                &family,
                &format!("new-{raw}"),
                now + Duration::days(7),
                &device,
            )
            .await;
            if valid {
                assert!(rotated.is_ok());
            } else {
                assert!(matches!(rotated, Err(krausen_core::Error::Unauthorized(_))));
            }
        }
    }
    // Expiry wins over theft detection: an expired replaced token must
    // not revoke a still-valid sibling in its family.
    let family = generate_family_id();
    let expired_raw = format!("rt_expired_replaced_{}", user.user_id);
    let expired_id = store_refresh_token(
        db,
        &user.user_id,
        &hash_refresh_token(&expired_raw),
        midnight,
        &device,
        &family,
    )
    .await
    .unwrap();
    let sibling_raw = format!("rt_live_sibling_{}", user.user_id);
    store_refresh_token(
        db,
        &user.user_id,
        &hash_refresh_token(&sibling_raw),
        now + Duration::days(1),
        &device,
        &family,
    )
    .await
    .unwrap();
    let replaced_at = now - Duration::hours(1);
    krausen_core::db_execute!(
        db,
        "UPDATE refresh_tokens SET replaced_at = $1 WHERE token_id = $2",
        &replaced_at,
        &expired_id
    )
    .unwrap();
    assert!(matches!(
        verify_refresh_token(db, &expired_raw).await.unwrap(),
        RefreshTokenVerifyResult::Invalid
    ));
    assert!(matches!(
        verify_refresh_token(db, &sibling_raw).await.unwrap(),
        RefreshTokenVerifyResult::Valid(_)
    ));
    // Persist the database's exact current boundary. Equality is expired;
    // conversion to RFC3339 must not turn that instant into a future value.
    let raw = format!("rt_boundary_{}", user.user_id);
    let family = generate_family_id();
    let id = store_refresh_token(
        db,
        &user.user_id,
        &hash_refresh_token(&raw),
        now + Duration::days(1),
        &device,
        &family,
    )
    .await
    .unwrap();
    let boundary = if db.is_postgres() {
        "NOW()"
    } else {
        "strftime('%Y-%m-%dT%H:%M:%fZ', 'now')"
    };
    let sql = format!("UPDATE refresh_tokens SET expires_at = {boundary} WHERE token_id = $1");
    krausen_core::db_execute!(db, &sql, &id).unwrap();
    assert!(matches!(
        verify_refresh_token(db, &raw).await.unwrap(),
        RefreshTokenVerifyResult::Invalid
    ));
    assert!(
        !get_user_sessions(db, &user.user_id)
            .await
            .unwrap()
            .iter()
            .any(|row| row.token_id == id)
    );
    assert!(matches!(
        rotate_refresh_token(
            db,
            &id,
            &user.user_id,
            &family,
            "boundary-replacement",
            now + Duration::days(7),
            &device
        )
        .await,
        Err(krausen_core::Error::Unauthorized(_))
    ));
}

async fn chronological_verification_expiry_regression(db: &DbPool) {
    let _ = krausen_core::constants::load_with_fallback();
    let email = email("chronological-verification");
    let raw = create_verification_token_with_expiry(db, &email, "email", Some(1.0))
        .await
        .unwrap();
    let expiry = Utc::now()
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_utc();
    krausen_core::db_execute!(
        db,
        "UPDATE verification_tokens SET expires_at = $1 WHERE email = $2",
        &expiry,
        &email
    )
    .unwrap();
    assert_eq!(
        verify_verification_token(db, &raw, "email").await.unwrap(),
        None
    );
    let future = Utc::now() + Duration::days(1);
    krausen_core::db_execute!(
        db,
        "UPDATE verification_tokens SET expires_at = $1 WHERE email = $2",
        &future,
        &email
    )
    .unwrap();
    assert_eq!(
        verify_verification_token(db, &raw, "email").await.unwrap(),
        Some(email.clone())
    );
    assert_eq!(
        verify_verification_token(db, &raw, "email").await.unwrap(),
        None
    );
}

both_backends!(
    chronological_expiry_regression,
    chronological_verification_expiry_regression,
);

#[tokio::test]
async fn chronological_expiry_sqlite_exact_fractional_boundary() {
    let _ = krausen_core::constants::load_with_fallback();
    let db = sqlite_db().await;
    let DbPool::Sqlite(sq) = &*db else {
        unreachable!("sqlite_db returns a SQLite pool")
    };
    let user = user_service::create_user(&db, &email("boundary"), None, true)
        .await
        .unwrap();
    // Fixed reference clock only for this predicate test: the production
    // verifier/rotation/listing paths are exercised above with the real clock.
    let predicate =
        unexpired_token_sql(false, "expires_at").replace("'now'", "'2026-01-15T12:00:00.500Z'");
    for expiry in [
        "2026-01-15T11:00:00Z",
        "2026-01-15 11:00:00",
        "2026-01-15T12:00:00.499Z",
        "2026-01-15 12:00:00.499",
        "2026-01-15T12:00:00.500Z",
        "2026-01-15 12:00:00.500",
        "2026-01-15T13:00:00.500+01:00",
    ] {
        sqlx::query("INSERT OR REPLACE INTO refresh_tokens (token_id, user_id, token_hash, expires_at, family_id) VALUES ('boundary', $1, 'boundary-hash', $2, 'boundary-family')")
            .bind(&user.user_id)
            .bind(expiry)
            .execute(sq)
            .await
            .unwrap();
        let count: i64 = sqlx::query_scalar(&format!(
            "SELECT COUNT(*) FROM refresh_tokens WHERE {predicate}"
        ))
        .fetch_one(sq)
        .await
        .unwrap();
        assert_eq!(count, 0, "expired or equal instant: {expiry}");
    }
    for expiry in [
        "2026-01-15T12:00:00.501Z",
        "2026-01-15 12:00:00.501",
        "2026-01-15T18:00:00Z",
    ] {
        sqlx::query("UPDATE refresh_tokens SET expires_at = $1 WHERE token_id = 'boundary'")
            .bind(expiry)
            .execute(sq)
            .await
            .unwrap();
        let count: i64 = sqlx::query_scalar(&format!(
            "SELECT COUNT(*) FROM refresh_tokens WHERE {predicate}"
        ))
        .fetch_one(sq)
        .await
        .unwrap();
        assert_eq!(count, 1, "future instant: {expiry}");
    }
}
