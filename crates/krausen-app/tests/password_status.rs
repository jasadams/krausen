// SPDX-License-Identifier: AGPL-3.0-or-later
use krausen_app::{
    Error,
    account_read::profile,
    invitation::AccountActor,
    security_status::{password_status, totp_status},
};
use krausen_core::DbPool;

async fn exercise(db: DbPool) {
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let user = format!("u-{suffix}");
    let other = format!("o-{suffix}");
    let credential = format!("c-{suffix}");
    macro_rules! run { ($sql:expr $(, $arg:expr)*) => { match &db {
        DbPool::Postgres(p) => sqlx::query($sql)$(.bind($arg))*.execute(p).await.unwrap().rows_affected(),
        DbPool::Sqlite(p) => sqlx::query($sql)$(.bind($arg))*.execute(p).await.unwrap().rows_affected(),
    } }; }
    run!(
        "INSERT INTO users (user_id,email) VALUES ($1,$2),($3,$4)",
        &user,
        format!("{suffix}@example.test"),
        &other,
        format!("other-{suffix}@example.test")
    );
    run!(
        "INSERT INTO auth_credentials (credential_id,user_id,kind,issuer,audience,expires_at) VALUES ($1,$2,'browser','test','test',$3)",
        &credential,
        &user,
        chrono::Utc::now().timestamp() + 3600
    );
    let actor = AccountActor {
        actor_id: user.clone(),
        credential_id: credential.clone(),
        access_expires_at: chrono::Utc::now().timestamp() + 3600,
    };
    assert!(!password_status(&db, &actor).await.unwrap());
    assert!(!totp_status(&db, &actor).await.unwrap());
    let initial = profile(&db, &actor).await.unwrap();
    assert_eq!(initial.user_id, user);
    assert_eq!(initial.email, format!("{suffix}@example.test"));
    assert_eq!(initial.theme, "system");
    run!(
        "INSERT INTO user_auth_methods (user_id,auth_type,auth_data,active) VALUES ($1,'password',$2,TRUE)",
        &user,
        serde_json::json!({"hash":"fixture"})
    );
    assert!(password_status(&db, &actor).await.unwrap());
    run!(
        "INSERT INTO user_auth_methods (user_id,auth_type,auth_data,active) VALUES ($1,'totp',$2,TRUE)",
        &user,
        serde_json::json!({"fixture":true})
    );
    assert!(totp_status(&db, &actor).await.unwrap());
    run!(
        "UPDATE users SET name=$1,extra_metadata=$2 WHERE user_id=$3",
        "Brewer",
        serde_json::json!({"theme":"dark"}),
        &user
    );
    let updated = profile(&db, &actor).await.unwrap();
    assert_eq!(updated.name.as_deref(), Some("Brewer"));
    assert_eq!(updated.theme, "dark");
    let forged = AccountActor {
        actor_id: other,
        credential_id: credential.clone(),
        access_expires_at: actor.access_expires_at,
    };
    assert!(matches!(
        password_status(&db, &forged).await,
        Err(Error::Forbidden)
    ));
    assert!(matches!(
        totp_status(&db, &forged).await,
        Err(Error::Forbidden)
    ));
    assert!(matches!(profile(&db, &forged).await, Err(Error::Forbidden)));
    run!(
        "UPDATE auth_credentials SET revoked_at=$1 WHERE credential_id=$2",
        chrono::Utc::now().timestamp(),
        &credential
    );
    assert!(matches!(
        password_status(&db, &actor).await,
        Err(Error::Forbidden)
    ));
    assert!(matches!(
        totp_status(&db, &actor).await,
        Err(Error::Forbidden)
    ));
    assert!(matches!(profile(&db, &actor).await, Err(Error::Forbidden)));
}

#[tokio::test]
async fn sqlite_password_status() {
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
async fn postgres_password_status() {
    let url =
        std::env::var("KRAUSEN_TEST_POSTGRES_URL").expect("disposable PostgreSQL URL required");
    let pool = sqlx::PgPool::connect(&url).await.unwrap();
    sqlx::migrate!("../../apps/server/migrations")
        .run(&pool)
        .await
        .unwrap();
    exercise(DbPool::Postgres(pool)).await;
}
