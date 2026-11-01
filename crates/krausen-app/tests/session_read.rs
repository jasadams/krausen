// SPDX-License-Identifier: AGPL-3.0-or-later
use krausen_app::{Error, invitation::AccountActor, session_read};
use krausen_core::DbPool;

async fn exercise(db: DbPool) {
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let user = format!("u-{suffix}");
    let other = format!("o-{suffix}");
    let family = format!("f-{suffix}");
    let other_family = format!("g-{suffix}");
    let second_family = format!("h-{suffix}");
    macro_rules! run { ($sql:expr $(, $arg:expr)*) => { match &db {
        DbPool::Postgres(p) => sqlx::query($sql)$(.bind($arg))*.execute(p).await.unwrap().rows_affected(),
        DbPool::Sqlite(p) => sqlx::query($sql)$(.bind($arg))*.execute(p).await.unwrap().rows_affected(),
    } }; }
    run!(
        "INSERT INTO users (user_id,email) VALUES ($1,$2),($3,$4)",
        &user,
        format!("u-{suffix}@example.test"),
        &other,
        format!("o-{suffix}@example.test")
    );
    for (credential, owner) in [(&family, &user), (&other_family, &other)] {
        run!(
            "INSERT INTO auth_credentials (credential_id,user_id,kind,issuer,audience,expires_at) VALUES ($1,$2,'browser','test','test',4102444800)",
            credential,
            owner
        );
    }
    let expiry = chrono::Utc::now() + chrono::Duration::days(1);
    for n in 0..3 {
        let token = format!("t{n}-{suffix}");
        let owner = if n == 2 { &other } else { &user };
        let f = if n == 0 {
            &family
        } else if n == 1 {
            &second_family
        } else {
            &other_family
        };
        run!(
            "INSERT INTO refresh_tokens (token_id,user_id,token_hash,family_id,expires_at,is_active) VALUES ($1,$2,$3,$4,$5,TRUE)",
            &token,
            owner,
            format!("secret-hash-{n}-{suffix}"),
            f,
            expiry
        );
    }
    let old_expiry = chrono::Utc::now() - chrono::Duration::days(1);
    run!(
        "INSERT INTO refresh_tokens (token_id,user_id,token_hash,family_id,expires_at,is_active) VALUES ($1,$2,$3,$4,$5,TRUE)",
        format!("expired-{suffix}"),
        &user,
        format!("expired-hash-{suffix}"),
        &family,
        old_expiry
    );
    run!(
        "INSERT INTO refresh_tokens (token_id,user_id,token_hash,family_id,expires_at,is_active,replaced_at) VALUES ($1,$2,$3,$4,$5,TRUE,$6)",
        format!("replaced-{suffix}"),
        &user,
        format!("replaced-hash-{suffix}"),
        &family,
        expiry,
        chrono::Utc::now()
    );
    let actor = AccountActor {
        actor_id: user.clone(),
        credential_id: family.clone(),
        access_expires_at: 4102444800,
    };
    let stranger = AccountActor {
        actor_id: other,
        credential_id: other_family,
        access_expires_at: 4102444800,
    };
    let first = session_read::sessions(&db, &actor, None, 1).await.unwrap();
    assert_eq!(first.items.len(), 1);
    assert!(matches!(
        session_read::sessions(&db, &stranger, first.next_cursor.as_deref(), 1).await,
        Err(Error::Invalid(_))
    ));
    let second = session_read::sessions(&db, &actor, first.next_cursor.as_deref(), 1)
        .await
        .unwrap();
    assert_eq!(second.items.len(), 1);
    assert!(second.next_cursor.is_none());
    assert_eq!(
        [first.items[0].is_current, second.items[0].is_current]
            .iter()
            .filter(|v| **v)
            .count(),
        1
    );
    assert!(
        !serde_json::to_string(&first)
            .unwrap()
            .contains("secret-hash")
    );
    assert!(matches!(
        session_read::sessions(&db, &actor, None, 101).await,
        Err(Error::Invalid(_))
    ));
    run!(
        "UPDATE auth_credentials SET revoked_at=$1 WHERE credential_id=$2",
        chrono::Utc::now().timestamp(),
        &family
    );
    assert!(matches!(
        session_read::sessions(&db, &actor, None, 1).await,
        Err(Error::Forbidden)
    ));
}

#[tokio::test]
async fn sqlite_session_read() {
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
async fn postgres_session_read() {
    let url =
        std::env::var("KRAUSEN_TEST_POSTGRES_URL").expect("disposable PostgreSQL URL required");
    let pool = sqlx::PgPool::connect(&url).await.unwrap();
    sqlx::migrate!("../../apps/server/migrations")
        .run(&pool)
        .await
        .unwrap();
    exercise(DbPool::Postgres(pool)).await;
}
