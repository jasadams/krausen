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
    let recipe_tool = discovered
        .tools
        .iter()
        .find(|tool| tool.name == "recipe_save")
        .unwrap();
    assert!(recipe_tool.input_schema.get("$defs").is_some());
    assert!(
        recipe_tool.input_schema["properties"]["document"]["properties"]
            .get("fermentables")
            .is_some()
    );
    // Both protocol versions and the browser-facing app use the same native
    // operation and revision contract, including request replay and authority.
    let mut native = krausen_types::brewery::RecipeDocumentV1::default();
    native.title = "MCP recipe".into();
    let invalid_args = json!({"workspace_id": ws, "document": native, "request_id": "invalid"})
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
    let denied_args = json!({"workspace_id": "not-a-member", "document": native, "request_id": uuid::Uuid::new_v4()}).as_object().unwrap().clone();
    let denied = modern
        .call_tool(CallToolRequestParams::new("recipe_save").with_arguments(denied_args))
        .await
        .unwrap();
    assert_eq!(denied.is_error, Some(true));
    assert_eq!(denied.structured_content.unwrap()["outcome"], "Forbidden");
    let save_key = uuid::Uuid::new_v4();
    let save_args = json!({"workspace_id": ws, "document": native, "request_id": save_key})
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
    let snapshot = krausen_app::recipe_editor::read(&db, &actor_for_recipe, &native_id).await;
    assert!(
        matches!(snapshot, krausen_types::recipe_editor::RecipeReadOutcome::Found(ref snapshot) if snapshot.document == native && snapshot.revision == 1)
    );
    native.title = "Browser edit".into();
    let saved = krausen_app::recipe_editor::save(
        &db,
        &actor_for_recipe,
        Some(&native_id),
        Some(1),
        &native,
        uuid::Uuid::new_v4(),
    )
    .await;
    assert!(matches!(
        saved,
        krausen_types::recipe_editor::RecipeSaveOutcome::Saved { revision: 2, .. }
    ));
    let read_args = json!({"workspace_id":ws,"recipe_id":native_id})
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
    let stale_args = json!({"workspace_id":ws,"recipe_id":native_id,"expected_revision":1,"document":native,"request_id":uuid::Uuid::new_v4()}).as_object().unwrap().clone();
    let stale = modern
        .call_tool(CallToolRequestParams::new("recipe_save").with_arguments(stale_args))
        .await
        .unwrap();
    assert_eq!(stale.is_error, Some(true));
    assert_eq!(
        stale.structured_content.unwrap()["outcome"]["Conflict"]["actual"],
        2
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
