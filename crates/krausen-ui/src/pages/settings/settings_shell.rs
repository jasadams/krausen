// SPDX-License-Identifier: AGPL-3.0-or-later

//! Settings shell: scoped route navigation with a validated return (IA-008).
//!
//! * Account (applies to you everywhere): Profile, Security.
//! * Workspace (applies to the current workspace): Workspace, Team.
//!
//! These are ordinary links with a labelled navigation landmark and the exact
//! current leaf marked `aria-current="page"`; they are not ARIA tabs. Management
//! options appear only after the verified user context says they apply; while it
//! loads or fails only Profile (always available) is shown. Direct URLs and the
//! shared service's own authorization are unchanged.
//!
//! Return: the `?return=` location is accepted only when it is a same-origin
//! in-app path (see `utils::nav_state::safe_return`); anything else falls back to
//! "Back to recipes". There is no `history.back()`. Sign out lives in the account
//! dropdown and its work/pending guard, not here.

use leptos::prelude::*;
use leptos_router::components::{Outlet, Redirect};
use leptos_router::hooks::{use_location, use_query_map};

use crate::components::wayfinding::PageTitle;
use crate::server_fns::context::UserContext;
use crate::utils::nav_state::{safe_return, settings_href};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Group {
    Account,
    Workspace,
}

/// A Settings destination.
struct SettingsLink {
    name: &'static str,
    path: &'static str,
    group: Group,
}

const LINKS: &[SettingsLink] = &[
    SettingsLink { name: "Profile", path: "profile", group: Group::Account },
    SettingsLink { name: "Security", path: "security", group: Group::Account },
    SettingsLink { name: "Workspace", path: "workspace", group: Group::Workspace },
    SettingsLink { name: "Team", path: "team", group: Group::Workspace },
];

/// Destinations that apply for a *verified* context. Loading/error callers pass
/// `None`, which yields only Profile; nothing administrative is speculated.
fn visible_paths(ctx: Option<&UserContext>) -> Vec<&'static str> {
    let Some(ctx) = ctx else { return vec!["profile"] };
    let is_admin = ctx.workspace_roles.iter().any(|r| r == "workspace_admin");
    let mut paths = vec!["profile"];
    if !ctx.is_personal_mode {
        paths.push("security");
        if is_admin {
            paths.push("workspace");
            paths.push("team");
        }
    }
    paths
}

fn group_copy(group: Group) -> (&'static str, &'static str) {
    match group {
        Group::Account => ("Account", "Your profile and sign-in. These apply to you in every workspace."),
        Group::Workspace => ("Workspace", "These apply to the current workspace and its members."),
    }
}

/// The validated return location carried on the current URL, if any.
fn validated_return(raw: Option<String>) -> Option<String> {
    raw.as_deref().and_then(safe_return)
}

/// `/settings` keeps its redirect and carries the return location through it.
#[component]
pub fn SettingsIndex() -> impl IntoView {
    let query = use_query_map();
    view! {
        {move || {
            let ret = validated_return(query.read().get("return"));
            view! { <Redirect path=settings_href("/settings/profile", ret.as_deref())/> }
        }}
    }
}

#[component]
pub fn SettingsShell() -> impl IntoView {
    // The user context resource is provided by the app root and shared by the
    // settings pages; this shell re-reads it without refetching.
    let user_ctx = expect_context::<LocalResource<Result<UserContext, ServerFnError>>>();
    let location = use_location();
    let query = use_query_map();
    let ret = Memo::new(move |_| validated_return(query.read().get("return")));
    let active = Memo::new(move |_| {
        location
            .pathname
            .get()
            .strip_prefix("/settings/")
            .unwrap_or("profile")
            .to_string()
    });
    let back_href = move || ret.get().unwrap_or_else(|| "/recipes".to_owned());
    let back_label = move || {
        if ret.get().is_some() {
            "Back to where you were"
        } else {
            "Back to recipes"
        }
    };

    let group_nav = move |visible: Vec<&'static str>, group: Group| {
        let links = LINKS
            .iter()
            .filter(|link| link.group == group && visible.contains(&link.path))
            .collect::<Vec<_>>();
        if links.is_empty() {
            return ().into_any();
        }
        let (title, scope) = group_copy(group);
        let heading_id = format!("settings-group-{}", title.to_lowercase());
        let scope_id = format!("{}-scope", heading_id);
        let labelled_by = heading_id.clone();
        let described_by = scope_id.clone();
        view! {
            <section class="settings-nav__group" aria-labelledby=labelled_by aria-describedby=described_by>
                <h2 id=heading_id class="settings-nav__heading">{title}</h2>
                <p id=scope_id class="settings-nav__scope">{scope}</p>
                <ul class="settings-nav__list">
                    {links.into_iter().map(|link| {
                        let path = link.path;
                        view! {
                            <li>
                                <a
                                    class="settings-nav__link"
                                    href=move || settings_href(&format!("/settings/{path}"), ret.get().as_deref())
                                    aria-current=move || (active.get() == path).then_some("page")
                                >{link.name}</a>
                            </li>
                        }
                    }).collect_view()}
                </ul>
            </section>
        }
        .into_any()
    };

    view! {
        <PageTitle text=Signal::derive(move || {
            let leaf = LINKS.iter().find(|link| link.path == active.get()).map(|link| link.name);
            match leaf { Some(name) => format!("{name} · Settings"), None => "Settings".to_owned() }
        })/>
        <div class="settings-shell">
            <div class="settings-shell__header">
                <div>
                    <h1 id="settings-heading" tabindex="-1" class="text-3xl font-display text-foreground">"Settings"</h1>
                    <p class="text-muted-foreground mt-2">"Manage your account and this workspace."</p>
                </div>
                <a id="settings-return" class="settings-shell__return" href=back_href>{back_label}</a>
            </div>

            <nav class="settings-nav" aria-label="Settings">
                <Transition fallback=|| view! { <p class="settings-nav__status" role="status">"Checking which settings apply…"</p> }>
                    {move || Suspend::new(async move {
                        let verified = user_ctx.await.ok();
                        let visible = visible_paths(verified.as_ref());
                        view! {
                            {group_nav(visible.clone(), Group::Account)}
                            {group_nav(visible, Group::Workspace)}
                        }
                    })}
                </Transition>
            </nav>

            <div class="settings-shell__content w-full bg-card rounded-lg shadow border border-border">
                <Outlet/>
            </div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(roles: &[&str], personal: bool) -> UserContext {
        UserContext {
            user_id: "u".into(),
            email: "u@example.test".into(),
            name: None,
            workspace_id: Some("w".into()),
            workspace_name: None,
            workspace_roles: roles.iter().map(|r| r.to_string()).collect(),
            is_owner: false,
            is_personal_mode: personal,
            is_self_hosted: true,
        }
    }

    #[test]
    fn unverified_context_never_speculates_management_links() {
        assert_eq!(visible_paths(None), vec!["profile"]);
    }

    #[test]
    fn verified_policy_selects_links_without_hiding_member_reads() {
        assert_eq!(visible_paths(Some(&ctx(&["workspace_member"], false))), vec!["profile", "security"]);
        assert_eq!(
            visible_paths(Some(&ctx(&["workspace_admin"], false))),
            vec!["profile", "security", "workspace", "team"]
        );
        assert_eq!(visible_paths(Some(&ctx(&["workspace_admin"], true))), vec!["profile"]);
    }

    #[test]
    fn return_is_validated_and_never_external() {
        assert_eq!(validated_return(Some("/recipes?q=a".into())).as_deref(), Some("/recipes?q=a"));
        assert_eq!(validated_return(Some("https://evil.example/".into())), None);
        assert_eq!(validated_return(Some("//evil.example".into())), None);
        assert_eq!(validated_return(None), None);
    }
}
