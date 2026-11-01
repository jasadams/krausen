// SPDX-License-Identifier: AGPL-3.0-or-later
use krausen_app::{Error, account_read::workspaces, invitation::AccountActor};
use krausen_core::DbPool;

async fn exercise(db: DbPool) {
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let user = format!("u-{suffix}");
    let other = format!("o-{suffix}");
    let credential = format!("c-{suffix}");
    let other_credential = format!("d-{suffix}");
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
    for (id, actor) in [(&credential, &user), (&other_credential, &other)] {
        run!(
            "INSERT INTO auth_credentials (credential_id,user_id,kind,issuer,audience,expires_at) VALUES ($1,$2,'browser','test','test',$3)",
            id,
            actor,
            chrono::Utc::now().timestamp() + 3600
        );
    }
    let mut expected = Vec::new();
    for n in 0..3 {
        let id = format!("w{n}-{suffix}");
        run!(
            "INSERT INTO workspaces (workspace_id,owner_user_id,status) VALUES ($1,$2,'active')",
            &id,
            &user
        );
        run!(
            "INSERT INTO workspace_users (workspace_id,user_id,role,active) VALUES ($1,$2,'workspace_user',TRUE)",
            &id,
            &user
        );
        expected.push(id);
    }
    let actor = AccountActor {
        actor_id: user,
        credential_id: credential.clone(),
        access_expires_at: chrono::Utc::now().timestamp() + 3600,
    };
    let stranger = AccountActor {
        actor_id: other,
        credential_id: other_credential,
        access_expires_at: actor.access_expires_at,
    };
    let mut seen = Vec::new();
    let mut cursor = None;
    loop {
        let page = workspaces(&db, &actor, cursor.as_deref(), 1).await.unwrap();
        assert!(page.items.len() <= 1);
        seen.extend(page.items.into_iter().map(|item| item.workspace_id));
        if let Some(value) = &page.next_cursor {
            assert!(matches!(
                workspaces(&db, &stranger, Some(value), 1).await,
                Err(Error::Invalid(_))
            ));
        }
        cursor = page.next_cursor;
        if cursor.is_none() {
            break;
        }
        assert!(seen.len() <= 3, "pagination must advance");
    }
    assert_eq!(seen, expected);
    assert!(
        workspaces(&db, &stranger, None, 50)
            .await
            .unwrap()
            .items
            .is_empty()
    );
    assert!(matches!(
        workspaces(&db, &actor, None, 101).await,
        Err(Error::Invalid(_))
    ));
    run!(
        "UPDATE auth_credentials SET revoked_at=$1 WHERE credential_id=$2",
        chrono::Utc::now().timestamp(),
        &credential
    );
    assert!(matches!(
        workspaces(&db, &actor, None, 50).await,
        Err(Error::Forbidden)
    ));
}

#[tokio::test]
async fn sqlite_account_workspaces() {
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
async fn postgres_account_workspaces() {
    let url =
        std::env::var("KRAUSEN_TEST_POSTGRES_URL").expect("disposable PostgreSQL URL required");
    let pool = sqlx::PgPool::connect(&url).await.unwrap();
    sqlx::migrate!("../../apps/server/migrations")
        .run(&pool)
        .await
        .unwrap();
    exercise(DbPool::Postgres(pool)).await;
}
