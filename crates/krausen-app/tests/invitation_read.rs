// SPDX-License-Identifier: AGPL-3.0-or-later
use krausen_app::{Error, invitation::AccountActor, invitation_read};
use krausen_core::DbPool;

async fn exercise(db: DbPool) {
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let user = format!("u-{suffix}");
    let other = format!("o-{suffix}");
    let workspace = format!("w-{suffix}");
    let credential = format!("c-{suffix}");
    let other_credential = format!("d-{suffix}");
    let email = format!("recipient-{suffix}@example.test");
    let other_email = format!("other-{suffix}@example.test");
    macro_rules! run { ($sql:expr $(, $arg:expr)*) => { match &db {
        DbPool::Postgres(p) => sqlx::query($sql)$(.bind($arg))*.execute(p).await.unwrap().rows_affected(),
        DbPool::Sqlite(p) => sqlx::query($sql)$(.bind($arg))*.execute(p).await.unwrap().rows_affected(),
    } }; }
    run!(
        "INSERT INTO users (user_id,email) VALUES ($1,$2),($3,$4)",
        &user,
        &email,
        &other,
        &other_email
    );
    run!(
        "INSERT INTO workspaces (workspace_id,owner_user_id,status) VALUES ($1,$2,'active')",
        &workspace,
        &other
    );
    for (id, owner) in [(&credential, &user), (&other_credential, &other)] {
        run!(
            "INSERT INTO auth_credentials (credential_id,user_id,kind,issuer,audience,expires_at) VALUES ($1,$2,'browser','test','test',4102444800)",
            id,
            owner
        );
    }
    let future = chrono::Utc::now() + chrono::Duration::days(1);
    for n in 0..3 {
        let destination = if n == 2 {
            other_email.clone()
        } else if n == 1 {
            email.to_uppercase()
        } else {
            email.clone()
        };
        run!(
            "INSERT INTO workspace_invitations (invitation_id,workspace_id,email,role,invited_by_user_id,status,expires_at) VALUES ($1,$2,$3,'workspace_user',$4,'pending',$5)",
            format!("i{n}-{suffix}"),
            &workspace,
            &destination,
            &other,
            future
        );
    }
    run!(
        "INSERT INTO workspace_invitations (invitation_id,workspace_id,email,role,invited_by_user_id,status,expires_at) VALUES ($1,$2,$3,'workspace_user',$4,'pending',$5)",
        format!("expired-{suffix}"),
        &workspace,
        &email,
        &other,
        chrono::Utc::now() - chrono::Duration::days(1)
    );
    run!(
        "INSERT INTO workspace_invitations (invitation_id,workspace_id,email,role,invited_by_user_id,status,expires_at) VALUES ($1,$2,$3,'workspace_user',$4,'declined',$5)",
        format!("declined-{suffix}"),
        &workspace,
        &email,
        &other,
        future
    );
    let actor = AccountActor {
        actor_id: user,
        credential_id: credential.clone(),
        access_expires_at: 4102444800,
    };
    let stranger = AccountActor {
        actor_id: other,
        credential_id: other_credential,
        access_expires_at: 4102444800,
    };
    let first = invitation_read::recipient_invitations(&db, &actor, None, 1)
        .await
        .unwrap();
    assert_eq!(first.items.len(), 1);
    assert!(matches!(
        invitation_read::recipient_invitations(&db, &stranger, first.next_cursor.as_deref(), 1)
            .await,
        Err(Error::Invalid(_))
    ));
    let second =
        invitation_read::recipient_invitations(&db, &actor, first.next_cursor.as_deref(), 1)
            .await
            .unwrap();
    assert_eq!(second.items.len(), 1);
    assert!(second.next_cursor.is_none());
    assert_ne!(first.items[0].invitation_id, second.items[0].invitation_id);
    assert!(matches!(
        invitation_read::recipient_invitations(&db, &actor, None, 101).await,
        Err(Error::Invalid(_))
    ));
    run!(
        "UPDATE auth_credentials SET revoked_at=$1 WHERE credential_id=$2",
        chrono::Utc::now().timestamp(),
        &credential
    );
    assert!(matches!(
        invitation_read::recipient_invitations(&db, &actor, None, 1).await,
        Err(Error::Forbidden)
    ));
}

#[tokio::test]
async fn sqlite_invitation_read() {
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
async fn postgres_invitation_read() {
    let url =
        std::env::var("KRAUSEN_TEST_POSTGRES_URL").expect("disposable PostgreSQL URL required");
    let pool = sqlx::PgPool::connect(&url).await.unwrap();
    sqlx::migrate!("../../apps/server/migrations")
        .run(&pool)
        .await
        .unwrap();
    exercise(DbPool::Postgres(pool)).await;
}
