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
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InvitationReply {
    invitation_id: String,
    action: krausen_types::invitation::InvitationAction,
    request_id: uuid::Uuid,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AccountQuery {}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AccountNameSet { name: String, expected_settings_revision: i64 }
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AccountThemeSet { theme: String, expected_settings_revision: i64 }
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AccountWorkspacePreferenceSet { workspace_id: String, expected_settings_revision: i64 }
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AccountWorkspacesQuery {
    cursor: Option<String>,
    limit: Option<i64>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TeamMembersQuery {
    workspace_id: String,
    cursor: Option<String>,
    limit: Option<i64>,
}

async fn execute_team_members(db: &krausen_core::DbPool, principal: &Principal, input: TeamMembersQuery) -> krausen_types::team_read::MemberListOutcome {
    let actor = krausen_app::ActorContext { actor_id: principal.user_id.clone(), credential_id: principal.credential_id.clone(), access_expires_at: principal.access_expires_at, workspace_id: input.workspace_id };
    krausen_app::team_read::project_members(krausen_app::team_read::members(db, &actor, input.cursor.as_deref(), input.limit.unwrap_or(50)).await)
}
fn account_actor(principal: &Principal) -> krausen_app::invitation::AccountActor {
    krausen_app::invitation::AccountActor {
        actor_id: principal.user_id.clone(),
        credential_id: principal.credential_id.clone(),
        access_expires_at: principal.access_expires_at,
    }
}

async fn execute_account_name(db: &krausen_core::DbPool, principal: &Principal, input: AccountNameSet) -> krausen_types::account_read::AccountUpdateOutcome {
    krausen_app::account_update::project(krausen_app::account_update::name(db, &account_actor(principal), input.expected_settings_revision, &input.name).await)
}
async fn execute_account_theme(db: &krausen_core::DbPool, principal: &Principal, input: AccountThemeSet) -> krausen_types::account_read::AccountUpdateOutcome {
    krausen_app::account_update::project(krausen_app::account_update::theme(db, &account_actor(principal), input.expected_settings_revision, &input.theme).await)
}
async fn execute_workspace_preference(db: &krausen_core::DbPool, principal: &Principal, input: AccountWorkspacePreferenceSet) -> krausen_types::account_read::AccountUpdateOutcome {
    let actor = krausen_app::ActorContext { actor_id: principal.user_id.clone(), credential_id: principal.credential_id.clone(), access_expires_at: principal.access_expires_at, workspace_id: input.workspace_id };
    krausen_app::account_update::project(krausen_app::account_update::workspace(db, &actor, input.expected_settings_revision).await)
}
async fn execute_sessions(db: &krausen_core::DbPool, principal: &Principal, input: AccountWorkspacesQuery) -> krausen_types::session_read::SessionListOutcome {
    krausen_app::session_read::project(krausen_app::session_read::sessions(db, &account_actor(principal), input.cursor.as_deref(), input.limit.unwrap_or(50)).await)
}

async fn execute_account_profile(
    db: &krausen_core::DbPool,
    principal: &Principal,
) -> krausen_types::account_read::AccountProfileOutcome {
    krausen_app::account_read::project_profile(krausen_app::account_read::profile(db, &account_actor(principal)).await)
}
async fn execute_account_workspaces(
    db: &krausen_core::DbPool,
    principal: &Principal,
    input: AccountWorkspacesQuery,
) -> krausen_types::account_read::AccountWorkspaceListOutcome {
    krausen_app::account_read::project_workspaces(
        krausen_app::account_read::workspaces(db, &account_actor(principal), input.cursor.as_deref(), input.limit.unwrap_or(50)).await,
    )
}
async fn execute_invitation_reply(
    db: &krausen_core::DbPool,
    principal: &Principal,
    input: InvitationReply,
) -> krausen_types::invitation::InvitationResponseOutcome {
    let actor = krausen_app::invitation::AccountActor {
        actor_id: principal.user_id.clone(),
        credential_id: principal.credential_id.clone(),
        access_expires_at: principal.access_expires_at,
    };
    krausen_app::invitation::project(
        krausen_app::invitation::respond(db, &actor, &input.invitation_id, input.action, input.request_id).await,
    )
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
#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct ProcessSummaryQuery {
    workspace_id: String,
    request: krausen_types::process_summary::ProcessSummaryRequest,
}
#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct RecipeScalePreview {
    workspace_id: String,
    intent: krausen_types::request_context::OperationIntent,
    recipe_id: String,
    target_batch_size_l: f64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RecipeScaleApply {
    workspace_id: String,
    request: krausen_types::scaling::ScaleApplyRequest,
}
#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct RecipeDuplicate {
    workspace_id: String,
    request: krausen_types::scaling::DuplicateRecipe,
}
#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct BeerXmlPreview {
    workspace_id: String,
    request: krausen_types::interchange::ImportRequest,
}
#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct BeerXmlImport {
    workspace_id: String,
    request: krausen_types::interchange::ImportRequest,
    request_id: String,
}
#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct BeerXmlExport {
    workspace_id: String,
    request: krausen_types::interchange::ExportRequest,
}
#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct BeerXmlOriginal {
    workspace_id: String,
    intent: krausen_types::request_context::OperationIntent,
    recipe_id: String,
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
equipment_request!(RecipeList, krausen_types::recipe_library::LibraryQuery);
equipment_request!(RecipeHistory, krausen_types::recipe_library::HistoryQuery);
equipment_request!(RecipeRevision, krausen_types::recipe_library::RevisionQuery);
equipment_request!(RecipeArchive, krausen_types::recipe_library::ArchiveRecipe);
equipment_request!(
    IngredientSave,
    krausen_types::catalogue::SaveIngredientEntry
);
equipment_request!(
    IngredientArchive,
    krausen_types::catalogue::ArchiveIngredientEntry
);
equipment_request!(IngredientGet, krausen_types::catalogue::IngredientQuery);
equipment_request!(IngredientSearch, krausen_types::catalogue::SearchCatalogue);
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
                operations::BEERXML_PREVIEW.mcp_tool,
                "Preview a BeerXML import",
                typed_schema::<BeerXmlPreview>(),
            ),
            Tool::new(
                operations::BEERXML_IMPORT.mcp_tool,
                "Commit a BeerXML import",
                typed_schema::<BeerXmlImport>(),
            ),
            Tool::new(
                operations::BEERXML_EXPORT.mcp_tool,
                "Export current native data as BeerXML",
                typed_schema::<BeerXmlExport>(),
            ),
            Tool::new(
                operations::BEERXML_ORIGINAL.mcp_tool,
                "Retrieve the unedited imported source",
                typed_schema::<BeerXmlOriginal>(),
            ),
            Tool::new(
                operations::RECIPE_LIST.mcp_tool,
                "Search and list recipes",
                typed_schema::<RecipeList>(),
            ),
            Tool::new(
                operations::RECIPE_HISTORY.mcp_tool,
                "List retained recipe revisions",
                typed_schema::<RecipeHistory>(),
            ),
            Tool::new(
                operations::RECIPE_REVISION.mcp_tool,
                "Read a retained recipe revision",
                typed_schema::<RecipeRevision>(),
            ),
            Tool::new(
                operations::RECIPE_ARCHIVE.mcp_tool,
                "Archive or restore a recipe",
                typed_schema::<RecipeArchive>(),
            ),
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
                operations::INGREDIENT_SAVE.mcp_tool,
                "Create or edit a workspace custom ingredient entry using observed revisions and a stable request UUID",
                schema(
                    serde_json::to_value(schemars::schema_for!(IngredientSave))
                        .expect("ingredient schema"),
                ),
            ),
            Tool::new(
                operations::INGREDIENT_ARCHIVE.mcp_tool,
                "Archive or restore a workspace custom ingredient entry",
                schema(
                    serde_json::to_value(schemars::schema_for!(IngredientArchive))
                        .expect("ingredient schema"),
                ),
            ),
            Tool::new(
                operations::INGREDIENT_GET.mcp_tool,
                "Read one ingredient entry, including public starter entries",
                schema(
                    serde_json::to_value(schemars::schema_for!(IngredientGet))
                        .expect("ingredient schema"),
                ),
            ),
            Tool::new(
                operations::INGREDIENT_SEARCH.mcp_tool,
                "Search the ingredient catalogue with bounded cursor pagination",
                schema(
                    serde_json::to_value(schemars::schema_for!(IngredientSearch))
                        .expect("ingredient schema"),
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
                operations::INVITATION_RESPOND.mcp_tool,
                "Accept or decline an invitation addressed to the authenticated account",
                schema(json!({"type":"object","properties":{"invitation_id":{"type":"string"},"action":{"type":"string","enum":["accept","decline"]},"request_id":{"type":"string","format":"uuid"}},"required":["invitation_id","action","request_id"],"additionalProperties":false})),
            ),
            Tool::new(
                operations::PASSWORD_STATUS.mcp_tool,
                "Read whether the authenticated account has a password",
                schema(json!({"type":"object","properties":{},"additionalProperties":false})),
            ),
            Tool::new(
                operations::TOTP_STATUS.mcp_tool,
                "Read whether the authenticated account has TOTP enabled",
                schema(json!({"type":"object","properties":{},"additionalProperties":false})),
            ),
            Tool::new(
                operations::ACCOUNT_PROFILE.mcp_tool,
                "Read the authenticated account profile and appearance preference",
                schema(json!({"type":"object","properties":{},"additionalProperties":false})),
            ),
            Tool::new(operations::ACCOUNT_NAME_SET.mcp_tool, "Update the authenticated account name using the observed settings revision", schema(json!({"type":"object","properties":{"name":{"type":"string"},"expected_settings_revision":{"type":"integer","minimum":0}},"required":["name","expected_settings_revision"],"additionalProperties":false}))),
            Tool::new(operations::ACCOUNT_THEME_SET.mcp_tool, "Update the authenticated account theme using the observed settings revision", schema(json!({"type":"object","properties":{"theme":{"type":"string","enum":["light","dark","system"]},"expected_settings_revision":{"type":"integer","minimum":0}},"required":["theme","expected_settings_revision"],"additionalProperties":false}))),
            Tool::new(operations::ACCOUNT_WORKSPACE_PREFERENCE_SET.mcp_tool, "Remember an authorized browser workspace preference; MCP calls still require an explicit workspace", schema(json!({"type":"object","properties":{"workspace_id":{"type":"string"},"expected_settings_revision":{"type":"integer","minimum":0}},"required":["workspace_id","expected_settings_revision"],"additionalProperties":false}))),
            Tool::new(operations::SECURITY_SESSIONS.mcp_tool, "List a page of the authenticated account's active browser sessions", schema(json!({"type":"object","properties":{"cursor":{"type":"string"},"limit":{"type":"integer","minimum":1,"maximum":100}},"additionalProperties":false}))),
            Tool::new(
                operations::ACCOUNT_WORKSPACES.mcp_tool,
                "List a page of workspaces for the authenticated account",
                schema(json!({"type":"object","properties":{"cursor":{"type":"string"},"limit":{"type":"integer","minimum":1,"maximum":100}},"additionalProperties":false})),
            ),
            Tool::new(
                operations::TEAM_MEMBERS.mcp_tool,
                "List a page of active members in an authorized workspace",
                schema(json!({"type":"object","properties":{"workspace_id":{"type":"string"},"cursor":{"type":"string"},"limit":{"type":"integer","minimum":1,"maximum":100}},"required":["workspace_id"],"additionalProperties":false})),
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
            Tool::new(
                operations::RECIPE_PROCESS_SUMMARY.mcp_tool,
                "Return the authorized structured process summary for a saved revision or an unsaved draft",
                typed_schema::<ProcessSummaryQuery>(),
            ),
            Tool::new(
                operations::RECIPE_SCALE_PREVIEW.mcp_tool,
                "Preview linear-ingredients-v1 batch scaling with changed amounts, water, targets and warnings",
                typed_schema::<RecipeScalePreview>(),
            ),
            Tool::new(
                operations::RECIPE_SCALE_APPLY.mcp_tool,
                "Apply a scaling preview guarded by the expected head and accepted calculation context",
                schema(
                    json!({"type":"object","properties":{"workspace_id":{"type":"string"},"request":{"type":"object"}},"required":["workspace_id","request"],"additionalProperties":false}),
                ),
            ),
            Tool::new(
                operations::RECIPE_DUPLICATE.mcp_tool,
                "Duplicate the current saved recipe under a new independent identity",
                typed_schema::<RecipeDuplicate>(),
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
            "security.sessions.v1" => {
                let input: AccountWorkspacesQuery = serde_json::from_value(input).map_err(|_| ErrorData::invalid_params("Invalid session query", None))?;
                let outcome = execute_sessions(&self.db, principal, input).await;
                if matches!(outcome, krausen_types::session_read::SessionListOutcome::Available { .. }) { CallToolResult::structured(json!({"outcome":outcome})) } else { CallToolResult::structured_error(json!({"outcome":outcome})) }
            }
            "account.workspace-preference.v1" => {
                let input: AccountWorkspacePreferenceSet = serde_json::from_value(input).map_err(|_| ErrorData::invalid_params("Invalid workspace preference input", None))?;
                let outcome = execute_workspace_preference(&self.db, principal, input).await;
                if matches!(outcome, krausen_types::account_read::AccountUpdateOutcome::Updated { .. }) { CallToolResult::structured(json!({"outcome":outcome})) } else { CallToolResult::structured_error(json!({"outcome":outcome})) }
            }
            "account.name.v1" => {
                let input: AccountNameSet = serde_json::from_value(input).map_err(|_| ErrorData::invalid_params("Invalid account name input", None))?;
                let outcome = execute_account_name(&self.db, principal, input).await;
                if matches!(outcome, krausen_types::account_read::AccountUpdateOutcome::Updated { .. }) { CallToolResult::structured(json!({"outcome":outcome})) } else { CallToolResult::structured_error(json!({"outcome":outcome})) }
            }
            "account.theme.v1" => {
                let input: AccountThemeSet = serde_json::from_value(input).map_err(|_| ErrorData::invalid_params("Invalid account theme input", None))?;
                let outcome = execute_account_theme(&self.db, principal, input).await;
                if matches!(outcome, krausen_types::account_read::AccountUpdateOutcome::Updated { .. }) { CallToolResult::structured(json!({"outcome":outcome})) } else { CallToolResult::structured_error(json!({"outcome":outcome})) }
            }
            "team.members.v1" => {
                let input: TeamMembersQuery = serde_json::from_value(input)
                    .map_err(|_| ErrorData::invalid_params("Invalid team member query", None))?;
                let outcome = execute_team_members(&self.db, principal, input).await;
                if matches!(outcome, krausen_types::team_read::MemberListOutcome::Available { .. }) {
                    CallToolResult::structured(json!({"outcome":outcome}))
                } else {
                    CallToolResult::structured_error(json!({"outcome":outcome}))
                }
            }
            "account.workspaces.v1" => {
                let input: AccountWorkspacesQuery = serde_json::from_value(input)
                    .map_err(|_| ErrorData::invalid_params("Invalid account workspace query", None))?;
                let outcome = execute_account_workspaces(&self.db, principal, input).await;
                if matches!(outcome, krausen_types::account_read::AccountWorkspaceListOutcome::Available { .. }) {
                    CallToolResult::structured(json!({"outcome":outcome}))
                } else {
                    CallToolResult::structured_error(json!({"outcome":outcome}))
                }
            }
            "account.profile.v1" => {
                let _: AccountQuery = serde_json::from_value(input)
                    .map_err(|_| ErrorData::invalid_params("Invalid account query", None))?;
                let outcome = execute_account_profile(&self.db, principal).await;
                if matches!(outcome, krausen_types::account_read::AccountProfileOutcome::Available { .. }) {
                    CallToolResult::structured(json!({"outcome":outcome}))
                } else {
                    CallToolResult::structured_error(json!({"outcome":outcome}))
                }
            }
            "security.totp-status.v1" => {
                let _: AccountQuery = serde_json::from_value(input)
                    .map_err(|_| ErrorData::invalid_params("Invalid account query", None))?;
                let actor = account_actor(principal);
                let outcome = krausen_app::security_status::project_totp_status(
                    krausen_app::security_status::totp_status(&self.db, &actor).await,
                );
                if matches!(outcome, krausen_types::security_status::TotpStatusOutcome::Available { .. }) {
                    CallToolResult::structured(json!({"outcome":outcome}))
                } else {
                    CallToolResult::structured_error(json!({"outcome":outcome}))
                }
            }
            "security.password-status.v1" => {
                let _: AccountQuery = serde_json::from_value(input)
                    .map_err(|_| ErrorData::invalid_params("Invalid account query", None))?;
                let actor = account_actor(principal);
                let outcome = krausen_app::security_status::project_password_status(
                    krausen_app::security_status::password_status(&self.db, &actor).await,
                );
                if matches!(outcome, krausen_types::security_status::PasswordStatusOutcome::Available { .. }) {
                    CallToolResult::structured(json!({"outcome":outcome}))
                } else {
                    CallToolResult::structured_error(json!({"outcome":outcome}))
                }
            }
            "invitation.respond.v1" => {
                let input: InvitationReply = serde_json::from_value(input)
                    .map_err(|_| ErrorData::invalid_params("Invalid invitation response", None))?;
                let outcome = execute_invitation_reply(&self.db, principal, input).await;
                if matches!(outcome, krausen_types::invitation::InvitationResponseOutcome::Accepted | krausen_types::invitation::InvitationResponseOutcome::Declined) {
                    CallToolResult::structured(json!({"outcome":outcome}))
                } else {
                    CallToolResult::structured_error(json!({"outcome":outcome}))
                }
            }
            "beerxml.preview.v1" => {
                let input: BeerXmlPreview = serde_json::from_value(input)
                    .map_err(|_| ErrorData::invalid_params("Invalid BeerXML preview", None))?;
                let actor = krausen_app::ActorContext {
                    actor_id: principal.user_id.clone(),
                    credential_id: principal.credential_id.clone(),
                    access_expires_at: principal.access_expires_at,
                    workspace_id: input.workspace_id,
                };
                let outcome =
                    krausen_app::interchange::preview(&self.db, &actor, &input.request).await;
                let success = matches!(
                    outcome,
                    krausen_types::interchange::ImportPreviewOutcome::Preview(_)
                );
                if success {
                    CallToolResult::structured(json!({"outcome":outcome}))
                } else {
                    CallToolResult::structured_error(json!({"outcome":outcome}))
                }
            }
            "beerxml.import.v1" => {
                let input: BeerXmlImport = serde_json::from_value(input)
                    .map_err(|_| ErrorData::invalid_params("Invalid BeerXML import", None))?;
                let Ok(id) = uuid::Uuid::parse_str(&input.request_id) else {
                    return Ok(CallToolResult::structured_error(json!({"outcome": krausen_types::interchange::ImportCommitOutcome::InvalidRequest})).into());
                };
                let actor = krausen_app::ActorContext {
                    actor_id: principal.user_id.clone(),
                    credential_id: principal.credential_id.clone(),
                    access_expires_at: principal.access_expires_at,
                    workspace_id: input.workspace_id,
                };
                let outcome =
                    krausen_app::interchange::commit(&self.db, &actor, &input.request, id).await;
                let success = matches!(
                    outcome,
                    krausen_types::interchange::ImportCommitOutcome::Imported { .. }
                );
                if success {
                    CallToolResult::structured(json!({"outcome":outcome}))
                } else {
                    CallToolResult::structured_error(json!({"outcome":outcome}))
                }
            }
            "beerxml.export.v1" => {
                let input: BeerXmlExport = serde_json::from_value(input)
                    .map_err(|_| ErrorData::invalid_params("Invalid BeerXML export", None))?;
                let actor = krausen_app::ActorContext {
                    actor_id: principal.user_id.clone(),
                    credential_id: principal.credential_id.clone(),
                    access_expires_at: principal.access_expires_at,
                    workspace_id: input.workspace_id,
                };
                let outcome =
                    krausen_app::interchange::export(&self.db, &actor, &input.request).await;
                let success = matches!(
                    outcome,
                    krausen_types::interchange::ExportOutcome::Exported { .. }
                        | krausen_types::interchange::ExportOutcome::LossAcknowledgementRequired(_)
                );
                if success {
                    CallToolResult::structured(json!({"outcome":outcome}))
                } else {
                    CallToolResult::structured_error(json!({"outcome":outcome}))
                }
            }
            "beerxml.original.v1" => {
                let input: BeerXmlOriginal = serde_json::from_value(input).map_err(|_| {
                    ErrorData::invalid_params("Invalid BeerXML original query", None)
                })?;
                let actor = krausen_app::ActorContext {
                    actor_id: principal.user_id.clone(),
                    credential_id: principal.credential_id.clone(),
                    access_expires_at: principal.access_expires_at,
                    workspace_id: input.workspace_id,
                };
                let outcome = krausen_app::interchange::original(
                    &self.db,
                    &actor,
                    &input.intent,
                    &input.recipe_id,
                )
                .await;
                let success = matches!(
                    outcome,
                    krausen_types::interchange::OriginalOutcome::Original { .. }
                );
                if success {
                    CallToolResult::structured(json!({"outcome":outcome}))
                } else {
                    CallToolResult::structured_error(json!({"outcome":outcome}))
                }
            }
            "recipe.list.v1" | "recipe.history.v1" | "recipe.revision.v1" | "recipe.archive.v1" => {
                macro_rules! parse_library {
                    ($ty:ty) => {
                        serde_json::from_value::<$ty>(input).map_err(|_| {
                            ErrorData::invalid_params("Invalid recipe library request", None)
                        })?
                    };
                }
                macro_rules! library_actor {
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
                    "recipe.list.v1" => {
                        let c = parse_library!(RecipeList);
                        let actor = library_actor!(c.workspace_id);
                        krausen_app::recipe_library::list(&self.db, &actor, c.request).await
                    }
                    "recipe.history.v1" => {
                        let c = parse_library!(RecipeHistory);
                        let actor = library_actor!(c.workspace_id);
                        krausen_app::recipe_library::history(&self.db, &actor, c.request).await
                    }
                    "recipe.revision.v1" => {
                        let c = parse_library!(RecipeRevision);
                        let actor = library_actor!(c.workspace_id);
                        krausen_app::recipe_library::revision(&self.db, &actor, c.request).await
                    }
                    "recipe.archive.v1" => {
                        let c = parse_library!(RecipeArchive);
                        let actor = library_actor!(c.workspace_id);
                        krausen_app::recipe_library::archive(&self.db, &actor, c.request).await
                    }
                    _ => unreachable!(),
                };
                let error = !matches!(
                    outcome,
                    krausen_types::recipe_library::LibraryOutcome::Recipes(_)
                        | krausen_types::recipe_library::LibraryOutcome::History(_)
                        | krausen_types::recipe_library::LibraryOutcome::Revision(_)
                        | krausen_types::recipe_library::LibraryOutcome::Archive { .. }
                        | krausen_types::recipe_library::LibraryOutcome::NotFound
                );
                let value = json!({"outcome":outcome});
                if error {
                    CallToolResult::structured_error(value)
                } else {
                    CallToolResult::structured(value)
                }
            }
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
                                c.request.cursor.as_deref(),
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
            "ingredient.save.v1"
            | "ingredient.archive.v1"
            | "ingredient.get.v1"
            | "ingredient.search.v1" => {
                macro_rules! parse {
                    ($ty:ty) => {
                        serde_json::from_value::<$ty>(input).map_err(|_| {
                            ErrorData::invalid_params("Invalid ingredient request", None)
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
                    "ingredient.save.v1" => {
                        let c = parse!(IngredientSave);
                        let actor = actor!(c.workspace_id);
                        krausen_app::catalogue::project(
                            krausen_app::catalogue::save(&self.db, &actor, c.request).await,
                        )
                    }
                    "ingredient.archive.v1" => {
                        let c = parse!(IngredientArchive);
                        let actor = actor!(c.workspace_id);
                        krausen_app::catalogue::project(
                            krausen_app::catalogue::set_archived(&self.db, &actor, c.request).await,
                        )
                    }
                    "ingredient.get.v1" => {
                        let c = parse!(IngredientGet);
                        let actor = actor!(c.workspace_id);
                        krausen_app::catalogue::project_get(
                            krausen_app::catalogue::get(
                                &self.db,
                                &actor,
                                &c.request.intent,
                                &c.request.entry_id,
                            )
                            .await,
                        )
                    }
                    "ingredient.search.v1" => {
                        let c = parse!(IngredientSearch);
                        let actor = actor!(c.workspace_id);
                        krausen_app::catalogue::project_page(
                            krausen_app::catalogue::search(&self.db, &actor, &c.request).await,
                        )
                    }
                    _ => unreachable!(),
                };
                let is_error = !matches!(
                    outcome,
                    krausen_types::catalogue::IngredientOutcome::Entry(_)
                        | krausen_types::catalogue::IngredientOutcome::Entries(_)
                        | krausen_types::catalogue::IngredientOutcome::NotFound
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
            "recipe.process-summary.v1" => {
                let input: ProcessSummaryQuery = serde_json::from_value(input).map_err(|_| {
                    ErrorData::invalid_params("Invalid recipe process summary", None)
                })?;
                let actor = krausen_app::ActorContext {
                    actor_id: principal.user_id.clone(),
                    credential_id: principal.credential_id.clone(),
                    access_expires_at: principal.access_expires_at,
                    workspace_id: input.workspace_id,
                };
                let outcome =
                    krausen_app::process_summary::read(&self.db, &actor, input.request).await;
                let success = matches!(
                    outcome,
                    krausen_types::process_summary::ProcessSummaryOutcome::Summary(_)
                );
                let value = json!({"outcome": outcome});
                if success {
                    CallToolResult::structured(value)
                } else {
                    CallToolResult::structured_error(value)
                }
            }
            "recipe.scale-preview.v1" => {
                let input: RecipeScalePreview = serde_json::from_value(input)
                    .map_err(|_| ErrorData::invalid_params("Invalid scaling preview", None))?;
                let actor = krausen_app::ActorContext {
                    actor_id: principal.user_id.clone(),
                    credential_id: principal.credential_id.clone(),
                    access_expires_at: principal.access_expires_at,
                    workspace_id: input.workspace_id,
                };
                let outcome = krausen_app::scaling::preview(
                    &self.db,
                    &actor,
                    krausen_types::scaling::ScalePreviewRequest {
                        intent: input.intent,
                        recipe_id: input.recipe_id,
                        target_batch_size_l: input.target_batch_size_l,
                    },
                )
                .await;
                let success = matches!(outcome, krausen_types::scaling::ScaleOutcome::Preview(_));
                let value = json!({"outcome": outcome});
                if success {
                    CallToolResult::structured(value)
                } else {
                    CallToolResult::structured_error(value)
                }
            }
            "recipe.scale-apply.v1" => {
                let input: RecipeScaleApply = serde_json::from_value(input)
                    .map_err(|_| ErrorData::invalid_params("Invalid scaling apply", None))?;
                let actor = krausen_app::ActorContext {
                    actor_id: principal.user_id.clone(),
                    credential_id: principal.credential_id.clone(),
                    access_expires_at: principal.access_expires_at,
                    workspace_id: input.workspace_id,
                };
                let outcome = krausen_app::scaling::apply(&self.db, &actor, input.request).await;
                let success = matches!(
                    outcome,
                    krausen_types::scaling::ScaleApplyOutcome::Applied { .. }
                        | krausen_types::scaling::ScaleApplyOutcome::Unchanged { .. }
                );
                let value = json!({"outcome": outcome});
                if success {
                    CallToolResult::structured(value)
                } else {
                    CallToolResult::structured_error(value)
                }
            }
            "recipe.duplicate.v1" => {
                let input: RecipeDuplicate = serde_json::from_value(input)
                    .map_err(|_| ErrorData::invalid_params("Invalid recipe duplicate", None))?;
                let actor = krausen_app::ActorContext {
                    actor_id: principal.user_id.clone(),
                    credential_id: principal.credential_id.clone(),
                    access_expires_at: principal.access_expires_at,
                    workspace_id: input.workspace_id,
                };
                let outcome =
                    krausen_app::scaling::duplicate(&self.db, &actor, input.request).await;
                let success = matches!(
                    outcome,
                    krausen_types::scaling::DuplicateOutcome::Duplicated { .. }
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

#[cfg(test)]
mod invitation_tests {
    use super::*;
    use krausen_types::invitation::{InvitationAction, InvitationResponseOutcome};

    #[tokio::test]
    async fn mcp_invitation_rejects_wrong_recipient() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./migrations-sqlite").run(&pool).await.unwrap();
        sqlx::query("INSERT INTO users (user_id,email) VALUES ('owner','owner@example.test'),('recipient','recipient@example.test'),('other','other@example.test')").execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO workspaces (workspace_id,owner_user_id) VALUES ('w','owner')").execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO auth_credentials (credential_id,user_id,kind,issuer,audience,expires_at) VALUES ('c','other','mcp','test','test',4102444800)").execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO workspace_invitations (invitation_id,workspace_id,email,role,invited_by_user_id,expires_at) VALUES ('i','w','recipient@example.test','workspace_user','owner','2099-01-01T00:00:00Z')").execute(&pool).await.unwrap();
        let db = krausen_core::DbPool::Sqlite(pool);
        let principal = Principal { user_id: "other".into(), credential_id: "c".into(), access_expires_at: 4102444800 };
        let input = InvitationReply { invitation_id: "i".into(), action: InvitationAction::Decline, request_id: uuid::Uuid::new_v4() };
        assert_eq!(execute_invitation_reply(&db, &principal, input).await, InvitationResponseOutcome::NotFound);
        let profile = execute_account_profile(&db, &principal).await;
        assert!(matches!(profile, krausen_types::account_read::AccountProfileOutcome::Available { profile } if profile.email == "other@example.test"));
        let workspaces = execute_account_workspaces(&db, &principal, AccountWorkspacesQuery { cursor: None, limit: None }).await;
        assert!(matches!(workspaces, krausen_types::account_read::AccountWorkspaceListOutcome::Available { page } if page.items.is_empty()));
        assert!(serde_json::from_value::<AccountWorkspacesQuery>(json!({"actor_id":"recipient"})).is_err());
        let query = || TeamMembersQuery { workspace_id: "w".into(), cursor: None, limit: Some(1) };
        assert!(matches!(execute_team_members(&db, &principal, query()).await, krausen_types::team_read::MemberListOutcome::Forbidden));
        let krausen_core::DbPool::Sqlite(pool) = &db else { unreachable!() };
        sqlx::query("INSERT INTO workspace_users (workspace_id,user_id,role,active) VALUES ('w','other','workspace_user',TRUE),('w','owner','workspace_admin',TRUE)").execute(pool).await.unwrap();
        let first = execute_team_members(&db, &principal, query()).await;
        let krausen_types::team_read::MemberListOutcome::Available { page } = first else { panic!("ordinary member should be allowed") };
        assert_eq!(page.items.len(), 1);
        let second = execute_team_members(&db, &principal, TeamMembersQuery { cursor: page.next_cursor, ..query() }).await;
        assert!(matches!(second, krausen_types::team_read::MemberListOutcome::Available { page } if page.items.len() == 1 && page.next_cursor.is_none()));
        assert!(serde_json::from_value::<TeamMembersQuery>(json!({"workspace_id":"w","actor_id":"owner"})).is_err());
        let updated = execute_account_name(&db, &principal, AccountNameSet { name: "MCP user".into(), expected_settings_revision: 0 }).await;
        assert!(matches!(updated, krausen_types::account_read::AccountUpdateOutcome::Updated { settings_revision: 1 }));
        let stale = execute_account_theme(&db, &principal, AccountThemeSet { theme: "dark".into(), expected_settings_revision: 0 }).await;
        assert!(matches!(stale, krausen_types::account_read::AccountUpdateOutcome::Conflict { expected: 0, current: 1 }));
        assert!(serde_json::from_value::<AccountNameSet>(json!({"name":"spoof","expected_settings_revision":1,"actor_id":"owner"})).is_err());
        let selected = execute_workspace_preference(&db, &principal, AccountWorkspacePreferenceSet { workspace_id: "w".into(), expected_settings_revision: 1 }).await;
        assert!(matches!(selected, krausen_types::account_read::AccountUpdateOutcome::Updated { settings_revision: 2 }));
        sqlx::query("INSERT INTO workspaces (workspace_id,owner_user_id) VALUES ('foreign','owner')").execute(pool).await.unwrap();
        let explicit_foreign = execute_team_members(&db, &principal, TeamMembersQuery { workspace_id: "foreign".into(), cursor: None, limit: None }).await;
        assert!(matches!(explicit_foreign, krausen_types::team_read::MemberListOutcome::Forbidden));
        sqlx::query("INSERT INTO refresh_tokens (token_id,user_id,token_hash,family_id,expires_at,is_active) VALUES ('session','other','secret-hash','c',$1,TRUE)").bind(chrono::Utc::now() + chrono::Duration::days(1)).execute(pool).await.unwrap();
        let sessions = execute_sessions(&db, &principal, AccountWorkspacesQuery { cursor: None, limit: Some(1) }).await;
        assert!(matches!(&sessions, krausen_types::session_read::SessionListOutcome::Available { page } if page.items.len() == 1 && !page.items[0].is_current));
        assert!(!serde_json::to_string(&sessions).unwrap().contains("secret-hash"));
    }
}
