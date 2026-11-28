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
use krausen_auth::{
    jwt,
    middleware::{AuthUser, Principal},
};
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
#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct ProcessSummaryQuery {
    workspace_id: String,
    request: krausen_types::process_summary::ProcessSummaryRequest,
}
#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct BrewStart {
    workspace_id: String,
    request: krausen_types::brew::StartBrewRequest,
}
#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct BrewEvidenceViewInput {
    workspace_id: String,
    request: krausen_types::brew::BrewEvidenceViewRequest,
}
#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct BrewMeasurementInput {
    workspace_id: String,
    request: krausen_types::brew::MeasurementCommand,
}
#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct BrewNoteInput {
    workspace_id: String,
    request: krausen_types::brew::NoteCommand,
}
#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct BrewActualInput {
    workspace_id: String,
    request: krausen_types::brew::ActualFormulationCommand,
}
#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct BrewLifecycleInput {
    workspace_id: String,
    request: krausen_types::brew::TransitionBrewRequest,
}
#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct BrewArchiveInput {
    workspace_id: String,
    request: krausen_types::brew::ArchiveBrewRequest,
}
#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct BrewRecipeHistoryInput {
    workspace_id: String,
    request: krausen_types::brew_history::RecipeBrewHistoryQuery,
}
#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct BrewComparisonInput {
    workspace_id: String,
    request: krausen_types::brew_history::BrewComparisonRequest,
}
#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct BrewFeedbackPreviewInput {
    workspace_id: String,
    request: krausen_types::brew_history::BrewFeedbackPreviewRequest,
}
#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct BrewFeedbackApplyInput {
    workspace_id: String,
    request: krausen_types::brew_history::BrewFeedbackApplyRequest,
}
#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct BrewPrintInput {
    workspace_id: String,
    request: krausen_types::brew_history::BrewPrintRequest,
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
            Tool::new(
                operations::BREW_START.mcp_tool,
                "Atomically start a brew from one immutable saved recipe revision and accepted calculation context",
                typed_schema::<BrewStart>(),
            ),
            Tool::new(
                operations::BREW_EVIDENCE_VIEW.mcp_tool,
                "Read a started brew's frozen plan, effective actual formulation, selected readings, notes and lifecycle evidence",
                typed_schema::<BrewEvidenceViewInput>(),
            ),
            Tool::new(
                operations::BREW_MEASUREMENT.mcp_tool,
                "Record, correct, retract, select or list traceable brew measurements with revision guards",
                typed_schema::<BrewMeasurementInput>(),
            ),
            Tool::new(
                operations::BREW_NOTE.mcp_tool,
                "Record, correct, retract or list append-only brew notes",
                typed_schema::<BrewNoteInput>(),
            ),
            Tool::new(
                operations::BREW_ACTUAL.mcp_tool,
                "Read, amend or list complete actual formulation revisions derived from the frozen plan",
                typed_schema::<BrewActualInput>(),
            ),
            Tool::new(
                operations::BREW_LIFECYCLE.mcp_tool,
                "Complete, cancel or reasonedly reopen a brew with its expected aggregate revision",
                typed_schema::<BrewLifecycleInput>(),
            ),
            Tool::new(
                operations::BREW_ARCHIVE.mcp_tool,
                "Archive or unarchive a brew with its expected metadata revision without pruning evidence",
                typed_schema::<BrewArchiveInput>(),
            ),
            Tool::new(
                operations::BREW_RECIPE_HISTORY.mcp_tool,
                "List a bounded explicit recipe brew history including archived evidence",
                typed_schema::<BrewRecipeHistoryInput>(),
            ),
            Tool::new(
                operations::BREW_COMPARISON.mcp_tool,
                "Compare frozen planned targets and formulation against the selected actual formulation and readings",
                typed_schema::<BrewComparisonInput>(),
            ),
            Tool::new(
                operations::BREW_FEEDBACK_PREVIEW.mcp_tool,
                "Preview selected actual-formulation changes as one proposed new recipe revision",
                typed_schema::<BrewFeedbackPreviewInput>(),
            ),
            Tool::new(
                operations::BREW_FEEDBACK_APPLY.mcp_tool,
                "Apply a guarded feedback preview as one new recipe revision without mutating the brew plan or actuals",
                typed_schema::<BrewFeedbackApplyInput>(),
            ),
            Tool::new(
                operations::BREW_PRINT.mcp_tool,
                "Read the frozen historical plan and optional actuals for printing or structured export",
                typed_schema::<BrewPrintInput>(),
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
                    session_iat_us: principal.session_iat_us,
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
                    session_iat_us: principal.session_iat_us,
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
                    session_iat_us: principal.session_iat_us,
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
                    session_iat_us: principal.session_iat_us,
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
                            session_iat_us: principal.session_iat_us,
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
                            session_iat_us: principal.session_iat_us,
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
                            session_iat_us: principal.session_iat_us,
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
                    session_iat_us: principal.session_iat_us,
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
                    session_iat_us: principal.session_iat_us,
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
                    session_iat_us: principal.session_iat_us,
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
                    session_iat_us: principal.session_iat_us,
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
                    session_iat_us: principal.session_iat_us,
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
                    session_iat_us: principal.session_iat_us,
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
                    session_iat_us: principal.session_iat_us,
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
                    session_iat_us: principal.session_iat_us,
                    access_expires_at: principal.access_expires_at,
                    workspace_id: input.workspace_id,
                };
                // A new recipe without a profile takes the workspace default, like the editor.
                let outcome = krausen_app::recipe_editor::save_with_default(
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
                    session_iat_us: principal.session_iat_us,
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
                    session_iat_us: principal.session_iat_us,
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
                    session_iat_us: principal.session_iat_us,
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
                    session_iat_us: principal.session_iat_us,
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
            "brew.start.v1" => {
                let input: BrewStart = serde_json::from_value(input)
                    .map_err(|_| ErrorData::invalid_params("Invalid brew start", None))?;
                let actor = krausen_app::ActorContext {
                    actor_id: principal.user_id.clone(),
                    session_iat_us: principal.session_iat_us,
                    access_expires_at: principal.access_expires_at,
                    workspace_id: input.workspace_id,
                };
                let outcome = krausen_app::brew_start::start(&self.db, &actor, input.request).await;
                let success = matches!(
                    outcome,
                    krausen_types::brew::StartBrewOutcome::Started { .. }
                );
                let value = json!({"outcome": outcome});
                if success {
                    CallToolResult::structured(value)
                } else {
                    CallToolResult::structured_error(value)
                }
            }
            "brew.evidence-view.v1" => {
                let input: BrewEvidenceViewInput = serde_json::from_value(input)
                    .map_err(|_| ErrorData::invalid_params("Invalid brew evidence view", None))?;
                let actor = krausen_app::ActorContext {
                    actor_id: principal.user_id.clone(),
                    session_iat_us: principal.session_iat_us,
                    access_expires_at: principal.access_expires_at,
                    workspace_id: input.workspace_id,
                };
                let outcome =
                    krausen_app::brew_evidence::view(&self.db, &actor, input.request).await;
                let success = matches!(
                    outcome,
                    krausen_types::brew::BrewEvidenceViewOutcome::View(_)
                );
                let value = json!({"outcome": outcome});
                if success {
                    CallToolResult::structured(value)
                } else {
                    CallToolResult::structured_error(value)
                }
            }
            "brew.measurement.v1" => {
                let input: BrewMeasurementInput = serde_json::from_value(input)
                    .map_err(|_| ErrorData::invalid_params("Invalid brew measurement", None))?;
                let actor = krausen_app::ActorContext {
                    actor_id: principal.user_id.clone(),
                    session_iat_us: principal.session_iat_us,
                    access_expires_at: principal.access_expires_at,
                    workspace_id: input.workspace_id,
                };
                let outcome =
                    krausen_app::brew_evidence::measurements(&self.db, &actor, input.request).await;
                let success = matches!(
                    outcome,
                    krausen_types::brew::MeasurementOutcome::Recorded(_)
                        | krausen_types::brew::MeasurementOutcome::Corrected(_)
                        | krausen_types::brew::MeasurementOutcome::Retracted(_)
                        | krausen_types::brew::MeasurementOutcome::Selected(_)
                        | krausen_types::brew::MeasurementOutcome::Page(_)
                );
                let value = json!({"outcome": outcome});
                if success {
                    CallToolResult::structured(value)
                } else {
                    CallToolResult::structured_error(value)
                }
            }
            "brew.note.v1" => {
                let input: BrewNoteInput = serde_json::from_value(input)
                    .map_err(|_| ErrorData::invalid_params("Invalid brew note", None))?;
                let actor = krausen_app::ActorContext {
                    actor_id: principal.user_id.clone(),
                    session_iat_us: principal.session_iat_us,
                    access_expires_at: principal.access_expires_at,
                    workspace_id: input.workspace_id,
                };
                let outcome =
                    krausen_app::brew_evidence::notes(&self.db, &actor, input.request).await;
                let success = matches!(
                    outcome,
                    krausen_types::brew::NoteOutcome::Recorded(_)
                        | krausen_types::brew::NoteOutcome::Corrected(_)
                        | krausen_types::brew::NoteOutcome::Retracted(_)
                        | krausen_types::brew::NoteOutcome::Page(_)
                        | krausen_types::brew::NoteOutcome::History(_)
                );
                let value = json!({"outcome": outcome});
                if success {
                    CallToolResult::structured(value)
                } else {
                    CallToolResult::structured_error(value)
                }
            }
            "brew.actual.v1" => {
                let input: BrewActualInput = serde_json::from_value(input)
                    .map_err(|_| ErrorData::invalid_params("Invalid brew actual", None))?;
                let actor = krausen_app::ActorContext {
                    actor_id: principal.user_id.clone(),
                    session_iat_us: principal.session_iat_us,
                    access_expires_at: principal.access_expires_at,
                    workspace_id: input.workspace_id,
                };
                let outcome =
                    krausen_app::brew_evidence::actual(&self.db, &actor, input.request).await;
                let success = matches!(
                    outcome,
                    krausen_types::brew::ActualFormulationOutcome::Amended(_)
                        | krausen_types::brew::ActualFormulationOutcome::View(_)
                        | krausen_types::brew::ActualFormulationOutcome::History(_)
                );
                let value = json!({"outcome": outcome});
                if success {
                    CallToolResult::structured(value)
                } else {
                    CallToolResult::structured_error(value)
                }
            }
            "brew.lifecycle.v1" => {
                let input: BrewLifecycleInput = serde_json::from_value(input)
                    .map_err(|_| ErrorData::invalid_params("Invalid brew lifecycle", None))?;
                let actor = krausen_app::ActorContext {
                    actor_id: principal.user_id.clone(),
                    session_iat_us: principal.session_iat_us,
                    access_expires_at: principal.access_expires_at,
                    workspace_id: input.workspace_id,
                };
                let outcome =
                    krausen_app::brew_evidence::transition(&self.db, &actor, input.request).await;
                let success = matches!(
                    outcome,
                    krausen_types::brew::BrewLifecycleOutcome::Transitioned { .. }
                );
                let value = json!({"outcome": outcome});
                if success {
                    CallToolResult::structured(value)
                } else {
                    CallToolResult::structured_error(value)
                }
            }
            "brew.archive.v1" => {
                let input: BrewArchiveInput = serde_json::from_value(input)
                    .map_err(|_| ErrorData::invalid_params("Invalid brew archive", None))?;
                let actor = krausen_app::ActorContext {
                    actor_id: principal.user_id.clone(),
                    session_iat_us: principal.session_iat_us,
                    access_expires_at: principal.access_expires_at,
                    workspace_id: input.workspace_id,
                };
                let outcome =
                    krausen_app::brew_evidence::archive(&self.db, &actor, input.request).await;
                let success = matches!(
                    outcome,
                    krausen_types::brew::BrewArchiveOutcome::Archived { .. }
                );
                let value = json!({"outcome": outcome});
                if success {
                    CallToolResult::structured(value)
                } else {
                    CallToolResult::structured_error(value)
                }
            }
            "brew.recipe-history.v1" => {
                let input: BrewRecipeHistoryInput = serde_json::from_value(input)
                    .map_err(|_| ErrorData::invalid_params("Invalid brew history", None))?;
                let actor = krausen_app::ActorContext {
                    actor_id: principal.user_id.clone(),
                    session_iat_us: principal.session_iat_us,
                    access_expires_at: principal.access_expires_at,
                    workspace_id: input.workspace_id,
                };
                let outcome =
                    krausen_app::brew_history::history(&self.db, &actor, input.request).await;
                let success = matches!(
                    outcome,
                    krausen_types::brew_history::RecipeBrewHistoryOutcome::History(_)
                );
                let value = json!({"outcome": outcome});
                if success {
                    CallToolResult::structured(value)
                } else {
                    CallToolResult::structured_error(value)
                }
            }
            "brew.comparison.v1" => {
                let input: BrewComparisonInput = serde_json::from_value(input)
                    .map_err(|_| ErrorData::invalid_params("Invalid brew comparison", None))?;
                let actor = krausen_app::ActorContext {
                    actor_id: principal.user_id.clone(),
                    session_iat_us: principal.session_iat_us,
                    access_expires_at: principal.access_expires_at,
                    workspace_id: input.workspace_id,
                };
                let outcome =
                    krausen_app::brew_history::comparison(&self.db, &actor, input.request).await;
                let success = matches!(
                    outcome,
                    krausen_types::brew_history::BrewComparisonOutcome::Comparison(_)
                );
                let value = json!({"outcome": outcome});
                if success {
                    CallToolResult::structured(value)
                } else {
                    CallToolResult::structured_error(value)
                }
            }
            "brew.feedback-preview.v1" => {
                let input: BrewFeedbackPreviewInput =
                    serde_json::from_value(input).map_err(|_| {
                        ErrorData::invalid_params("Invalid brew feedback preview", None)
                    })?;
                let actor = krausen_app::ActorContext {
                    actor_id: principal.user_id.clone(),
                    session_iat_us: principal.session_iat_us,
                    access_expires_at: principal.access_expires_at,
                    workspace_id: input.workspace_id,
                };
                let outcome =
                    krausen_app::brew_history::feedback_preview(&self.db, &actor, input.request)
                        .await;
                let success = matches!(
                    outcome,
                    krausen_types::brew_history::BrewFeedbackPreviewOutcome::Preview(_)
                );
                let value = json!({"outcome": outcome});
                if success {
                    CallToolResult::structured(value)
                } else {
                    CallToolResult::structured_error(value)
                }
            }
            "brew.feedback-apply.v1" => {
                let input: BrewFeedbackApplyInput = serde_json::from_value(input)
                    .map_err(|_| ErrorData::invalid_params("Invalid brew feedback apply", None))?;
                let actor = krausen_app::ActorContext {
                    actor_id: principal.user_id.clone(),
                    session_iat_us: principal.session_iat_us,
                    access_expires_at: principal.access_expires_at,
                    workspace_id: input.workspace_id,
                };
                let outcome =
                    krausen_app::brew_history::feedback_apply(&self.db, &actor, input.request)
                        .await;
                let success = matches!(
                    outcome,
                    krausen_types::brew_history::BrewFeedbackApplyOutcome::Applied { .. }
                        | krausen_types::brew_history::BrewFeedbackApplyOutcome::Unchanged { .. }
                );
                let value = json!({"outcome": outcome});
                if success {
                    CallToolResult::structured(value)
                } else {
                    CallToolResult::structured_error(value)
                }
            }
            "brew.print.v1" => {
                let input: BrewPrintInput = serde_json::from_value(input)
                    .map_err(|_| ErrorData::invalid_params("Invalid brew print", None))?;
                let actor = krausen_app::ActorContext {
                    actor_id: principal.user_id.clone(),
                    session_iat_us: principal.session_iat_us,
                    access_expires_at: principal.access_expires_at,
                    workspace_id: input.workspace_id,
                };
                let outcome =
                    krausen_app::brew_history::historical_print(&self.db, &actor, input.request)
                        .await;
                let success = matches!(
                    outcome,
                    krausen_types::brew_history::BrewPrintOutcome::Print(_)
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
    // The bearer token is the authority on /mcp. rmcp's default loopback host
    // allow-list would refuse any real hostname, so rmcp's transport-level Host
    // and Origin checks are switched off.
    let config = StreamableHttpServerConfig::default()
        .with_legacy_session_mode(true)
        .disable_allowed_hosts()
        .disable_allowed_origins();
    let service = StreamableHttpService::new(
        move || Ok(McpHandler { db: db.clone() }),
        Arc::new(LocalSessionManager::default()),
        config,
    );
    Router::new()
        .route_service("/", service)
        .layer(middleware::from_fn_with_state(state.clone(), authenticate))
        .layer(middleware::from_fn_with_state(state, mcp_www_authenticate_layer))
        .route(
            "/.well-known/openid-configuration",
            axum::routing::get(super::oauth::mcp_openid_configuration),
        )
}

/// Response layer that adds `WWW-Authenticate` to 401 responses per RFC 6750.
///
/// MCP clients use this header to discover OAuth endpoints and distinguish
/// "online, requires auth" from "broken/offline". The `resource_metadata`
/// parameter points to the RFC 9728 protected resource metadata endpoint,
/// which in turn references the authorization server.
async fn mcp_www_authenticate_layer(
    State(state): State<AppState>,
    request: Request<Body>,
    next: Next,
) -> Response {
    // Capture the request's host/scheme before passing ownership to `next`.
    let base_url = {
        let headers = request.headers();
        let scheme = headers
            .get("x-forwarded-proto")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("https");
        let host = headers
            .get("host")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_else(|| {
                state
                    .config
                    .base_url
                    .trim_start_matches("https://")
                    .trim_start_matches("http://")
            });
        format!("{scheme}://{host}")
    };

    let mut response = next.run(request).await;

    if response.status() == StatusCode::UNAUTHORIZED {
        let www_auth = format!(
            r#"Bearer realm="OAuth", resource_metadata="{base_url}/.well-known/oauth-protected-resource", error="invalid_token", error_description="Missing or invalid access token""#
        );
        if let Ok(val) = www_auth.parse() {
            response.headers_mut().insert("www-authenticate", val);
        }
    }

    response
}

/// Authenticate the caller of `/mcp`: an MCP OAuth access token (signed JWT,
/// `purpose = mcp`), or a browser session. Anything else is a 401.
async fn authenticate(
    State(state): State<AppState>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let (mut parts, body) = request.into_parts();
    let bearer = parts
        .headers
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|v| v.split_once(' '))
        .and_then(|(scheme, value)| scheme.eq_ignore_ascii_case("bearer").then_some(value));
    let mcp_claims = bearer
        .and_then(|token| jwt::validate_token(token, &state.config.jwt_secret).ok())
        .map(|data| data.claims)
        .filter(|claims| claims.purpose == Some(jwt::TokenPurpose::Mcp));

    let principal = match mcp_claims {
        Some(claims) => mcp_principal(&state, &claims).await,
        None => {
            use axum::extract::FromRequestParts;
            AuthUser::from_request_parts(&mut parts, &state)
                .await
                .map(|user| Principal {
                    user_id: user.user_id,
                    session_iat_us: user.session_iat_us,
                    access_expires_at: user.token_exp.unwrap_or_default(),
                })
        }
    };
    // Authentication failures are 401; a backend failure is a 500 with the
    // shared JSON error body, so a transient database error never makes a
    // client discard a valid token.
    let principal = match principal {
        Ok(principal) => principal,
        Err(error) => return error.into_response(),
    };
    parts.extensions.insert(principal);
    next.run(Request::from_parts(parts, body)).await
}

/// An MCP access token names its user; the account must still exist and be
/// active and, when the token names a workspace, still a member of it. The token
/// has no session authority, so the browser session cutoff does not apply to it.
async fn mcp_principal(
    state: &AppState,
    claims: &jwt::Claims,
) -> Result<Principal, krausen_core::Error> {
    let user = krausen_auth::user_service::get_user_by_id(&state.db, &claims.sub)
        .await
        .map_err(|e| {
            tracing::error!("database error loading user: {e}");
            krausen_core::Error::Internal("database error".into())
        })?
        .ok_or_else(|| krausen_core::Error::Unauthorized("User not found".into()))?;
    if !user.active {
        return Err(krausen_core::Error::Unauthorized(
            "User account is inactive".into(),
        ));
    }
    // As the browser extractor: a member removed from the token's workspace is
    // refused (401); a lookup failure is a 500, never a fall-through.
    if let Some(ws_id) = claims.extra.get("workspace_id").and_then(|v| v.as_str()) {
        match krausen_auth::user_service::get_workspace(&state.db, ws_id).await {
            Ok(Some(_)) => {
                match krausen_auth::user_service::get_workspace_user(&state.db, ws_id, &user.user_id)
                    .await
                {
                    Ok(Some(_)) => {}
                    Ok(None) => {
                        return Err(krausen_core::Error::Unauthorized(
                            "Workspace membership revoked. Please log in again.".into(),
                        ));
                    }
                    Err(e) => {
                        tracing::error!("database error loading workspace membership: {e}");
                        return Err(krausen_core::Error::Internal("database error".into()));
                    }
                }
            }
            Ok(None) => tracing::warn!("workspace {ws_id} not found"),
            Err(e) => {
                tracing::error!("database error loading workspace: {e}");
                return Err(krausen_core::Error::Internal("database error".into()));
            }
        }
    }
    Ok(Principal {
        user_id: user.user_id,
        session_iat_us: None,
        access_expires_at: claims.exp,
    })
}

#[cfg(test)]
mod brew_evidence_adapter_tests {
    use super::*;
    use serde_json::json;

    fn intent() -> krausen_types::request_context::OperationIntent {
        krausen_types::request_context::OperationIntent {
            account_id: "account".into(),
            workspace_id: "workspace".into(),
        }
    }

    #[test]
    fn brew_measurement_accepts_each_tagged_action() {
        for action in ["record", "correct", "retract", "select", "list"] {
            let mut request = json!({
                "workspace_id": "workspace",
                "request": {
                    "action": action,
                    "intent": intent(),
                    "brew_id": "brew"
                }
            });
            let object = request.as_object_mut().unwrap();
            let inner = object.get_mut("request").unwrap().as_object_mut().unwrap();
            match action {
                "record" => {
                    inner.insert("quantity_id".into(), json!("gravity.original.v1"));
                    inner.insert("value".into(), json!(1.050));
                    inner.insert("request_id".into(), json!(uuid::Uuid::new_v4().to_string()));
                }
                "correct" => {
                    inner.insert("entry_id".into(), json!("entry"));
                    inner.insert("value".into(), json!(1.052));
                    inner.insert("request_id".into(), json!(uuid::Uuid::new_v4().to_string()));
                }
                "retract" => {
                    inner.insert("entry_id".into(), json!("entry"));
                    inner.insert("request_id".into(), json!(uuid::Uuid::new_v4().to_string()));
                }
                "select" => {
                    inner.insert("quantity_id".into(), json!("gravity.original.v1"));
                    inner.insert("root_observation_id".into(), json!("entry"));
                    inner.insert("expected_selection_revision".into(), json!(1));
                    inner.insert("request_id".into(), json!(uuid::Uuid::new_v4().to_string()));
                }
                "list" => {}
                _ => unreachable!(),
            }
            let parsed: BrewMeasurementInput = serde_json::from_value(request)
                .unwrap_or_else(|error| panic!("{action} command did not parse: {error}"));
            assert_eq!(parsed.workspace_id, "workspace");
        }
    }

    #[test]
    fn brew_family_inputs_parse_and_are_registered() {
        let note: BrewNoteInput = serde_json::from_value(json!({
            "workspace_id": "workspace",
            "request": {
                "action": "record",
                "intent": intent(),
                "brew_id": "brew",
                "text": "note",
                "request_id": uuid::Uuid::new_v4().to_string()
            }
        }))
        .unwrap();
        assert_eq!(note.workspace_id, "workspace");

        let actual: BrewActualInput = serde_json::from_value(json!({
            "workspace_id": "workspace",
            "request": {
                "action": "get",
                "intent": intent(),
                "brew_id": "brew"
            }
        }))
        .unwrap();
        assert_eq!(actual.workspace_id, "workspace");

        let lifecycle: BrewLifecycleInput = serde_json::from_value(json!({
            "workspace_id": "workspace",
            "request": {
                "intent": intent(),
                "brew_id": "brew",
                "expected_revision": 1,
                "transition": "complete",
                "request_id": uuid::Uuid::new_v4().to_string()
            }
        }))
        .unwrap();
        assert_eq!(lifecycle.workspace_id, "workspace");

        let archive: BrewArchiveInput = serde_json::from_value(json!({
            "workspace_id": "workspace",
            "request": {
                "intent": intent(),
                "brew_id": "brew",
                "archived": true,
                "expected_metadata_revision": 1,
                "request_id": uuid::Uuid::new_v4().to_string()
            }
        }))
        .unwrap();
        assert_eq!(archive.workspace_id, "workspace");

        let view: BrewEvidenceViewInput = serde_json::from_value(json!({
            "workspace_id": "workspace",
            "request": {"intent": intent(), "brew_id": "brew"}
        }))
        .unwrap();
        assert_eq!(view.workspace_id, "workspace");

        for tool in [
            "brew_evidence_view",
            "brew_measurement",
            "brew_note",
            "brew_actual",
            "brew_lifecycle",
            "brew_archive",
        ] {
            assert!(krausen_types::operations::by_tool(tool).is_some());
        }
    }
}
