// SPDX-License-Identifier: AGPL-3.0-or-later
//! Versioned shared operation inventory for adapters and discovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Operation {
    pub id: &'static str,
    pub mcp_tool: &'static str,
    pub input_contract: &'static str,
    pub output_contract: &'static str,
}
pub const WORKSPACE_GET: Operation = Operation {
    id: "workspace.get.v1",
    mcp_tool: "workspace_get",
    input_contract: "workspace-id.v1",
    output_contract: "workspace-snapshot.v1",
};
pub const WORKSPACE_RENAME: Operation = Operation {
    id: "workspace.rename.v1",
    mcp_tool: "workspace_rename",
    input_contract: "workspace-rename.v1",
    output_contract: "WorkspaceRenameOutcome",
};
pub const RECIPE_RECOVER: Operation = Operation {
    id: "recipe.recover-legacy.v1",
    mcp_tool: "recipe_recover_legacy",
    input_contract: "legacy-recovery.v1",
    output_contract: "legacy-source-and-diagnostic.v1",
};
pub const RECIPE_CREATE: Operation = Operation {
    id: "recipe.create-legacy.v1",
    mcp_tool: "recipe_create",
    input_contract: "legacy-authoring.v1",
    output_contract: "legacy-authoring-unavailable.v1",
};
pub const RECIPE_IMPORT: Operation = Operation {
    id: "recipe.import-legacy.v1",
    mcp_tool: "recipe_import",
    input_contract: "legacy-authoring.v1",
    output_contract: "legacy-authoring-unavailable.v1",
};
pub const FORMULATION_PREVIEW: Operation = Operation {
    id: "formulation.preview.v1",
    mcp_tool: "formulation_preview",
    input_contract: "formulation-preview-request.v1",
    output_contract: "PreviewOutcome",
};
pub const STYLE_LIST: Operation = Operation {
    id: "formulation.styles.v1",
    mcp_tool: "formulation_styles",
    input_contract: "formulation-style-query.v1",
    output_contract: "StyleListOutcome",
};
pub const RECIPE_READ: Operation = Operation {
    id: "recipe.read.v1",
    mcp_tool: "recipe_read",
    input_contract: "recipe-read.v1",
    output_contract: "RecipeReadOutcome",
};
pub const RECIPE_SAVE: Operation = Operation {
    id: "recipe.save.v1",
    mcp_tool: "recipe_save",
    input_contract: "recipe-save.v1",
    output_contract: "RecipeSaveOutcome",
};
pub const RECIPE_PROCESS_SUMMARY: Operation = Operation {
    id: "recipe.process-summary.v1",
    mcp_tool: "recipe_process_summary",
    input_contract: "ProcessSummaryRequest",
    output_contract: "ProcessSummaryOutcome",
};
pub const RECIPE_SCALE_PREVIEW: Operation = Operation {
    id: "recipe.scale-preview.v1",
    mcp_tool: "recipe_scale_preview",
    input_contract: "scale-preview-request.v1",
    output_contract: "ScaleOutcome",
};
pub const RECIPE_SCALE_APPLY: Operation = Operation {
    id: "recipe.scale-apply.v1",
    mcp_tool: "recipe_scale_apply",
    input_contract: "scale-apply-request.v1",
    output_contract: "ScaleApplyOutcome",
};
pub const RECIPE_DUPLICATE: Operation = Operation {
    id: "recipe.duplicate.v1",
    mcp_tool: "recipe_duplicate",
    input_contract: "duplicate-recipe.v1",
    output_contract: "DuplicateOutcome",
};
pub const BEERXML_PREVIEW: Operation = Operation {
    id: "beerxml.preview.v1",
    mcp_tool: "beerxml_preview",
    input_contract: "ImportRequest",
    output_contract: "ImportPreviewOutcome",
};
pub const BEERXML_IMPORT: Operation = Operation {
    id: "beerxml.import.v1",
    mcp_tool: "beerxml_import",
    input_contract: "ImportRequest",
    output_contract: "ImportCommitOutcome",
};
pub const BEERXML_EXPORT: Operation = Operation {
    id: "beerxml.export.v1",
    mcp_tool: "beerxml_export",
    input_contract: "ExportRequest",
    output_contract: "ExportOutcome",
};
pub const BEERXML_ORIGINAL: Operation = Operation {
    id: "beerxml.original.v1",
    mcp_tool: "beerxml_original",
    input_contract: "beerxml-original.v1",
    output_contract: "OriginalOutcome",
};
pub const EQUIPMENT_SAVE: Operation = Operation {
    id: "equipment.save.v1",
    mcp_tool: "equipment_save",
    input_contract: "SaveEquipment",
    output_contract: "EquipmentOutcome",
};
pub const EQUIPMENT_ARCHIVE: Operation = Operation {
    id: "equipment.archive.v1",
    mcp_tool: "equipment_archive",
    input_contract: "ArchiveEquipment",
    output_contract: "EquipmentOutcome",
};
pub const EQUIPMENT_GET: Operation = Operation {
    id: "equipment.get.v1",
    mcp_tool: "equipment_get",
    input_contract: "EquipmentQuery",
    output_contract: "EquipmentOutcome",
};
pub const EQUIPMENT_LIST: Operation = Operation {
    id: "equipment.list.v1",
    mcp_tool: "equipment_list",
    input_contract: "EquipmentListQuery",
    output_contract: "EquipmentOutcome",
};
pub const EQUIPMENT_REVISION: Operation = Operation {
    id: "equipment.revision.v1",
    mcp_tool: "equipment_revision",
    input_contract: "EquipmentRevisionQuery",
    output_contract: "EquipmentOutcome",
};
pub const RECIPE_LIST: Operation = Operation {
    id: "recipe.list.v1",
    mcp_tool: "recipe_list",
    input_contract: "LibraryQuery",
    output_contract: "LibraryOutcome",
};
pub const RECIPE_HISTORY: Operation = Operation {
    id: "recipe.history.v1",
    mcp_tool: "recipe_history",
    input_contract: "HistoryQuery",
    output_contract: "LibraryOutcome",
};
pub const RECIPE_REVISION: Operation = Operation {
    id: "recipe.revision.v1",
    mcp_tool: "recipe_revision",
    input_contract: "RevisionQuery",
    output_contract: "LibraryOutcome",
};
pub const RECIPE_ARCHIVE: Operation = Operation {
    id: "recipe.archive.v1",
    mcp_tool: "recipe_archive",
    input_contract: "ArchiveRecipe",
    output_contract: "LibraryOutcome",
};
pub const INGREDIENT_SAVE: Operation = Operation {
    id: "ingredient.save.v1",
    mcp_tool: "ingredient_save",
    input_contract: "SaveIngredientEntry",
    output_contract: "IngredientOutcome",
};
pub const INGREDIENT_ARCHIVE: Operation = Operation {
    id: "ingredient.archive.v1",
    mcp_tool: "ingredient_archive",
    input_contract: "ArchiveIngredientEntry",
    output_contract: "IngredientOutcome",
};
pub const INGREDIENT_GET: Operation = Operation {
    id: "ingredient.get.v1",
    mcp_tool: "ingredient_get",
    input_contract: "IngredientQuery",
    output_contract: "IngredientOutcome",
};
pub const INGREDIENT_SEARCH: Operation = Operation {
    id: "ingredient.search.v1",
    mcp_tool: "ingredient_search",
    input_contract: "SearchCatalogue",
    output_contract: "IngredientOutcome",
};
pub const BREW_START: Operation = Operation {
    id: "brew.start.v1",
    mcp_tool: "brew_start",
    input_contract: "StartBrewRequest",
    output_contract: "StartBrewOutcome",
};
pub const OPERATIONS: &[Operation] = &[
    BEERXML_PREVIEW,
    BEERXML_IMPORT,
    BEERXML_EXPORT,
    BEERXML_ORIGINAL,
    RECIPE_LIST,
    RECIPE_HISTORY,
    RECIPE_REVISION,
    RECIPE_ARCHIVE,
    EQUIPMENT_SAVE,
    EQUIPMENT_ARCHIVE,
    EQUIPMENT_GET,
    EQUIPMENT_LIST,
    EQUIPMENT_REVISION,
    INGREDIENT_SAVE,
    INGREDIENT_ARCHIVE,
    INGREDIENT_GET,
    INGREDIENT_SEARCH,
    WORKSPACE_GET,
    WORKSPACE_RENAME,
    RECIPE_RECOVER,
    RECIPE_CREATE,
    RECIPE_IMPORT,
    FORMULATION_PREVIEW,
    STYLE_LIST,
    RECIPE_READ,
    RECIPE_SAVE,
    RECIPE_PROCESS_SUMMARY,
    BREW_START,
    RECIPE_SCALE_PREVIEW,
    RECIPE_SCALE_APPLY,
    RECIPE_DUPLICATE,
];
pub fn by_tool(name: &str) -> Option<&'static Operation> {
    OPERATIONS
        .iter()
        .find(|operation| operation.mcp_tool == name)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn operation_ids_and_adapter_names_are_unique() {
        for (i, op) in OPERATIONS.iter().enumerate() {
            assert!(
                !op.id.is_empty()
                    && !op.input_contract.is_empty()
                    && !op.output_contract.is_empty()
            );
            assert!(
                OPERATIONS[..i]
                    .iter()
                    .all(|other| other.id != op.id && other.mcp_tool != op.mcp_tool)
            );
            assert_eq!(by_tool(op.mcp_tool), Some(op));
        }
    }
}
