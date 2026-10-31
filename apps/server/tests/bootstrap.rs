// SPDX-License-Identifier: AGPL-3.0-or-later
use krausen_auth::{
    auth_service::{
        LoginServiceResult, LoginWithPasswordParams, SignupStartParams, SignupStartServiceResult,
        login_with_password_service, signup_start_service,
    },
    token_service::DeviceInfo,
};
use krausen_core::DbPool;

async fn exercise(db: DbPool) {
    let _ = krausen_core::constants::load_with_fallback();
    let password = "example-operator-password";
    let (a, b) = tokio::join!(
        krausen_server::bootstrap_owner(&db, "first@example.test", password),
        krausen_server::bootstrap_owner(&db, "second@example.test", password)
    );
    assert_ne!(
        a.is_ok(),
        b.is_ok(),
        "exactly one concurrent bootstrap may succeed"
    );
    assert!(
        krausen_server::bootstrap_owner(&db, "third@example.test", password)
            .await
            .is_err()
    );
    macro_rules! run { ($sql:expr $(, $arg:expr)*) => { match &db {
        DbPool::Postgres(p) => sqlx::query($sql)$(.bind($arg))*.execute(p).await.unwrap().rows_affected(),
        DbPool::Sqlite(p) => sqlx::query($sql)$(.bind($arg))*.execute(p).await.unwrap().rows_affected(),
    } }; }
    run!("DELETE FROM workspace_users");
    run!("DELETE FROM workspaces");
    run!("DELETE FROM user_auth_methods");
    run!("DELETE FROM users");
    run!(
        "INSERT INTO users (user_id,email,name,verified,active) VALUES ('user-local','local@localhost','Local User',TRUE,TRUE)"
    );
    run!(
        "INSERT INTO workspaces (workspace_id,name,owner_user_id,status) VALUES ('workspace-local','My Workspace','user-local','active')"
    );
    run!(
        "INSERT INTO workspace_users (workspace_id,user_id,role,active) VALUES ('workspace-local','user-local','workspace_admin',TRUE)"
    );
    run!(
        "INSERT INTO recipes (workspace_id,recipe_id,name,document,native_migration_error) VALUES ('workspace-local','legacy','Legacy','original','Recovery required')"
    );
    let kv = krausen_core::kv_store_memory::InMemoryKVStore::new_pool();
    let device = DeviceInfo {
        user_agent: None,
        ip_address: None,
        country_code: None,
        oauth_client_id: None,
    };
    let signup = signup_start_service(SignupStartParams {
        db: &db,
        kv: &kv,
        jwt_secret: "secret",
        email: "other@example.test",
        name: Some("Other"),
        password: Some(password),
        ip: "test",
        device: &device,
        self_hosted: true,
        smtp_configured: false,
        frontend_url: "http://localhost",
        config: None,
    })
    .await
    .unwrap();
    assert!(matches!(signup, SignupStartServiceResult::Error { .. }));
    for kind in ["password", "webauthn", "totp"] {
        let sql = if db.is_postgres() {
            "INSERT INTO user_auth_methods (user_id,auth_type,auth_data) VALUES ('user-local',$1,CAST('{}' AS JSON))"
        } else {
            "INSERT INTO user_auth_methods (user_id,auth_type,auth_data) VALUES ('user-local',$1,'{}')"
        };
        run!(sql, kind);
        assert!(
            krausen_server::claim_legacy_personal_owner(&db, "owner@example.test", password)
                .await
                .is_err()
        );
        run!("DELETE FROM user_auth_methods WHERE auth_type = $1", kind);
    }
    run!(
        "INSERT INTO auth_credentials (credential_id,user_id,kind,issuer,audience,expires_at) VALUES ('prior','user-local','browser','test','test',4102444800)"
    );
    assert!(
        krausen_server::claim_legacy_personal_owner(&db, "owner@example.test", password)
            .await
            .is_err()
    );
    run!("DELETE FROM auth_credentials WHERE credential_id = 'prior'");
    let (a, b) = tokio::join!(
        krausen_server::claim_legacy_personal_owner(&db, "owner@example.test", password),
        krausen_server::claim_legacy_personal_owner(&db, "other@example.test", password)
    );
    assert_ne!(a.is_ok(), b.is_ok());
    let email = if a.is_ok() {
        "owner@example.test"
    } else {
        "other@example.test"
    };
    assert!(
        krausen_server::claim_legacy_personal_owner(&db, email, password)
            .await
            .is_err()
    );
    let login = login_with_password_service(LoginWithPasswordParams {
        db: &db,
        kv: &kv,
        jwt_secret: "test-secret-at-least-32-bytes-long",
        email,
        password,
        totp_code: None,
        ip: "test",
        device: &device,
    })
    .await
    .unwrap();
    let LoginServiceResult::Success(session) = login else {
        panic!("claimed owner must be able to log in")
    };
    let claims = krausen_auth::jwt::validate_browser_token(
        &session.access_token,
        "test-secret-at-least-32-bytes-long",
    )
    .unwrap()
    .claims;
    let actor = krausen_app::ActorContext {
        actor_id: "user-local".into(),
        workspace_id: "workspace-local".into(),
        credential_id: claims.extra["credential_id"].as_str().unwrap().into(),
        access_expires_at: claims.exp,
    };
    let recovered = krausen_app::recipe_migration::recover_legacy_recipe(&db, &actor, "legacy")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(recovered.0, "original");
}

#[tokio::test]
async fn sqlite_operator_bootstrap() {
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
async fn postgres_operator_bootstrap() {
    let url =
        std::env::var("KRAUSEN_TEST_POSTGRES_URL").expect("disposable PostgreSQL URL required");
    let admin = sqlx::PgPool::connect(&url).await.unwrap();
    let name = format!("krausen_bootstrap_{}", uuid::Uuid::new_v4().simple());
    sqlx::query(&format!("CREATE DATABASE {name}"))
        .execute(&admin)
        .await
        .unwrap();
    let mut db_url = url::Url::parse(&url).unwrap();
    db_url.set_path(&name);
    let pool = sqlx::PgPool::connect(db_url.as_str()).await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    exercise(DbPool::Postgres(pool.clone())).await;
    pool.close().await;
    sqlx::query(&format!("DROP DATABASE {name} WITH (FORCE)"))
        .execute(&admin)
        .await
        .unwrap();
}
