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

async fn brew_ui_outcome(
    base: &str,
    token: &str,
    path: &str,
    request: &serde_json::Value,
) -> serde_json::Value {
    let response = reqwest::Client::new()
        .post(format!("{base}{path}"))
        .bearer_auth(token)
        .form(&[("request_json", serde_json::to_string(request).unwrap())])
        .send()
        .await
        .unwrap();
    assert!(
        response.status().is_success(),
        "brew UI HTTP {}",
        response.status()
    );
    response.json().await.unwrap()
}

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
    let discovered = modern.list_tools(None).await.unwrap();
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
    // Brew start uses the same shared command through MCP and the browser,
    // including immutable plan replay and the optional head guard.
    let brew_profile_id = uuid::Uuid::new_v4().to_string();
    let mut brew_profile_request = brew_test_profile(&intent, &brew_profile_id);
    brew_profile_request.request_id = uuid::Uuid::new_v4().to_string();
    krausen_app::equipment::save(&db, &browser_actor, brew_profile_request)
        .await
        .unwrap();
    let brew_doc = brew_test_document(&brew_profile_id);
    let krausen_types::recipe_editor::RecipeSaveOutcome::Saved {
        recipe_id: brew_recipe,
        revision: 1,
    } = krausen_app::recipe_editor::save(
        &db,
        &browser_actor,
        &intent,
        None,
        None,
        &brew_doc,
        uuid::Uuid::new_v4(),
    )
    .await
    else {
        panic!("brew recipe fixture");
    };
    let brew_preview = match krausen_app::formulation::preview(
        &db,
        &browser_actor,
        krausen_types::formulation::PreviewRequest {
            draft_generation: "mcp-brew-start".into(),
            intent: intent.clone(),
            draft: brew_doc.clone(),
            equipment_draft: Default::default(),
        },
    )
    .await
    .unwrap()
    {
        krausen_types::formulation::PreviewOutcome::Preview(preview) => preview,
        other => panic!("brew preview unavailable: {other:?}"),
    };
    let brew_start_request = krausen_types::brew::StartBrewRequest {
        intent: intent.clone(),
        recipe_id: brew_recipe.clone(),
        selected_recipe_revision: 1,
        expected_head_revision: Some(1),
        expected_calculation_context: brew_preview.context_digest.clone().unwrap(),
        request_id: uuid::Uuid::new_v4().to_string(),
    };
    let brew_start_args = json!({
        "workspace_id": ws,
        "request": brew_start_request.clone(),
    })
    .as_object()
    .unwrap()
    .clone();
    let brew_start = modern
        .call_tool(CallToolRequestParams::new("brew_start").with_arguments(brew_start_args))
        .await
        .unwrap();
    assert_ne!(brew_start.is_error, Some(true));
    let brew_start = brew_start.structured_content.unwrap();
    assert_eq!(
        brew_start["outcome"]["Started"]["selected_recipe_revision"],
        1
    );
    let browser_brew_start =
        krausen_ui::server_fns::brew::execute_start_brew(&db, &browser_actor, brew_start_request)
            .await;
    assert_eq!(
        serde_json::to_value(browser_brew_start).unwrap()["Started"],
        brew_start["outcome"]["Started"]
    );

    // KRA-21 evidence families through the authenticated MCP adapter. The same
    // shared application operations are exercised by the browser/app tests;
    // this block proves the MCP envelope, principal/workspace selection, typed
    // retry, revision conflicts, invalid input and authorization outcomes.
    let brew_id = brew_start["outcome"]["Started"]["brew_id"]
        .as_str()
        .unwrap()
        .to_string();
    let view_args = json!({
        "workspace_id": &ws,
        "request": {"intent": intent.clone(), "brew_id": &brew_id},
    })
    .as_object()
    .unwrap()
    .clone();
    let evidence_view = modern
        .call_tool(
            CallToolRequestParams::new("brew_evidence_view").with_arguments(view_args.clone()),
        )
        .await
        .unwrap();
    assert_ne!(evidence_view.is_error, Some(true));
    let evidence_view = evidence_view.structured_content.unwrap();
    let ui_view = brew_ui_outcome(
        &base,
        &browser.access_token,
        <krausen_ui::server_fns::brew::BrewEvidenceView as ServerFn>::PATH,
        &view_args["request"],
    )
    .await;
    assert_eq!(ui_view, evidence_view["outcome"]);
    assert_eq!(evidence_view["outcome"]["View"]["brew_id"], brew_id);
    assert_eq!(
        evidence_view["outcome"]["View"]["measurements"]
            .as_array()
            .unwrap()
            .len(),
        8
    );

    let record_request_id = uuid::Uuid::new_v4().to_string();
    let record_args = json!({
        "workspace_id": &ws,
        "request": {
            "action": "record",
            "intent": intent.clone(),
            "brew_id": &brew_id,
            "quantity_id": "gravity.original.v1",
            "value": 1.050,
            "source": "manual",
            "request_id": &record_request_id,
        },
    })
    .as_object()
    .unwrap()
    .clone();
    let recorded = modern
        .call_tool(
            CallToolRequestParams::new("brew_measurement").with_arguments(record_args.clone()),
        )
        .await
        .unwrap();
    assert_ne!(recorded.is_error, Some(true));
    let recorded = recorded.structured_content.unwrap();
    let entry_id = recorded["outcome"]["Recorded"]["entry_id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(recorded["outcome"]["Recorded"]["value"], 1.050);
    let replayed = modern
        .call_tool(
            CallToolRequestParams::new("brew_measurement").with_arguments(record_args.clone()),
        )
        .await
        .unwrap();
    assert_ne!(replayed.is_error, Some(true));
    let replayed = replayed.structured_content.unwrap();
    assert_eq!(replayed["outcome"]["Recorded"]["entry_id"], entry_id);
    let ui_replay = brew_ui_outcome(
        &base,
        &browser.access_token,
        <krausen_ui::server_fns::brew::BrewMeasurement as ServerFn>::PATH,
        &record_args["request"],
    )
    .await;
    assert_eq!(ui_replay, replayed["outcome"]);

    let invalid_measurement = json!({
        "workspace_id": &ws,
        "request": {
            "action": "record",
            "intent": intent.clone(),
            "brew_id": &brew_id,
            "quantity_id": "not.a.quantity",
            "value": 1.0,
            "request_id": uuid::Uuid::new_v4().to_string(),
        },
    })
    .as_object()
    .unwrap()
    .clone();
    let ui_invalid = brew_ui_outcome(
        &base,
        &browser.access_token,
        <krausen_ui::server_fns::brew::BrewMeasurement as ServerFn>::PATH,
        &invalid_measurement["request"],
    )
    .await;
    let invalid_measurement = modern
        .call_tool(
            CallToolRequestParams::new("brew_measurement")
                .with_arguments(invalid_measurement.clone()),
        )
        .await
        .unwrap();
    assert_eq!(invalid_measurement.is_error, Some(true));
    let invalid_result = invalid_measurement.structured_content.unwrap();
    assert_eq!(invalid_result["outcome"], "Invalid");
    assert_eq!(ui_invalid, invalid_result["outcome"]);

    let note_args = json!({
        "workspace_id": &ws,
        "request": {
            "action": "record",
            "intent": intent.clone(),
            "brew_id": &brew_id,
            "text": "MCP evidence note",
            "request_id": uuid::Uuid::new_v4().to_string(),
        },
    })
    .as_object()
    .unwrap()
    .clone();
    let note = modern
        .call_tool(CallToolRequestParams::new("brew_note").with_arguments(note_args))
        .await
        .unwrap();
    assert_ne!(note.is_error, Some(true));
    let note = note.structured_content.unwrap();
    let note_id = note["outcome"]["Recorded"]["note_id"]
        .as_str()
        .unwrap()
        .to_string();
    let stale_note = json!({
        "workspace_id": &ws,
        "request": {
            "action": "correct",
            "intent": intent.clone(),
            "brew_id": &brew_id,
            "note_id": &note_id,
            "expected_revision": 99,
            "text": "stale note correction",
            "request_id": uuid::Uuid::new_v4().to_string(),
        },
    })
    .as_object()
    .unwrap()
    .clone();
    let stale_note = modern
        .call_tool(CallToolRequestParams::new("brew_note").with_arguments(stale_note))
        .await
        .unwrap();
    assert_eq!(stale_note.is_error, Some(true));
    assert_eq!(
        stale_note.structured_content.unwrap()["outcome"]["Conflict"]["expected"],
        99
    );

    let actual_get = json!({
        "workspace_id": &ws,
        "request": {
            "action": "get",
            "intent": intent.clone(),
            "brew_id": &brew_id,
        },
    })
    .as_object()
    .unwrap()
    .clone();
    let actual_get = modern
        .call_tool(CallToolRequestParams::new("brew_actual").with_arguments(actual_get))
        .await
        .unwrap();
    assert_ne!(actual_get.is_error, Some(true));
    let actual_get = actual_get.structured_content.unwrap();
    assert_eq!(actual_get["outcome"]["View"]["source"], "plan");
    let mut actual_document = actual_get["outcome"]["View"]["document"].clone();
    actual_document["boil_minutes"] = json!(75.0);
    let actual_amend_args = json!({
        "workspace_id": &ws,
        "request": {
            "action": "amend",
            "intent": intent.clone(),
            "brew_id": &brew_id,
            "expected_actual_revision": null,
            "document": actual_document,
            "reason": "MCP boil deviation",
            "request_id": uuid::Uuid::new_v4().to_string(),
        },
    })
    .as_object()
    .unwrap()
    .clone();
    let actual_amend_response = modern
        .call_tool(
            CallToolRequestParams::new("brew_actual").with_arguments(actual_amend_args.clone()),
        )
        .await
        .unwrap();
    assert_ne!(actual_amend_response.is_error, Some(true));
    let actual_amend = actual_amend_response.structured_content.unwrap();
    assert_eq!(actual_amend["outcome"]["Amended"]["revision"], 1);
    assert_eq!(
        actual_amend["outcome"]["Amended"]["document"]["boil_minutes"],
        75.0
    );
    // Identical same-request replay recovers the committed amendment before
    // the current-revision guard.
    let actual_replay = modern
        .call_tool(
            CallToolRequestParams::new("brew_actual").with_arguments(actual_amend_args.clone()),
        )
        .await
        .unwrap();
    assert_ne!(actual_replay.is_error, Some(true));
    let actual_replay = actual_replay.structured_content.unwrap();
    assert_eq!(
        actual_replay["outcome"]["Amended"],
        actual_amend["outcome"]["Amended"]
    );
    // A new request identity with the stale expected revision is a conflict.
    let mut stale_actual_args = actual_amend_args;
    stale_actual_args["request"]["request_id"] = json!(uuid::Uuid::new_v4().to_string());
    let ui_stale_actual = brew_ui_outcome(
        &base,
        &browser.access_token,
        <krausen_ui::server_fns::brew::BrewActual as ServerFn>::PATH,
        &stale_actual_args["request"],
    )
    .await;
    let stale_actual = modern
        .call_tool(CallToolRequestParams::new("brew_actual").with_arguments(stale_actual_args))
        .await
        .unwrap();
    assert_eq!(stale_actual.is_error, Some(true));
    let stale_actual = stale_actual.structured_content.unwrap();
    assert_eq!(ui_stale_actual, stale_actual["outcome"]);
    assert_eq!(
        stale_actual["outcome"]["Conflict"]["expected"],
        serde_json::Value::Null
    );
    assert_eq!(stale_actual["outcome"]["Conflict"]["actual"], 1);

    let lifecycle_args = json!({
        "workspace_id": &ws,
        "request": {
            "intent": intent.clone(),
            "brew_id": &brew_id,
            "expected_revision": 1,
            "transition": "complete",
            "request_id": uuid::Uuid::new_v4().to_string(),
        },
    })
    .as_object()
    .unwrap()
    .clone();
    let lifecycle = modern
        .call_tool(CallToolRequestParams::new("brew_lifecycle").with_arguments(lifecycle_args))
        .await
        .unwrap();
    assert_ne!(lifecycle.is_error, Some(true));
    let lifecycle = lifecycle.structured_content.unwrap();
    assert_eq!(lifecycle["outcome"]["Transitioned"]["status"], "completed");
    assert_eq!(lifecycle["outcome"]["Transitioned"]["revision"], 2);
    let invalid_transition = json!({
        "workspace_id": &ws,
        "request": {
            "intent": intent.clone(),
            "brew_id": &brew_id,
            "expected_revision": 2,
            "transition": "complete",
            "request_id": uuid::Uuid::new_v4().to_string(),
        },
    })
    .as_object()
    .unwrap()
    .clone();
    let ui_invalid_transition = brew_ui_outcome(
        &base,
        &browser.access_token,
        <krausen_ui::server_fns::brew::BrewLifecycle as ServerFn>::PATH,
        &invalid_transition["request"],
    )
    .await;
    let invalid_transition = modern
        .call_tool(CallToolRequestParams::new("brew_lifecycle").with_arguments(invalid_transition))
        .await
        .unwrap();
    assert_eq!(invalid_transition.is_error, Some(true));
    let invalid_transition = invalid_transition.structured_content.unwrap();
    assert!(
        invalid_transition["outcome"]
            .get("InvalidTransition")
            .is_some()
    );
    assert_eq!(ui_invalid_transition, invalid_transition["outcome"]);

    let archive_args = json!({
        "workspace_id": &ws,
        "request": {
            "intent": intent.clone(),
            "brew_id": &brew_id,
            "archived": true,
            "expected_metadata_revision": 1,
            "request_id": uuid::Uuid::new_v4().to_string(),
        },
    })
    .as_object()
    .unwrap()
    .clone();
    let archived = modern
        .call_tool(CallToolRequestParams::new("brew_archive").with_arguments(archive_args))
        .await
        .unwrap();
    assert_ne!(archived.is_error, Some(true));
    assert_eq!(
        archived.structured_content.unwrap()["outcome"]["Archived"]["archived"],
        true
    );
    let archived_record = json!({
        "workspace_id": &ws,
        "request": {
            "action": "record",
            "intent": intent.clone(),
            "brew_id": &brew_id,
            "quantity_id": "gravity.final.v1",
            "value": 1.010,
            "request_id": uuid::Uuid::new_v4().to_string(),
        },
    })
    .as_object()
    .unwrap()
    .clone();
    let archived_record = modern
        .call_tool(CallToolRequestParams::new("brew_measurement").with_arguments(archived_record))
        .await
        .unwrap();
    assert_eq!(archived_record.is_error, Some(true));
    assert_eq!(
        archived_record.structured_content.unwrap()["outcome"],
        "Archived"
    );
    let unarchive_args = json!({
        "workspace_id": &ws,
        "request": {
            "intent": intent.clone(),
            "brew_id": &brew_id,
            "archived": false,
            "expected_metadata_revision": 2,
            "request_id": uuid::Uuid::new_v4().to_string(),
        },
    })
    .as_object()
    .unwrap()
    .clone();
    let unarchived = modern
        .call_tool(CallToolRequestParams::new("brew_archive").with_arguments(unarchive_args))
        .await
        .unwrap();
    assert_ne!(unarchived.is_error, Some(true));
    assert_eq!(
        unarchived.structured_content.unwrap()["outcome"]["Archived"]["archived"],
        false
    );

    let forbidden_view = json!({
        "workspace_id": "not-a-member",
        "request": {"intent": intent.clone(), "brew_id": &brew_id},
    })
    .as_object()
    .unwrap()
    .clone();
    let forbidden_view = modern
        .call_tool(CallToolRequestParams::new("brew_evidence_view").with_arguments(forbidden_view))
        .await
        .unwrap();
    assert_eq!(forbidden_view.is_error, Some(true));
    assert_eq!(
        forbidden_view.structured_content.unwrap()["outcome"],
        "Forbidden"
    );
    let mut brew_edit = brew_doc.clone();
    brew_edit.title = "Brew edit".into();
    assert_eq!(
        krausen_app::recipe_editor::save(
            &db,
            &browser_actor,
            &intent,
            Some(&brew_recipe),
            Some(1),
            &brew_edit,
            uuid::Uuid::new_v4(),
        )
        .await,
        krausen_types::recipe_editor::RecipeSaveOutcome::Saved {
            recipe_id: brew_recipe.clone(),
            revision: 2
        }
    );
    let head_conflict_request = krausen_types::brew::StartBrewRequest {
        intent: intent.clone(),
        recipe_id: brew_recipe.clone(),
        selected_recipe_revision: 1,
        expected_head_revision: Some(1),
        expected_calculation_context: brew_preview.context_digest.unwrap(),
        request_id: uuid::Uuid::new_v4().to_string(),
    };
    let head_conflict_args = json!({
        "workspace_id": ws,
        "request": head_conflict_request,
    })
    .as_object()
    .unwrap()
    .clone();
    let head_conflict = modern
        .call_tool(CallToolRequestParams::new("brew_start").with_arguments(head_conflict_args))
        .await
        .unwrap();
    assert_eq!(head_conflict.is_error, Some(true));
    assert_eq!(
        head_conflict.structured_content.unwrap()["outcome"]["HeadConflict"]["actual_head"],
        2
    );
    let zero_context = "0".repeat(64);
    let denied_brew_args = json!({
        "workspace_id": "not-a-member",
        "request": {
            "intent": intent.clone(),
            "recipe_id": brew_recipe,
            "selected_recipe_revision": 1,
            "expected_head_revision": null,
            "expected_calculation_context": zero_context,
            "request_id": uuid::Uuid::new_v4().to_string()
        }
    })
    .as_object()
    .unwrap()
    .clone();
    let denied_brew = modern
        .call_tool(CallToolRequestParams::new("brew_start").with_arguments(denied_brew_args))
        .await
        .unwrap();
    assert_eq!(denied_brew.is_error, Some(true));
    assert_eq!(
        denied_brew.structured_content.unwrap()["outcome"],
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
    // Scaling preview/apply and duplication use the same shared application
    // operations through MCP; apply is guarded by head and accepted context.
    let mut scale_doc = krausen_types::brewery::RecipeDocumentV1::default();
    scale_doc.title = "MCP scale".into();
    scale_doc.batch_size_l = Some(20.0);
    scale_doc.boil_minutes = Some(60.0);
    scale_doc.process.mash_water_method = Some("manual".into());
    scale_doc.process.mash_water_l = Some(12.0);
    scale_doc.process.apparent_attenuation_fraction = Some(0.8);
    scale_doc.process.kettle_top_up_l = Some(2.0);
    scale_doc.equipment_overrides.mash_efficiency_fraction = Some(0.75);
    scale_doc.equipment_overrides.boil_off_l_per_hour = Some(3.0);
    scale_doc.equipment_overrides.transfer_loss_l = Some(2.0);
    scale_doc.equipment_overrides.grain_absorption_l_per_kg = Some(0.8);
    scale_doc.equipment_overrides.kettle_capacity_l = Some(30.0);
    scale_doc
        .fermentables
        .push(krausen_types::brewery::FermentableAddition {
            row_id: "scale-grain".into(),
            amount_kg: Some(4.0),
            addition: krausen_types::brewery::AdditionStage {
                stage: "mash".into(),
                time_minutes: Some(60.0),
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
    scale_doc.hops.push(krausen_types::brewery::HopAddition {
        row_id: "scale-hop".into(),
        amount_g: Some(30.0),
        stage: "boil".into(),
        contact_minutes: Some(60.0),
        temperature_c: Some(100.0),
        specification: krausen_types::brewery::HopSpecification {
            name: None,
            producer: None,
            alpha_fraction: Some(0.055),
        },
        source: None,
    });
    let scale_saved = krausen_app::recipe_editor::save(
        &db,
        &actor_for_recipe,
        &intent,
        None,
        None,
        &scale_doc,
        uuid::Uuid::new_v4(),
    )
    .await;
    let krausen_types::recipe_editor::RecipeSaveOutcome::Saved {
        recipe_id: scale_id,
        ..
    } = scale_saved
    else {
        panic!("create scaling recipe");
    };
    let scale_args = json!({
        "workspace_id": ws,
        "intent": intent,
        "recipe_id": scale_id,
        "target_batch_size_l": 40.0
    })
    .as_object()
    .unwrap()
    .clone();
    let scale_preview_mcp = modern
        .call_tool(
            CallToolRequestParams::new("recipe_scale_preview").with_arguments(scale_args.clone()),
        )
        .await
        .unwrap();
    assert_eq!(scale_preview_mcp.is_error, Some(false));
    let scale_preview_value = scale_preview_mcp.structured_content.unwrap();
    assert_eq!(scale_preview_value["outcome"]["Preview"]["factor"], 2.0);
    assert_eq!(
        scale_preview_value["outcome"]["Preview"]["scaled_document"]["fermentables"][0]["amount_kg"],
        8.0
    );
    let browser_scale = krausen_ui::server_fns::scaling::execute_preview(
        &db,
        &actor_for_recipe,
        krausen_types::scaling::ScalePreviewRequest {
            intent: intent.clone(),
            recipe_id: scale_id.clone(),
            target_batch_size_l: 40.0,
        },
    )
    .await;
    assert_eq!(
        serde_json::to_value(browser_scale).unwrap(),
        scale_preview_value["outcome"]
    );
    let preview_outcome: krausen_types::scaling::ScaleOutcome =
        serde_json::from_value(scale_preview_value["outcome"].clone()).unwrap();
    let krausen_types::scaling::ScaleOutcome::Preview(scale_preview) = preview_outcome else {
        panic!("scale preview");
    };
    let scale_request = krausen_types::scaling::ScaleApplyRequest {
        intent: intent.clone(),
        recipe_id: scale_id.clone(),
        expected_revision: 1,
        target_batch_size_l: 40.0,
        accepted: scale_preview.accepted.clone(),
        request_id: uuid::Uuid::new_v4().to_string(),
    };
    let apply_args = json!({"workspace_id": ws, "request": scale_request})
        .as_object()
        .unwrap()
        .clone();
    let applied = modern
        .call_tool(
            CallToolRequestParams::new("recipe_scale_apply").with_arguments(apply_args.clone()),
        )
        .await
        .unwrap();
    assert_eq!(applied.is_error, Some(false));
    assert_eq!(
        applied.structured_content.as_ref().unwrap()["outcome"]["Applied"]["revision"],
        2
    );
    let replayed = legacy
        .call_tool(CallToolRequestParams::new("recipe_scale_apply").with_arguments(apply_args))
        .await
        .unwrap();
    assert_eq!(applied.structured_content, replayed.structured_content);
    let duplicate_request = krausen_types::scaling::DuplicateRecipe {
        intent: intent.clone(),
        source_recipe_id: scale_id.clone(),
        expected_revision: 2,
        title: Some("MCP copy".into()),
        request_id: uuid::Uuid::new_v4().to_string(),
    };
    let duplicate_args = json!({"workspace_id": ws, "request": duplicate_request})
        .as_object()
        .unwrap()
        .clone();
    let duplicated = modern
        .call_tool(CallToolRequestParams::new("recipe_duplicate").with_arguments(duplicate_args))
        .await
        .unwrap();
    assert_eq!(duplicated.is_error, Some(false));
    assert_eq!(
        duplicated.structured_content.as_ref().unwrap()["outcome"]["Duplicated"]["revision"],
        1
    );
    assert_eq!(
        duplicated.structured_content.unwrap()["outcome"]["Duplicated"]["source_revision"],
        2
    );
    let bad_scale = modern
        .call_tool(
            CallToolRequestParams::new("recipe_scale_apply").with_arguments(
                json!({
                    "workspace_id": ws,
                    "request": {
                        "intent": intent,
                        "recipe_id": scale_id,
                        "expected_revision": 2,
                        "target_batch_size_l": 40.0,
                        "accepted": scale_preview.accepted,
                        "request_id": "invalid"
                    }
                })
                .as_object()
                .unwrap()
                .clone(),
            ),
        )
        .await
        .unwrap();
    assert_eq!(bad_scale.is_error, Some(true));
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

    // KRA-22 history/comparison/feedback/print through the authenticated MCP
    // adapter, using the immutable plan and KRA-21 actual evidence above.
    {
        let history_view = modern
            .call_tool(
                CallToolRequestParams::new("brew_evidence_view").with_arguments(
                    json!({"workspace_id": &ws, "request": {"intent": intent.clone(), "brew_id": &brew_id}})
                        .as_object()
                        .unwrap()
                        .clone(),
                ),
            )
            .await
            .unwrap();
        let history_recipe =
            history_view.structured_content.unwrap()["outcome"]["View"]["recipe_id"]
                .as_str()
                .unwrap()
                .to_string();
        let history_args = json!({
            "workspace_id": &ws,
            "request": {
                "intent": intent.clone(),
                "recipe_id": &history_recipe,
                "include_archived": true,
                "cursor": null,
                "limit": 10
            }
        })
        .as_object()
        .unwrap()
        .clone();
        let history = modern
            .call_tool(
                CallToolRequestParams::new("brew_recipe_history")
                    .with_arguments(history_args.clone()),
            )
            .await
            .unwrap();
        assert_ne!(history.is_error, Some(true));
        let history = history.structured_content.unwrap();
        assert_eq!(
            brew_ui_outcome(
                &base,
                &browser.access_token,
                <krausen_ui::server_fns::brew_history::ExecuteBrewHistory as ServerFn>::PATH,
                &history_args["request"]
            )
            .await,
            history["outcome"]
        );
        assert_eq!(
            history["outcome"]["History"]["items"][0]["brew_id"],
            brew_id
        );
        let comparison_args = json!({
            "workspace_id": &ws,
            "request": {"intent": intent.clone(), "brew_id": &brew_id}
        })
        .as_object()
        .unwrap()
        .clone();
        let comparison = modern
            .call_tool(
                CallToolRequestParams::new("brew_comparison")
                    .with_arguments(comparison_args.clone()),
            )
            .await
            .unwrap();
        assert_ne!(comparison.is_error, Some(true));
        let comparison = comparison.structured_content.unwrap();
        assert_eq!(
            brew_ui_outcome(
                &base,
                &browser.access_token,
                <krausen_ui::server_fns::brew_history::ExecuteBrewComparison as ServerFn>::PATH,
                &comparison_args["request"]
            )
            .await,
            comparison["outcome"]
        );
        assert_eq!(
            comparison["outcome"]["Comparison"]["selected_recipe_revision"],
            1
        );
        let change = comparison["outcome"]["Comparison"]["changes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|change| change["id"] == "process:boil_minutes")
            .unwrap()
            .clone();
        let change_ids = json!([change["id"].as_str().unwrap()]);
        let feedback_preview_args = json!({
            "workspace_id": &ws,
            "request": {
                "intent": intent.clone(),
                "brew_id": &brew_id,
                "expected_actual_revision": 1,
                "change_ids": change_ids
            }
        })
        .as_object()
        .unwrap()
        .clone();
        let feedback_preview = modern
            .call_tool(
                CallToolRequestParams::new("brew_feedback_preview")
                    .with_arguments(feedback_preview_args.clone()),
            )
            .await
            .unwrap();
        assert_ne!(feedback_preview.is_error, Some(true));
        let feedback_preview = feedback_preview.structured_content.unwrap();
        assert_eq!(brew_ui_outcome(&base, &browser.access_token, <krausen_ui::server_fns::brew_history::ExecuteBrewFeedbackPreview as ServerFn>::PATH, &feedback_preview_args["request"]).await, feedback_preview["outcome"]);
        assert_eq!(
            feedback_preview["outcome"]["Preview"]["proposed_recipe"]["boil_minutes"],
            75.0
        );
        let digest = feedback_preview["outcome"]["Preview"]["preview"]["context_digest"]
            .as_str()
            .unwrap()
            .to_string();
        let mut stale_preview_args = feedback_preview_args;
        stale_preview_args["request"]["expected_actual_revision"] = json!(99);
        let stale_preview = modern
            .call_tool(
                CallToolRequestParams::new("brew_feedback_preview")
                    .with_arguments(stale_preview_args.clone()),
            )
            .await
            .unwrap();
        assert_eq!(stale_preview.is_error, Some(true));
        let stale_preview = stale_preview.structured_content.unwrap();
        assert!(stale_preview["outcome"].get("Conflict").is_some());
        assert_eq!(brew_ui_outcome(&base, &browser.access_token, <krausen_ui::server_fns::brew_history::ExecuteBrewFeedbackPreview as ServerFn>::PATH, &stale_preview_args["request"]).await, stale_preview["outcome"]);
        let feedback_apply_args = json!({
            "workspace_id": &ws,
            "request": {
                "intent": intent.clone(),
                "brew_id": &brew_id,
                "expected_actual_revision": 1,
                "change_ids": change_ids,
                "expected_recipe_head": 2,
                "expected_calculation_context": digest,
                "request_id": uuid::Uuid::new_v4().to_string()
            }
        })
        .as_object()
        .unwrap()
        .clone();
        let feedback_apply = modern
            .call_tool(
                CallToolRequestParams::new("brew_feedback_apply")
                    .with_arguments(feedback_apply_args.clone()),
            )
            .await
            .unwrap();
        assert_ne!(feedback_apply.is_error, Some(true));
        let feedback_apply = feedback_apply.structured_content.unwrap();
        assert_eq!(feedback_apply["outcome"]["Applied"]["recipe_revision"], 3);
        assert_eq!(
            brew_ui_outcome(
                &base,
                &browser.access_token,
                <krausen_ui::server_fns::brew_history::ExecuteBrewFeedbackApply as ServerFn>::PATH,
                &feedback_apply_args["request"]
            )
            .await,
            feedback_apply["outcome"]
        );
        let print_args = json!({
            "workspace_id": &ws,
            "request": {"intent": intent.clone(), "brew_id": &brew_id}
        })
        .as_object()
        .unwrap()
        .clone();
        let print = modern
            .call_tool(CallToolRequestParams::new("brew_print").with_arguments(print_args.clone()))
            .await
            .unwrap();
        assert_ne!(print.is_error, Some(true));
        let print = print.structured_content.unwrap();
        assert_eq!(
            brew_ui_outcome(
                &base,
                &browser.access_token,
                <krausen_ui::server_fns::brew_history::ExecuteBrewPrint as ServerFn>::PATH,
                &print_args["request"]
            )
            .await,
            print["outcome"]
        );
        assert_eq!(
            print["outcome"]["Print"]["summary"]["source"]["kind"],
            "historical_brew"
        );
        assert_eq!(print["outcome"]["Print"]["summary"]["boil_minutes"], 60.0);
        assert!(print["outcome"]["Print"]["actual"].is_object());
    }

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
    krausen_core::db_execute!(
        &db,
        "UPDATE auth_credentials SET revoked_at = $1 WHERE credential_id = $2",
        chrono::Utc::now().timestamp(),
        &browser_actor.credential_id
    )
    .unwrap();
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
}
// The authenticated exercise covers every MCP tool and the server runs on the
// driving thread. KRA-22 added five tools, so give this integration test its own
// stack rather than relying on libtest's default.
fn run_mcp_exercise(body: impl FnOnce(tokio::runtime::Runtime) + Send + 'static) {
    std::thread::Builder::new()
        .name("mcp-exercise".into())
        .stack_size(16 * 1024 * 1024)
        .spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            body(runtime);
        })
        .unwrap()
        .join()
        .unwrap();
}
#[test]
fn sqlite_sdk_clients() {
    run_mcp_exercise(|runtime| {
        runtime.block_on(async {
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
        });
    });
}
#[test]
fn postgres_sdk_clients() {
    run_mcp_exercise(|runtime| {
        runtime.block_on(async {
            let url = std::env::var("KRAUSEN_TEST_POSTGRES_URL")
                .expect("disposable PostgreSQL URL required");
            let pool = sqlx::PgPool::connect(&url).await.unwrap();
            sqlx::migrate!("./migrations").run(&pool).await.unwrap();
            exercise(DbPool::Postgres(pool)).await;
        });
    });
}

fn brew_test_profile(
    intent: &krausen_types::request_context::OperationIntent,
    profile_id: &str,
) -> krausen_types::equipment::SaveEquipment {
    krausen_types::equipment::SaveEquipment {
        intent: intent.clone(),
        profile_id: profile_id.into(),
        name: "MCP 20 L kettle".into(),
        settings: krausen_types::brewery::EquipmentSettingsV1 {
            mash_efficiency_fraction: Some(0.75),
            boil_off_l_per_hour: Some(3.0),
            transfer_loss_l: Some(2.0),
            pre_boil_deadspace_l: Some(0.0),
            grain_absorption_l_per_kg: Some(0.8),
            mash_thickness_l_per_kg: Some(2.5),
            kettle_capacity_l: Some(30.0),
            kettle_diameter_cm: Some(35.0),
            kettle_opening_cm: Some(30.0),
            boil_temperature_c: Some(100.0),
            chilling: Some(krausen_types::brewery::ChillingV1::Natural),
            ..Default::default()
        },
        provenance: krausen_types::brewery::EquipmentProvenance {
            source: "test fixture".into(),
            assumptions: vec!["brewhouse_loss_calibrated".into()],
        },
        expected_revision: 0,
        expected_metadata_revision: 0,
        request_id: String::new(),
    }
}

fn brew_test_document(profile_id: &str) -> krausen_types::brewery::RecipeDocumentV1 {
    krausen_types::brewery::RecipeDocumentV1 {
        schema_version: 1,
        title: "MCP brew start".into(),
        style: Some(krausen_types::brewery::StyleReference {
            name: "Munich Helles".into(),
            edition: Some("BJCP 2021".into()),
            style_id: Some("4A".into()),
        }),
        batch_size_l: Some(20.0),
        boil_minutes: Some(60.0),
        equipment_profile_id: Some(profile_id.into()),
        fermentables: vec![krausen_types::brewery::FermentableAddition {
            row_id: "grain-1".into(),
            amount_kg: Some(4.2),
            addition: krausen_types::brewery::AdditionStage {
                stage: "mash".into(),
                time_minutes: None,
            },
            specification: krausen_types::brewery::FermentableSpecification {
                name: Some("Pilsner malt".into()),
                producer: Some("Weyermann".into()),
                kind: Some("grain".into()),
                yield_fraction: Some(0.8),
                colour_ebc: Some(5.0),
            },
            source: None,
        }],
        process: krausen_types::brewery::ProcessV1 {
            apparent_attenuation_fraction: Some(0.78),
            mash_water_l: Some(15.0),
            mash_water_method: Some("manual".into()),
            whirlpool_minutes: Some(15.0),
            kettle_top_up_l: Some(0.0),
            ..Default::default()
        },
        ..Default::default()
    }
}
