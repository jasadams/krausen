// SPDX-License-Identifier: AGPL-3.0-or-later
//! Ownership, transactional rollback and revocation ordering for credentials.
use krausen_app::{ActorContext, Error};
use krausen_core::DbPool;

async fn exercise(db: DbPool) {
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let user = format!("session-user-{suffix}");
    let outsider = format!("session-other-{suffix}");
    let ws = format!("session-ws-{suffix}");
    let main = format!("session-main-{suffix}");
    let target = format!("session-target-{suffix}");
    let rollback = format!("session-rollback-{suffix}");
    let foreign = format!("session-foreign-{suffix}");
    macro_rules! run { ($sql:expr $(,$arg:expr)*) => { match &db { DbPool::Postgres(p)=>sqlx::query($sql)$(.bind($arg))*.execute(p).await.unwrap().rows_affected(),DbPool::Sqlite(p)=>sqlx::query($sql)$(.bind($arg))*.execute(p).await.unwrap().rows_affected() } }; }
    for (id, email) in [
        (&user, format!("{suffix}@example.test")),
        (&outsider, format!("other-{suffix}@example.test")),
    ] {
        run!(
            "INSERT INTO users (user_id,email,active,verified) VALUES ($1,$2,TRUE,TRUE)",
            id,
            email
        );
    }
    run!(
        "INSERT INTO workspaces (workspace_id,owner_user_id,status) VALUES ($1,$2,'active')",
        &ws,
        &user
    );
    run!(
        "INSERT INTO workspace_users (workspace_id,user_id,role,active) VALUES ($1,$2,'workspace_admin',TRUE)",
        &ws,
        &user
    );
    let expiry = chrono::Utc::now().timestamp() + 3600;
    for (id, owner, kind) in [
        (&main, &user, "browser"),
        (&target, &user, "mcp"),
        (&rollback, &user, "browser"),
        (&foreign, &outsider, "browser"),
    ] {
        run!(
            "INSERT INTO auth_credentials (credential_id,user_id,kind,issuer,audience,expires_at) VALUES ($1,$2,$3,'test','test',$4)",
            id,
            owner,
            kind,
            expiry
        );
    }
    let actor = ActorContext {
        actor_id: user.clone(),
        workspace_id: ws.clone(),
        credential_id: main.clone(),
        access_expires_at: expiry,
    };
    let foreign_actor = ActorContext {
        actor_id: outsider.clone(),
        workspace_id: String::new(),
        credential_id: foreign.clone(),
        access_expires_at: expiry,
    };
    let sessions = krausen_app::security::sessions(&db, &actor).await.unwrap();
    run!("INSERT INTO user_auth_methods (user_id,auth_type,auth_data,active) VALUES ($1,'webauthn','{\"credentials\":{\"public-id\":{\"device_name\":\"Test key\",\"private_material\":\"never-expose\"}}}',TRUE)", &user);
    run!("INSERT INTO user_auth_methods (user_id,auth_type,auth_data,active) VALUES ($1,'password','{}',TRUE)", &user);
    let status=krausen_app::security::status(&db,&actor).await.unwrap();
    assert!(status.has_password);
    assert!(!status.totp_enabled);
    let summaries=krausen_app::security::passkeys(&db,&actor).await.unwrap();
    assert_eq!(summaries.len(),1);
    assert_eq!(summaries[0].name,"Test key");
    assert!(!serde_json::to_string(&summaries).unwrap().contains("never-expose"));
    assert!(krausen_app::security::passkeys(&db,&foreign_actor).await.unwrap().is_empty());
    assert_eq!(sessions.len(), 3);
    assert!(
        sessions
            .iter()
            .any(|s| s.credential_id == main && s.is_current)
    );
    assert!(!sessions.iter().any(|s| s.credential_id == foreign));
    assert!(matches!(
        krausen_app::security::revoke(&db, &foreign_actor, &target, 0, uuid::Uuid::new_v4()).await,
        Err(Error::Forbidden)
    ));
    assert!(matches!(
        krausen_app::security::revoke(&db, &actor, &target, 1, uuid::Uuid::new_v4()).await,
        Err(Error::Conflict { .. })
    ));
    if db.is_postgres() {
        run!(
            "CREATE OR REPLACE FUNCTION test_fail_session_change() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.operation='account.credential.revoke.v1' THEN RAISE EXCEPTION 'test rollback'; END IF; RETURN NEW; END $$"
        );
        run!(
            "CREATE TRIGGER test_fail_session_change BEFORE INSERT ON app_changes FOR EACH ROW EXECUTE FUNCTION test_fail_session_change()"
        );
    } else {
        run!(
            "CREATE TRIGGER test_fail_session_change BEFORE INSERT ON app_changes WHEN NEW.operation='account.credential.revoke.v1' BEGIN SELECT RAISE(ABORT,'test rollback'); END"
        );
    }
    assert!(
        krausen_app::security::revoke(&db, &actor, &rollback, 0, uuid::Uuid::new_v4())
            .await
            .is_err()
    );
    if db.is_postgres() {
        run!("DROP TRIGGER test_fail_session_change ON app_changes");
        run!("DROP FUNCTION test_fail_session_change()");
    } else {
        run!("DROP TRIGGER test_fail_session_change");
    }
    assert!(
        krausen_app::security::sessions(&db, &actor)
            .await
            .unwrap()
            .iter()
            .any(|s| s.credential_id == rollback)
    );
    let target_actor = ActorContext {
        credential_id: target.clone(),
        ..actor.clone()
    };
    let request = uuid::Uuid::new_v4();
    let (revoked, protected) = tokio::join!(
        krausen_app::security::revoke(&db, &actor, &target, 0, request),
        krausen_app::workspace_snapshot(&db, &target_actor)
    );
    assert_eq!(revoked.unwrap(), 1);
    assert!(
        protected.is_ok() || matches!(protected, Err(Error::Forbidden) | Err(Error::Retryable))
    );
    assert!(matches!(
        krausen_app::workspace_snapshot(&db, &target_actor).await,
        Err(Error::Forbidden)
    ));
    assert_eq!(
        krausen_app::security::revoke(&db, &actor, &target, 0, request)
            .await
            .unwrap(),
        1
    );
    assert!(matches!(
        krausen_app::security::revoke(&db, &actor, &rollback, 0, request).await,
        Err(Error::RequestIdentityConflict)
    ));
    let count = krausen_app::security::revoke_all(&db, &actor, uuid::Uuid::new_v4())
        .await
        .unwrap();
    assert_eq!(count, 2);
    assert!(matches!(
        krausen_app::security::sessions(&db, &actor).await,
        Err(Error::Forbidden)
    ));
    assert_eq!(
        krausen_app::security::sessions(&db, &foreign_actor)
            .await
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn sqlite_sessions() {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("./migrations-sqlite")
        .run(&pool)
        .await
        .unwrap();
    exercise(DbPool::Sqlite(pool)).await;
}
#[tokio::test]
async fn postgres_sessions() {
    let url =
        std::env::var("KRAUSEN_TEST_POSTGRES_URL").expect("disposable PostgreSQL URL required");
    let pool = sqlx::PgPool::connect(&url).await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    exercise(DbPool::Postgres(pool)).await;
}
