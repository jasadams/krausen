// SPDX-License-Identifier: AGPL-3.0-or-later
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use krausen_auth::{
    oauth_service::{self, AuthorizationRequest, Principal},
    session,
    token_service::DeviceInfo,
    user_service,
};
use krausen_core::DbPool;
use rmcp::{
    model::{CallToolRequestParams, ClientConfig, ProtocolVersion},
    service::{ClientLifecycleMode, ClientServiceExt},
    transport::{
        StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::sync::Arc;

async fn exercise(db: DbPool) {
    let _ = krausen_core::constants::load_with_fallback();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://localhost:{}", listener.local_addr().unwrap().port());
    let mut config = krausen_core::Config::test_config();
    config.base_url = base.clone();
    config.frontend_url = base.clone();
    config.jwt_secret = "test-secret-at-least-32-bytes-long".into();
    let kv = krausen_core::kv_store_memory::InMemoryKVStore::new_pool();
    let webauthn = krausen_auth::webauthn::build_webauthn(
        "localhost",
        "test",
        &url::Url::parse(&base).unwrap(),
    )
    .unwrap();
    let state = krausen_server::state::AppState {
        db: db.clone(),
        kv: kv.clone(),
        redis: None,
        config: Arc::new(config),
        encryption_key: Arc::new([0; 32]),
        webauthn: Arc::new(webauthn),
        ws_manager: krausen_auth::websocket::WebSocketManager::new(None, db.clone()),
        mcp_sessions: krausen_auth::mcp_session_manager::MCPSessionManager::new(kv.clone()),
    };
    let server = tokio::spawn(async move {
        axum::serve(listener, krausen_server::build_router(state))
            .await
            .unwrap()
    });
    let suffix = uuid::Uuid::new_v4();
    let email = format!("{suffix}@example.test");
    let user = user_service::create_user(&db, &email, Some("Test"), true)
        .await
        .unwrap();
    let ws =
        user_service::create_workspace_for_user(&db, &user.user_id, Some("Test"), &email, None)
            .await
            .unwrap();
    krausen_core::db_execute!(
        &db,
        "UPDATE workspaces SET status = 'active' WHERE workspace_id = $1",
        &ws
    )
    .unwrap();
    krausen_core::db_execute!(&db, "INSERT INTO recipes (workspace_id,recipe_id,name,document,native_migration_error) VALUES ($1,$2,'Broken','original source','Recovery required')", &ws, format!("r-{suffix}")).unwrap();
    let device = DeviceInfo {
        user_agent: None,
        ip_address: None,
        country_code: None,
        oauth_client_id: None,
    };
    let browser = session::create_authenticated_session(
        &db,
        &kv,
        "test-secret-at-least-32-bytes-long",
        &user,
        &device,
    )
    .await
    .unwrap();
    let claims = krausen_auth::jwt::validate_browser_token(
        &browser.access_token,
        "test-secret-at-least-32-bytes-long",
    )
    .unwrap()
    .claims;
    let principal = Principal {
        user_id: user.user_id,
        credential_id: claims.extra["credential_id"].as_str().unwrap().into(),
        access_expires_at: claims.exp,
    };
    let redirect = "http://127.0.0.1:34567/callback";
    let reg = oauth_service::register_client(&db, "SDK client", &[redirect.into()])
        .await
        .unwrap();
    let verifier = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
    let req = AuthorizationRequest {
        client_id: reg.client_id.clone(),
        redirect_uri: redirect.into(),
        resource: format!("{base}/mcp"),
        state: "state".into(),
        code_challenge: URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes())),
        code_challenge_method: "S256".into(),
        response_type: "code".into(),
    };
    let pending = oauth_service::begin_authorization(&db, &req, &req.resource)
        .await
        .unwrap();
    oauth_service::bind_pending(&db, &pending, &principal)
        .await
        .unwrap();
    let (_, _, code) = oauth_service::approve(&db, &pending, &principal)
        .await
        .unwrap();
    let tokens = oauth_service::exchange_code(
        &db,
        &base,
        &reg.client_id,
        None,
        redirect,
        &req.resource,
        &code,
        verifier,
    )
    .await
    .unwrap();
    // Exercise the browser-facing OAuth endpoints, not just the grant service.
    let http_oauth = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let metadata: serde_json::Value = http_oauth
        .get(format!("{base}/.well-known/oauth-authorization-server"))
        .header("Forwarded", "proto=https;host=attacker.example")
        .header("X-Forwarded-Host", "attacker.example")
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(metadata["issuer"], base);
    assert_eq!(
        metadata["token_endpoint"],
        format!("{base}/api/v1/oauth/token")
    );
    assert_eq!(
        metadata["token_endpoint_auth_methods_supported"],
        json!(["none", "client_secret_basic"])
    );
    let protected: serde_json::Value = http_oauth
        .get(format!("{base}/.well-known/oauth-protected-resource/mcp"))
        .header("X-Forwarded-Proto", "https")
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(protected["resource"], req.resource);
    assert_eq!(protected["authorization_servers"], json!([base]));
    let registration = http_oauth.post(format!("{base}/api/v1/oauth/register"))
        .json(&json!({"client_name":"HTTP client","redirect_uris":[redirect],"token_endpoint_auth_method":"none"})).send().await.unwrap();
    assert_eq!(registration.status(), reqwest::StatusCode::CREATED);
    let registered: serde_json::Value = registration.json().await.unwrap();
    let private_registration = http_oauth.post(format!("{base}/api/v1/oauth/register")).json(&json!({"client_name":"HTTP confidential", "redirect_uris":[redirect], "token_endpoint_auth_method":"client_secret_basic"})).send().await.unwrap();
    assert_eq!(private_registration.status(), reqwest::StatusCode::CREATED);
    let private_registration: serde_json::Value = private_registration.json().await.unwrap();
    assert!(private_registration["client_secret"].as_str().is_some());
    assert_eq!(private_registration["client_secret_expires_at"], 0);

    let http_client_id = registered["client_id"].as_str().unwrap();
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let started = http_oauth
        .get(format!("{base}/api/v1/oauth/authorize"))
        .query(&[
            ("client_id", http_client_id),
            ("redirect_uri", redirect),
            ("resource", req.resource.as_str()),
            ("state", "http-state"),
            ("code_challenge", challenge.as_str()),
            ("code_challenge_method", "S256"),
            ("response_type", "code"),
        ])
        .send()
        .await
        .unwrap();
    assert!(started.status().is_redirection());
    let continue_path = started.headers()[reqwest::header::LOCATION]
        .to_str()
        .unwrap();
    let pending_id = url::Url::parse(&format!("{base}{continue_path}"))
        .unwrap()
        .query_pairs()
        .find(|(k, _)| k == "state")
        .unwrap()
        .1
        .into_owned();
    let consent = http_oauth
        .get(format!("{base}{continue_path}"))
        .bearer_auth(&browser.access_token)
        .send()
        .await
        .unwrap();
    assert_eq!(consent.status(), reqwest::StatusCode::OK);
    let denied = http_oauth
        .post(format!("{base}/api/v1/oauth/authorize/approve"))
        .bearer_auth(&browser.access_token)
        .header("Origin", "https://evil.example")
        .form(&[("state", pending_id.as_str())])
        .send()
        .await
        .unwrap();
    assert_eq!(denied.status(), reqwest::StatusCode::FORBIDDEN);
    let approval = http_oauth
        .post(format!("{base}/api/v1/oauth/authorize/approve"))
        .bearer_auth(&browser.access_token)
        .header("Origin", &base)
        .form(&[("state", pending_id.as_str())])
        .send()
        .await
        .unwrap();
    assert!(approval.status().is_redirection());
    let location = url::Url::parse(
        approval.headers()[reqwest::header::LOCATION]
            .to_str()
            .unwrap(),
    )
    .unwrap();
    let http_code = location
        .query_pairs()
        .find(|(k, _)| k == "code")
        .unwrap()
        .1
        .into_owned();
    let exchange = http_oauth
        .post(format!("{base}/api/v1/oauth/token"))
        .form(&[
            ("grant_type", "authorization_code"),
            ("client_id", http_client_id),
            ("resource", req.resource.as_str()),
            ("code", http_code.as_str()),
            ("redirect_uri", redirect),
            ("code_verifier", verifier),
        ])
        .send()
        .await
        .unwrap();
    assert_eq!(exchange.status(), reqwest::StatusCode::OK);
    assert!(
        exchange.json::<serde_json::Value>().await.unwrap()["access_token"]
            .as_str()
            .is_some()
    );
    // Confidential client authentication is enforced by the HTTP token and
    // revocation endpoints as well as by the shared grant service.
    let private = oauth_service::register_client_with_method(
        &db,
        "private",
        &[redirect.into()],
        "client_secret_basic",
    )
    .await
    .unwrap();
    let mut private_request = req.clone();
    private_request.client_id = private.client_id.clone();
    let private_pending = oauth_service::begin_authorization(&db, &private_request, &req.resource)
        .await
        .unwrap();
    oauth_service::bind_pending(&db, &private_pending, &principal)
        .await
        .unwrap();
    let (_, _, private_code) = oauth_service::approve(&db, &private_pending, &principal)
        .await
        .unwrap();
    let private_secret = private.client_secret.as_deref().unwrap();
    let form = [
        ("grant_type", "authorization_code"),
        ("resource", req.resource.as_str()),
        ("code", private_code.as_str()),
        ("redirect_uri", redirect),
        ("code_verifier", verifier),
    ];
    let wrong = http_oauth
        .post(format!("{base}/api/v1/oauth/token"))
        .basic_auth(&private.client_id, Some("wrong"))
        .form(&form)
        .send()
        .await
        .unwrap();
    assert_eq!(wrong.status(), reqwest::StatusCode::UNAUTHORIZED);
    let granted = http_oauth
        .post(format!("{base}/api/v1/oauth/token"))
        .basic_auth(&private.client_id, Some(private_secret))
        .form(&form)
        .send()
        .await
        .unwrap();
    assert_eq!(granted.status(), reqwest::StatusCode::OK);
    let granted: serde_json::Value = granted.json().await.unwrap();
    let private_refresh = granted["refresh_token"].as_str().unwrap();
    let refresh = http_oauth
        .post(format!("{base}/api/v1/oauth/token"))
        .basic_auth(&private.client_id, Some(private_secret))
        .form(&[
            ("grant_type", "refresh_token"),
            ("resource", req.resource.as_str()),
            ("refresh_token", private_refresh),
        ])
        .send()
        .await
        .unwrap();
    assert_eq!(refresh.status(), reqwest::StatusCode::OK);
    let refresh: serde_json::Value = refresh.json().await.unwrap();
    let revoked = http_oauth
        .post(format!("{base}/api/v1/oauth/revoke"))
        .basic_auth(&private.client_id, Some(private_secret))
        .form(&[("token", refresh["refresh_token"].as_str().unwrap())])
        .send()
        .await
        .unwrap();
    assert_eq!(revoked.status(), reqwest::StatusCode::OK);
    let transport = || {
        StreamableHttpClientTransport::from_config(
            StreamableHttpClientTransportConfig::with_uri(req.resource.clone())
                .auth_header(tokens.access_token.clone()),
        )
    };
    let legacy = ClientConfig::default()
        .with_protocol_version(ProtocolVersion::V_2025_11_25)
        .serve_with_lifecycle(transport(), ClientLifecycleMode::Initialize)
        .await
        .unwrap();
    assert_eq!(
        legacy.peer_info().unwrap().protocol_version,
        ProtocolVersion::V_2025_11_25
    );
    let modern = ClientConfig::default()
        .serve_with_lifecycle(
            transport(),
            ClientLifecycleMode::Discover {
                preferred_versions: vec![ProtocolVersion::V_2026_07_28],
            },
        )
        .await
        .unwrap();
    assert_eq!(
        modern.peer_info().unwrap().protocol_version,
        ProtocolVersion::V_2026_07_28
    );
    assert!(
        legacy
            .list_tools(None)
            .await
            .unwrap()
            .tools
            .iter()
            .any(|t| t.name == "workspace_rename")
    );
    assert!(
        modern
            .list_tools(None)
            .await
            .unwrap()
            .tools
            .iter()
            .any(|t| t.name == "workspace_rename")
    );
    let recovery_args = json!({"workspace_id": ws, "recipe_id":format!("r-{suffix}")})
        .as_object()
        .unwrap()
        .clone();
    let recovered = modern
        .call_tool(
            CallToolRequestParams::new("recipe_recover_legacy").with_arguments(recovery_args),
        )
        .await
        .unwrap();
    assert_eq!(
        recovered.structured_content.unwrap()["source"],
        "original source"
    );
    let request_id = uuid::Uuid::new_v4().to_string();
    let args = json!({"workspace_id": ws, "name":"SDK name", "expected_revision":0, "request_id":request_id}).as_object().unwrap().clone();
    let first = legacy
        .call_tool(CallToolRequestParams::new("workspace_rename").with_arguments(args.clone()))
        .await
        .unwrap();
    let retry = modern
        .call_tool(CallToolRequestParams::new("workspace_rename").with_arguments(args))
        .await
        .unwrap();
    assert_eq!(first.structured_content, retry.structured_content);
    assert_eq!(first.structured_content.unwrap()["Saved"]["revision"], 1);
    let http = reqwest::Client::new();
    assert_eq!(
        http.post(&req.resource).send().await.unwrap().status(),
        reqwest::StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        http.post(&req.resource)
            .bearer_auth(&tokens.access_token)
            .header("Origin", "https://evil.example")
            .send()
            .await
            .unwrap()
            .status(),
        reqwest::StatusCode::FORBIDDEN
    );
    let rotated = oauth_service::refresh(
        &db,
        &base,
        &reg.client_id,
        None,
        &req.resource,
        &tokens.refresh_token,
    )
    .await
    .unwrap();
    assert!(
        oauth_service::refresh(
            &db,
            &base,
            &reg.client_id,
            None,
            &req.resource,
            &tokens.refresh_token
        )
        .await
        .is_err()
    );
    assert!(
        oauth_service::verify_access(&db, &base, &req.resource, &rotated.access_token)
            .await
            .is_err()
    );
    assert!(
        modern
            .call_tool(
                CallToolRequestParams::new("workspace_get")
                    .with_arguments(json!({"workspace_id":ws}).as_object().unwrap().clone())
            )
            .await
            .is_err()
    );
    server.abort();
}
#[tokio::test]
async fn sqlite_sdk_clients() {
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
async fn postgres_sdk_clients() {
    let url =
        std::env::var("KRAUSEN_TEST_POSTGRES_URL").expect("disposable PostgreSQL URL required");
    let pool = sqlx::PgPool::connect(&url).await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    exercise(DbPool::Postgres(pool)).await;
}
