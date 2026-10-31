// SPDX-License-Identifier: AGPL-3.0-or-later

//! Workspace settings page — admin-only workspace configuration.
//!
//! All data fetching uses server functions instead of REST API calls.

use leptos::prelude::*;

use crate::components::{
    Card, CardContent, CardDescription, CardHeader, CardTitle, INPUT_CLASS, Skeleton,
};
use crate::server_fns::workspace::*;
use crate::types::WorkspaceSettingsData;
use krausen_types::workspace::WorkspaceRenameOutcome;

// ─────────────────────────────────────────────────────────────────────────────
// Main page
// ─────────────────────────────────────────────────────────────────────────────

#[component]
pub fn WorkspacePage() -> impl IntoView {
    let settings = Resource::new(|| (), |_| get_workspace_settings());

    view! {
        <div class="p-4 sm:p-6">
            <h2 class="text-xl font-display text-foreground mb-4">"Workspace Settings"</h2>
            <p class="text-muted-foreground mb-6">
                "Configure workspace-wide preferences (admin only)."
            </p>

            <Transition fallback=move || view! {
                <div class="space-y-6">
                    <Card>
                        <CardHeader>
                            <Skeleton class="h-5 w-1/3"/>
                            <Skeleton class="h-4 w-2/3 mt-1"/>
                        </CardHeader>
                        <CardContent>
                            <Skeleton class="h-10 w-full"/>
                        </CardContent>
                    </Card>
                    <Card>
                        <CardHeader>
                            <Skeleton class="h-5 w-1/3"/>
                            <Skeleton class="h-4 w-2/3 mt-1"/>
                        </CardHeader>
                        <CardContent>
                            <Skeleton class="h-20 w-full"/>
                        </CardContent>
                    </Card>
                </div>
            }>
                {move || Suspend::new(async move {
                    match settings.await {
                        Ok(data) => {
                            view! {
                                <div class="space-y-6">
                                    <WorkspaceNameCard data=data/>
                                </div>
                            }.into_any()
                        },
                        Err(e) => {
                            let msg = e.to_string();
                            view! {
                                <Card>
                                    <div class="p-6">
                                        <p class="text-error-foreground">"Failed to load workspace settings: " {msg}</p>
                                    </div>
                                </Card>
                            }.into_any()
                        },
                    }
                })}
            </Transition>
        </div>
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Workspace Name Card
// ─────────────────────────────────────────────────────────────────────────────

#[component]
fn WorkspaceNameCard(data: WorkspaceSettingsData) -> impl IntoView {
    let (name, set_name) = signal(data.workspace_name.clone());
    let (revision, set_revision) = signal(data.revision);
    let (identity_error, set_identity_error) = signal(false);
    let pending_request = RwSignal::new(None::<(String, i64, String)>);
    let save_action = Action::new(|input: &(String, i64, String)| {
        let (name, revision, request_id) = input.clone();
        async move { update_workspace_name(name, revision, request_id).await }
    });
    Effect::new(move |_| {
        if let Some(Ok(WorkspaceRenameOutcome::Saved { revision })) = save_action.value().get() {
            set_revision.set(revision);
            pending_request.set(None);
        }
    });

    let on_blur = move |_| {
        if save_action.pending().get_untracked() {
            return;
        }
        let current = name.get();
        let rev = revision.get_untracked();
        if current.trim().is_empty() {
            return;
        }
        let request = pending_request.get_untracked();
        let id = match request {
            Some((ref old, old_rev, ref id)) if *old == current && old_rev == rev => id.clone(),
            _ => match crate::utils::request_id::new_request_id() {
                Some(id) => id,
                None => {
                    set_identity_error.set(true);
                    return;
                }
            },
        };
        set_identity_error.set(false);
        pending_request.set(Some((current.clone(), rev, id.clone())));
        save_action.dispatch((current, rev, id));
    };

    view! {
        <Card>
            <CardHeader>
                <div class="flex items-center justify-between">
                    <div>
                        <CardTitle>"Workspace Name"</CardTitle>
                        <CardDescription>
                            "Give your workspace a meaningful name to help identify it."
                        </CardDescription>
                    </div>
                    <span class="text-sm">{move || {
                        if identity_error.get() { return "Secure request identity unavailable; save did not start.".to_string(); }
                        if save_action.pending().get() { return "Saving…".to_string(); }
                        match save_action.value().get() {
                            None => String::new(),
                            Some(Ok(WorkspaceRenameOutcome::Saved { .. })) => "Saved".into(),
                            Some(Ok(WorkspaceRenameOutcome::Conflict { expected, actual })) => format!("Changed elsewhere (expected revision {expected}, current {actual}); reload before saving."),
                            Some(Ok(WorkspaceRenameOutcome::RequestIdentityConflict)) => "Save request conflicts with an earlier request.".into(),
                            Some(Ok(WorkspaceRenameOutcome::ResultExpired { .. })) => "Save result expired; reload to check the workspace.".into(),
                            Some(Ok(WorkspaceRenameOutcome::Forbidden)) => "Access denied.".into(),
                            Some(Ok(WorkspaceRenameOutcome::Invalid)) => "Invalid workspace name or request.".into(),
                            Some(Ok(WorkspaceRenameOutcome::Retryable)) => "Temporarily unavailable; retry the save.".into(),
                            Some(Ok(WorkspaceRenameOutcome::Unavailable)) | Some(Err(_)) => "Save could not be confirmed; retry the same save.".into(),
                        }
                    }}</span>
                </div>
            </CardHeader>
            <CardContent>
                <input
                    type="text"
                    class=INPUT_CLASS
                    placeholder="My Workspace"
                    prop:value=name
                    prop:disabled=move || save_action.pending().get()
                    on:input=move |ev| set_name.set(event_target_value(&ev))
                    on:blur=on_blur
                />
            </CardContent>
        </Card>
    }
}
