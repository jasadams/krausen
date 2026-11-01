// SPDX-License-Identifier: AGPL-3.0-or-later
use krausen_app::{ActorContext, Error, team_invitation_read};
use krausen_core::DbPool;

async fn exercise(db: DbPool) {
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let owner = format!("o-{suffix}");
    let member = format!("m-{suffix}");
    let workspace = format!("w-{suffix}");
    let second = format!("v-{suffix}");
    let credential = format!("c-{suffix}");
    let member_credential = format!("d-{suffix}");
    macro_rules! run { ($sql:expr $(, $arg:expr)*) => { match &db {
        DbPool::Postgres(p) => sqlx::query($sql)$(.bind($arg))*.execute(p).await.unwrap().rows_affected(),
        DbPool::Sqlite(p) => sqlx::query($sql)$(.bind($arg))*.execute(p).await.unwrap().rows_affected(),
    } }; }
    run!(
        "INSERT INTO users (user_id,email) VALUES ($1,$2),($3,$4)",
        &owner,
        format!("o-{suffix}@example.test"),
        &member,
        format!("m-{suffix}@example.test")
    );
    for id in [&workspace, &second] {
        run!(
            "INSERT INTO workspaces (workspace_id,owner_user_id,status) VALUES ($1,$2,'active')",
            id,
            &owner
        );
        run!(
            "INSERT INTO workspace_users (workspace_id,user_id,role,active) VALUES ($1,$2,'workspace_admin',TRUE)",
            id,
            &owner
        );
    }
    run!(
        "INSERT INTO workspace_users (workspace_id,user_id,role,active) VALUES ($1,$2,'workspace_user',TRUE)",
        &workspace,
        &member
    );
    for (id, owner_id) in [(&credential, &owner), (&member_credential, &member)] {
        run!(
            "INSERT INTO auth_credentials (credential_id,user_id,kind,issuer,audience,expires_at) VALUES ($1,$2,'browser','test','test',4102444800)",
            id,
            owner_id
        );
    }
    for n in 0..3 {
        let ws = if n == 2 { &second } else { &workspace };
        let expiry = if n == 1 {
            chrono::Utc::now() - chrono::Duration::days(1)
        } else {
            chrono::Utc::now() + chrono::Duration::days(1)
        };
        run!(
            "INSERT INTO workspace_invitations (invitation_id,workspace_id,email,role,invited_by_user_id,status,expires_at) VALUES ($1,$2,$3,'workspace_user',$4,'pending',$5)",
            format!("i{n}-{suffix}"),
            ws,
            format!("invite{n}-{suffix}@example.test"),
            &owner,
            expiry
        );
    }
    let admin = ActorContext {
        actor_id: owner,
        credential_id: credential,
        workspace_id: workspace.clone(),
        access_expires_at: 4102444800,
    };
    let ordinary = ActorContext {
        actor_id: member,
        credential_id: member_credential,
        ..admin.clone()
    };
    assert!(matches!(
        team_invitation_read::invitations(&db, &ordinary, None, 1).await,
        Err(Error::Forbidden)
    ));
    let first = team_invitation_read::invitations(&db, &admin, None, 1)
        .await
        .unwrap();
    assert_eq!(first.items.len(), 1);
    let foreign = ActorContext {
        workspace_id: second,
        ..admin.clone()
    };
    assert!(matches!(
        team_invitation_read::invitations(&db, &foreign, first.next_cursor.as_deref(), 1).await,
        Err(Error::Invalid(_))
    ));
    let second_page =
        team_invitation_read::invitations(&db, &admin, first.next_cursor.as_deref(), 1)
            .await
            .unwrap();
    assert_eq!(second_page.items.len(), 1);
    assert!(second_page.next_cursor.is_none());
    assert_ne!(
        first.items[0].invitation_id,
        second_page.items[0].invitation_id
    );
    assert!([&first.items[0], &second_page.items[0]].iter().any(|item| {
        chrono::DateTime::parse_from_rfc3339(&item.expires_at).unwrap() < chrono::Utc::now()
    }));
    assert!(matches!(
        team_invitation_read::invitations(&db, &admin, None, 101).await,
        Err(Error::Invalid(_))
    ));
    run!(
        "UPDATE workspace_users SET active=FALSE WHERE workspace_id=$1 AND user_id=$2",
        &workspace,
        &admin.actor_id
    );
    assert!(matches!(
        team_invitation_read::invitations(&db, &admin, None, 1).await,
        Err(Error::Forbidden)
    ));
}

#[tokio::test]
async fn sqlite_team_invitation_read() {
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
async fn postgres_team_invitation_read() {
    let url =
        std::env::var("KRAUSEN_TEST_POSTGRES_URL").expect("disposable PostgreSQL URL required");
    let pool = sqlx::PgPool::connect(&url).await.unwrap();
    sqlx::migrate!("../../apps/server/migrations")
        .run(&pool)
        .await
        .unwrap();
    exercise(DbPool::Postgres(pool)).await;
}
