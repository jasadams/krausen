// SPDX-License-Identifier: AGPL-3.0-or-later
use krausen_app::{ActorContext, Error, team_read::members};
use krausen_core::DbPool;

async fn exercise(db: DbPool) {
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let owner = format!("o-{suffix}");
    let member = format!("m-{suffix}");
    let stranger = format!("s-{suffix}");
    let workspace = format!("w-{suffix}");
    let second = format!("v-{suffix}");
    let credential = format!("c-{suffix}");
    let other_credential = format!("d-{suffix}");
    macro_rules! run { ($sql:expr $(, $arg:expr)*) => { match &db {
        DbPool::Postgres(p) => sqlx::query($sql)$(.bind($arg))*.execute(p).await.unwrap().rows_affected(),
        DbPool::Sqlite(p) => sqlx::query($sql)$(.bind($arg))*.execute(p).await.unwrap().rows_affected(),
    } }; }
    for (id, address) in [
        (&owner, format!("owner-{suffix}@example.test")),
        (&member, format!("member-{suffix}@example.test")),
        (&stranger, format!("other-{suffix}@example.test")),
    ] {
        run!(
            "INSERT INTO users (user_id,email) VALUES ($1,$2)",
            id,
            address
        );
    }
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
        run!(
            "INSERT INTO workspace_users (workspace_id,user_id,role,active) VALUES ($1,$2,'workspace_user',TRUE)",
            id,
            &member
        );
    }
    for (id, user) in [(&credential, &member), (&other_credential, &stranger)] {
        run!(
            "INSERT INTO auth_credentials (credential_id,user_id,kind,issuer,audience,expires_at) VALUES ($1,$2,'mcp','test','test',$3)",
            id,
            user,
            chrono::Utc::now().timestamp() + 3600
        );
    }
    let actor = ActorContext {
        actor_id: member,
        workspace_id: workspace.clone(),
        credential_id: credential.clone(),
        access_expires_at: chrono::Utc::now().timestamp() + 3600,
    };
    let other = ActorContext {
        actor_id: stranger,
        credential_id: other_credential,
        ..actor.clone()
    };
    assert!(matches!(
        members(&db, &other, None, 50).await,
        Err(Error::Forbidden)
    ));
    let first = members(&db, &actor, None, 1).await.unwrap();
    assert_eq!(first.items.len(), 1);
    let cursor = first.next_cursor.as_deref().unwrap();
    let foreign = ActorContext {
        workspace_id: second,
        ..actor.clone()
    };
    assert!(matches!(
        members(&db, &foreign, Some(cursor), 1).await,
        Err(Error::Invalid(_))
    ));
    let next = members(&db, &actor, Some(cursor), 1).await.unwrap();
    assert_eq!(next.items.len(), 1);
    assert!(next.next_cursor.is_none());
    let combined = [first.items[0].clone(), next.items[0].clone()];
    assert!(
        combined
            .iter()
            .any(|item| item.user_id == owner && item.is_owner)
    );
    assert!(
        combined
            .iter()
            .any(|item| item.user_id == actor.actor_id && item.role == "workspace_user")
    );
    assert!(matches!(
        members(&db, &actor, None, 101).await,
        Err(Error::Invalid(_))
    ));
    run!(
        "UPDATE workspace_users SET active=FALSE WHERE workspace_id=$1 AND user_id=$2",
        &workspace,
        &actor.actor_id
    );
    assert!(matches!(
        members(&db, &actor, None, 50).await,
        Err(Error::Forbidden)
    ));
}

#[tokio::test]
async fn sqlite_team_members() {
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
async fn postgres_team_members() {
    let url =
        std::env::var("KRAUSEN_TEST_POSTGRES_URL").expect("disposable PostgreSQL URL required");
    let pool = sqlx::PgPool::connect(&url).await.unwrap();
    sqlx::migrate!("../../apps/server/migrations")
        .run(&pool)
        .await
        .unwrap();
    exercise(DbPool::Postgres(pool)).await;
}
