// SPDX-License-Identifier: AGPL-3.0-or-later
//! Authenticated MCP Streamable HTTP adapter; policy stays in krausen-app.
use crate::state::AppState;
use axum::{
    Router,
    body::Body,
    extract::{Request, State},
    http::{StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
};
use krausen_auth::oauth_service::{self, Principal};
use krausen_types::operations;
use rmcp::{
    ErrorData, RoleServer, ServerHandler,
    model::{
        CallToolRequestParams, CallToolResponse, CallToolResult, InitializeRequestParams,
        InitializeResult, ListToolsResult, PaginatedRequestParams, ProtocolVersion,
        ServerCapabilities, ServerConfig, Tool,
    },
    service::RequestContext,
    transport::streamable_http_server::{
        session::local::LocalSessionManager,
        tower::{StreamableHttpServerConfig, StreamableHttpService},
    },
};
use serde::Deserialize;
use serde_json::json;
use std::{borrow::Cow, sync::Arc};

#[derive(Clone)]
struct McpHandler {
    db: krausen_core::DbPool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkspaceQuery {
    workspace_id: String,
}
#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct FormulationPreview {
    workspace_id: String,
    request: krausen_types::formulation::PreviewRequest,
}
#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct StyleQuery {
    workspace_id: String,
    intent: krausen_types::request_context::OperationIntent,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Rename {
    workspace_id: String,
    name: String,
    expected_revision: i64,
    request_id: uuid::Uuid,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Recovery {
    workspace_id: String,
    recipe_id: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyAuthoring {
    workspace_id: String,
    beerxml: String,
}
#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct RecipeRead {
    workspace_id: String,
    intent: krausen_types::request_context::OperationIntent,
    recipe_id: String,
}
#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct RecipeSave {
    workspace_id: String,
    intent: krausen_types::request_context::OperationIntent,
    recipe_id: Option<String>,
    expected_revision: Option<i64>,
    document: krausen_types::brewery::RecipeDocumentV1,
    request_id: String,
}
fn typed_schema<T: schemars::JsonSchema>() -> serde_json::Map<String, serde_json::Value> {
    let value = serde_json::to_value(schemars::schema_for!(T)).expect("shared input schema");
    schema(value)
}
macro_rules! equipment_request {
    ($name:ident, $ty:ty) => {
        #[derive(Deserialize, schemars::JsonSchema)]
        #[serde(deny_unknown_fields)]
        struct $name {
            workspace_id: String,
            request: $ty,
        }
    };
}
equipment_request!(EquipmentSave, krausen_types::equipment::SaveEquipment);
equipment_request!(EquipmentArchive, krausen_types::equipment::ArchiveEquipment);
equipment_request!(EquipmentGet, krausen_types::equipment::EquipmentQuery);
equipment_request!(EquipmentList, krausen_types::equipment::EquipmentListQuery);
equipment_request!(
    EquipmentRevision,
    krausen_types::equipment::EquipmentRevisionQuery
);
fn schema(value: serde_json::Value) -> serde_json::Map<String, serde_json::Value> {
    value.as_object().cloned().expect("static schema")
}
impl ServerHandler for McpHandler {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
    }
    fn supported_protocol_versions(&self) -> Cow<'static, [ProtocolVersion]> {
        Cow::Owned(vec![
            ProtocolVersion::V_2025_11_25,
            ProtocolVersion::V_2026_07_28,
        ])
    }
    fn negotiate_initialize(
        &self,
        request: &InitializeRequestParams,
    ) -> Result<InitializeResult, ErrorData> {
        if request.protocol_version != ProtocolVersion::V_2025_11_25 {
            return Err(ErrorData::unsupported_protocol_version(
                request.protocol_version.clone(),
                &self.supported_protocol_versions(),
            ));
        }
        Ok(self
            .get_info()
            .with_protocol_version(ProtocolVersion::V_2025_11_25))
    }
    async fn list_tools(
        &self,
        _: Option<PaginatedRequestParams>,
        _: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        Ok(ListToolsResult::with_all_items(vec![
            Tool::new(
                operations::EQUIPMENT_SAVE.mcp_tool,
                "Save a guarded equipment profile",
                schema(
                    serde_json::to_value(schemars::schema_for!(EquipmentSave))
                        .expect("equipment schema"),
                ),
            ),
            Tool::new(
                operations::EQUIPMENT_ARCHIVE.mcp_tool,
                "Archive or restore an equipment profile",
                schema(
                    serde_json::to_value(schemars::schema_for!(EquipmentArchive))
                        .expect("equipment schema"),
                ),
            ),
            Tool::new(
                operations::EQUIPMENT_GET.mcp_tool,
                "Read an equipment profile",
                schema(
                    serde_json::to_value(schemars::schema_for!(EquipmentGet))
                        .expect("equipment schema"),
                ),
            ),
            Tool::new(
                operations::EQUIPMENT_LIST.mcp_tool,
                "List active equipment profiles",
                schema(
                    serde_json::to_value(schemars::schema_for!(EquipmentList))
                        .expect("equipment schema"),
                ),
            ),
            Tool::new(
                operations::EQUIPMENT_REVISION.mcp_tool,
                "Read an immutable equipment revision",
                schema(
                    serde_json::to_value(schemars::schema_for!(EquipmentRevision))
                        .expect("equipment schema"),
                ),
            ),
            Tool::new(
                operations::FORMULATION_PREVIEW.mcp_tool,
                "Calculate an unsaved recipe draft",
                schema(
                    serde_json::to_value(schemars::schema_for!(FormulationPreview))
                        .expect("preview schema"),
                ),
            ),
            Tool::new(
                operations::STYLE_LIST.mcp_tool,
                "List supported beer style identities",
                schema(
                    serde_json::to_value(schemars::schema_for!(StyleQuery)).expect("style schema"),
                ),
            ),
            Tool::new(
                operations::WORKSPACE_GET.mcp_tool,
                "Read workspace settings",
                schema(
                    json!({"type":"object","properties":{"workspace_id":{"type":"string"}},"required":["workspace_id"],"additionalProperties":false}),
                ),
            ),
            Tool::new(
                operations::WORKSPACE_RENAME.mcp_tool,
                "Rename a workspace using its observed revision and a stable request UUID",
                schema(
                    json!({"type":"object","properties":{"workspace_id":{"type":"string"},"name":{"type":"string"},"expected_revision":{"type":"integer"},"request_id":{"type":"string","format":"uuid"}},"required":["workspace_id","name","expected_revision","request_id"],"additionalProperties":false}),
                ),
            ),
            Tool::new(
                operations::RECIPE_RECOVER.mcp_tool,
                "Recover the original source and diagnostic for a failed native recipe migration",
                schema(
                    json!({"type":"object","properties":{"workspace_id":{"type":"string"},"recipe_id":{"type":"string"}},"required":["workspace_id","recipe_id"],"additionalProperties":false}),
                ),
            ),
            Tool::new(
                operations::RECIPE_CREATE.mcp_tool,
                "Create a recipe from BeerXML; currently unavailable during native migration",
                schema(
                    json!({"type":"object","properties":{"workspace_id":{"type":"string"},"beerxml":{"type":"string"}},"required":["workspace_id","beerxml"],"additionalProperties":false}),
                ),
            ),
            Tool::new(
                operations::RECIPE_IMPORT.mcp_tool,
                "Import BeerXML recipes; currently unavailable during native migration",
                schema(
                    json!({"type":"object","properties":{"workspace_id":{"type":"string"},"beerxml":{"type":"string"}},"required":["workspace_id","beerxml"],"additionalProperties":false}),
                ),
            ),
            Tool::new(
                operations::RECIPE_READ.mcp_tool,
                "Read the current native recipe revision in a workspace",
                typed_schema::<RecipeRead>(),
            ),
            Tool::new(
                operations::RECIPE_SAVE.mcp_tool,
                "Create or explicitly save a native recipe using its observed revision and a stable request UUID",
                typed_schema::<RecipeSave>(),
            ),
        ]))
    }
    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let principal = context
            .extensions
            .get::<axum::http::request::Parts>()
            .and_then(|p| p.extensions.get::<Principal>())
            .ok_or_else(|| ErrorData::invalid_request("Authenticated principal required", None))?;
        let input = serde_json::Value::Object(request.arguments.unwrap_or_default());
        let operation = operations::by_tool(request.name.as_ref())
            .ok_or_else(|| ErrorData::invalid_params("Unknown tool", None))?;
        let result = match operation.id {
            "equipment.save.v1"
            | "equipment.archive.v1"
            | "equipment.get.v1"
            | "equipment.list.v1"
            | "equipment.revision.v1" => {
                macro_rules! parse {
                    ($ty:ty) => {
                        serde_json::from_value::<$ty>(input).map_err(|_| {
                            ErrorData::invalid_params("Invalid equipment request", None)
                        })?
                    };
                }
                macro_rules! actor {
                    ($workspace:expr) => {
                        krausen_app::ActorContext {
                            actor_id: principal.user_id.clone(),
                            credential_id: principal.credential_id.clone(),
                            access_expires_at: principal.access_expires_at,
                            workspace_id: $workspace,
                        }
                    };
                }
                let outcome = match operation.id {
                    "equipment.save.v1" => {
                        let c = parse!(EquipmentSave);
                        let actor = actor!(c.workspace_id);
                        krausen_app::equipment::project(
                            krausen_app::equipment::save(&self.db, &actor, c.request).await,
                        )
                    }
                    "equipment.archive.v1" => {
                        let c = parse!(EquipmentArchive);
                        let actor = actor!(c.workspace_id);
                        krausen_app::equipment::project(
                            krausen_app::equipment::set_archived(&self.db, &actor, c.request).await,
                        )
                    }
                    "equipment.get.v1" => {
                        let c = parse!(EquipmentGet);
                        let actor = actor!(c.workspace_id);
                        krausen_app::equipment::project_get(
                            krausen_app::equipment::get(
                                &self.db,
                                &actor,
                                &c.request.intent,
                                &c.request.profile_id,
                            )
                            .await,
                        )
                    }
                    "equipment.list.v1" => {
                        let c = parse!(EquipmentList);
                        let actor = actor!(c.workspace_id);
                        krausen_app::equipment::project_list(
                            krausen_app::equipment::list(
                                &self.db,
                                &actor,
                                &c.request.intent,
                                &c.request.after,
                                c.request.limit,
                                c.request.include_archived,
                            )
                            .await,
                        )
                    }
                    "equipment.revision.v1" => {
                        let c = parse!(EquipmentRevision);
                        let actor = actor!(c.workspace_id);
                        krausen_app::equipment::project_revision(
                            krausen_app::equipment::get_revision(
                                &self.db,
                                &actor,
                                &c.request.intent,
                                &c.request.profile_id,
                                c.request.revision,
                            )
                            .await,
                        )
                    }
                    _ => unreachable!(),
                };
                let is_error = !matches!(
                    outcome,
                    krausen_types::equipment::EquipmentOutcome::Profile(_)
                        | krausen_types::equipment::EquipmentOutcome::Profiles(_)
                        | krausen_types::equipment::EquipmentOutcome::Revision(_)
                        | krausen_types::equipment::EquipmentOutcome::NotFound
                );
                let value = json!({"outcome":outcome});
                if is_error {
                    CallToolResult::structured_error(value)
                } else {
                    CallToolResult::structured(value)
                }
            }
            "formulation.preview.v1" => {
                let input: FormulationPreview = serde_json::from_value(input)
                    .map_err(|_| ErrorData::invalid_params("Invalid formulation preview", None))?;
                let actor = krausen_app::ActorContext {
                    actor_id: principal.user_id.clone(),
                    credential_id: principal.credential_id.clone(),
                    access_expires_at: principal.access_expires_at,
                    workspace_id: input.workspace_id,
                };
                let outcome = krausen_app::formulation::project_preview(
                    krausen_app::formulation::preview(&self.db, &actor, input.request).await,
                );
                let is_error = matches!(
                    outcome,
                    krausen_types::formulation::PreviewOutcome::Forbidden
                        | krausen_types::formulation::PreviewOutcome::ContextChanged
                        | krausen_types::formulation::PreviewOutcome::Invalid
                        | krausen_types::formulation::PreviewOutcome::Unavailable
                );
                let value = json!({"outcome":outcome});
                if is_error {
                    CallToolResult::structured_error(value)
                } else {
                    CallToolResult::structured(value)
                }
            }
            "formulation.styles.v1" => {
                let input: StyleQuery = serde_json::from_value(input)
                    .map_err(|_| ErrorData::invalid_params("Invalid style query", None))?;
                let actor = krausen_app::ActorContext {
                    actor_id: principal.user_id.clone(),
                    credential_id: principal.credential_id.clone(),
                    access_expires_at: principal.access_expires_at,
                    workspace_id: input.workspace_id,
                };
                let outcome = krausen_app::formulation::project_styles(
                    krausen_app::formulation::list_styles(&self.db, &actor, &input.intent).await,
                );
                let is_error = !matches!(
                    outcome,
                    krausen_types::formulation::StyleListOutcome::Styles(_)
                );
                let value = json!({"outcome":outcome});
                if is_error {
                    CallToolResult::structured_error(value)
                } else {
                    CallToolResult::structured(value)
                }
            }
            "workspace.get.v1" => {
                let input: WorkspaceQuery = serde_json::from_value(input)
                    .map_err(|_| ErrorData::invalid_params("Invalid workspace query", None))?;
                let actor = krausen_app::ActorContext {
                    actor_id: principal.user_id.clone(),
                    credential_id: principal.credential_id.clone(),
                    access_expires_at: principal.access_expires_at,
                    workspace_id: input.workspace_id,
                };
                match krausen_app::workspace_snapshot(&self.db, &actor).await {
                    Ok(snapshot) => CallToolResult::structured(
                        json!({"workspace_name":snapshot.name.unwrap_or_default(),"revision":snapshot.revision}),
                    ),
                    Err(_) => CallToolResult::structured_error(
                        json!({"error":"unavailable_or_forbidden"}),
                    ),
                }
            }
            "workspace.rename.v1" => {
                let input: Rename = serde_json::from_value(input)
                    .map_err(|_| ErrorData::invalid_params("Invalid workspace rename", None))?;
                let actor = krausen_app::ActorContext {
                    actor_id: principal.user_id.clone(),
                    credential_id: principal.credential_id.clone(),
                    access_expires_at: principal.access_expires_at,
                    workspace_id: input.workspace_id,
                };
                let outcome = krausen_app::project_rename_result(
                    krausen_app::rename_workspace(
                        &self.db,
                        &actor,
                        &input.name,
                        input.expected_revision,
                        input.request_id,
                    )
                    .await,
                );
                let success = matches!(
                    outcome,
                    krausen_types::workspace::WorkspaceRenameOutcome::Saved { .. }
                );
                let value = serde_json::to_value(outcome)
                    .map_err(|_| ErrorData::internal_error("Result unavailable", None))?;
                if success {
                    CallToolResult::structured(value)
                } else {
                    CallToolResult::structured_error(value)
                }
            }
            "recipe.recover-legacy.v1" => {
                let input: Recovery = serde_json::from_value(input)
                    .map_err(|_| ErrorData::invalid_params("Invalid recovery query", None))?;
                let actor = krausen_app::ActorContext {
                    actor_id: principal.user_id.clone(),
                    credential_id: principal.credential_id.clone(),
                    access_expires_at: principal.access_expires_at,
                    workspace_id: input.workspace_id,
                };
                match krausen_app::recipe_migration::recover_legacy_recipe(
                    &self.db,
                    &actor,
                    &input.recipe_id,
                )
                .await
                {
                    Ok(Some((source, diagnostic))) => {
                        CallToolResult::structured(json!({"source":source,"diagnostic":diagnostic}))
                    }
                    Ok(None) => CallToolResult::structured(json!({"source":null})),
                    Err(_) => CallToolResult::structured_error(
                        json!({"error":"unavailable_or_forbidden"}),
                    ),
                }
            }
            "recipe.create-legacy.v1" | "recipe.import-legacy.v1" => {
                let input: LegacyAuthoring = serde_json::from_value(input)
                    .map_err(|_| ErrorData::invalid_params("Invalid recipe input", None))?;
                let _ = input.beerxml;
                let actor = krausen_app::ActorContext {
                    actor_id: principal.user_id.clone(),
                    credential_id: principal.credential_id.clone(),
                    access_expires_at: principal.access_expires_at,
                    workspace_id: input.workspace_id,
                };
                match krausen_app::recipe_migration::reject_legacy_authoring(&self.db, &actor).await
                {
                    Err(krausen_app::Error::LegacyAuthoringUnavailable) => {
                        CallToolResult::structured_error(
                            json!({"error":"legacy_authoring_unavailable"}),
                        )
                    }
                    _ => CallToolResult::structured_error(
                        json!({"error":"unavailable_or_forbidden"}),
                    ),
                }
            }
            "recipe.read.v1" => {
                let input: RecipeRead = serde_json::from_value(input)
                    .map_err(|_| ErrorData::invalid_params("Invalid recipe query", None))?;
                let actor = krausen_app::ActorContext {
                    actor_id: principal.user_id.clone(),
                    credential_id: principal.credential_id.clone(),
                    access_expires_at: principal.access_expires_at,
                    workspace_id: input.workspace_id,
                };
                let outcome = krausen_app::recipe_editor::read(
                    &self.db,
                    &actor,
                    &input.intent,
                    &input.recipe_id,
                )
                .await;
                let success = matches!(
                    outcome,
                    krausen_types::recipe_editor::RecipeReadOutcome::Found(_)
                );
                let value = json!({"outcome": outcome});
                if success {
                    CallToolResult::structured(value)
                } else {
                    CallToolResult::structured_error(value)
                }
            }
            "recipe.save.v1" => {
                let input: RecipeSave = serde_json::from_value(input)
                    .map_err(|_| ErrorData::invalid_params("Invalid recipe save", None))?;
                let Ok(request_id) = uuid::Uuid::parse_str(&input.request_id) else {
                    return Ok(CallToolResult::structured_error(json!({"outcome": krausen_types::recipe_editor::RecipeSaveOutcome::InvalidRequest})).into());
                };
                let actor = krausen_app::ActorContext {
                    actor_id: principal.user_id.clone(),
                    credential_id: principal.credential_id.clone(),
                    access_expires_at: principal.access_expires_at,
                    workspace_id: input.workspace_id,
                };
                let outcome = krausen_app::recipe_editor::save(
                    &self.db,
                    &actor,
                    &input.intent,
                    input.recipe_id.as_deref(),
                    input.expected_revision,
                    &input.document,
                    request_id,
                )
                .await;
                let success = matches!(
                    outcome,
                    krausen_types::recipe_editor::RecipeSaveOutcome::Saved { .. }
                );
                let value = json!({"outcome": outcome});
                if success {
                    CallToolResult::structured(value)
                } else {
                    CallToolResult::structured_error(value)
                }
            }
            _ => return Err(ErrorData::invalid_params("Unknown tool", None)),
        };
        Ok(result.into())
    }
}

pub fn routes(state: AppState) -> Router<AppState> {
    let db = state.db.clone();
    let base = url::Url::parse(&state.config.base_url).expect("valid BASE_URL for MCP");
    let front = url::Url::parse(&state.config.frontend_url).expect("valid FRONTEND_URL for MCP");
    let host = base.host_str().expect("BASE_URL host");
    let mut config = StreamableHttpServerConfig::default();
    config.legacy_session_mode = true;
    config.allowed_hosts = vec![match base.port() {
        Some(port) => format!("{host}:{port}"),
        None => host.to_owned(),
    }];
    config.allowed_origins = vec![front.origin().ascii_serialization()];
    config = config.enforce_origin_validation();
    let service = StreamableHttpService::new(
        move || Ok(McpHandler { db: db.clone() }),
        Arc::new(LocalSessionManager::default()),
        config,
    );
    Router::new()
        .route_service("/", service)
        .layer(middleware::from_fn_with_state(state, authenticate))
}
async fn authenticate(
    State(state): State<AppState>,
    mut request: Request<Body>,
    next: Next,
) -> Response {
    let base = state.config.base_url.trim_end_matches('/');
    if !super::oauth::secure_external_origin(base) {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    let Ok(base_url) = url::Url::parse(base) else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    let Some(host) = request
        .headers()
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
    else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let actual = url::Url::parse(&format!("{}://{host}/", base_url.scheme()));
    if actual.as_ref().map(|u| u.origin()).ok() != Some(base_url.origin())
        || actual.as_ref().is_ok_and(|u| {
            !u.username().is_empty()
                || u.password().is_some()
                || u.path() != "/"
                || u.query().is_some()
                || u.fragment().is_some()
        })
    {
        return StatusCode::FORBIDDEN.into_response();
    }
    if let Some(origin) = request.headers().get(header::ORIGIN) {
        let permitted = origin
            .to_str()
            .ok()
            .and_then(|v| url::Url::parse(v).ok())
            .zip(url::Url::parse(&state.config.frontend_url).ok())
            .is_some_and(|(a, b)| a.origin() == b.origin());
        if !permitted {
            return StatusCode::FORBIDDEN.into_response();
        }
    }
    let audience = format!("{base}/mcp");
    let token = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|v| v.split_once(' '))
        .and_then(|(scheme, value)| scheme.eq_ignore_ascii_case("bearer").then_some(value));
    let principal = match token {
        Some(token) => oauth_service::verify_access(&state.db, base, &audience, token)
            .await
            .ok(),
        None => None,
    };
    let Some(principal) = principal else {
        let challenge =
            format!("Bearer resource_metadata=\"{base}/.well-known/oauth-protected-resource/mcp\"");
        let Ok(challenge) = header::HeaderValue::from_str(&challenge) else {
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        };
        return (
            StatusCode::UNAUTHORIZED,
            [(header::WWW_AUTHENTICATE, challenge)],
        )
            .into_response();
    };
    request.extensions_mut().insert(principal);
    next.run(request).await
}
