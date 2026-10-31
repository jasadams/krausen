// SPDX-License-Identifier: AGPL-3.0-or-later

//! Settings page shell — header + tab navigation bar.
//!
//! - Page header: "Settings" title + description
//! - Tab bar: Profile, Security, Workspace, Team
//! - Content area: renders the active tab's component
//!
//! Tab visibility is role-based:
//! - Profile: always visible
//! - Security: hidden in personal mode
//! - Workspace: admin only, hidden in personal mode
//! - Team: admin only, hidden in personal mode

use leptos::prelude::*;
use phosphor_leptos::{Icon, IconWeight};
use leptos_router::components::Outlet;
use leptos_router::hooks::use_location;

use crate::server_fns::context::UserContext;

/// Settings tab definition.
struct SettingsTab {
    id: &'static str,
    name: &'static str,
    icon: phosphor_leptos::IconData,
    /// URL path suffix. Defaults to `id` but can differ.
    path: &'static str,
}

/// All settings tabs (visibility filtered at render time based on user context).
const TABS: &[SettingsTab] = &[
    SettingsTab { id: "profile", name: "Profile", icon: phosphor_leptos::USER, path: "profile" },
    SettingsTab { id: "security", name: "Security", icon: phosphor_leptos::SHIELD, path: "security" },
    SettingsTab { id: "workspace", name: "Workspace", icon: phosphor_leptos::GEAR, path: "workspace" },
    SettingsTab { id: "team", name: "Team", icon: phosphor_leptos::USERS, path: "team" },
];

/// Return the list of tab IDs that should be visible for the given user context.
fn visible_tabs(ctx: &UserContext) -> Vec<&'static str> {
    let is_admin = ctx.workspace_roles.iter().any(|r| r == "workspace_admin");

    let mut tabs = Vec::new();

    tabs.push("profile");

    if !ctx.is_personal_mode {
        tabs.push("security");
    }

    if is_admin && !ctx.is_personal_mode {
        tabs.push("workspace");
    }

    if is_admin && !ctx.is_personal_mode {
        tabs.push("team");
    }

    tabs
}

/// Settings shell component — wraps settings tab content.
///
/// Reads the shared `UserContext` resource provided by the app root via
/// `expect_context`. Settings tabs continue to read it with
/// `expect_context::<LocalResource<Result<UserContext, ServerFnError>>>()`.
#[component]
pub fn SettingsShell() -> impl IntoView {
    // The user context resource is provided by the app root — one fetch
    // per session, shared across every authed page. Settings tabs continue to
    // read it via `expect_context`, so this shell just re-exposes the parent
    // resource (no-op from the tabs' perspective) without refetching.
    let user_ctx = expect_context::<LocalResource<Result<UserContext, ServerFnError>>>();

    // Reactive pathname from the Leptos router — updates when the URL changes.
    let location = use_location();
    let active_tab = Memo::new(move |_| {
        let path = location.pathname.get();
        path.strip_prefix("/settings/")
            .unwrap_or("profile")
            .to_string()
    });

    view! {
        <div class="w-full space-y-8" style:display="block">
            // Settings Header
            <div class="mb-8">
                <h1 class="text-3xl font-display text-foreground">"Settings"</h1>
                <p class="text-muted-foreground mt-2">"Manage your workspace configuration"</p>
            </div>

            // Settings Navigation Tabs
            //
            // The tab structure is rendered once when user_ctx loads. Each tab's
            // active class is an independent reactive signal so clicking a tab
            // only updates CSS classes — no DOM teardown/rebuild, no flicker.
            <div class="w-full bg-card rounded-lg shadow border border-border mb-6 overflow-hidden">
                <div class="border-b border-border overflow-x-auto scrollbar-thin scrollbar-thumb-muted-foreground/30 scrollbar-track-transparent">
                    <div class="flex space-x-4 md:space-x-8 px-4 md:px-6 min-w-max">
                        <Transition fallback=move || {
                            // While user_ctx loads, show always-visible tabs.
                            TABS.iter()
                                .filter(|tab| matches!(tab.id, "profile" | "security" | "workspace" | "team"))
                                .map(|tab| {
                                    let tab_path = tab.path;
                                    let tab_class = move || {
                                        if active_tab.get() == tab_path {
                                            "flex items-center space-x-2 py-4 border-b-2 font-medium text-sm transition-colors whitespace-nowrap flex-shrink-0 border-primary text-primary"
                                        } else {
                                            "flex items-center space-x-2 py-4 border-b-2 font-medium text-sm transition-colors whitespace-nowrap flex-shrink-0 border-transparent text-muted-foreground hover:text-foreground hover:border-border"
                                        }
                                    };
                                    let tab_weight = Memo::new(move |_| {
                                        if active_tab.get() == tab_path { IconWeight::Fill } else { IconWeight::Light }
                                    });
                                    let href = format!("/settings/{}", tab.path);
                                    view! {
                                        <a href=href class=tab_class>
                                            <Icon icon=tab.icon weight=tab_weight size="16px"/>
                                            <span>{tab.name}</span>
                                        </a>
                                    }
                                })
                                .collect_view()
                        }>
                            {move || Suspend::new(async move {
                                let visible_ids: Vec<&'static str> = match user_ctx.await {
                                    Ok(ctx) => visible_tabs(&ctx),
                                    _ => vec!["profile", "security", "workspace"],
                                };

                                TABS.iter()
                                    .filter(|tab| visible_ids.contains(&tab.id))
                                    .map(|tab| {
                                        let tab_path = tab.path;
                                        let tab_class = move || {
                                            if active_tab.get() == tab_path {
                                                "flex items-center space-x-2 py-4 border-b-2 font-medium text-sm transition-colors whitespace-nowrap flex-shrink-0 border-primary text-primary"
                                            } else {
                                                "flex items-center space-x-2 py-4 border-b-2 font-medium text-sm transition-colors whitespace-nowrap flex-shrink-0 border-transparent text-muted-foreground hover:text-foreground hover:border-border"
                                            }
                                        };
                                        let tab_weight = Memo::new(move |_| {
                                            if active_tab.get() == tab_path { IconWeight::Fill } else { IconWeight::Light }
                                        });
                                        let href = format!("/settings/{}", tab.path);
                                        view! {
                                            <a href=href class=tab_class>
                                                <Icon icon=tab.icon weight=tab_weight size="16px"/>
                                                <span>{tab.name}</span>
                                            </a>
                                        }
                                    })
                                    .collect_view()
                            })}
                        </Transition>
                    </div>
                </div>
            </div>

            // Settings Content — child route renders here via <Outlet/>
            <div class="w-full bg-card rounded-lg shadow border border-border">
                <Outlet/>
            </div>
        </div>
    }
}
