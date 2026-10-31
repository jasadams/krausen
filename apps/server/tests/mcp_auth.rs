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
use std::sync::{Arc, Mutex, OnceLock};
use tracing_subscriber::{EnvFilter, filter::filter_fn, prelude::*};

const PASSWORD_SENTINEL: &str = "password-sentinel-never-log-81f";
const NEW_PASSWORD_SENTINEL: &str = "new-password-sentinel-never-log-82e";
const WRONG_PASSWORD_SENTINEL: &str = "wrong-password-sentinel-never-log-92a";
const UI_PASSWORD_SENTINEL: &str = "ui-password-sentinel-never-log-739";
const PROOF_SENTINEL: &str = "proof-sentinel-never-log-629";
const TOKEN_SENTINEL: &str = "token-sentinel-never-log-43d";
const RESPONSE_SENTINEL: &str = "response-sentinel-never-log-b67";
const NOTIFICATION_SENTINEL: &str = "notification-sentinel-never-log-0cd";

#[derive(Clone)]
struct LogCapture(Arc<Mutex<Vec<u8>>>);
impl std::io::Write for LogCapture {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for LogCapture {
    type Writer = LogCapture;
    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}
fn capture_logs() -> &'static Arc<Mutex<Vec<u8>>> {
    static CAPTURE: OnceLock<Arc<Mutex<Vec<u8>>>> = OnceLock::new();
    CAPTURE.get_or_init(|| {
        let buffer = Arc::new(Mutex::new(Vec::new()));
        let filter = EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new("trace,rmcp=trace"));
        tracing_subscriber::registry()
            .with(filter)
            .with(
                tracing_subscriber::fmt::layer()
                    .with_ansi(false)
                    .with_writer(LogCapture(buffer.clone()))
                    .with_filter(filter_fn(krausen_server::logging::allow_metadata)),
            )
            .try_init()
            .expect("install regression log capture");
        buffer
    })
}

async fn exercise(db: DbPool) {
    let logs = capture_logs();
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
    krausen_core::db_execute!(&db, "INSERT INTO recipes (workspace_id,recipe_id,name,document,native_migration_error) VALUES ($1,$2,'Broken',$3,'Recovery required')", &ws, format!("r-{suffix}"), RESPONSE_SENTINEL).unwrap();
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
    let discovered = modern.list_tools(None).await.unwrap();
    for tool in [
        "account_password_set",
        "account_totp_setup",
        "account_totp_enable",
        "account_totp_disable",
        "account_passkey_register_begin",
        "account_passkey_register_complete",
        "account_passkey_rename",
        "account_passkey_delete",
    ] {
        assert!(
            discovered
                .tools
                .iter()
                .any(|candidate| candidate.name == tool),
            "missing security parity tool {tool}"
        );
    }
    let notification =
        rmcp::model::CancelledNotification::new(rmcp::model::CancelledNotificationParam::new(
            Some(rmcp::model::RequestId::Number(987654)),
            Some(NOTIFICATION_SENTINEL.into()),
        ));
    modern
        .peer()
        .send_notification(notification.into())
        .await
        .unwrap();
    let secret_argument = json!({"password":PASSWORD_SENTINEL})
        .as_object()
        .unwrap()
        .clone();
    assert!(
        modern
            .call_tool(
                CallToolRequestParams::new("account_security_status")
                    .with_arguments(secret_argument)
            )
            .await
            .is_err()
    );
    let proof_argument = json!({"interaction_id":PROOF_SENTINEL})
        .as_object()
        .unwrap()
        .clone();
    let bad_proof = modern
        .call_tool(
            CallToolRequestParams::new("invitation_response_status").with_arguments(proof_argument),
        )
        .await
        .unwrap();
    assert_eq!(bad_proof.is_error, Some(true));
    let bad_token = reqwest::Client::new()
        .post(&req.resource)
        .bearer_auth(TOKEN_SENTINEL)
        .send()
        .await
        .unwrap();
    assert_eq!(bad_token.status(), reqwest::StatusCode::UNAUTHORIZED);
    let preview_tool = discovered
        .tools
        .iter()
        .find(|tool| tool.name == "formulation_preview")
        .unwrap();
    assert!(preview_tool.input_schema.contains_key("$defs"));
    assert_eq!(
        discovered.tools.len(),
        krausen_types::operations::OPERATIONS.len()
    );
    assert!(
        discovered
            .tools
            .iter()
            .all(|tool| krausen_types::operations::by_tool(&tool.name).is_some())
    );
    // The browser adapter and both supported SDK protocol revisions use the
    // same shared preview and style operations for this account/workspace.
    let browser_actor = krausen_app::ActorContext {
        actor_id: principal.user_id.clone(),
        credential_id: principal.credential_id.clone(),
        access_expires_at: principal.access_expires_at,
        workspace_id: ws.clone(),
    };
    let mut draft = krausen_types::brewery::RecipeDocumentV1::default();
    draft.batch_size_l = Some(20.0);
    draft.boil_minutes = Some(60.0);
    draft
        .fermentables
        .push(krausen_types::brewery::FermentableAddition {
            row_id: "grain".into(),
            amount_kg: Some(4.0),
            addition: krausen_types::brewery::AdditionStage {
                stage: "mash".into(),
                time_minutes: None,
            },
            specification: krausen_types::brewery::FermentableSpecification {
                name: None,
                producer: None,
                kind: Some("grain".into()),
                yield_fraction: Some(0.8),
                colour_ebc: Some(5.0),
            },
            source: None,
        });
    draft.process.mash_water_method = Some("manual".into());
    draft.process.mash_water_l = Some(12.0);
    let mut equipment = krausen_types::brewery::EquipmentSettingsV1::default();
    equipment.boil_off_l_per_hour = Some(3.0);
    equipment.transfer_loss_l = Some(2.0);
    equipment.pre_boil_deadspace_l = Some(0.0);
    equipment.grain_absorption_l_per_kg = Some(0.8);
    equipment.mash_efficiency_fraction = Some(0.75);
    let request = krausen_types::formulation::PreviewRequest {
        draft_generation: "test-generation".into(),
        intent: krausen_types::request_context::OperationIntent {
            account_id: browser_actor.actor_id.clone(),
            workspace_id: ws.clone(),
        },
        draft,
        equipment_draft: equipment,
    };
    let browser_preview =
        krausen_ui::server_fns::formulation::execute_preview(&db, &browser_actor, request.clone())
            .await;
    // Exercise the actual browser server-function endpoint using the verified
    // browser credential, not just its transport-independent helper.
    use leptos::server_fn::ServerFn;
    let ui_path = <krausen_ui::server_fns::formulation::PreviewFormulation as ServerFn>::PATH;
    let ui_response = reqwest::Client::new()
        .post(format!("{base}{ui_path}"))
        .bearer_auth(&browser.access_token)
        .form(&[("request_json", serde_json::to_string(&request).unwrap())])
        .send()
        .await
        .unwrap();
    assert!(
        ui_response.status().is_success(),
        "browser preview HTTP {}",
        ui_response.status()
    );
    let ui_value: serde_json::Value = ui_response.json().await.unwrap();
    assert_eq!(ui_value, serde_json::to_value(&browser_preview).unwrap());
    let preview_args = json!({"workspace_id":ws,"request":request.clone()})
        .as_object()
        .unwrap()
        .clone();
    let a = legacy
        .call_tool(
            CallToolRequestParams::new("formulation_preview").with_arguments(preview_args.clone()),
        )
        .await
        .unwrap();
    let b = modern
        .call_tool(
            CallToolRequestParams::new("formulation_preview").with_arguments(preview_args.clone()),
        )
        .await
        .unwrap();
    assert_eq!(a.structured_content, b.structured_content);
    let result = b.structured_content.unwrap();
    assert_eq!(
        result["outcome"],
        serde_json::to_value(browser_preview).unwrap()
    );
    assert_eq!(
        result["outcome"]["Preview"]["draft_generation"],
        "test-generation"
    );
    assert!(result["outcome"]["Preview"]["context"].is_object());
    assert!(result["outcome"]["Preview"]["targets"]["original_sg"].is_number());
    assert!(result["outcome"]["Preview"]["targets"]["final_sg"].is_null());
    assert!(result["outcome"]["Preview"]["targets"]["total_water_l"].is_number());
    let mut stale_request = request.clone();
    stale_request.intent.account_id = "old-account".into();
    let stale_browser = krausen_ui::server_fns::formulation::execute_preview(
        &db,
        &browser_actor,
        stale_request.clone(),
    )
    .await;
    let stale_http = reqwest::Client::new()
        .post(format!("{base}{ui_path}"))
        .bearer_auth(&browser.access_token)
        .form(&[(
            "request_json",
            serde_json::to_string(&stale_request).unwrap(),
        )])
        .send()
        .await
        .unwrap();
    assert!(stale_http.status().is_success());
    let stale_http_value: serde_json::Value = stale_http.json().await.unwrap();
    assert_eq!(
        stale_http_value,
        serde_json::to_value(&stale_browser).unwrap()
    );
    let stale_mcp = modern
        .call_tool(
            CallToolRequestParams::new("formulation_preview").with_arguments(
                json!({"workspace_id":ws,"request":stale_request})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        )
        .await
        .unwrap();
    assert_eq!(
        stale_mcp.structured_content.unwrap()["outcome"],
        serde_json::to_value(stale_browser).unwrap()
    );
    assert_eq!(stale_mcp.is_error, Some(true));
    let mut denied_args = preview_args;
    denied_args.insert("workspace_id".into(), json!("not-a-member"));
    let denied_preview = modern
        .call_tool(CallToolRequestParams::new("formulation_preview").with_arguments(denied_args))
        .await
        .unwrap();
    let denied_actor = krausen_app::ActorContext {
        workspace_id: "not-a-member".into(),
        ..browser_actor.clone()
    };
    let denied_browser =
        krausen_ui::server_fns::formulation::execute_preview(&db, &denied_actor, request).await;
    assert_eq!(
        denied_preview.structured_content.unwrap()["outcome"],
        serde_json::to_value(denied_browser).unwrap()
    );
    assert_eq!(denied_preview.is_error, Some(true));
    let intent = krausen_types::request_context::OperationIntent {
        account_id: browser_actor.actor_id.clone(),
        workspace_id: ws.clone(),
    };
    // One equipment mutation through MCP, then retry the identical command
    // through the authenticated browser HTTP adapter and the other SDK.
    let equipment_request = krausen_types::equipment::SaveEquipment {
        intent: intent.clone(),
        profile_id: uuid::Uuid::new_v4().to_string(),
        name: "Shared equipment".into(),
        settings: krausen_types::brewery::EquipmentSettingsV1::default(),
        provenance: krausen_types::equipment::EquipmentProvenance {
            source: "manual".into(),
            assumptions: vec![],
        },
        expected_revision: 0,
        expected_metadata_revision: 0,
        request_id: uuid::Uuid::new_v4().to_string(),
    };
    let args = json!({"workspace_id":ws,"request":equipment_request})
        .as_object()
        .unwrap()
        .clone();
    let saved = modern
        .call_tool(CallToolRequestParams::new("equipment_save").with_arguments(args.clone()))
        .await
        .unwrap();
    let retried = legacy
        .call_tool(CallToolRequestParams::new("equipment_save").with_arguments(args))
        .await
        .unwrap();
    assert_eq!(saved.structured_content, retried.structured_content);
    let equipment_path =
        <krausen_ui::server_fns::equipment::ExecuteEquipmentCommand as ServerFn>::PATH;
    let command = krausen_types::equipment::EquipmentCommand::Save(equipment_request.clone());
    let http_equipment = reqwest::Client::new()
        .post(format!("{base}{equipment_path}"))
        .bearer_auth(&browser.access_token)
        .form(&[("command_json", serde_json::to_string(&command).unwrap())])
        .send()
        .await
        .unwrap();
    assert!(http_equipment.status().is_success());
    let browser_equipment: serde_json::Value = http_equipment.json().await.unwrap();
    assert_eq!(
        saved.structured_content.unwrap()["outcome"],
        browser_equipment
    );
    // Read, list and immutable revision results must use the same shared
    // outcome through both protocol clients and the browser adapter.
    use krausen_types::equipment::{
        EquipmentCommand, EquipmentListQuery, EquipmentQuery, EquipmentRevisionQuery,
    };
    let query = EquipmentQuery {
        intent: intent.clone(),
        profile_id: equipment_request.profile_id.clone(),
    };
    let list = EquipmentListQuery {
        intent: intent.clone(),
        cursor: None,
        limit: 20,
        include_archived: false,
    };
    let revision = EquipmentRevisionQuery {
        intent: intent.clone(),
        profile_id: equipment_request.profile_id.clone(),
        revision: 1,
    };
    for (tool, request, command) in [
        (
            "equipment_get",
            serde_json::to_value(&query).unwrap(),
            EquipmentCommand::Get(query),
        ),
        (
            "equipment_list",
            serde_json::to_value(&list).unwrap(),
            EquipmentCommand::List(list),
        ),
        (
            "equipment_revision",
            serde_json::to_value(&revision).unwrap(),
            EquipmentCommand::Revision(revision),
        ),
    ] {
        let args = json!({"workspace_id":ws,"request":request})
            .as_object()
            .unwrap()
            .clone();
        let a = modern
            .call_tool(CallToolRequestParams::new(tool).with_arguments(args.clone()))
            .await
            .unwrap();
        let b = legacy
            .call_tool(CallToolRequestParams::new(tool).with_arguments(args))
            .await
            .unwrap();
        assert_eq!(a.structured_content, b.structured_content);
        let response = reqwest::Client::new()
            .post(format!("{base}{equipment_path}"))
            .bearer_auth(&browser.access_token)
            .form(&[("command_json", serde_json::to_string(&command).unwrap())])
            .send()
            .await
            .unwrap();
        assert!(response.status().is_success());
        let browser_result: serde_json::Value = response.json().await.unwrap();
        assert_eq!(a.structured_content.unwrap()["outcome"], browser_result);
    }
    let mut second_profile = equipment_request.clone();
    second_profile.profile_id = uuid::Uuid::new_v4().to_string();
    second_profile.request_id = uuid::Uuid::new_v4().to_string();
    second_profile.name = "Second shared equipment".into();
    let second_args = json!({"workspace_id":ws,"request":second_profile})
        .as_object()
        .unwrap()
        .clone();
    let second_save = modern
        .call_tool(CallToolRequestParams::new("equipment_save").with_arguments(second_args))
        .await
        .unwrap();
    assert!(second_save.structured_content.unwrap()["outcome"]["Profile"].is_object());
    let first_query = EquipmentListQuery {
        intent: intent.clone(),
        cursor: None,
        limit: 1,
        include_archived: false,
    };
    let first_args = json!({"workspace_id":ws,"request":first_query})
        .as_object()
        .unwrap()
        .clone();
    let first_result = modern
        .call_tool(CallToolRequestParams::new("equipment_list").with_arguments(first_args))
        .await
        .unwrap();
    let first_outcome = first_result.structured_content.unwrap();
    let next = first_outcome["outcome"]["Profiles"]["next_cursor"]
        .as_str()
        .unwrap()
        .to_owned();
    let next_query = EquipmentListQuery {
        cursor: Some(next),
        ..first_query
    };
    let next_args = json!({"workspace_id":ws,"request":next_query})
        .as_object()
        .unwrap()
        .clone();
    let next_result = legacy
        .call_tool(CallToolRequestParams::new("equipment_list").with_arguments(next_args))
        .await
        .unwrap();
    let next_outcome = next_result.structured_content.unwrap();
    assert_ne!(
        first_outcome["outcome"]["Profiles"]["items"][0]["profile_id"],
        next_outcome["outcome"]["Profiles"]["items"][0]["profile_id"]
    );
    let next_http = reqwest::Client::new()
        .post(format!("{base}{equipment_path}"))
        .bearer_auth(&browser.access_token)
        .form(&[(
            "command_json",
            serde_json::to_string(&EquipmentCommand::List(next_query)).unwrap(),
        )])
        .send()
        .await
        .unwrap();
    assert_eq!(
        next_http.json::<serde_json::Value>().await.unwrap(),
        next_outcome["outcome"]
    );
    // The saved profile is usable by recipe save and preview through the
    // browser and both MCP protocol clients under the same verified principal.
    let linked_document = krausen_types::brewery::RecipeDocumentV1 {
        title: "Profile linked recipe".into(),
        equipment_profile_id: Some(equipment_request.profile_id.clone()),
        ..Default::default()
    };
    let linked_key = uuid::Uuid::new_v4().to_string();
    let linked_args = json!({"workspace_id":ws,"intent":intent,"document":linked_document,"request_id":linked_key}).as_object().unwrap().clone();
    let linked_a = modern
        .call_tool(CallToolRequestParams::new("recipe_save").with_arguments(linked_args.clone()))
        .await
        .unwrap();
    let linked_b = legacy
        .call_tool(CallToolRequestParams::new("recipe_save").with_arguments(linked_args))
        .await
        .unwrap();
    assert_eq!(linked_a.structured_content, linked_b.structured_content);
    let linked_outcome = linked_a.structured_content.unwrap();
    assert_eq!(linked_outcome["outcome"]["Saved"]["revision"], 1);
    let recipe_save_path = <krausen_ui::server_fns::recipe_editor::SaveRecipe as ServerFn>::PATH;
    let linked_http = reqwest::Client::new().post(format!("{base}{recipe_save_path}"))
        .bearer_auth(&browser.access_token).json(&json!({"account_id":intent.account_id,"workspace_id":ws,"recipe_id":null,"expected_revision":null,"document":linked_document,"request_id":linked_key}))
        .send().await.unwrap();
    assert!(linked_http.status().is_success());
    assert_eq!(
        linked_http.json::<serde_json::Value>().await.unwrap(),
        linked_outcome["outcome"]
    );
    let linked_preview = krausen_types::formulation::PreviewRequest {
        draft_generation: "profile-generation".into(),
        intent: intent.clone(),
        draft: linked_document.clone(),
        equipment_draft: Default::default(),
    };
    let linked_preview_args = json!({"workspace_id":ws,"request":linked_preview})
        .as_object()
        .unwrap()
        .clone();
    let preview_a = modern
        .call_tool(
            CallToolRequestParams::new("formulation_preview")
                .with_arguments(linked_preview_args.clone()),
        )
        .await
        .unwrap();
    let preview_b = legacy
        .call_tool(
            CallToolRequestParams::new("formulation_preview").with_arguments(linked_preview_args),
        )
        .await
        .unwrap();
    assert_eq!(preview_a.structured_content, preview_b.structured_content);
    let profile_preview = preview_a.structured_content.unwrap();
    assert_eq!(
        profile_preview["outcome"]["Preview"]["context"]["equipment_profile"]["profile_id"],
        equipment_request.profile_id
    );
    let preview_http = reqwest::Client::new()
        .post(format!("{base}{ui_path}"))
        .bearer_auth(&browser.access_token)
        .form(&[(
            "request_json",
            serde_json::to_string(&linked_preview).unwrap(),
        )])
        .send()
        .await
        .unwrap();
    assert!(preview_http.status().is_success());
    assert_eq!(
        preview_http.json::<serde_json::Value>().await.unwrap(),
        profile_preview["outcome"]
    );
    let archive = krausen_types::equipment::ArchiveEquipment {
        intent: intent.clone(),
        profile_id: equipment_request.profile_id.clone(),
        archived: true,
        expected_metadata_revision: 1,
        request_id: uuid::Uuid::new_v4().to_string(),
    };
    let archive_args = json!({"workspace_id":ws,"request":archive})
        .as_object()
        .unwrap()
        .clone();
    let archived = modern
        .call_tool(
            CallToolRequestParams::new("equipment_archive").with_arguments(archive_args.clone()),
        )
        .await
        .unwrap();
    let retried_archive = legacy
        .call_tool(CallToolRequestParams::new("equipment_archive").with_arguments(archive_args))
        .await
        .unwrap();
    assert_eq!(
        archived.structured_content,
        retried_archive.structured_content
    );
    let archive_command = EquipmentCommand::Archive(archive);
    let archived_http = reqwest::Client::new()
        .post(format!("{base}{equipment_path}"))
        .bearer_auth(&browser.access_token)
        .form(&[(
            "command_json",
            serde_json::to_string(&archive_command).unwrap(),
        )])
        .send()
        .await
        .unwrap();
    assert!(archived_http.status().is_success());
    let archive_value: serde_json::Value = archived_http.json().await.unwrap();
    assert_eq!(
        archived.structured_content.unwrap()["outcome"],
        archive_value
    );
    let archived_preview_args = json!({"workspace_id":ws,"request":linked_preview})
        .as_object()
        .unwrap()
        .clone();
    let archived_preview = modern
        .call_tool(
            CallToolRequestParams::new("formulation_preview").with_arguments(archived_preview_args),
        )
        .await
        .unwrap();
    assert_eq!(
        archived_preview.structured_content.unwrap()["outcome"],
        "UnresolvedProfile"
    );
    let archived_http_preview = reqwest::Client::new()
        .post(format!("{base}{ui_path}"))
        .bearer_auth(&browser.access_token)
        .form(&[(
            "request_json",
            serde_json::to_string(&linked_preview).unwrap(),
        )])
        .send()
        .await
        .unwrap();
    assert_eq!(
        archived_http_preview
            .json::<serde_json::Value>()
            .await
            .unwrap(),
        "UnresolvedProfile"
    );
    let browser_styles =
        krausen_ui::server_fns::formulation::execute_styles(&db, &browser_actor, &intent).await;
    let style_path = <krausen_ui::server_fns::formulation::ListFormulationStyles as ServerFn>::PATH;
    let style_response = reqwest::Client::new()
        .post(format!("{base}{style_path}"))
        .bearer_auth(&browser.access_token)
        .form(&[
            ("account_id", intent.account_id.as_str()),
            ("workspace_id", intent.workspace_id.as_str()),
        ])
        .send()
        .await
        .unwrap();
    assert!(style_response.status().is_success());
    let style_value: serde_json::Value = style_response.json().await.unwrap();
    assert_eq!(style_value, serde_json::to_value(&browser_styles).unwrap());
    let styles = modern
        .call_tool(
            CallToolRequestParams::new("formulation_styles").with_arguments(
                json!({"workspace_id":ws,"intent":intent})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        )
        .await
        .unwrap();
    let legacy_styles = legacy
        .call_tool(
            CallToolRequestParams::new("formulation_styles").with_arguments(
                json!({"workspace_id":ws,"intent":intent})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        )
        .await
        .unwrap();
    assert_eq!(styles.structured_content, legacy_styles.structured_content);
    let mut stale_intent = intent.clone();
    stale_intent.workspace_id = "old-workspace".into();
    let stale_styles_browser =
        krausen_ui::server_fns::formulation::execute_styles(&db, &browser_actor, &stale_intent)
            .await;
    let stale_styles_mcp = modern
        .call_tool(
            CallToolRequestParams::new("formulation_styles").with_arguments(
                json!({"workspace_id":ws,"intent":stale_intent})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        )
        .await
        .unwrap();
    assert_eq!(
        stale_styles_mcp.structured_content.unwrap()["outcome"],
        serde_json::to_value(stale_styles_browser).unwrap()
    );
    assert_eq!(stale_styles_mcp.is_error, Some(true));
    assert_eq!(
        styles.structured_content.unwrap()["outcome"],
        serde_json::to_value(browser_styles).unwrap()
    );
    let denied = modern
        .call_tool(
            CallToolRequestParams::new("formulation_styles").with_arguments(
                json!({"workspace_id":"not-a-member","intent":intent})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        )
        .await
        .unwrap();
    assert_eq!(denied.is_error, Some(true));
    assert_eq!(denied.structured_content.unwrap()["outcome"], "Forbidden");
    let import_request = json!({
        "intent": intent,
        "xml": "<RECIPES><RECIPE><NAME>SDK import</NAME><HOPS><HOP><NAME>Fuggles</NAME><USE>Dry Hop</USE><TIME>10080</TIME><AMOUNT>0.03</AMOUNT></HOP></HOPS></RECIPE></RECIPES>",
        "recipe_index": 0,
        "choices": {"colour":null,"efficiency":null},
        "equipment_profile_id": null,
        "expected_profile_revision": null,
        "expected_profile_metadata_revision": null,
        "equipment_draft": krausen_types::brewery::EquipmentSettingsV1::default(),
        "draft_generation": "sdk-test"
    });
    let import_args = json!({"workspace_id":ws,"request":import_request})
        .as_object()
        .unwrap()
        .clone();
    let mcp_preview = modern
        .call_tool(CallToolRequestParams::new("beerxml_preview").with_arguments(import_args))
        .await
        .unwrap();
    let browser_path = <krausen_ui::server_fns::interchange::PreviewBeerxml as ServerFn>::PATH;
    let browser_preview = reqwest::Client::new()
        .post(format!("{base}{browser_path}"))
        .bearer_auth(&browser.access_token)
        .json(&json!({"request":import_request}))
        .send()
        .await
        .unwrap();
    assert!(browser_preview.status().is_success());
    let preview_outcome = mcp_preview.structured_content.unwrap()["outcome"].clone();
    assert_eq!(
        preview_outcome["Preview"]["document"]["hops"][0]["contact_minutes"],
        10080.0
    );
    assert_eq!(
        preview_outcome,
        browser_preview.json::<serde_json::Value>().await.unwrap()
    );
    let import_key = uuid::Uuid::new_v4().to_string();
    let import_args = json!({"workspace_id":ws,"request":import_request,"request_id":import_key})
        .as_object()
        .unwrap()
        .clone();
    let imported = modern
        .call_tool(CallToolRequestParams::new("beerxml_import").with_arguments(import_args))
        .await
        .unwrap();
    let browser_path = <krausen_ui::server_fns::interchange::CommitBeerxml as ServerFn>::PATH;
    let browser_import = reqwest::Client::new()
        .post(format!("{base}{browser_path}"))
        .bearer_auth(&browser.access_token)
        .json(&json!({"request":import_request,"request_id":import_key}))
        .send()
        .await
        .unwrap();
    assert!(browser_import.status().is_success());
    let imported = imported.structured_content.unwrap()["outcome"].clone();
    assert_eq!(
        imported,
        browser_import.json::<serde_json::Value>().await.unwrap()
    );
    let imported_id = imported["Imported"]["recipe_id"].as_str().unwrap();
    let export_request = json!({"intent":intent,"selection":{"Saved":{"recipe_id":imported_id,"revision":1}},"acknowledge_loss":true});
    let export_args = json!({"workspace_id":ws,"request":export_request})
        .as_object()
        .unwrap()
        .clone();
    let exported = modern
        .call_tool(CallToolRequestParams::new("beerxml_export").with_arguments(export_args))
        .await
        .unwrap();
    let browser_path = <krausen_ui::server_fns::interchange::ExportBeerxml as ServerFn>::PATH;
    let browser_export = reqwest::Client::new()
        .post(format!("{base}{browser_path}"))
        .bearer_auth(&browser.access_token)
        .json(&json!({"request":export_request}))
        .send()
        .await
        .unwrap();
    assert!(browser_export.status().is_success());
    let export_outcome = exported.structured_content.unwrap()["outcome"].clone();
    assert!(
        export_outcome["Exported"]["xml"]
            .as_str()
            .unwrap()
            .contains("<TIME>10080.0</TIME>")
    );
    assert_eq!(
        export_outcome,
        browser_export.json::<serde_json::Value>().await.unwrap()
    );
    let original_args = json!({"workspace_id":ws,"intent":intent,"recipe_id":imported_id})
        .as_object()
        .unwrap()
        .clone();
    let original = modern
        .call_tool(CallToolRequestParams::new("beerxml_original").with_arguments(original_args))
        .await
        .unwrap();
    let browser_path = <krausen_ui::server_fns::interchange::OriginalBeerxml as ServerFn>::PATH;
    let browser_original = reqwest::Client::new()
        .post(format!("{base}{browser_path}"))
        .bearer_auth(&browser.access_token)
        .json(&json!({"intent":intent,"recipe_id":imported_id}))
        .send()
        .await
        .unwrap();
    assert!(browser_original.status().is_success());
    assert_eq!(
        original.structured_content.unwrap()["outcome"],
        browser_original.json::<serde_json::Value>().await.unwrap()
    );
    let recipe_tool = discovered
        .tools
        .iter()
        .find(|tool| tool.name == "recipe_save")
        .unwrap();
    assert!(recipe_tool.input_schema.get("$defs").is_some());
    assert!(
        recipe_tool.input_schema["$defs"]["RecipeDocumentV1"]["properties"]
            .get("fermentables")
            .is_some()
    );
    // Both protocol versions and the browser-facing app use the same native
    // operation and revision contract, including request replay and authority.
    let mut native = krausen_types::brewery::RecipeDocumentV1::default();
    native.title = "MCP recipe".into();
    let invalid_args =
        json!({"workspace_id": ws, "intent": intent, "document": native, "request_id": "invalid"})
            .as_object()
            .unwrap()
            .clone();
    let invalid = modern
        .call_tool(CallToolRequestParams::new("recipe_save").with_arguments(invalid_args))
        .await
        .unwrap();
    assert_eq!(invalid.is_error, Some(true));
    assert_eq!(
        invalid.structured_content.unwrap()["outcome"],
        "InvalidRequest"
    );
    let denied_args = json!({"workspace_id": "not-a-member", "intent": intent, "document": native, "request_id": uuid::Uuid::new_v4()}).as_object().unwrap().clone();
    let denied = modern
        .call_tool(CallToolRequestParams::new("recipe_save").with_arguments(denied_args))
        .await
        .unwrap();
    assert_eq!(denied.is_error, Some(true));
    assert_eq!(denied.structured_content.unwrap()["outcome"], "Forbidden");
    let changed_intent = json!({"account_id":"different-account","workspace_id":ws});
    let changed_args = json!({"workspace_id":ws,"intent":changed_intent,"document":native,"request_id":uuid::Uuid::new_v4()}).as_object().unwrap().clone();
    let changed = modern
        .call_tool(CallToolRequestParams::new("recipe_save").with_arguments(changed_args))
        .await
        .unwrap();
    assert_eq!(changed.is_error, Some(true));
    assert_eq!(
        changed.structured_content.unwrap()["outcome"],
        "ContextChanged"
    );
    let save_path = <krausen_ui::server_fns::recipe_editor::SaveRecipe as ServerFn>::PATH;
    let changed_browser = reqwest::Client::new().post(format!("{base}{save_path}"))
        .bearer_auth(&browser.access_token)
        .json(&json!({"account_id":"different-account","workspace_id":ws,"recipe_id":null,"expected_revision":null,"document":native,"request_id":uuid::Uuid::new_v4().to_string()}))
        .send().await.unwrap();
    assert!(changed_browser.status().is_success());
    assert_eq!(
        changed_browser.json::<serde_json::Value>().await.unwrap(),
        "ContextChanged"
    );
    let save_key = uuid::Uuid::new_v4();
    let save_args =
        json!({"workspace_id": ws, "intent": intent, "document": native, "request_id": save_key})
            .as_object()
            .unwrap()
            .clone();
    let native_first = legacy
        .call_tool(CallToolRequestParams::new("recipe_save").with_arguments(save_args.clone()))
        .await
        .unwrap();
    let native_retry = modern
        .call_tool(CallToolRequestParams::new("recipe_save").with_arguments(save_args))
        .await
        .unwrap();
    assert_eq!(
        native_first.structured_content,
        native_retry.structured_content
    );
    let native_result = native_first.structured_content.unwrap();
    let native_id = native_result["outcome"]["Saved"]["recipe_id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(native_result["outcome"]["Saved"]["revision"], 1);
    let actor_for_recipe = krausen_app::ActorContext {
        actor_id: principal.user_id.clone(),
        credential_id: principal.credential_id.clone(),
        access_expires_at: principal.access_expires_at,
        workspace_id: ws.clone(),
    };
    let snapshot =
        krausen_app::recipe_editor::read(&db, &actor_for_recipe, &intent, &native_id).await;
    assert!(
        matches!(snapshot, krausen_types::recipe_editor::RecipeReadOutcome::Found(ref snapshot) if snapshot.document == native && snapshot.revision == 1)
    );
    native.title = "Browser edit".into();
    let edit_key = uuid::Uuid::new_v4();
    let browser_save = reqwest::Client::new().post(format!("{base}{save_path}"))
        .bearer_auth(&browser.access_token)
        .json(&json!({"account_id":intent.account_id,"workspace_id":intent.workspace_id,"recipe_id":native_id,"expected_revision":1,"document":native,"request_id":edit_key.to_string()}))
        .send().await.unwrap();
    assert!(browser_save.status().is_success());
    let browser_outcome: serde_json::Value = browser_save.json().await.unwrap();
    assert_eq!(browser_outcome["Saved"]["revision"], 2);
    let replay_args = json!({"workspace_id":ws,"intent":intent,"recipe_id":native_id,"expected_revision":1,"document":native,"request_id":edit_key}).as_object().unwrap().clone();
    let replay = modern
        .call_tool(CallToolRequestParams::new("recipe_save").with_arguments(replay_args))
        .await
        .unwrap();
    assert_eq!(
        replay.structured_content.unwrap()["outcome"],
        browser_outcome
    );
    let read_args = json!({"workspace_id":ws,"intent":intent,"recipe_id":native_id})
        .as_object()
        .unwrap()
        .clone();
    let native_read = modern
        .call_tool(CallToolRequestParams::new("recipe_read").with_arguments(read_args))
        .await
        .unwrap();
    assert_eq!(
        native_read.structured_content.unwrap()["outcome"]["Found"]["document"]["title"],
        "Browser edit"
    );
    // The structured print/process summary is the same shared query through MCP
    // and the browser adapter: saved revision and unsaved draft remain distinct.
    let saved_summary_request = krausen_types::process_summary::ProcessSummaryRequest {
        intent: intent.clone(),
        selection: krausen_types::process_summary::ProcessSummarySelection::Saved {
            recipe_id: native_id.clone(),
            revision: 2,
        },
    };
    let saved_summary_args = json!({
        "workspace_id": ws,
        "request": saved_summary_request.clone(),
    })
    .as_object()
    .unwrap()
    .clone();
    let saved_summary = modern
        .call_tool(
            CallToolRequestParams::new("recipe_process_summary").with_arguments(saved_summary_args),
        )
        .await
        .unwrap();
    assert_ne!(saved_summary.is_error, Some(true));
    let saved_summary = saved_summary.structured_content.unwrap();
    assert_eq!(saved_summary["outcome"]["Summary"]["title"], "Browser edit");
    assert_eq!(saved_summary["outcome"]["Summary"]["source"]["revision"], 2);
    assert_eq!(
        saved_summary["outcome"]["Summary"]["source"]["kind"],
        "saved"
    );
    let browser_summary = krausen_ui::server_fns::process_summary::execute_process_summary(
        &db,
        &browser_actor,
        saved_summary_request,
    )
    .await;
    assert_eq!(
        serde_json::to_value(browser_summary).unwrap()["Summary"],
        saved_summary["outcome"]["Summary"]
    );

    let mut draft_document = native.clone();
    draft_document.title = "Draft print".into();
    let draft_summary_request = krausen_types::process_summary::ProcessSummaryRequest {
        intent: intent.clone(),
        selection: krausen_types::process_summary::ProcessSummarySelection::Draft {
            recipe_id: Some(native_id.clone()),
            base_revision: Some(2),
            document: draft_document,
        },
    };
    let draft_summary_args = json!({
        "workspace_id": ws,
        "request": draft_summary_request,
    })
    .as_object()
    .unwrap()
    .clone();
    let draft_summary = modern
        .call_tool(
            CallToolRequestParams::new("recipe_process_summary").with_arguments(draft_summary_args),
        )
        .await
        .unwrap();
    assert_ne!(draft_summary.is_error, Some(true));
    let draft_summary = draft_summary.structured_content.unwrap();
    assert_eq!(draft_summary["outcome"]["Summary"]["title"], "Draft print");
    assert_eq!(
        draft_summary["outcome"]["Summary"]["source"]["kind"],
        "unsaved_draft"
    );
    let denied_summary_args = json!({
        "workspace_id": "not-a-member",
        "request": {
            "intent": intent.clone(),
            "selection": {"kind":"saved","recipe_id":native_id,"revision":2}
        }
    })
    .as_object()
    .unwrap()
    .clone();
    let denied_summary = modern
        .call_tool(
            CallToolRequestParams::new("recipe_process_summary")
                .with_arguments(denied_summary_args),
        )
        .await
        .unwrap();
    assert_eq!(denied_summary.is_error, Some(true));
    assert_eq!(
        denied_summary.structured_content.unwrap()["outcome"],
        "Forbidden"
    );
    let stale_args = json!({"workspace_id":ws,"intent":intent,"recipe_id":native_id,"expected_revision":1,"document":native,"request_id":uuid::Uuid::new_v4()}).as_object().unwrap().clone();
    let stale = modern
        .call_tool(CallToolRequestParams::new("recipe_save").with_arguments(stale_args))
        .await
        .unwrap();
    assert_eq!(stale.is_error, Some(true));
    assert_eq!(
        stale.structured_content.unwrap()["outcome"]["Conflict"]["actual"],
        2
    );
    // Real browser and both SDK transports use the same library, retained
    // history and guarded archive operations under the verified principal.
    use krausen_types::recipe_library::{
        ArchiveRecipe, HistoryQuery, LibraryCommand, LibraryQuery, RevisionQuery,
    };
    let library_query = LibraryQuery {
        intent: intent.clone(),
        search: "Browser edit".into(),
        archived: false,
        cursor: None,
        limit: 50,
    };
    let history_query = HistoryQuery {
        intent: intent.clone(),
        recipe_id: native_id.clone(),
        cursor: None,
        limit: 50,
    };
    let revision_query = RevisionQuery {
        intent: intent.clone(),
        recipe_id: native_id.clone(),
        revision: 1,
    };
    let library_path =
        <krausen_ui::server_fns::recipe_library::ExecuteLibraryCommand as ServerFn>::PATH;
    for (tool, request, command) in [
        (
            "recipe_list",
            serde_json::to_value(&library_query).unwrap(),
            LibraryCommand::List(library_query),
        ),
        (
            "recipe_history",
            serde_json::to_value(&history_query).unwrap(),
            LibraryCommand::History(history_query.clone()),
        ),
        (
            "recipe_revision",
            serde_json::to_value(&revision_query).unwrap(),
            LibraryCommand::Revision(revision_query),
        ),
    ] {
        let args = json!({"workspace_id":ws,"request":request})
            .as_object()
            .unwrap()
            .clone();
        let a = modern
            .call_tool(CallToolRequestParams::new(tool).with_arguments(args.clone()))
            .await
            .unwrap();
        let b = legacy
            .call_tool(CallToolRequestParams::new(tool).with_arguments(args))
            .await
            .unwrap();
        assert_eq!(a.structured_content, b.structured_content);
        let http = reqwest::Client::new()
            .post(format!("{base}{library_path}"))
            .bearer_auth(&browser.access_token)
            .form(&[("command_json", serde_json::to_string(&command).unwrap())])
            .send()
            .await
            .unwrap();
        assert!(http.status().is_success());
        assert_eq!(
            http.json::<serde_json::Value>().await.unwrap(),
            a.structured_content.unwrap()["outcome"]
        );
    }
    let archive_recipe = ArchiveRecipe {
        intent: intent.clone(),
        recipe_id: native_id.clone(),
        expected_metadata_revision: 0,
        archived: true,
        request_id: uuid::Uuid::new_v4().to_string(),
    };
    let archive_args = json!({"workspace_id":ws,"request":archive_recipe})
        .as_object()
        .unwrap()
        .clone();
    let archive_a = modern
        .call_tool(
            CallToolRequestParams::new("recipe_archive").with_arguments(archive_args.clone()),
        )
        .await
        .unwrap();
    let archive_b = legacy
        .call_tool(CallToolRequestParams::new("recipe_archive").with_arguments(archive_args))
        .await
        .unwrap();
    assert_eq!(archive_a.structured_content, archive_b.structured_content);
    let archive_http = reqwest::Client::new()
        .post(format!("{base}{library_path}"))
        .bearer_auth(&browser.access_token)
        .form(&[(
            "command_json",
            serde_json::to_string(&LibraryCommand::Archive(archive_recipe.clone())).unwrap(),
        )])
        .send()
        .await
        .unwrap();
    assert_eq!(
        archive_http.json::<serde_json::Value>().await.unwrap(),
        archive_a.structured_content.unwrap()["outcome"]
    );
    let history_args = json!({"workspace_id":ws,"request":history_query})
        .as_object()
        .unwrap()
        .clone();
    let archived_history = modern
        .call_tool(CallToolRequestParams::new("recipe_history").with_arguments(history_args))
        .await
        .unwrap();
    assert_eq!(
        archived_history.structured_content.unwrap()["outcome"]["History"]["items"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let restore = ArchiveRecipe {
        archived: false,
        expected_metadata_revision: 1,
        request_id: uuid::Uuid::new_v4().to_string(),
        ..archive_recipe
    };
    let restored = modern
        .call_tool(
            CallToolRequestParams::new("recipe_archive").with_arguments(
                json!({"workspace_id":ws,"request":restore})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        )
        .await
        .unwrap();
    assert_eq!(
        restored.structured_content.unwrap()["outcome"]["Archive"]["archived"],
        false
    );
    let profile = modern
        .call_tool(CallToolRequestParams::new("account_profile_get"))
        .await
        .unwrap();
    assert_eq!(
        profile.structured_content.unwrap()["user_id"],
        principal.user_id
    );
    let profile_revision = krausen_app::account::profile(&db, &browser_actor)
        .await
        .unwrap()
        .revision;
    let profile_request = uuid::Uuid::new_v4().to_string();
    let profile_change = json!({"field":"theme","theme":"dark"});
    let profile_path = <krausen_ui::server_fns::profile::UpdateProfile as ServerFn>::PATH;
    let browser_profile = reqwest::Client::new()
        .post(format!("{base}{profile_path}"))
        .bearer_auth(&browser.access_token)
        .form(&[
            ("change_json", profile_change.to_string()),
            ("expected_revision", profile_revision.to_string()),
            ("request_id", profile_request.clone()),
        ])
        .send()
        .await
        .unwrap();
    assert!(browser_profile.status().is_success());
    let browser_profile: serde_json::Value = browser_profile.json().await.unwrap();
    let mcp_profile = modern.call_tool(CallToolRequestParams::new("account_profile_save").with_arguments(json!({"change":profile_change,"expected_revision":profile_revision,"request_id":profile_request}).as_object().unwrap().clone())).await.unwrap();
    assert_eq!(
        mcp_profile.structured_content.unwrap()["outcome"],
        browser_profile
    );
    let pending =
        krausen_app::invitation::begin_discovery(&db, &browser_actor, uuid::Uuid::new_v4(), true)
            .await
            .unwrap();
    let proof: krausen_app::invitation::Proof = krausen_core::db_fetch_optional!(
        &db,
        krausen_app::invitation::Proof,
        "SELECT * FROM account_action_proofs WHERE proof_id=$1",
        &pending.interaction_id
    )
    .unwrap()
    .unwrap();
    let proof_token =
        krausen_app::invitation::delivery_token(b"test-secret-at-least-32-bytes-long", &proof)
            .unwrap();
    let confirmation = reqwest::Client::new()
        .post(format!("{base}/account/proof/preview"))
        .bearer_auth(&browser.access_token)
        .header("Origin", &base)
        .form(&[
            ("interaction", pending.interaction_id.as_str()),
            ("code", proof_token.as_str()),
        ])
        .send()
        .await
        .unwrap();
    assert!(confirmation.status().is_success());
    assert!(
        confirmation
            .text()
            .await
            .unwrap()
            .contains("Verify your mailbox to review invitations")
    );
    let bad_origin = reqwest::Client::new()
        .post(format!("{base}/account/proof/preview"))
        .bearer_auth(&browser.access_token)
        .header("Origin", "https://attacker.example")
        .form(&[
            ("interaction", pending.interaction_id.as_str()),
            ("code", proof_token.as_str()),
        ])
        .send()
        .await
        .unwrap();
    assert_eq!(bad_origin.status(), reqwest::StatusCode::FORBIDDEN);
    let confirmed = reqwest::Client::new()
        .post(format!("{base}/account/proof"))
        .bearer_auth(&browser.access_token)
        .header("Origin", &base)
        .form(&[
            ("interaction", pending.interaction_id.as_str()),
            ("code", proof_token.as_str()),
        ])
        .send()
        .await
        .unwrap();
    assert!(confirmed.status().is_success());
    let own_user = user_service::get_user_by_id(&db, &principal.user_id)
        .await
        .unwrap()
        .unwrap();
    let extra_browser = session::create_authenticated_session(
        &db,
        &kv,
        "test-secret-at-least-32-bytes-long",
        &own_user,
        &device,
    )
    .await
    .unwrap();
    let extra_claims = krausen_auth::jwt::validate_browser_token(
        &extra_browser.access_token,
        "test-secret-at-least-32-bytes-long",
    )
    .unwrap()
    .claims;
    let extra_credential = extra_claims.extra["credential_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let session_path = <krausen_ui::server_fns::security::GetSessions as ServerFn>::PATH;
    let browser_sessions: krausen_ui::server_fns::security::SessionList = reqwest::Client::new()
        .post(format!("{base}{session_path}"))
        .bearer_auth(&browser.access_token)
        .form(&[] as &[(&str, &str)])
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(
        browser_sessions
            .sessions
            .iter()
            .any(|s| s.token_id == extra_credential)
    );
    let sdk_sessions = modern
        .call_tool(CallToolRequestParams::new("account_sessions"))
        .await
        .unwrap();
    let old_hash = krausen_auth::password::hash_password(PASSWORD_SENTINEL).unwrap();
    user_service::upsert_auth_method(
        &db,
        &principal.user_id,
        "password",
        &json!({"hash":&old_hash}),
    )
    .await
    .unwrap();
    let password_path = <krausen_ui::server_fns::security::HasPassword as ServerFn>::PATH;
    let browser_password: bool = reqwest::Client::new()
        .post(format!("{base}{password_path}"))
        .bearer_auth(&browser.access_token)
        .form(&[] as &[(&str, &str)])
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let sdk_status = modern
        .call_tool(CallToolRequestParams::new("account_security_status"))
        .await
        .unwrap();
    let sdk_status = sdk_status.structured_content.unwrap();
    assert_eq!(sdk_status["has_password"], browser_password);
    let observed_revision = sdk_status["revision"].as_i64().unwrap();
    let wrong_change=modern.call_tool(CallToolRequestParams::new("account_password_change").with_arguments(json!({"current_password":WRONG_PASSWORD_SENTINEL,"new_password":NEW_PASSWORD_SENTINEL,"expected_revision":observed_revision,"request_id":uuid::Uuid::new_v4()}).as_object().unwrap().clone())).await.unwrap();
    assert_eq!(wrong_change.is_error, Some(true));
    let password_request = uuid::Uuid::new_v4().to_string();
    let change_args=json!({"current_password":PASSWORD_SENTINEL,"new_password":NEW_PASSWORD_SENTINEL,"expected_revision":observed_revision,"request_id":password_request}).as_object().unwrap().clone();
    let changed = modern
        .call_tool(
            CallToolRequestParams::new("account_password_change")
                .with_arguments(change_args.clone()),
        )
        .await
        .unwrap();
    let retry = modern
        .call_tool(
            CallToolRequestParams::new("account_password_change").with_arguments(change_args),
        )
        .await
        .unwrap();
    assert_eq!(changed.structured_content, retry.structured_content);
    assert!(
        changed.structured_content.unwrap()["outcome"]
            .get("Saved")
            .is_some()
    );
    let intermediate_data:String=match &db {DbPool::Postgres(p)=>sqlx::query_scalar("SELECT CAST(auth_data AS TEXT) FROM user_auth_methods WHERE user_id=$1 AND auth_type='password'").bind(&principal.user_id).fetch_one(p).await.unwrap(),DbPool::Sqlite(p)=>sqlx::query_scalar("SELECT CAST(auth_data AS TEXT) FROM user_auth_methods WHERE user_id=$1 AND auth_type='password'").bind(&principal.user_id).fetch_one(p).await.unwrap()};
    let intermediate_hash = serde_json::from_str::<serde_json::Value>(&intermediate_data).unwrap()
        ["hash"]
        .as_str()
        .unwrap()
        .to_owned();
    let change_path = <krausen_ui::server_fns::security::ChangePassword as ServerFn>::PATH;
    let browser_change = reqwest::Client::new()
        .post(format!("{base}{change_path}"))
        .bearer_auth(&browser.access_token)
        .form(&[
            ("current_password", NEW_PASSWORD_SENTINEL.to_owned()),
            ("new_password", UI_PASSWORD_SENTINEL.to_owned()),
            ("expected_revision", (observed_revision + 1).to_string()),
            ("request_id", uuid::Uuid::new_v4().to_string()),
        ])
        .send()
        .await
        .unwrap();
    assert!(browser_change.status().is_success());
    let browser_change: serde_json::Value = browser_change.json().await.unwrap();
    assert!(browser_change.get("Saved").is_some());
    let final_data:String=match &db {DbPool::Postgres(p)=>sqlx::query_scalar("SELECT CAST(auth_data AS TEXT) FROM user_auth_methods WHERE user_id=$1 AND auth_type='password'").bind(&principal.user_id).fetch_one(p).await.unwrap(),DbPool::Sqlite(p)=>sqlx::query_scalar("SELECT CAST(auth_data AS TEXT) FROM user_auth_methods WHERE user_id=$1 AND auth_type='password'").bind(&principal.user_id).fetch_one(p).await.unwrap()};
    let final_hash = serde_json::from_str::<serde_json::Value>(&final_data).unwrap()["hash"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(
        sdk_sessions.structured_content.unwrap()["sessions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["credential_id"] == extra_credential)
    );
    let revoke_path = <krausen_ui::server_fns::security::RevokeSession as ServerFn>::PATH;
    let revoke_id = uuid::Uuid::new_v4().to_string();
    let revoked = reqwest::Client::new()
        .post(format!("{base}{revoke_path}"))
        .bearer_auth(&browser.access_token)
        .form(&[
            ("token_id", extra_credential.clone()),
            ("expected_revision", "0".into()),
            ("request_id", revoke_id.clone()),
        ])
        .send()
        .await
        .unwrap();
    assert!(revoked.status().is_success());
    let revoked: serde_json::Value = revoked.json().await.unwrap();
    let sdk_retry=modern.call_tool(CallToolRequestParams::new("account_session_revoke").with_arguments(json!({"credential_id":extra_credential,"expected_revision":0,"request_id":revoke_id}).as_object().unwrap().clone())).await.unwrap();
    assert_eq!(sdk_retry.structured_content.unwrap()["outcome"], revoked);
    let invalid = reqwest::Client::new()
        .post(format!("{base}{session_path}"))
        .bearer_auth(&extra_browser.access_token)
        .form(&[] as &[(&str, &str)])
        .send()
        .await
        .unwrap();
    assert!(!invalid.status().is_success());
    let target_email = format!("member-{suffix}@example.test");
    let target = user_service::create_user(&db, &target_email, Some("Member"), true)
        .await
        .unwrap();
    let target_browser = session::create_authenticated_session(
        &db,
        &kv,
        "test-secret-at-least-32-bytes-long",
        &target,
        &device,
    )
    .await
    .unwrap();
    let target_claims = krausen_auth::jwt::validate_browser_token(
        &target_browser.access_token,
        "test-secret-at-least-32-bytes-long",
    )
    .unwrap()
    .claims;
    let target_credential = target_claims.extra["credential_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let cross_account = modern.call_tool(CallToolRequestParams::new("account_session_revoke").with_arguments(json!({"credential_id":target_credential,"expected_revision":0,"request_id":uuid::Uuid::new_v4()}).as_object().unwrap().clone())).await.unwrap();
    assert_eq!(cross_account.is_error, Some(true));
    krausen_core::db_execute!(&db, "INSERT INTO workspace_users (workspace_id,user_id,role,active) VALUES ($1,$2,'workspace_user',TRUE)", &ws, &target.user_id).unwrap();
    let team_snapshot = krausen_app::team::members(&db, &browser_actor)
        .await
        .unwrap();
    let team_id = uuid::Uuid::new_v4().to_string();
    let team_path = <krausen_ui::server_fns::team::UpdateMemberRole as ServerFn>::PATH;
    let browser_team = reqwest::Client::new()
        .post(format!("{base}{team_path}"))
        .bearer_auth(&browser.access_token)
        .form(&[
            ("user_id", target.user_id.clone()),
            ("role", "admin".into()),
            ("expected_revision", team_snapshot.revision.to_string()),
            ("request_id", team_id.clone()),
        ])
        .send()
        .await
        .unwrap();
    assert!(browser_team.status().is_success());
    let browser_team: serde_json::Value = browser_team.json().await.unwrap();
    let mcp_team = modern.call_tool(CallToolRequestParams::new("team_member_change").with_arguments(json!({"workspace_id":ws,"change":{"action":"set_role","user_id":target.user_id,"role":"workspace_admin"},"expected_revision":team_snapshot.revision,"request_id":team_id}).as_object().unwrap().clone())).await.unwrap();
    assert_eq!(
        mcp_team.structured_content.unwrap()["outcome"],
        browser_team
    );
    let owner_revision = krausen_app::team::members(&db, &browser_actor)
        .await
        .unwrap()
        .revision;
    let transfer = krausen_app::ownership::start(
        &db,
        &browser_actor,
        &target.user_id,
        owner_revision,
        uuid::Uuid::new_v4(),
        true,
    )
    .await
    .unwrap();
    let sdk_transfer = modern
        .call_tool(
            CallToolRequestParams::new("ownership_get").with_arguments(
                json!({"transfer_id":transfer.transfer_id})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        )
        .await
        .unwrap();
    assert_eq!(
        sdk_transfer.structured_content.unwrap()["transfer_id"],
        transfer.transfer_id
    );
    let get_transfer_path =
        <krausen_ui::server_fns::ownership::GetOwnershipTransfer as ServerFn>::PATH;
    let detail = reqwest::Client::new()
        .post(format!("{base}{get_transfer_path}"))
        .bearer_auth(&target_browser.access_token)
        .form(&[("transfer_id", transfer.transfer_id.as_str())])
        .send()
        .await
        .unwrap();
    assert!(detail.status().is_success());
    let detail: serde_json::Value = detail.json().await.unwrap();
    assert_eq!(detail["transfer_id"], transfer.transfer_id);
    let target_actor = krausen_app::ActorContext {
        actor_id: target.user_id.clone(),
        workspace_id: ws.clone(),
        credential_id: target_credential.clone(),
        access_expires_at: target_claims.exp,
    };
    let confirmation = krausen_app::ownership::begin_response(
        &db,
        &target_actor,
        &transfer.transfer_id,
        true,
        transfer.revision,
        uuid::Uuid::new_v4(),
        true,
    )
    .await
    .unwrap();
    let proof: krausen_app::invitation::Proof = krausen_core::db_fetch_optional!(
        &db,
        krausen_app::invitation::Proof,
        "SELECT * FROM account_action_proofs WHERE proof_id=$1",
        &confirmation.interaction_id
    )
    .unwrap()
    .unwrap();
    let code =
        krausen_app::invitation::delivery_token(b"test-secret-at-least-32-bytes-long", &proof)
            .unwrap();
    let review = reqwest::Client::new()
        .post(format!("{base}/account/proof/preview"))
        .bearer_auth(&target_browser.access_token)
        .header("Origin", &base)
        .form(&[
            ("interaction", confirmation.interaction_id.as_str()),
            ("code", code.as_str()),
        ])
        .send()
        .await
        .unwrap();
    assert!(review.status().is_success());
    assert!(review.text().await.unwrap().contains("Accept ownership of"));
    let complete = reqwest::Client::new()
        .post(format!("{base}/account/proof"))
        .bearer_auth(&target_browser.access_token)
        .header("Origin", &base)
        .form(&[
            ("interaction", confirmation.interaction_id.as_str()),
            ("code", code.as_str()),
        ])
        .send()
        .await
        .unwrap();
    assert!(complete.status().is_success());
    assert!(
        krausen_app::account::context(&db, &target_actor)
            .await
            .unwrap()
            .is_owner
    );
    let second = user_service::create_workspace_for_user(
        &db,
        &principal.user_id,
        Some("Second"),
        &email,
        None,
    )
    .await
    .unwrap();
    krausen_core::db_execute!(
        &db,
        "UPDATE workspaces SET status = 'active' WHERE workspace_id = $1",
        &second
    )
    .unwrap();
    let switch_path = <krausen_ui::server_fns::sidebar::SwitchWorkspace as ServerFn>::PATH;
    let switch_id = uuid::Uuid::new_v4().to_string();
    let switched = reqwest::Client::new()
        .post(format!("{base}{switch_path}"))
        .bearer_auth(&browser.access_token)
        .form(&[
            ("workspace_id", second.as_str()),
            ("request_id", switch_id.as_str()),
        ])
        .send()
        .await
        .unwrap();
    assert!(switched.status().is_success());
    let cookie = switched
        .headers()
        .get_all(reqwest::header::SET_COOKIE)
        .iter()
        .map(|h| h.to_str().unwrap())
        .find(|s| s.starts_with("access_token="))
        .unwrap();
    let new_token = cookie
        .split(';')
        .next()
        .unwrap()
        .strip_prefix("access_token=")
        .unwrap();
    let switched_claims =
        krausen_auth::jwt::validate_browser_token(new_token, "test-secret-at-least-32-bytes-long")
            .unwrap()
            .claims;
    assert_eq!(switched_claims.extra["workspace_id"], second);
    assert_eq!(
        switched_claims.extra["credential_id"],
        claims.extra["credential_id"]
    );
    assert_eq!(switched_claims.exp, claims.exp);
    let back = reqwest::Client::new()
        .post(format!("{base}{switch_path}"))
        .bearer_auth(new_token)
        .form(&[
            ("workspace_id", ws.as_str()),
            ("request_id", uuid::Uuid::new_v4().to_string().as_str()),
        ])
        .send()
        .await
        .unwrap();
    assert!(back.status().is_success());
    let back_cookie = back
        .headers()
        .get_all(reqwest::header::SET_COOKIE)
        .iter()
        .map(|h| h.to_str().unwrap())
        .find(|s| s.starts_with("access_token="))
        .unwrap();
    let back_token = back_cookie
        .split(';')
        .next()
        .unwrap()
        .strip_prefix("access_token=")
        .unwrap();
    let back_claims =
        krausen_auth::jwt::validate_browser_token(back_token, "test-secret-at-least-32-bytes-long")
            .unwrap()
            .claims;
    assert_eq!(back_claims.extra["workspace_id"], ws);
    assert_eq!(
        back_claims.extra["credential_id"],
        claims.extra["credential_id"]
    );
    for workspace in [&ws, &second] {
        let context = modern
            .call_tool(
                CallToolRequestParams::new("account_context").with_arguments(
                    json!({"workspace_id":workspace})
                        .as_object()
                        .unwrap()
                        .clone(),
                ),
            )
            .await
            .unwrap();
        assert_eq!(
            context.structured_content.unwrap()["workspace"]["workspace_id"],
            *workspace
        );
    }
    let denied_context = modern
        .call_tool(
            CallToolRequestParams::new("account_context").with_arguments(
                json!({"workspace_id":"not-a-member"})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        )
        .await
        .unwrap();
    assert_eq!(denied_context.is_error, Some(true));
    let denied_switch = reqwest::Client::new()
        .post(format!("{base}{switch_path}"))
        .bearer_auth(&browser.access_token)
        .form(&[
            ("workspace_id", "not-a-member"),
            ("request_id", uuid::Uuid::new_v4().to_string().as_str()),
        ])
        .send()
        .await
        .unwrap();
    assert!(!denied_switch.status().is_success());
    assert!(
        denied_switch
            .headers()
            .get(reqwest::header::SET_COOKIE)
            .is_none()
    );
    let memberships = modern
        .call_tool(CallToolRequestParams::new("account_workspaces"))
        .await
        .unwrap();
    assert!(
        memberships.structured_content.unwrap()["workspaces"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["workspace_id"] == ws)
    );
    let selection_id = uuid::Uuid::new_v4().to_string();
    let selection = json!({"workspace_id":ws,"request_id":selection_id})
        .as_object()
        .unwrap()
        .clone();
    let first = modern
        .call_tool(
            CallToolRequestParams::new("account_workspace_select")
                .with_arguments(selection.clone()),
        )
        .await
        .unwrap();
    let retry = modern
        .call_tool(CallToolRequestParams::new("account_workspace_select").with_arguments(selection))
        .await
        .unwrap();
    assert_eq!(first.structured_content, retry.structured_content);
    let denied_selection = modern
        .call_tool(
            CallToolRequestParams::new("account_workspace_select").with_arguments(
                json!({"workspace_id":"not-a-member","request_id":uuid::Uuid::new_v4()})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        )
        .await
        .unwrap();
    assert_eq!(denied_selection.is_error, Some(true));
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
        RESPONSE_SENTINEL
    );
    for tool in ["recipe_create", "recipe_import"] {
        let valid = json!({"workspace_id":ws, "beerxml":"<RECIPES/>"})
            .as_object()
            .unwrap()
            .clone();
        let result = modern
            .call_tool(CallToolRequestParams::new(tool).with_arguments(valid))
            .await
            .unwrap();
        assert_eq!(result.is_error, Some(true));
        assert_eq!(
            result.structured_content.unwrap()["error"],
            "legacy_authoring_unavailable"
        );
        let denied = json!({"workspace_id":"not-a-member", "beerxml":"<RECIPES/>"})
            .as_object()
            .unwrap()
            .clone();
        let result = modern
            .call_tool(CallToolRequestParams::new(tool).with_arguments(denied))
            .await
            .unwrap();
        assert_eq!(result.is_error, Some(true));
        assert_eq!(
            result.structured_content.unwrap()["error"],
            "unavailable_or_forbidden"
        );
    }
    let request_id = uuid::Uuid::new_v4().to_string();
    let workspace_revision = krausen_app::workspace_snapshot(&db, &browser_actor)
        .await
        .unwrap()
        .revision;
    let args = json!({"workspace_id": ws, "name":"SDK name", "expected_revision":workspace_revision, "request_id":request_id}).as_object().unwrap().clone();
    let first = legacy
        .call_tool(CallToolRequestParams::new("workspace_rename").with_arguments(args.clone()))
        .await
        .unwrap();
    let retry = modern
        .call_tool(CallToolRequestParams::new("workspace_rename").with_arguments(args))
        .await
        .unwrap();
    assert_eq!(first.structured_content, retry.structured_content);
    assert_eq!(
        first.structured_content.unwrap()["Saved"]["revision"],
        workspace_revision + 1
    );
    krausen_core::db_execute!(&db, "UPDATE workspace_users SET role = 'workspace_user' WHERE workspace_id = $1 AND user_id = $2", &ws, &principal.user_id).unwrap();
    let actor = krausen_app::ActorContext {
        actor_id: principal.user_id.clone(),
        workspace_id: ws.clone(),
        credential_id: principal.credential_id.clone(),
        access_expires_at: principal.access_expires_at,
    };
    assert!(matches!(
        krausen_app::workspace_snapshot(&db, &actor).await,
        Err(krausen_app::Error::Forbidden)
    ));
    let denied = modern
        .call_tool(
            CallToolRequestParams::new("workspace_get")
                .with_arguments(json!({"workspace_id":ws}).as_object().unwrap().clone()),
        )
        .await
        .unwrap();
    assert_eq!(denied.is_error, Some(true));
    krausen_core::db_execute!(&db, "UPDATE workspace_users SET role = 'workspace_admin' WHERE workspace_id = $1 AND user_id = $2", &ws, &principal.user_id).unwrap();
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
                CallToolRequestParams::new("formulation_styles").with_arguments(
                    json!({"workspace_id":ws,"intent":intent})
                        .as_object()
                        .unwrap()
                        .clone()
                )
            )
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
    let logout_path = <krausen_ui::server_fns::security::LogoutAllSessions as ServerFn>::PATH;
    let logout = reqwest::Client::new()
        .post(format!("{base}{logout_path}"))
        .bearer_auth(&browser.access_token)
        .form(&[("request_id", uuid::Uuid::new_v4().to_string())])
        .send()
        .await
        .unwrap();
    assert!(logout.status().is_success());
    assert!(
        logout
            .headers()
            .get_all(reqwest::header::SET_COOKIE)
            .iter()
            .any(|value| value.to_str().unwrap().contains("Max-Age=0"))
    );
    let logout_result: serde_json::Value = logout.json().await.unwrap();
    assert!(logout_result.get("Revoked").is_some());
    assert!(matches!(
        krausen_ui::server_fns::formulation::execute_styles(&db, &browser_actor, &intent).await,
        krausen_types::formulation::StyleListOutcome::Forbidden
    ));
    let revoked_library = LibraryQuery {
        intent: intent.clone(),
        search: String::new(),
        archived: false,
        cursor: None,
        limit: 50,
    };
    assert!(matches!(
        krausen_ui::server_fns::recipe_library::execute(
            &db,
            &browser_actor,
            LibraryCommand::List(revoked_library.clone())
        )
        .await,
        krausen_types::recipe_library::LibraryOutcome::Forbidden
    ));
    assert!(
        modern
            .call_tool(
                CallToolRequestParams::new("recipe_list").with_arguments(
                    json!({"workspace_id":ws,"request":revoked_library})
                        .as_object()
                        .unwrap()
                        .clone()
                )
            )
            .await
            .is_err()
    );
    server.abort();
    let captured = String::from_utf8(logs.lock().unwrap().clone()).unwrap();
    assert!(
        captured.contains("MCP tool request"),
        "sanitized application diagnostics missing"
    );
    for sentinel in [
        PASSWORD_SENTINEL,
        NEW_PASSWORD_SENTINEL,
        WRONG_PASSWORD_SENTINEL,
        UI_PASSWORD_SENTINEL,
        PROOF_SENTINEL,
        TOKEN_SENTINEL,
        RESPONSE_SENTINEL,
        NOTIFICATION_SENTINEL,
    ] {
        assert!(
            !captured.contains(sentinel),
            "sensitive transport payload appeared in logs"
        );
    }
    for hash in [&old_hash, &intermediate_hash, &final_hash] {
        assert!(
            !captured.contains(hash),
            "password verifier appeared in logs"
        );
    }
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
