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
    mail_available: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InvitationBegin {
    discovery_id: String,
    workspace_id: String,
    invitation_id: String,
    action: krausen_app::invitation::InvitationAction,
    request_id: uuid::Uuid,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DiscoveryBegin {
    request_id: uuid::Uuid,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InteractionQuery {
    interaction_id: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InvitationCancel {
    workspace_id: String,
    invitation_id: String,
    expected_revision: i64,
    request_id: uuid::Uuid,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkspaceQuery {
    workspace_id: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AccountContextQuery {
    workspace_id: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkspaceSelection {
    workspace_id: String,
    request_id: uuid::Uuid,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProfileSave {
    change: krausen_types::account::ProfileChange,
    expected_revision: i64,
    request_id: uuid::Uuid,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TeamChange {
    workspace_id: String,
    change: krausen_types::account::TeamChange,
    expected_revision: i64,
    request_id: uuid::Uuid,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TeamInvite {
    workspace_id: String,
    email: String,
    role: String,
    expected_revision: i64,
    request_id: uuid::Uuid,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OwnershipId {
    transfer_id: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OwnershipStart {
    workspace_id: String,
    to_user_id: String,
    expected_revision: i64,
    request_id: uuid::Uuid,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OwnershipCancel {
    workspace_id: String,
    transfer_id: String,
    expected_revision: i64,
    request_id: uuid::Uuid,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OwnershipResponse {
    workspace_id: String,
    transfer_id: String,
    accept: bool,
    expected_revision: i64,
    request_id: uuid::Uuid,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CredentialRevoke {
    credential_id: String,
    expected_revision: i64,
    request_id: uuid::Uuid,
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
                operations::SECURITY_STATUS.mcp_tool,
                "Read the current account's security status",
                schema(json!({"type":"object","properties":{},"additionalProperties":false})),
            ),
            Tool::new(
                operations::PASSKEYS_LIST.mcp_tool,
                "List the current account's passkey display metadata",
                schema(json!({"type":"object","properties":{},"additionalProperties":false})),
            ),
            Tool::new(
                operations::SESSIONS_REVOKE_ALL.mcp_tool,
                "Revoke all credentials belonging to the current account",
                schema(
                    json!({"type":"object","properties":{"request_id":{"type":"string","format":"uuid"}},"required":["request_id"],"additionalProperties":false}),
                ),
            ),
            Tool::new(
                operations::SESSIONS_LIST.mcp_tool,
                "List credentials belonging to the current account",
                schema(json!({"type":"object","properties":{},"additionalProperties":false})),
            ),
            Tool::new(
                operations::SESSION_REVOKE.mcp_tool,
                "Revoke an owned browser session or MCP connection",
                schema(
                    json!({"type":"object","properties":{"credential_id":{"type":"string"},"expected_revision":{"type":"integer"},"request_id":{"type":"string","format":"uuid"}},"required":["credential_id","expected_revision","request_id"],"additionalProperties":false}),
                ),
            ),
            Tool::new(
                operations::OWNERSHIP_LIST.mcp_tool,
                "List transfers visible to the current account",
                schema(
                    json!({"type":"object","properties":{"workspace_id":{"type":"string"}},"required":["workspace_id"],"additionalProperties":false}),
                ),
            ),
            Tool::new(
                operations::OWNERSHIP_GET.mcp_tool,
                "Read a transfer as its current owner or named recipient",
                schema(
                    json!({"type":"object","properties":{"transfer_id":{"type":"string"}},"required":["transfer_id"],"additionalProperties":false}),
                ),
            ),
            Tool::new(
                operations::OWNERSHIP_START.mcp_tool,
                "Start an ownership transfer to a workspace member",
                schema(
                    json!({"type":"object","properties":{"workspace_id":{"type":"string"},"to_user_id":{"type":"string"},"expected_revision":{"type":"integer"},"request_id":{"type":"string","format":"uuid"}},"required":["workspace_id","to_user_id","expected_revision","request_id"],"additionalProperties":false}),
                ),
            ),
            Tool::new(
                operations::OWNERSHIP_CANCEL.mcp_tool,
                "Cancel a pending ownership transfer",
                schema(
                    json!({"type":"object","properties":{"workspace_id":{"type":"string"},"transfer_id":{"type":"string"},"expected_revision":{"type":"integer"},"request_id":{"type":"string","format":"uuid"}},"required":["workspace_id","transfer_id","expected_revision","request_id"],"additionalProperties":false}),
                ),
            ),
            Tool::new(
                operations::OWNERSHIP_RESPONSE.mcp_tool,
                "Begin mailbox confirmation of an ownership response",
                schema(
                    json!({"type":"object","properties":{"workspace_id":{"type":"string"},"transfer_id":{"type":"string"},"accept":{"type":"boolean"},"expected_revision":{"type":"integer"},"request_id":{"type":"string","format":"uuid"}},"required":["workspace_id","transfer_id","accept","expected_revision","request_id"],"additionalProperties":false}),
                ),
            ),
            Tool::new(
                operations::OWNERSHIP_STATUS.mcp_tool,
                "Check an ownership confirmation interaction",
                schema(
                    json!({"type":"object","properties":{"interaction_id":{"type":"string"}},"required":["interaction_id"],"additionalProperties":false}),
                ),
            ),
            Tool::new(
                operations::ACCOUNT_CONTEXT.mcp_tool,
                "Read account context with an optional explicit workspace",
                schema(
                    json!({"type":"object","properties":{"workspace_id":{"type":"string"}},"additionalProperties":false}),
                ),
            ),
            Tool::new(
                operations::TEAM_INVITATIONS.mcp_tool,
                "List pending invitations for an administered workspace",
                schema(
                    json!({"type":"object","properties":{"workspace_id":{"type":"string"}},"required":["workspace_id"],"additionalProperties":false}),
                ),
            ),
            Tool::new(
                operations::TEAM_INVITE.mcp_tool,
                "Queue a workspace invitation for email delivery",
                schema(
                    json!({"type":"object","properties":{"workspace_id":{"type":"string"},"email":{"type":"string"},"role":{"enum":["workspace_admin","workspace_user"]},"expected_revision":{"type":"integer"},"request_id":{"type":"string","format":"uuid"}},"required":["workspace_id","email","role","expected_revision","request_id"],"additionalProperties":false}),
                ),
            ),
            Tool::new(
                operations::TEAM_MEMBERS.mcp_tool,
                "List workspace members and current revision",
                schema(
                    json!({"type":"object","properties":{"workspace_id":{"type":"string"}},"required":["workspace_id"],"additionalProperties":false}),
                ),
            ),
            Tool::new(
                operations::TEAM_CHANGE.mcp_tool,
                "Update or remove a workspace member",
                schema(
                    json!({"type":"object","properties":{"workspace_id":{"type":"string"},"change":{"oneOf":[{"type":"object","properties":{"action":{"const":"set_role"},"user_id":{"type":"string"},"role":{"enum":["workspace_admin","workspace_user"]}},"required":["action","user_id","role"],"additionalProperties":false},{"type":"object","properties":{"action":{"const":"remove"},"user_id":{"type":"string"}},"required":["action","user_id"],"additionalProperties":false}]},"expected_revision":{"type":"integer"},"request_id":{"type":"string","format":"uuid"}},"required":["workspace_id","change","expected_revision","request_id"],"additionalProperties":false}),
                ),
            ),
            Tool::new(
                operations::ACCOUNT_PROFILE_SAVE.mcp_tool,
                "Save an account profile field using its observed revision",
                schema(
                    json!({"type":"object","properties":{"change":{"oneOf":[{"type":"object","properties":{"field":{"const":"name"},"name":{"type":"string"}},"required":["field","name"],"additionalProperties":false},{"type":"object","properties":{"field":{"const":"theme"},"theme":{"enum":["light","dark","system"]}},"required":["field","theme"],"additionalProperties":false}]},"expected_revision":{"type":"integer"},"request_id":{"type":"string","format":"uuid"}},"required":["change","expected_revision","request_id"],"additionalProperties":false}),
                ),
            ),
            Tool::new(
                operations::ACCOUNT_WORKSPACES.mcp_tool,
                "List active workspace memberships",
                schema(json!({"type":"object","properties":{},"additionalProperties":false})),
            ),
            Tool::new(
                operations::ACCOUNT_WORKSPACE_SELECT.mcp_tool,
                "Select a preferred workspace; subsequent tool calls still require an explicit workspace",
                schema(
                    json!({"type":"object","properties":{"workspace_id":{"type":"string"},"request_id":{"type":"string","format":"uuid"}},"required":["workspace_id","request_id"],"additionalProperties":false}),
                ),
            ),
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
                operations::INVITATION_CANCEL.mcp_tool,
                "Cancel a pending workspace invitation",
                schema(
                    json!({"type":"object","properties":{"workspace_id":{"type":"string"},"invitation_id":{"type":"string"},"expected_revision":{"type":"integer"},"request_id":{"type":"string","format":"uuid"}},"required":["workspace_id","invitation_id","expected_revision","request_id"],"additionalProperties":false}),
                ),
            ),
            Tool::new(
                operations::INVITATION_DISCOVERY_BEGIN.mcp_tool,
                "Begin mailbox verification for invitation discovery",
                schema(
                    json!({"type":"object","properties":{"request_id":{"type":"string","format":"uuid"}},"required":["request_id"],"additionalProperties":false}),
                ),
            ),
            Tool::new(
                operations::INVITATION_LIST.mcp_tool,
                "List invitations after mailbox verification",
                schema(
                    json!({"type":"object","properties":{"interaction_id":{"type":"string"}},"required":["interaction_id"],"additionalProperties":false}),
                ),
            ),
            Tool::new(
                operations::INVITATION_PROOF_BEGIN.mcp_tool,
                "Begin recipient mailbox proof for an invitation response",
                schema(
                    json!({"type":"object","properties":{"discovery_id":{"type":"string"},"workspace_id":{"type":"string"},"invitation_id":{"type":"string"},"action":{"type":"string","enum":["accept","decline"]},"request_id":{"type":"string","format":"uuid"}},"required":["discovery_id","workspace_id","invitation_id","action","request_id"],"additionalProperties":false}),
                ),
            ),
            Tool::new(
                operations::INVITATION_PROOF_STATUS.mcp_tool,
                "Check the current account interaction",
                schema(
                    json!({"type":"object","properties":{"interaction_id":{"type":"string"}},"required":["interaction_id"],"additionalProperties":false}),
                ),
            ),
            Tool::new(
                operations::ACCOUNT_PROFILE_GET.mcp_tool,
                "Read the authenticated account profile",
                schema(json!({"type":"object", "properties":{}, "additionalProperties":false})),
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
            Tool::new(
                operations::RECIPE_PROCESS_SUMMARY.mcp_tool,
                "Return the authorized structured process summary for a saved revision or an unsaved draft",
                typed_schema::<ProcessSummaryQuery>(),
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
            "account.security.status.v1" | "account.passkeys.list.v1" => {
                if input.as_object().is_none_or(|value| !value.is_empty()) {
                    return Err(ErrorData::invalid_params("Invalid security query", None));
                }
                let actor = krausen_app::ActorContext {
                    actor_id: principal.user_id.clone(),
                    credential_id: principal.credential_id.clone(),
                    access_expires_at: principal.access_expires_at,
                    workspace_id: String::new(),
                };
                if operation.id == "account.security.status.v1" {
                    match krausen_app::security::status(&self.db, &actor).await {
                        Ok(status) => {
                            CallToolResult::structured(serde_json::to_value(status).map_err(
                                |_| ErrorData::internal_error("Result unavailable", None),
                            )?)
                        }
                        Err(_) => CallToolResult::structured_error(
                            json!({"error":"unavailable_or_forbidden"}),
                        ),
                    }
                } else {
                    match krausen_app::security::passkeys(&self.db, &actor).await {
                        Ok(passkeys) => CallToolResult::structured(json!({"passkeys":passkeys})),
                        Err(_) => CallToolResult::structured_error(
                            json!({"error":"unavailable_or_forbidden"}),
                        ),
                    }
                }
            }
            "account.credentials.revoke_all.v1" => {
                let input: DiscoveryBegin = serde_json::from_value(input)
                    .map_err(|_| ErrorData::invalid_params("Invalid request", None))?;
                let actor = krausen_app::ActorContext {
                    actor_id: principal.user_id.clone(),
                    credential_id: principal.credential_id.clone(),
                    access_expires_at: principal.access_expires_at,
                    workspace_id: String::new(),
                };
                let outcome = krausen_app::security::project_revoke_all(
                    krausen_app::security::revoke_all(&self.db, &actor, input.request_id).await,
                );
                let error = !matches!(
                    outcome,
                    krausen_types::account::RevokeAllOutcome::Revoked { .. }
                );
                let value = json!({"outcome":outcome});
                if error {
                    CallToolResult::structured_error(value)
                } else {
                    CallToolResult::structured(value)
                }
            }
            "account.sessions.list.v1" => {
                if input.as_object().is_none_or(|value| !value.is_empty()) {
                    return Err(ErrorData::invalid_params("Invalid session query", None));
                }
                let actor = krausen_app::ActorContext {
                    actor_id: principal.user_id.clone(),
                    credential_id: principal.credential_id.clone(),
                    access_expires_at: principal.access_expires_at,
                    workspace_id: String::new(),
                };
                match krausen_app::security::sessions(&self.db, &actor).await {
                    Ok(sessions) => CallToolResult::structured(json!({"sessions":sessions})),
                    Err(_) => CallToolResult::structured_error(
                        json!({"error":"unavailable_or_forbidden"}),
                    ),
                }
            }
            "account.credential.revoke.v1" => {
                let input: CredentialRevoke = serde_json::from_value(input).map_err(|_| {
                    ErrorData::invalid_params("Invalid credential revocation", None)
                })?;
                let actor = krausen_app::ActorContext {
                    actor_id: principal.user_id.clone(),
                    credential_id: principal.credential_id.clone(),
                    access_expires_at: principal.access_expires_at,
                    workspace_id: String::new(),
                };
                let outcome = krausen_app::team::project_change(
                    krausen_app::security::revoke(
                        &self.db,
                        &actor,
                        &input.credential_id,
                        input.expected_revision,
                        input.request_id,
                    )
                    .await,
                );
                let error = !matches!(
                    outcome,
                    krausen_types::account::TeamChangeOutcome::Saved { .. }
                );
                let value = json!({"outcome":outcome});
                if error {
                    CallToolResult::structured_error(value)
                } else {
                    CallToolResult::structured(value)
                }
            }
            "workspace.ownership.list.v1" => {
                let input: WorkspaceQuery = serde_json::from_value(input)
                    .map_err(|_| ErrorData::invalid_params("Invalid workspace query", None))?;
                let actor = krausen_app::ActorContext {
                    actor_id: principal.user_id.clone(),
                    credential_id: principal.credential_id.clone(),
                    access_expires_at: principal.access_expires_at,
                    workspace_id: input.workspace_id,
                };
                match krausen_app::ownership::list(&self.db, &actor).await {
                    Ok(transfers) => CallToolResult::structured(json!({"transfers":transfers})),
                    Err(_) => CallToolResult::structured_error(
                        json!({"error":"unavailable_or_forbidden"}),
                    ),
                }
            }
            "workspace.ownership.get.v1" => {
                let input: OwnershipId = serde_json::from_value(input)
                    .map_err(|_| ErrorData::invalid_params("Invalid transfer", None))?;
                let actor = krausen_app::ActorContext {
                    actor_id: principal.user_id.clone(),
                    credential_id: principal.credential_id.clone(),
                    access_expires_at: principal.access_expires_at,
                    workspace_id: String::new(),
                };
                match krausen_app::ownership::get(&self.db, &actor, &input.transfer_id).await {
                    Ok(detail) => CallToolResult::structured(
                        serde_json::to_value(detail)
                            .map_err(|_| ErrorData::internal_error("Result unavailable", None))?,
                    ),
                    Err(_) => CallToolResult::structured_error(
                        json!({"error":"unavailable_or_forbidden"}),
                    ),
                }
            }
            "workspace.ownership.start.v1" => {
                let input: OwnershipStart = serde_json::from_value(input)
                    .map_err(|_| ErrorData::invalid_params("Invalid transfer", None))?;
                let actor = krausen_app::ActorContext {
                    actor_id: principal.user_id.clone(),
                    credential_id: principal.credential_id.clone(),
                    access_expires_at: principal.access_expires_at,
                    workspace_id: input.workspace_id,
                };
                let outcome = krausen_app::ownership::project_start(
                    krausen_app::ownership::start(
                        &self.db,
                        &actor,
                        &input.to_user_id,
                        input.expected_revision,
                        input.request_id,
                        self.mail_available,
                    )
                    .await,
                );
                let error = !matches!(
                    outcome,
                    krausen_types::account::OwnershipStartOutcome::Queued(_)
                );
                let value = json!({"outcome":outcome});
                if error {
                    CallToolResult::structured_error(value)
                } else {
                    CallToolResult::structured(value)
                }
            }
            "workspace.ownership.cancel.v1" => {
                let input: OwnershipCancel = serde_json::from_value(input)
                    .map_err(|_| ErrorData::invalid_params("Invalid transfer", None))?;
                let actor = krausen_app::ActorContext {
                    actor_id: principal.user_id.clone(),
                    credential_id: principal.credential_id.clone(),
                    access_expires_at: principal.access_expires_at,
                    workspace_id: input.workspace_id,
                };
                let outcome = krausen_app::team::project_change(
                    krausen_app::ownership::cancel(
                        &self.db,
                        &actor,
                        &input.transfer_id,
                        input.expected_revision,
                        input.request_id,
                    )
                    .await,
                );
                let error = !matches!(
                    outcome,
                    krausen_types::account::TeamChangeOutcome::Saved { .. }
                );
                let value = json!({"outcome":outcome});
                if error {
                    CallToolResult::structured_error(value)
                } else {
                    CallToolResult::structured(value)
                }
            }
            "workspace.ownership.response.begin.v1" => {
                let input: OwnershipResponse = serde_json::from_value(input)
                    .map_err(|_| ErrorData::invalid_params("Invalid transfer response", None))?;
                let actor = krausen_app::ActorContext {
                    actor_id: principal.user_id.clone(),
                    credential_id: principal.credential_id.clone(),
                    access_expires_at: principal.access_expires_at,
                    workspace_id: input.workspace_id,
                };
                let outcome = krausen_app::ownership::project_response(
                    krausen_app::ownership::begin_response(
                        &self.db,
                        &actor,
                        &input.transfer_id,
                        input.accept,
                        input.expected_revision,
                        input.request_id,
                        self.mail_available,
                    )
                    .await,
                );
                let error = !matches!(
                    outcome,
                    krausen_types::account::OwnershipResponseOutcome::Pending(_)
                );
                let value = json!({"outcome":outcome});
                if error {
                    CallToolResult::structured_error(value)
                } else {
                    CallToolResult::structured(value)
                }
            }
            "workspace.ownership.response.status.v1" => {
                let input: InteractionQuery = serde_json::from_value(input)
                    .map_err(|_| ErrorData::invalid_params("Invalid interaction", None))?;
                let actor = krausen_app::ActorContext {
                    actor_id: principal.user_id.clone(),
                    credential_id: principal.credential_id.clone(),
                    access_expires_at: principal.access_expires_at,
                    workspace_id: String::new(),
                };
                match krausen_app::invitation::status(&self.db, &actor, &input.interaction_id).await
                {
                    Ok(status) => CallToolResult::structured(
                        serde_json::to_value(status)
                            .map_err(|_| ErrorData::internal_error("Result unavailable", None))?,
                    ),
                    Err(_) => CallToolResult::structured_error(
                        json!({"error":"unavailable_or_forbidden"}),
                    ),
                }
            }
            "account.context.get.v1" => {
                let input: AccountContextQuery = serde_json::from_value(input).map_err(|_| {
                    ErrorData::invalid_params("Invalid account context query", None)
                })?;
                let actor = krausen_app::ActorContext {
                    actor_id: principal.user_id.clone(),
                    credential_id: principal.credential_id.clone(),
                    access_expires_at: principal.access_expires_at,
                    workspace_id: input.workspace_id.unwrap_or_default(),
                };
                match krausen_app::account::context(&self.db, &actor).await {
                    Ok(context) => CallToolResult::structured(
                        serde_json::to_value(context)
                            .map_err(|_| ErrorData::internal_error("Result unavailable", None))?,
                    ),
                    Err(_) => CallToolResult::structured_error(
                        json!({"error":"unavailable_or_forbidden"}),
                    ),
                }
            }
            "workspace.invitations.list.v1" => {
                let input: WorkspaceQuery = serde_json::from_value(input)
                    .map_err(|_| ErrorData::invalid_params("Invalid workspace query", None))?;
                let actor = krausen_app::ActorContext {
                    actor_id: principal.user_id.clone(),
                    credential_id: principal.credential_id.clone(),
                    access_expires_at: principal.access_expires_at,
                    workspace_id: input.workspace_id,
                };
                match krausen_app::team::invitations(&self.db, &actor).await {
                    Ok(invitations) => {
                        CallToolResult::structured(json!({"invitations":invitations}))
                    }
                    Err(_) => CallToolResult::structured_error(
                        json!({"error":"unavailable_or_forbidden"}),
                    ),
                }
            }
            "workspace.invitation.create.v1" => {
                let input: TeamInvite = serde_json::from_value(input)
                    .map_err(|_| ErrorData::invalid_params("Invalid invitation", None))?;
                let actor = krausen_app::ActorContext {
                    actor_id: principal.user_id.clone(),
                    credential_id: principal.credential_id.clone(),
                    access_expires_at: principal.access_expires_at,
                    workspace_id: input.workspace_id,
                };
                let outcome = krausen_app::team::project_invite(
                    krausen_app::team::invite(
                        &self.db,
                        &actor,
                        &input.email,
                        &input.role,
                        input.expected_revision,
                        input.request_id,
                        self.mail_available,
                    )
                    .await,
                );
                let is_error = !matches!(
                    outcome,
                    krausen_types::account::InvitationCreateOutcome::Queued(_)
                );
                let result = json!({"outcome":outcome});
                if is_error {
                    CallToolResult::structured_error(result)
                } else {
                    CallToolResult::structured(result)
                }
            }
            "workspace.members.list.v1" => {
                let input: WorkspaceQuery = serde_json::from_value(input)
                    .map_err(|_| ErrorData::invalid_params("Invalid workspace query", None))?;
                let actor = krausen_app::ActorContext {
                    actor_id: principal.user_id.clone(),
                    credential_id: principal.credential_id.clone(),
                    access_expires_at: principal.access_expires_at,
                    workspace_id: input.workspace_id,
                };
                match krausen_app::team::members(&self.db, &actor).await {
                    Ok(snapshot) => CallToolResult::structured(
                        serde_json::to_value(snapshot)
                            .map_err(|_| ErrorData::internal_error("Result unavailable", None))?,
                    ),
                    Err(_) => CallToolResult::structured_error(
                        json!({"error":"unavailable_or_forbidden"}),
                    ),
                }
            }
            "workspace.member.change.v1" => {
                let input: TeamChange = serde_json::from_value(input)
                    .map_err(|_| ErrorData::invalid_params("Invalid team change", None))?;
                let actor = krausen_app::ActorContext {
                    actor_id: principal.user_id.clone(),
                    credential_id: principal.credential_id.clone(),
                    access_expires_at: principal.access_expires_at,
                    workspace_id: input.workspace_id,
                };
                let outcome = krausen_app::team::project_change(
                    krausen_app::team::change(
                        &self.db,
                        &actor,
                        input.change,
                        input.expected_revision,
                        input.request_id,
                    )
                    .await,
                );
                let is_error = !matches!(
                    outcome,
                    krausen_types::account::TeamChangeOutcome::Saved { .. }
                );
                let result = json!({"outcome":outcome});
                if is_error {
                    CallToolResult::structured_error(result)
                } else {
                    CallToolResult::structured(result)
                }
            }
            "account.profile.save.v1" => {
                let input: ProfileSave = serde_json::from_value(input)
                    .map_err(|_| ErrorData::invalid_params("Invalid profile change", None))?;
                let actor = krausen_app::ActorContext {
                    actor_id: principal.user_id.clone(),
                    credential_id: principal.credential_id.clone(),
                    access_expires_at: principal.access_expires_at,
                    workspace_id: String::new(),
                };
                let outcome = krausen_app::account::project_save(
                    krausen_app::account::save(
                        &self.db,
                        &actor,
                        input.change,
                        input.expected_revision,
                        input.request_id,
                    )
                    .await,
                );
                let is_error = !matches!(
                    outcome,
                    krausen_types::account::ProfileSaveOutcome::Saved(_)
                );
                let result = json!({"outcome":outcome});
                if is_error {
                    CallToolResult::structured_error(result)
                } else {
                    CallToolResult::structured(result)
                }
            }
            "account.workspaces.list.v1" => {
                if input.as_object().is_none_or(|v| !v.is_empty()) {
                    return Err(ErrorData::invalid_params("Invalid workspace query", None));
                }
                let actor = krausen_app::ActorContext {
                    actor_id: principal.user_id.clone(),
                    credential_id: principal.credential_id.clone(),
                    access_expires_at: principal.access_expires_at,
                    workspace_id: String::new(),
                };
                match krausen_app::account::workspaces(&self.db, &actor).await {
                    Ok(items) => CallToolResult::structured(json!({"workspaces":items})),
                    Err(_) => CallToolResult::structured_error(
                        json!({"error":"unavailable_or_forbidden"}),
                    ),
                }
            }
            "account.workspace.select.v1" => {
                let input: WorkspaceSelection = serde_json::from_value(input)
                    .map_err(|_| ErrorData::invalid_params("Invalid workspace selection", None))?;
                let actor = krausen_app::ActorContext {
                    actor_id: principal.user_id.clone(),
                    credential_id: principal.credential_id.clone(),
                    access_expires_at: principal.access_expires_at,
                    workspace_id: input.workspace_id.clone(),
                };
                match krausen_app::account::select_workspace(
                    &self.db,
                    &actor,
                    &input.workspace_id,
                    input.request_id,
                )
                .await
                {
                    Ok(item) => CallToolResult::structured(json!({"workspace":item})),
                    Err(_) => CallToolResult::structured_error(
                        json!({"error":"unavailable_or_forbidden"}),
                    ),
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
            "account.invitation.cancel.v1" => {
                let input: InvitationCancel = serde_json::from_value(input)
                    .map_err(|_| ErrorData::invalid_params("Invalid invitation", None))?;
                let actor = krausen_app::ActorContext {
                    actor_id: principal.user_id.clone(),
                    credential_id: principal.credential_id.clone(),
                    access_expires_at: principal.access_expires_at,
                    workspace_id: input.workspace_id,
                };
                let outcome = krausen_app::team::project_change(
                    krausen_app::invitation::cancel(
                        &self.db,
                        &actor,
                        &input.invitation_id,
                        input.expected_revision,
                        input.request_id,
                    )
                    .await,
                );
                let is_error = !matches!(
                    outcome,
                    krausen_types::account::TeamChangeOutcome::Saved { .. }
                );
                let result = json!({"outcome":outcome});
                if is_error {
                    CallToolResult::structured_error(result)
                } else {
                    CallToolResult::structured(result)
                }
            }
            "account.invitation.discovery.begin.v1" => {
                let input: DiscoveryBegin = serde_json::from_value(input)
                    .map_err(|_| ErrorData::invalid_params("Invalid discovery request", None))?;
                let actor = krausen_app::ActorContext {
                    actor_id: principal.user_id.clone(),
                    credential_id: principal.credential_id.clone(),
                    access_expires_at: principal.access_expires_at,
                    workspace_id: String::new(),
                };
                match krausen_app::invitation::begin_discovery(
                    &self.db,
                    &actor,
                    input.request_id,
                    self.mail_available,
                )
                .await
                {
                    Ok(pending) => CallToolResult::structured(
                        serde_json::to_value(pending)
                            .map_err(|_| ErrorData::internal_error("Result unavailable", None))?,
                    ),
                    Err(krausen_app::Error::DeliveryUnavailable) => {
                        CallToolResult::structured_error(json!({"error":"delivery_unavailable"}))
                    }
                    Err(_) => CallToolResult::structured_error(
                        json!({"error":"unavailable_or_forbidden"}),
                    ),
                }
            }
            "account.invitation.list.v1" => {
                let input: InteractionQuery = serde_json::from_value(input)
                    .map_err(|_| ErrorData::invalid_params("Invalid interaction", None))?;
                let actor = krausen_app::ActorContext {
                    actor_id: principal.user_id.clone(),
                    credential_id: principal.credential_id.clone(),
                    access_expires_at: principal.access_expires_at,
                    workspace_id: String::new(),
                };
                match krausen_app::invitation::list(&self.db, &actor, &input.interaction_id).await {
                    Ok(invitations) => {
                        CallToolResult::structured(json!({"invitations":invitations}))
                    }
                    Err(_) => CallToolResult::structured_error(
                        json!({"error":"unavailable_or_forbidden"}),
                    ),
                }
            }
            "account.invitation.proof.begin.v1" => {
                let input: InvitationBegin = serde_json::from_value(input)
                    .map_err(|_| ErrorData::invalid_params("Invalid invitation response", None))?;
                let actor = krausen_app::ActorContext {
                    actor_id: principal.user_id.clone(),
                    credential_id: principal.credential_id.clone(),
                    access_expires_at: principal.access_expires_at,
                    workspace_id: input.workspace_id,
                };
                match krausen_app::invitation::begin(
                    &self.db,
                    &actor,
                    &input.invitation_id,
                    &input.discovery_id,
                    input.action,
                    input.request_id,
                    self.mail_available,
                )
                .await
                {
                    Ok(pending) => CallToolResult::structured(
                        serde_json::to_value(pending)
                            .map_err(|_| ErrorData::internal_error("Result unavailable", None))?,
                    ),
                    Err(krausen_app::Error::DeliveryUnavailable) => {
                        CallToolResult::structured_error(json!({"error":"delivery_unavailable"}))
                    }
                    Err(_) => CallToolResult::structured_error(
                        json!({"error":"unavailable_or_forbidden"}),
                    ),
                }
            }
            "account.invitation.proof.status.v1" => {
                let input: InteractionQuery = serde_json::from_value(input)
                    .map_err(|_| ErrorData::invalid_params("Invalid interaction", None))?;
                let actor = krausen_app::ActorContext {
                    actor_id: principal.user_id.clone(),
                    credential_id: principal.credential_id.clone(),
                    access_expires_at: principal.access_expires_at,
                    workspace_id: String::new(),
                };
                match krausen_app::invitation::status(&self.db, &actor, &input.interaction_id).await
                {
                    Ok(status) => CallToolResult::structured(
                        serde_json::to_value(status)
                            .map_err(|_| ErrorData::internal_error("Result unavailable", None))?,
                    ),
                    Err(_) => CallToolResult::structured_error(
                        json!({"error":"unavailable_or_forbidden"}),
                    ),
                }
            }
            "account.profile.get.v1" => {
                if input.as_object().is_none_or(|object| !object.is_empty()) {
                    return Err(ErrorData::invalid_params("Invalid profile query", None));
                }
                let actor = krausen_app::ActorContext {
                    actor_id: principal.user_id.clone(),
                    credential_id: principal.credential_id.clone(),
                    access_expires_at: principal.access_expires_at,
                    workspace_id: String::new(),
                };
                match krausen_app::account::profile(&self.db, &actor).await {
                    Ok(profile) => CallToolResult::structured(
                        serde_json::to_value(profile)
                            .map_err(|_| ErrorData::internal_error("Result unavailable", None))?,
                    ),
                    Err(_) => CallToolResult::structured_error(
                        json!({"error":"unavailable_or_forbidden"}),
                    ),
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
            _ => return Err(ErrorData::invalid_params("Unknown tool", None)),
        };
        Ok(result.into())
    }
}

pub fn routes(state: AppState) -> Router<AppState> {
    let db = state.db.clone();
    let mail_available = krausen_app::invitation::delivery_available(
        krausen_auth::email_service::EmailService::from_env().is_configured(),
        &state.config.base_url,
    );
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
        move || {
            Ok(McpHandler {
                db: db.clone(),
                mail_available,
            })
        },
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
