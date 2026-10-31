// SPDX-License-Identifier: AGPL-3.0-or-later
//! HTTPS client through an isolated, local TLS-terminating reverse proxy.
//! Requires openssl and socat, and FRONTEND_URL=https://localhost for secure cookies.
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use krausen_auth::{
    oauth_service::{self, AuthorizationRequest},
    session,
    token_service::DeviceInfo,
    user_service,
};
use krausen_core::DbPool;
use rmcp::{
    model::{ClientConfig, ProtocolVersion},
    service::{ClientLifecycleMode, ClientServiceExt},
    transport::{
        StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    process::{Child, Command, Stdio},
    sync::Arc,
};
struct Proxy(Child, std::path::PathBuf);
impl Drop for Proxy {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
        let _ = std::fs::remove_dir_all(&self.1);
    }
}

#[tokio::test]
async fn https_authorization_and_sdk_through_reverse_proxy() {
    assert!(
        krausen_core::constants::get().cookies.secure,
        "run with an HTTPS FRONTEND_URL"
    );
    let dir = std::env::temp_dir().join(format!("krausen-tls-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&dir).unwrap();
    let key = dir.join("key.pem");
    let cert = dir.join("cert.pem");
    let status = Command::new("openssl")
        .args([
            "req",
            "-x509",
            "-newkey",
            "rsa:2048",
            "-nodes",
            "-days",
            "1",
            "-subj",
            "/CN=localhost",
            "-addext",
            "subjectAltName=DNS:localhost",
            "-addext",
            "basicConstraints=critical,CA:FALSE",
            "-keyout",
        ])
        .arg(&key)
        .arg("-out")
        .arg(&cert)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();
    assert!(status.success());
    let backend = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let backend_port = backend.local_addr().unwrap().port();
    let reservation = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let tls_port = reservation.local_addr().unwrap().port();
    drop(reservation);
    let base = format!("https://localhost:{tls_port}");
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("./migrations-sqlite")
        .run(&pool)
        .await
        .unwrap();
    let db = DbPool::Sqlite(pool);
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
        axum::serve(backend, krausen_server::build_router(state))
            .await
            .unwrap()
    });
    let child = Command::new("socat")
        .arg(format!(
            "OPENSSL-LISTEN:{tls_port},bind=127.0.0.1,reuseaddr,fork,cert={},key={},verify=0",
            cert.display(),
            key.display()
        ))
        .arg(format!("TCP:127.0.0.1:{backend_port}"))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let _proxy = Proxy(child, dir);
    let ca = reqwest::Certificate::from_pem(&std::fs::read(&cert).unwrap()).unwrap();
    let http = reqwest::Client::builder()
        .add_root_certificate(ca)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let metadata = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            match http
                .get(format!("{base}/.well-known/oauth-authorization-server"))
                .send()
                .await
            {
                Ok(response) => break response,
                Err(_) => tokio::time::sleep(std::time::Duration::from_millis(25)).await,
            }
        }
    })
    .await
    .expect("TLS proxy did not become ready");
    assert_eq!(metadata.status(), reqwest::StatusCode::OK);
    let metadata: serde_json::Value = metadata.json().await.unwrap();
    assert_eq!(metadata["issuer"], base);
    let email = format!("{}@example.test", uuid::Uuid::new_v4());
    let user = user_service::create_user(&db, &email, Some("TLS"), true)
        .await
        .unwrap();
    let ws = user_service::create_workspace_for_user(&db, &user.user_id, Some("TLS"), &email, None)
        .await
        .unwrap();
    krausen_core::db_execute!(
        &db,
        "UPDATE workspaces SET status = 'active' WHERE workspace_id = $1",
        &ws
    )
    .unwrap();
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
    let redirect = "http://127.0.0.1:34567/callback";
    let verifier = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
    let client = oauth_service::register_client(&db, "TLS client", &[redirect.into()])
        .await
        .unwrap();
    let resource = format!("{base}/mcp");
    let request = AuthorizationRequest {
        client_id: client.client_id.clone(),
        redirect_uri: redirect.into(),
        resource: resource.clone(),
        state: "tls-state".into(),
        code_challenge: URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes())),
        code_challenge_method: "S256".into(),
        response_type: "code".into(),
    };
    let pending = oauth_service::begin_authorization(&db, &request, &resource)
        .await
        .unwrap();
    let consent = http
        .get(format!("{base}/api/v1/oauth/authorize/continue"))
        .query(&[("state", &pending)])
        .bearer_auth(&browser.access_token)
        .send()
        .await
        .unwrap();
    assert_eq!(consent.status(), reqwest::StatusCode::OK);
    let approved = http
        .post(format!("{base}/api/v1/oauth/authorize/approve"))
        .header("Origin", &base)
        .bearer_auth(&browser.access_token)
        .form(&[("state", &pending)])
        .send()
        .await
        .unwrap();
    assert!(approved.status().is_redirection());
    let callback = url::Url::parse(
        approved.headers()[reqwest::header::LOCATION]
            .to_str()
            .unwrap(),
    )
    .unwrap();
    let code = callback
        .query_pairs()
        .find(|(k, _)| k == "code")
        .unwrap()
        .1
        .into_owned();
    let tokens = http
        .post(format!("{base}/api/v1/oauth/token"))
        .form(&[
            ("grant_type", "authorization_code"),
            ("client_id", client.client_id.as_str()),
            ("resource", resource.as_str()),
            ("code", code.as_str()),
            ("redirect_uri", redirect),
            ("code_verifier", verifier),
        ])
        .send()
        .await
        .unwrap();
    assert_eq!(tokens.status(), reqwest::StatusCode::OK);
    let tokens: serde_json::Value = tokens.json().await.unwrap();
    let sdk_ca = reqwest_13::Certificate::from_pem(&std::fs::read(&cert).unwrap()).unwrap();
    let sdk_http = reqwest_13::Client::builder()
        .add_root_certificate(sdk_ca)
        .build()
        .unwrap();
    let transport = StreamableHttpClientTransport::with_client(
        sdk_http,
        StreamableHttpClientTransportConfig::with_uri(resource)
            .auth_header(tokens["access_token"].as_str().unwrap()),
    );
    let peer = ClientConfig::default()
        .serve_with_lifecycle(
            transport,
            ClientLifecycleMode::Discover {
                preferred_versions: vec![ProtocolVersion::V_2026_07_28],
            },
        )
        .await
        .unwrap();
    assert!(
        peer.list_tools(None)
            .await
            .unwrap()
            .tools
            .iter()
            .any(|tool| tool.name == "workspace_rename")
    );
    assert_eq!(
        json!(peer.peer_info().unwrap().protocol_version),
        json!(ProtocolVersion::V_2026_07_28)
    );
    server.abort();
}
