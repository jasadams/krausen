// SPDX-License-Identifier: AGPL-3.0-or-later
//! Exercise the actual refresh algorithm against the shared revocation operation.
use krausen_app::{invitation::AccountActor, session_revoke};
use krausen_auth::token_service::{self, DeviceInfo};
use krausen_core::DbPool;
use krausen_types::session_revoke::{SessionRevokeCommand, SessionRevokeOutcome};

async fn exercise(db: DbPool) {
    let _ = krausen_core::constants::load_with_fallback();
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let user = format!("race-{suffix}");
    let caller = format!("mcp-{suffix}");
    let family = format!("family-{suffix}");
    krausen_core::db_execute!(
        &db,
        "INSERT INTO users (user_id,email) VALUES ($1,$2)",
        &user,
        format!("{user}@example.test")
    )
    .unwrap();
    krausen_core::db_execute!(&db, "INSERT INTO auth_credentials (credential_id,user_id,kind,issuer,audience,expires_at) VALUES ($1,$2,'mcp','test','test',4102444800)", &caller, &user).unwrap();
    let device = DeviceInfo {
        user_agent: None,
        ip_address: None,
        country_code: None,
        oauth_client_id: None,
    };
    let raw = format!("refresh-secret-{suffix}");
    let observed = token_service::store_refresh_token(
        &db,
        &user,
        &token_service::hash_refresh_token(&raw),
        chrono::Utc::now() + chrono::Duration::days(1),
        &device,
        &family,
    )
    .await
    .unwrap();
    let actor = AccountActor {
        actor_id: user,
        credential_id: caller,
        access_expires_at: 4102444800,
    };
    let command = SessionRevokeCommand {
        expected_current_token_id: observed,
        request_id: uuid::Uuid::new_v4().to_string(),
    };
    let (revocation, refresh) = tokio::join!(
        session_revoke::revoke(&db, &actor, &command),
        token_service::rotate_browser_refresh(&db, &raw, &device)
    );
    match (revocation.unwrap(), refresh) {
        (SessionRevokeOutcome::Revoked { .. }, Err(_)) => {}
        (SessionRevokeOutcome::Conflict, Ok(_)) => {}
        _ => panic!(
            "rotation and revocation must serialize without revoking a newly rotated session"
        ),
    }
}

#[tokio::test]
async fn sqlite_refresh_revoke_race() {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("../../apps/server/migrations-sqlite")
        .run(&pool)
        .await
        .unwrap();
    exercise(DbPool::Sqlite(pool)).await;
}

#[tokio::test]
async fn postgres_refresh_revoke_race() {
    let url =
        std::env::var("KRAUSEN_TEST_POSTGRES_URL").expect("disposable PostgreSQL URL required");
    let pool = sqlx::PgPool::connect(&url).await.unwrap();
    sqlx::migrate!("../../apps/server/migrations")
        .run(&pool)
        .await
        .unwrap();
    exercise(DbPool::Postgres(pool)).await;
}
