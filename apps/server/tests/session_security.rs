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
    let sessions = krausen_app::security::sessions(&db, &actor)
        .await
        .unwrap()
        .items;
    run!(
        "INSERT INTO user_auth_methods (user_id,auth_type,auth_data,active) VALUES ($1,'webauthn','{\"credentials\":{\"public-id\":{\"device_name\":\"Test key\",\"private_material\":\"never-expose\"}}}',TRUE)",
        &user
    );
    let old_password = "start-password-sentinel";
    let new_password = "next-password-sentinel";
    let old_hash = krausen_auth::password::hash_password(old_password).unwrap();
    let auth_data = serde_json::json!({"hash":old_hash}).to_string();
    let insert = if db.is_postgres() {
        "INSERT INTO user_auth_methods (user_id,auth_type,auth_data,active) VALUES ($1,'password',CAST($2 AS JSON),TRUE)"
    } else {
        "INSERT INTO user_auth_methods (user_id,auth_type,auth_data,active) VALUES ($1,'password',$2,TRUE)"
    };
    run!(insert, &user, &auth_data);
    let status = krausen_app::security::status(&db, &actor).await.unwrap();
    assert!(status.has_password);
    assert!(!status.totp_enabled);
    let key = b"test-only-password-commitment-key-32-bytes";
    let password_request = uuid::Uuid::new_v4();
    assert!(matches!(
        krausen_app::security::change_password(
            &db,
            &actor,
            "incorrect".into(),
            new_password.into(),
            status.revision,
            uuid::Uuid::new_v4(),
            key
        )
        .await,
        Err(Error::InvalidPassword)
    ));
    if db.is_postgres() {
        run!(
            "CREATE OR REPLACE FUNCTION test_fail_password_change() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.operation='account.password.change.v1' THEN RAISE EXCEPTION 'test rollback'; END IF; RETURN NEW; END $$"
        );
        run!(
            "CREATE TRIGGER test_fail_password_change BEFORE INSERT ON app_changes FOR EACH ROW EXECUTE FUNCTION test_fail_password_change()"
        );
    } else {
        run!(
            "CREATE TRIGGER test_fail_password_change BEFORE INSERT ON app_changes WHEN NEW.operation='account.password.change.v1' BEGIN SELECT RAISE(ABORT,'test rollback'); END"
        );
    }
    assert!(
        krausen_app::security::change_password(
            &db,
            &actor,
            old_password.into(),
            new_password.into(),
            status.revision,
            password_request,
            key
        )
        .await
        .is_err()
    );
    if db.is_postgres() {
        run!("DROP TRIGGER test_fail_password_change ON app_changes");
        run!("DROP FUNCTION test_fail_password_change()");
    } else {
        run!("DROP TRIGGER test_fail_password_change");
    }
    let revision = krausen_app::security::change_password(
        &db,
        &actor,
        old_password.into(),
        new_password.into(),
        status.revision,
        password_request,
        key,
    )
    .await
    .unwrap();
    assert_eq!(revision, status.revision + 1);
    let changed_data:String=match &db { DbPool::Postgres(p)=>sqlx::query_scalar("SELECT CAST(auth_data AS TEXT) FROM user_auth_methods WHERE user_id=$1 AND auth_type='password'").bind(&user).fetch_one(p).await.unwrap(),DbPool::Sqlite(p)=>sqlx::query_scalar("SELECT CAST(auth_data AS TEXT) FROM user_auth_methods WHERE user_id=$1 AND auth_type='password'").bind(&user).fetch_one(p).await.unwrap() };
    let changed_data: serde_json::Value = serde_json::from_str(&changed_data).unwrap();
    let changed_hash = changed_data["hash"].as_str().unwrap();
    assert!(krausen_auth::password::verify_password(new_password, changed_hash).unwrap());
    assert!(!krausen_auth::password::verify_password(old_password, changed_hash).unwrap());
    assert_eq!(
        revision,
        krausen_app::security::change_password(
            &db,
            &actor,
            old_password.into(),
            new_password.into(),
            status.revision,
            password_request,
            key
        )
        .await
        .unwrap()
    );
    let other_credential_actor = ActorContext {
        credential_id: rollback.clone(),
        ..actor.clone()
    };
    assert!(matches!(
        krausen_app::security::change_password(
            &db,
            &other_credential_actor,
            old_password.into(),
            new_password.into(),
            status.revision,
            password_request,
            key
        )
        .await,
        Err(Error::Forbidden)
    ));
    assert!(matches!(
        krausen_app::security::change_password(
            &db,
            &actor,
            old_password.into(),
            "different-password".into(),
            status.revision,
            password_request,
            key
        )
        .await,
        Err(Error::RequestIdentityConflict)
    ));
    assert!(matches!(
        krausen_app::security::change_password(
            &db,
            &actor,
            old_password.into(),
            new_password.into(),
            status.revision,
            password_request,
            b"rotated-password-commitment-key-32-bytes"
        )
        .await,
        Err(Error::RetryKeyUnavailable)
    ));
    let expired_actor = ActorContext {
        access_expires_at: chrono::Utc::now().timestamp() - 1,
        ..actor.clone()
    };
    assert!(matches!(
        krausen_app::security::change_password(
            &db,
            &expired_actor,
            old_password.into(),
            new_password.into(),
            status.revision,
            password_request,
            key
        )
        .await,
        Err(Error::Forbidden)
    ));
    let retained:String=match &db { DbPool::Postgres(p)=>sqlx::query_scalar("SELECT payload_digest || COALESCE(result_json,'') FROM app_requests WHERE actor_id=$1 AND operation='account.password.change.v1'").bind(&user).fetch_one(p).await.unwrap(),DbPool::Sqlite(p)=>sqlx::query_scalar("SELECT payload_digest || COALESCE(result_json,'') FROM app_requests WHERE actor_id=$1 AND operation='account.password.change.v1'").bind(&user).fetch_one(p).await.unwrap() };
    assert!(
        !retained.contains(old_password)
            && !retained.contains(new_password)
            && !retained.contains("$argon2")
    );
    let (first_change, second_change) = tokio::join!(
        krausen_app::security::change_password(
            &db,
            &actor,
            new_password.into(),
            "race-password-one".into(),
            revision,
            uuid::Uuid::new_v4(),
            key
        ),
        krausen_app::security::change_password(
            &db,
            &actor,
            new_password.into(),
            "race-password-two".into(),
            revision,
            uuid::Uuid::new_v4(),
            key
        )
    );
    assert_ne!(first_change.is_ok(), second_change.is_ok());
    assert!(
        matches!(first_change, Err(Error::Conflict { .. }))
            || matches!(second_change, Err(Error::Conflict { .. }))
    );
    assert_eq!(
        revision,
        krausen_app::security::change_password(
            &db,
            &actor,
            old_password.into(),
            new_password.into(),
            status.revision,
            password_request,
            key
        )
        .await
        .unwrap()
    );
    run!(
        "UPDATE app_requests SET result_expires_at=$1 WHERE actor_id=$2 AND operation='account.password.change.v1' AND request_id=$3",
        chrono::Utc::now().timestamp() - 1,
        &user,
        password_request.to_string()
    );
    assert!(matches!(
        krausen_app::security::change_password(
            &db,
            &actor,
            old_password.into(),
            new_password.into(),
            status.revision,
            password_request,
            key
        )
        .await,
        Err(Error::ResultExpired { .. })
    ));
    let summaries = krausen_app::security::passkeys(&db, &actor).await.unwrap();
    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0].name, "Test key");
    assert!(
        !serde_json::to_string(&summaries)
            .unwrap()
            .contains("never-expose")
    );
    assert!(
        krausen_app::security::passkeys(&db, &foreign_actor)
            .await
            .unwrap()
            .is_empty()
    );
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
            .items
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
    assert!(matches!(
        krausen_app::security::change_password(
            &db,
            &actor,
            old_password.into(),
            new_password.into(),
            status.revision,
            password_request,
            key
        )
        .await,
        Err(Error::Forbidden)
    ));
    assert_eq!(
        krausen_app::security::sessions(&db, &foreign_actor)
            .await
            .unwrap()
            .items
            .len(),
        1
    );
}

async fn exercise_credential_management(db: DbPool, kv: krausen_core::KVPool) {
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let user = format!("credential-user-{suffix}");
    let other = format!("credential-other-{suffix}");
    let ws = format!("credential-ws-{suffix}");
    let main = format!("credential-main-{suffix}");
    let foreign = format!("credential-foreign-{suffix}");
    macro_rules! run { ($sql:expr $(,$arg:expr)*) => { match &db { DbPool::Postgres(p)=>sqlx::query($sql)$(.bind($arg))*.execute(p).await.unwrap().rows_affected(),DbPool::Sqlite(p)=>sqlx::query($sql)$(.bind($arg))*.execute(p).await.unwrap().rows_affected() } }; }
    for (id, email) in [
        (&user, format!("{suffix}@example.test")),
        (&other, format!("other-{suffix}@example.test")),
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
    for (id, owner) in [(&main, &user), (&foreign, &other)] {
        run!(
            "INSERT INTO auth_credentials (credential_id,user_id,kind,issuer,audience,expires_at) VALUES ($1,$2,'browser','test','test',$3)",
            id,
            owner,
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
        actor_id: other.clone(),
        workspace_id: String::new(),
        credential_id: foreign.clone(),
        access_expires_at: expiry,
    };
    let wrong_credential_actor = ActorContext {
        credential_id: foreign.clone(),
        ..actor.clone()
    };
    let key = b"test-only-password-commitment-key-32-bytes";
    let password = "credential-management-password";

    assert!(matches!(
        krausen_app::security::set_password(
            &db,
            &actor,
            "short".into(),
            0,
            uuid::Uuid::new_v4(),
            key
        )
        .await,
        Err(Error::Invalid(_))
    ));
    let set_request = uuid::Uuid::new_v4();
    assert_eq!(
        krausen_app::security::set_password(&db, &actor, password.into(), 0, set_request, key)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        krausen_app::security::set_password(&db, &actor, password.into(), 0, set_request, key)
            .await
            .unwrap(),
        1
    );
    assert!(matches!(
        krausen_app::security::set_password(
            &db,
            &actor,
            "another-password".into(),
            1,
            uuid::Uuid::new_v4(),
            key
        )
        .await,
        Err(Error::PasswordAlreadySet)
    ));
    assert!(matches!(
        krausen_app::security::set_password(
            &db,
            &wrong_credential_actor,
            "cross-account-password".into(),
            0,
            uuid::Uuid::new_v4(),
            key
        )
        .await,
        Err(Error::Forbidden)
    ));
    assert!(
        !krausen_app::security::status(&db, &foreign_actor)
            .await
            .unwrap()
            .has_password
    );

    assert!(matches!(
        krausen_app::security::setup_totp(&db, &kv, &actor, 0).await,
        Err(Error::Conflict { .. })
    ));
    let setup = krausen_app::security::setup_totp(&db, &kv, &actor, 1)
        .await
        .unwrap();
    assert!(setup.qr_uri.starts_with("data:image/png;base64,"));
    let current = krausen_auth::totp::current_code(&setup.secret).unwrap();
    let invalid = format!("{:06}", (current.parse::<u32>().unwrap() + 1) % 1_000_000);
    assert!(matches!(
        krausen_app::security::enable_totp(
            &db,
            &kv,
            &actor,
            &invalid,
            1,
            uuid::Uuid::new_v4(),
            key
        )
        .await,
        Err(Error::InvalidCode)
    ));
    assert_eq!(
        krausen_app::security::enable_totp(
            &db,
            &kv,
            &actor,
            &current,
            1,
            uuid::Uuid::new_v4(),
            key
        )
        .await
        .unwrap(),
        2
    );
    let status = krausen_app::security::status(&db, &actor).await.unwrap();
    assert!(status.has_password && status.totp_enabled);
    assert_eq!(status.revision, 2);
    assert_eq!(
        krausen_app::security::disable_totp(&db, &actor, 2, uuid::Uuid::new_v4())
            .await
            .unwrap(),
        3
    );
    assert!(matches!(
        krausen_app::security::disable_totp(&db, &actor, 3, uuid::Uuid::new_v4()).await,
        Err(Error::TotpNotEnabled)
    ));
    let retained: String = match &db {
        DbPool::Postgres(p) => {
            sqlx::query_scalar(
                "SELECT string_agg(payload_digest || COALESCE(result_json,''),' ') FROM app_requests WHERE actor_id=$1",
            )
            .bind(&user)
            .fetch_one(p)
            .await
            .unwrap()
        }
        DbPool::Sqlite(p) => {
            sqlx::query_scalar(
                "SELECT group_concat(payload_digest || COALESCE(result_json,''),' ') FROM app_requests WHERE actor_id=$1",
            )
            .bind(&user)
            .fetch_one(p)
            .await
            .unwrap()
        }
    };
    assert!(
        !retained.contains(password)
            && !retained.contains(&setup.secret)
            && !retained.contains("secret")
    );

    let webauthn_data = serde_json::json!({
        "credentials": {
            "passkey-one": {
                "public_key": "test-public-key",
                "sign_count": 0,
                "device_name": "Old device",
                "created_at": "2026-01-01T00:00:00Z",
                "last_used": null,
                "passkey": {}
            }
        }
    })
    .to_string();
    let insert = if db.is_postgres() {
        "INSERT INTO user_auth_methods (user_id,auth_type,auth_data,active) VALUES ($1,'webauthn',CAST($2 AS JSON),TRUE) ON CONFLICT (user_id,auth_type) DO UPDATE SET auth_data=EXCLUDED.auth_data, active=TRUE"
    } else {
        "INSERT INTO user_auth_methods (user_id,auth_type,auth_data,active) VALUES ($1,'webauthn',$2,1) ON CONFLICT (user_id,auth_type) DO UPDATE SET auth_data=EXCLUDED.auth_data, active=1"
    };
    run!(insert, &user, &webauthn_data);
    assert!(matches!(
        krausen_app::security::rename_passkey(
            &db,
            &actor,
            "missing-passkey",
            "New name",
            3,
            uuid::Uuid::new_v4()
        )
        .await,
        Err(Error::NotFound)
    ));
    assert_eq!(
        krausen_app::security::rename_passkey(
            &db,
            &actor,
            "passkey-one",
            "Renamed device",
            3,
            uuid::Uuid::new_v4()
        )
        .await
        .unwrap(),
        4
    );
    let renamed = krausen_app::security::passkeys(&db, &actor).await.unwrap();
    assert_eq!(renamed.len(), 1);
    assert_eq!(renamed[0].name, "Renamed device");
    assert!(matches!(
        krausen_app::security::rename_passkey(
            &db,
            &wrong_credential_actor,
            "passkey-one",
            "Cross account",
            4,
            uuid::Uuid::new_v4()
        )
        .await,
        Err(Error::Forbidden)
    ));
    assert_eq!(
        krausen_app::security::delete_passkey(&db, &actor, "passkey-one", 4, uuid::Uuid::new_v4())
            .await
            .unwrap(),
        5
    );

    // A passkey-only account cannot remove its final sign-in method.
    let only = format!("passkey-only-{suffix}");
    run!(
        "INSERT INTO users (user_id,email,active,verified) VALUES ($1,$2,TRUE,TRUE)",
        &only,
        format!("only-{suffix}@example.test")
    );
    let only_credential = format!("credential-only-{suffix}");
    run!(
        "INSERT INTO auth_credentials (credential_id,user_id,kind,issuer,audience,expires_at) VALUES ($1,$2,'browser','test','test',$3)",
        &only_credential,
        &only,
        expiry
    );
    let only_actor = ActorContext {
        actor_id: only.clone(),
        workspace_id: String::new(),
        credential_id: only_credential,
        access_expires_at: expiry,
    };
    run!(insert, &only, &webauthn_data);
    assert!(matches!(
        krausen_app::security::delete_passkey(
            &db,
            &only_actor,
            "passkey-one",
            0,
            uuid::Uuid::new_v4()
        )
        .await,
        Err(Error::LastSignInMethod)
    ));
    let password_data = serde_json::json!({"hash":"not-a-real-verifier"}).to_string();
    let insert_password = if db.is_postgres() {
        "INSERT INTO user_auth_methods (user_id,auth_type,auth_data,active) VALUES ($1,'password',CAST($2 AS JSON),TRUE)"
    } else {
        "INSERT INTO user_auth_methods (user_id,auth_type,auth_data,active) VALUES ($1,'password',$2,TRUE)"
    };
    run!(insert_password, &only, &password_data);
    assert_eq!(
        krausen_app::security::delete_passkey(
            &db,
            &only_actor,
            "passkey-one",
            0,
            uuid::Uuid::new_v4()
        )
        .await
        .unwrap(),
        1
    );

    let origin = url::Url::parse("http://localhost:3000").unwrap();
    let webauthn =
        krausen_auth::webauthn::build_webauthn("localhost", "Krausen Test", &origin).unwrap();
    run!(insert, &only, &webauthn_data);
    assert!(matches!(
        krausen_app::security::start_passkey_registration(
            &db,
            &kv,
            &webauthn,
            &only_actor,
            "Second device",
            99
        )
        .await,
        Err(Error::Conflict { .. })
    ));
    let registration = krausen_app::security::start_passkey_registration(
        &db,
        &kv,
        &webauthn,
        &only_actor,
        "Second device",
        1,
    )
    .await
    .unwrap();
    assert!(!registration.challenge_id.is_empty());
    assert!(registration.options.get("publicKey").is_some());
    assert!(matches!(
        krausen_app::security::start_passkey_registration(
            &db,
            &kv,
            &webauthn,
            &wrong_credential_actor,
            "Foreign device",
            0
        )
        .await,
        Err(Error::Forbidden)
    ));

    // The session inventory is bounded and reports truncation instead of
    // silently dropping rows or returning an unbounded set.
    for index in 0..=krausen_app::INVENTORY_LIMIT {
        run!(
            "INSERT INTO auth_credentials (credential_id,user_id,kind,issuer,audience,expires_at) VALUES ($1,$2,'browser','test','test',$3)",
            format!("credential-extra-{index}-{suffix}"),
            &user,
            expiry
        );
    }
    let page = krausen_app::security::sessions(&db, &actor).await.unwrap();
    assert_eq!(page.items.len(), krausen_app::INVENTORY_LIMIT);
    assert!(page.truncated);
}

#[tokio::test]
async fn sqlite_credential_management() {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("./migrations-sqlite")
        .run(&pool)
        .await
        .unwrap();
    let kv = krausen_core::create_kv_store(None).await.unwrap();
    exercise_credential_management(DbPool::Sqlite(pool), kv).await;
}

#[tokio::test]
async fn postgres_credential_management() {
    let url =
        std::env::var("KRAUSEN_TEST_POSTGRES_URL").expect("disposable PostgreSQL URL required");
    let pool = sqlx::PgPool::connect(&url).await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    let kv = krausen_core::create_kv_store(None).await.unwrap();
    exercise_credential_management(DbPool::Postgres(pool), kv).await;
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
