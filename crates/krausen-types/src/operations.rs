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
pub const OPERATIONS: &[Operation] = &[
    WORKSPACE_GET,
    WORKSPACE_RENAME,
    RECIPE_RECOVER,
    RECIPE_CREATE,
    RECIPE_IMPORT,
    FORMULATION_PREVIEW,
    STYLE_LIST,
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
