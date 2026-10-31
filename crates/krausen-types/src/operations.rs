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
pub const ACCOUNT_PROFILE_GET: Operation = Operation {
    id: "account.profile.get.v1",
    mcp_tool: "account_profile_get",
    input_contract: "empty.v1",
    output_contract: "account-profile.v1",
};
pub const ACCOUNT_CONTEXT: Operation = Operation {
    id: "account.context.get.v1",
    mcp_tool: "account_context",
    input_contract: "account-context-query.v1",
    output_contract: "account-context.v1",
};
pub const OWNERSHIP_LIST: Operation = Operation {
    id: "workspace.ownership.list.v1",
    mcp_tool: "ownership_list",
    input_contract: "workspace-id.v1",
    output_contract: "ownership-list.v1",
};
pub const OWNERSHIP_GET: Operation = Operation {
    id: "workspace.ownership.get.v1",
    mcp_tool: "ownership_get",
    input_contract: "ownership-id.v1",
    output_contract: "ownership-detail.v1",
};
pub const OWNERSHIP_START: Operation = Operation {
    id: "workspace.ownership.start.v1",
    mcp_tool: "ownership_start",
    input_contract: "ownership-start.v1",
    output_contract: "ownership-start-outcome.v1",
};
pub const OWNERSHIP_CANCEL: Operation = Operation {
    id: "workspace.ownership.cancel.v1",
    mcp_tool: "ownership_cancel",
    input_contract: "ownership-cancel.v1",
    output_contract: "team-change-outcome.v1",
};
pub const OWNERSHIP_RESPONSE: Operation = Operation {
    id: "workspace.ownership.response.begin.v1",
    mcp_tool: "ownership_response_begin",
    input_contract: "ownership-response-begin.v1",
    output_contract: "ownership-response-outcome.v1",
};
pub const OWNERSHIP_STATUS: Operation = Operation {
    id: "workspace.ownership.response.status.v1",
    mcp_tool: "ownership_response_status",
    input_contract: "interaction-id.v1",
    output_contract: "interaction-status.v1",
};
pub const SESSIONS_LIST: Operation = Operation {
    id: "account.sessions.list.v1",
    mcp_tool: "account_sessions",
    input_contract: "empty.v1",
    output_contract: "account-sessions.v1",
};
pub const SESSION_REVOKE: Operation = Operation {
    id: "account.credential.revoke.v1",
    mcp_tool: "account_session_revoke",
    input_contract: "credential-revoke.v1",
    output_contract: "team-change-outcome.v1",
};
pub const ACCOUNT_PROFILE_SAVE: Operation = Operation {
    id: "account.profile.save.v1",
    mcp_tool: "account_profile_save",
    input_contract: "profile-save.v1",
    output_contract: "profile-save-outcome.v1",
};
pub const TEAM_MEMBERS: Operation = Operation {
    id: "workspace.members.list.v1",
    mcp_tool: "team_members",
    input_contract: "workspace-id.v1",
    output_contract: "team-snapshot.v1",
};
pub const TEAM_CHANGE: Operation = Operation {
    id: "workspace.member.change.v1",
    mcp_tool: "team_member_change",
    input_contract: "team-change.v1",
    output_contract: "team-change-outcome.v1",
};
pub const TEAM_INVITE: Operation = Operation {
    id: "workspace.invitation.create.v1",
    mcp_tool: "team_invite",
    input_contract: "team-invitation-create.v1",
    output_contract: "invitation-create-outcome.v1",
};
pub const TEAM_INVITATIONS: Operation = Operation {
    id: "workspace.invitations.list.v1",
    mcp_tool: "team_invitations",
    input_contract: "workspace-id.v1",
    output_contract: "team-invitations.v1",
};
pub const ACCOUNT_WORKSPACES: Operation = Operation {
    id: "account.workspaces.list.v1",
    mcp_tool: "account_workspaces",
    input_contract: "empty.v1",
    output_contract: "workspace-memberships.v1",
};
pub const ACCOUNT_WORKSPACE_SELECT: Operation = Operation {
    id: "account.workspace.select.v1",
    mcp_tool: "account_workspace_select",
    input_contract: "workspace-selection.v1",
    output_contract: "workspace-membership.v1",
};
pub const INVITATION_PROOF_BEGIN: Operation = Operation {
    id: "account.invitation.proof.begin.v1",
    mcp_tool: "invitation_response_begin",
    input_contract: "invitation-response-begin.v1",
    output_contract: "pending-interaction.v1",
};
pub const INVITATION_PROOF_STATUS: Operation = Operation {
    id: "account.invitation.proof.status.v1",
    mcp_tool: "invitation_response_status",
    input_contract: "interaction-id.v1",
    output_contract: "interaction-status.v1",
};
pub const INVITATION_DISCOVERY_BEGIN: Operation = Operation {
    id: "account.invitation.discovery.begin.v1",
    mcp_tool: "invitation_discovery_begin",
    input_contract: "request-id.v1",
    output_contract: "pending-interaction.v1",
};
pub const INVITATION_LIST: Operation = Operation {
    id: "account.invitation.list.v1",
    mcp_tool: "invitation_list",
    input_contract: "interaction-id.v1",
    output_contract: "invitation-list.v1",
};
pub const INVITATION_CANCEL: Operation = Operation {
    id: "account.invitation.cancel.v1",
    mcp_tool: "invitation_cancel",
    input_contract: "invitation-cancel-request.v1",
    output_contract: "team-change-outcome.v1",
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
pub const PASSWORD_CHANGE: Operation = Operation {
    id: "account.password.change.v1",
    mcp_tool: "account_password_change",
    input_contract: "password-change.v1",
    output_contract: "password-change-outcome.v1",
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
pub const SECURITY_STATUS: Operation = Operation {
    id: "account.security.status.v1",
    mcp_tool: "account_security_status",
    input_contract: "empty.v1",
    output_contract: "security-status.v1",
};
pub const PASSKEYS_LIST: Operation = Operation {
    id: "account.passkeys.list.v1",
    mcp_tool: "account_passkeys",
    input_contract: "empty.v1",
    output_contract: "passkey-summaries.v1",
};
pub const PASSWORD_SET: Operation = Operation {
    id: "account.password.set.v1",
    mcp_tool: "account_password_set",
    input_contract: "password-set.v1",
    output_contract: "password-set-outcome.v1",
};
pub const TOTP_SETUP: Operation = Operation {
    id: "account.totp.setup.v1",
    mcp_tool: "account_totp_setup",
    input_contract: "totp-setup.v1",
    output_contract: "totp-setup-outcome.v1",
};
pub const TOTP_ENABLE: Operation = Operation {
    id: "account.totp.enable.v1",
    mcp_tool: "account_totp_enable",
    input_contract: "totp-enable.v1",
    output_contract: "totp-enable-outcome.v1",
};
pub const TOTP_DISABLE: Operation = Operation {
    id: "account.totp.disable.v1",
    mcp_tool: "account_totp_disable",
    input_contract: "totp-disable.v1",
    output_contract: "totp-disable-outcome.v1",
};
pub const PASSKEY_REGISTER_BEGIN: Operation = Operation {
    id: "account.passkey.register.begin.v1",
    mcp_tool: "account_passkey_register_begin",
    input_contract: "passkey-register-begin.v1",
    output_contract: "passkey-register-begin-outcome.v1",
};
pub const PASSKEY_REGISTER_COMPLETE: Operation = Operation {
    id: "account.passkey.register.complete.v1",
    mcp_tool: "account_passkey_register_complete",
    input_contract: "passkey-register-complete.v1",
    output_contract: "passkey-register-complete-outcome.v1",
};
pub const PASSKEY_RENAME: Operation = Operation {
    id: "account.passkey.rename.v1",
    mcp_tool: "account_passkey_rename",
    input_contract: "passkey-rename.v1",
    output_contract: "passkey-rename-outcome.v1",
};
pub const PASSKEY_DELETE: Operation = Operation {
    id: "account.passkey.delete.v1",
    mcp_tool: "account_passkey_delete",
    input_contract: "passkey-delete.v1",
    output_contract: "passkey-delete-outcome.v1",
};
pub const SESSIONS_REVOKE_ALL: Operation = Operation {
    id: "account.credentials.revoke_all.v1",
    mcp_tool: "account_sessions_revoke_all",
    input_contract: "request-id.v1",
    output_contract: "revoke-all-outcome.v1",
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
    PASSWORD_CHANGE,
    PASSWORD_SET,
    SECURITY_STATUS,
    PASSKEYS_LIST,
    PASSKEY_REGISTER_BEGIN,
    PASSKEY_REGISTER_COMPLETE,
    PASSKEY_RENAME,
    PASSKEY_DELETE,
    TOTP_SETUP,
    TOTP_ENABLE,
    TOTP_DISABLE,
    SESSIONS_REVOKE_ALL,
    SESSIONS_LIST,
    SESSION_REVOKE,
    OWNERSHIP_LIST,
    OWNERSHIP_GET,
    OWNERSHIP_START,
    OWNERSHIP_CANCEL,
    OWNERSHIP_RESPONSE,
    OWNERSHIP_STATUS,
    ACCOUNT_CONTEXT,
    TEAM_INVITATIONS,
    TEAM_INVITE,
    TEAM_MEMBERS,
    TEAM_CHANGE,
    ACCOUNT_PROFILE_SAVE,
    ACCOUNT_WORKSPACES,
    ACCOUNT_WORKSPACE_SELECT,
    INVITATION_CANCEL,
    INVITATION_DISCOVERY_BEGIN,
    INVITATION_LIST,
    INVITATION_PROOF_BEGIN,
    INVITATION_PROOF_STATUS,
    ACCOUNT_PROFILE_GET,
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
