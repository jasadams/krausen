// SPDX-License-Identifier: AGPL-3.0-or-later

//! App layout — sidebar + content area.

use leptos::prelude::*;
use leptos_router::components::Outlet;

use crate::components::Spinner;
use crate::server_fns::sidebar::{
    SidebarUser, get_sidebar_user, list_user_workspaces, switch_workspace,
};

/// A cross-tab invalidation signal; it contains no account or credential data.
pub const CONTEXT_CHANGED_KEY: &str = "krausen-context-changed";

#[derive(Clone)]
pub enum EditorDestination {
    Href(String),
    Workspace(String),
    Logout,
}

/// Prevent an in-progress recipe draft from being discarded by workspace switching.
#[derive(Clone, Copy)]
pub struct EditorGuard {
    pub dirty: RwSignal<bool>,
    pub notice: RwSignal<bool>,
    pub destination: RwSignal<Option<EditorDestination>>,
    pub save: StoredValue<Option<Callback<()>>>,
    pub discard: StoredValue<Option<Callback<(), bool>>>,
    pub saving_to_leave: RwSignal<bool>,
    pub pending: RwSignal<bool>,
    pub message: RwSignal<String>,
    pub navigation_error: RwSignal<String>,
    pub current_url: RwSignal<String>,
}

impl EditorGuard {
    pub fn new() -> Self {
        Self {
            dirty: RwSignal::new(false),
            notice: RwSignal::new(false),
            destination: RwSignal::new(None),
            save: StoredValue::new(None),
            discard: StoredValue::new(None),
            saving_to_leave: RwSignal::new(false),
            pending: RwSignal::new(false),
            message: RwSignal::new(String::new()),
            navigation_error: RwSignal::new(String::new()),
            current_url: RwSignal::new(String::new()),
        }
    }
    /// Install before the router so a dirty form cannot be unmounted by its
    /// earlier popstate handler before the user chooses how to leave.
    #[cfg(target_arch = "wasm32")]
    pub fn install_history_guard(self) {
        use wasm_bindgen::{JsCast, closure::Closure};
        let window = web_sys::window().expect("browser window");
        let pop = Closure::<dyn FnMut(web_sys::PopStateEvent)>::new(
            move |event: web_sys::PopStateEvent| {
                if !self.dirty.get_untracked() {
                    return;
                }
                let Some(window) = web_sys::window() else {
                    tracing::warn!("Browser window unavailable during history navigation");
                    return;
                };
                let Ok(destination) = window.location().href() else {
                    tracing::warn!("Unable to read the browser navigation destination");
                    event.stop_immediate_propagation();
                    let previous = self.current_url.get_untracked();
                    if window
                        .history()
                        .and_then(|history| {
                            history.push_state_with_url(
                                &wasm_bindgen::JsValue::NULL,
                                "",
                                Some(&previous),
                            )
                        })
                        .is_err()
                    {
                        tracing::warn!(
                            "Unable to restore the editor address after browser navigation"
                        );
                    }
                    self.navigation_error.set(
                        "Could not read the navigation destination. Your draft remains open."
                            .into(),
                    );
                    return;
                };
                let previous = self.current_url.get_untracked();
                if previous.is_empty() || destination == previous {
                    return;
                }
                event.stop_immediate_propagation();
                if window
                    .history()
                    .and_then(|history| {
                        history.push_state_with_url(
                            &wasm_bindgen::JsValue::NULL,
                            "",
                            Some(&previous),
                        )
                    })
                    .is_err()
                {
                    tracing::warn!("Unable to restore the editor address after browser navigation");
                    self.navigation_error.set(
                        "Could not restore the editor address. Your draft remains open.".into(),
                    );
                }
                self.request(EditorDestination::Href(destination));
            },
        );
        if window
            .add_event_listener_with_callback("popstate", pop.as_ref().unchecked_ref())
            .is_err()
        {
            tracing::warn!("Unable to install the browser history guard");
            self.navigation_error
                .set("Browser Back protection is unavailable; keep this page open.".into());
        }
        let listener = send_wrapper::SendWrapper::new((window, pop));
        on_cleanup(move || {
            let (window, pop) = &*listener;
            if window
                .remove_event_listener_with_callback("popstate", pop.as_ref().unchecked_ref())
                .is_err()
            {
                tracing::warn!("Unable to remove the browser history guard");
            }
        });
    }
    pub fn request(self, destination: EditorDestination) {
        self.destination.set(Some(destination));
        if self.dirty.get_untracked() {
            self.notice.set(true);
        } else {
            self.finish();
        }
    }
    pub fn finish(self) {
        self.navigation_error.set(String::new());
        self.saving_to_leave.set(false);
        self.notice.set(false);
        let Some(destination) = self.destination.get_untracked() else {
            return;
        };
        self.destination.set(None);
        self.dirty.set(false);
        leptos::task::spawn_local(async move {
            let failure = match destination {
                EditorDestination::Href(href) => web_sys::window()
                    .is_none_or(|w| w.location().set_href(&href).is_err())
                    .then_some("Could not open that destination."),
                EditorDestination::Workspace(id) => {
                    if switch_workspace(id).await.is_err() {
                        Some("Could not switch workspaces.")
                    } else {
                        web_sys::window()
                            .is_none_or(|w| w.location().reload().is_err())
                            .then_some("Workspace changed, but the page could not reload.")
                    }
                }
                EditorDestination::Logout => {
                    if crate::server_fns::security::logout().await.is_err() {
                        Some("Could not sign out.")
                    } else {
                        web_sys::window()
                            .is_none_or(|w| w.location().set_href("/login").is_err())
                            .then_some("Signed out, but the login page could not open.")
                    }
                }
            };
            if let Some(message) = failure {
                tracing::warn!("Unable to complete a requested navigation or account action");
                self.navigation_error.set(message.into());
            }
        });
    }
}

/// Main authenticated layout with sidebar and content area.
#[component]
pub fn Layout() -> impl IntoView {
    let user_info = LocalResource::new(get_sidebar_user);
    let (user_menu_open, set_user_menu_open) = signal(false);
    let mobile_sidebar_open = RwSignal::new(false);
    let editor_guard = expect_context::<EditorGuard>();
    Effect::new(move |_| {
        if !editor_guard.notice.get() {
            editor_guard.saving_to_leave.set(false);
            editor_guard.destination.set(None);
        }
    });

    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::{JsCast, closure::Closure};
        let document = web_sys::window()
            .and_then(|w| w.document())
            .expect("browser document");
        let click =
            Closure::<dyn FnMut(web_sys::MouseEvent)>::new(move |event: web_sys::MouseEvent| {
                if !editor_guard.dirty.get_untracked()
                    || event.default_prevented()
                    || event.button() != 0
                    || event.meta_key()
                    || event.ctrl_key()
                    || event.shift_key()
                    || event.alt_key()
                {
                    return;
                }
                let Some(anchor) = event
                    .target()
                    .and_then(|target| target.dyn_into::<web_sys::Element>().ok())
                    .and_then(|element| match element.closest("a[href]") {
                        Ok(anchor) => anchor,
                        Err(_) => {
                            tracing::warn!("Unable to inspect a navigation link");
                            None
                        }
                    })
                    .and_then(|element| element.dyn_into::<web_sys::HtmlAnchorElement>().ok())
                else {
                    return;
                };
                if !anchor.target().is_empty() || anchor.has_attribute("download") {
                    return;
                }
                let Some(window) = web_sys::window() else {
                    return;
                };
                if anchor.origin() != window.location().origin().unwrap_or_default() {
                    return;
                }
                event.prevent_default();
                event.stop_immediate_propagation();
                editor_guard.request(EditorDestination::Href(anchor.href()));
            });
        if document
            .add_event_listener_with_callback_and_bool(
                "click",
                click.as_ref().unchecked_ref(),
                true,
            )
            .is_err()
        {
            tracing::warn!("Unable to install the recipe navigation guard");
            editor_guard
                .navigation_error
                .set("Navigation protection is unavailable; keep this page open.".into());
        }
        let listener = send_wrapper::SendWrapper::new((document, click));
        on_cleanup(move || {
            let (document, click) = &*listener;
            if document
                .remove_event_listener_with_callback_and_bool(
                    "click",
                    click.as_ref().unchecked_ref(),
                    true,
                )
                .is_err()
            {
                tracing::warn!("Unable to remove the recipe navigation guard");
            }
        });
    }

    #[cfg(target_arch = "wasm32")]
    Effect::new(move |_| {
        if matches!(user_info.get(), Some(Ok(_))) {
            if let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok().flatten())
            {
                if let Err(error) =
                    storage.set_item(CONTEXT_CHANGED_KEY, &js_sys::Date::now().to_string())
                {
                    tracing::warn!(error = ?error, "Unable to notify other tabs of context change");
                }
            }
        }
    });

    let auth_confirmed = RwSignal::new(false);
    let nav = leptos_router::hooks::use_navigate();

    Effect::new(move || match user_info.get() {
        Some(Ok(_)) => auth_confirmed.set(true),
        Some(Err(_)) => {
            nav("/login", Default::default());
        }
        None => {}
    });

    view! {
        <Show
            when=move || auth_confirmed.get()
            fallback=|| view! {
                <div class="min-h-screen flex items-center justify-center">
                    <Spinner/>
                </div>
            }
        >
            <div class="h-dvh flex bg-background">
                // Desktop sidebar
                <div class="hidden md:block print-hide">
                    <Sidebar user_info=user_info user_menu_open=user_menu_open set_user_menu_open=set_user_menu_open/>
                </div>

                <crate::components::BreweryDialog id="workspace-draft-warning" title="Unsaved recipe changes" description="Save, discard, or stay with your draft." open=editor_guard.notice>
                    <p>"Your draft remains in this account and workspace until you choose what to do."</p>
                    <p role="status">{move || editor_guard.message.get()}</p>
                    <crate::components::Button disabled=editor_guard.pending on:click=move |_| {
                        editor_guard.saving_to_leave.set(true);
                        if let Some(save) = editor_guard.save.get_value() { save.run(()); } else { editor_guard.saving_to_leave.set(false); }
                    }>"Save and leave"</crate::components::Button>
                    <crate::components::Button variant=crate::components::ButtonVariant::Secondary disabled=editor_guard.pending on:click=move |_| {
                        if editor_guard.discard.get_value().is_some_and(|discard| discard.run(())) { editor_guard.finish(); }
                    }>"Discard and leave"</crate::components::Button>
                    <crate::components::Button variant=crate::components::ButtonVariant::Secondary on:click=move |_| { editor_guard.destination.set(None); editor_guard.saving_to_leave.set(false); editor_guard.notice.set(false); }>"Stay"</crate::components::Button>
                </crate::components::BreweryDialog>

                <crate::components::BreweryDialog id="mobile-navigation" title="Navigation" description="Choose a destination in your workspace." open=mobile_sidebar_open>
                    <Sidebar user_info=user_info user_menu_open=user_menu_open set_user_menu_open=set_user_menu_open/>
                </crate::components::BreweryDialog>

                <div class="flex-1 flex flex-col overflow-hidden">
                    // Mobile header bar
                    <div class="md:hidden flex items-center gap-3 px-4 py-3 border-b border-border bg-background print-hide">
                        <button
                            class="p-2 rounded-md hover:bg-secondary transition-colors"
                            on:click=move |_| mobile_sidebar_open.set(true)
                            aria-label="Toggle menu"
                            aria-controls="mobile-navigation"
                            aria-expanded=move || mobile_sidebar_open.get().to_string()
                        >
                            <svg class="w-5 h-5 text-foreground" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 6h16M4 12h16M4 18h16"/>
                            </svg>
                        </button>
                        <crate::components::BreweryBrand/>
                    </div>
                    <main class="flex-1 overflow-y-auto">
                        <Show when=move || !editor_guard.navigation_error.get().is_empty()>
                            <p role="alert">{move || editor_guard.navigation_error.get()}</p>
                        </Show>
                        <Outlet/>
                    </main>
                </div>
            </div>
        </Show>
    }
}

/// Sidebar component with navigation and user menu.
#[component]
fn Sidebar(
    user_info: LocalResource<Result<SidebarUser, ServerFnError>>,
    user_menu_open: ReadSignal<bool>,
    set_user_menu_open: WriteSignal<bool>,
) -> impl IntoView {
    let editor_guard = expect_context::<EditorGuard>();

    view! {
        <div class="w-[var(--k-sidebar-width)] bg-[var(--color-sidebar)] border-r border-[var(--color-sidebar-border)] text-[var(--color-sidebar-foreground)] flex flex-col h-full">
            // Logo
            <div class="p-4 border-b border-[var(--color-sidebar-border)]">
                <crate::components::BreweryBrand/>
            </div>

            // Navigation
            <nav class="flex-1 p-3 space-y-1">
                <a href="/recipes" class="flex items-center gap-3 px-3 py-2 rounded-md text-sm text-[var(--color-sidebar-foreground-secondary)] hover:text-[var(--color-sidebar-foreground)] hover:bg-[var(--color-sidebar-hover)] transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring">
                    <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 5h16v14H4zM8 9h8M8 13h8M8 17h5"/></svg>
                    "Recipes"
                </a>
                <a href="/settings/profile" class="flex items-center gap-3 px-3 py-2 rounded-md text-sm text-[var(--color-sidebar-foreground-secondary)] hover:text-[var(--color-sidebar-foreground)] hover:bg-[var(--color-sidebar-hover)] transition-colors">
                    <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.066 2.573c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.573 1.066c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.066-2.573c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z"/>
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z"/>
                    </svg>
                    "Settings"
                </a>
            </nav>

            // User menu at bottom
            <div class="border-t border-[var(--color-sidebar-border)] p-3 relative">
                <Suspense fallback=|| view! {
                    <div class="px-3 py-2 text-sm text-[var(--color-sidebar-foreground-muted)]">"Loading..."</div>
                }>
                    {move || user_info.get().map(|result| {
                        match result {
                            Ok(ref user) => {
                            let display_name = user.name.clone().unwrap_or_else(|| user.email.clone());
                            let avatar_char = user.name.as_ref().and_then(|n| n.chars().next()).unwrap_or('?').to_uppercase().to_string();
                            let ws_name = user.workspace_name.clone().unwrap_or_default();
                            view! {
                                <button
                                    class="w-full flex items-center gap-3 px-3 py-2 rounded-md text-sm hover:bg-[var(--color-sidebar-hover)] transition-colors text-left"
                                    on:click=move |_| set_user_menu_open.update(|v| *v = !*v)
                                >
                                    // Avatar
                                    <div class="w-8 h-8 rounded-full bg-primary flex items-center justify-center text-primary-foreground text-xs font-semibold flex-shrink-0">
                                        {avatar_char.clone()}
                                    </div>
                                    <div class="flex-1 min-w-0">
                                        <div class="text-[var(--color-sidebar-foreground)] font-medium truncate text-sm">
                                            {display_name.clone()}
                                        </div>
                                        <div class="text-[var(--color-sidebar-foreground-muted)] text-xs truncate">
                                            {ws_name.clone()}
                                        </div>
                                    </div>
                                    // Chevron
                                    <svg class="w-4 h-4 text-[var(--color-sidebar-foreground-muted)]" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7"/>
                                    </svg>
                                </button>

                                // Dropdown menu
                                <Show when=move || user_menu_open.get()>
                                    <div class="absolute bottom-full left-3 right-3 mb-1 bg-popover border border-border rounded-lg shadow-lg py-1 z-50">
                                        // Workspace switcher (includes separator only when shown)
                                        <WorkspaceSwitcher set_user_menu_open=set_user_menu_open/>
                                        <a href="/settings/profile" class="block px-4 py-2 text-sm text-foreground hover:bg-secondary transition-colors">
                                            "Settings"
                                        </a>
                                        <button
                                            class="w-full text-left px-4 py-2 text-sm text-foreground hover:bg-secondary transition-colors"
                                            on:click=move |_| {
                                                set_user_menu_open.set(false);
                                                editor_guard.request(EditorDestination::Logout);
                                            }
                                        >
                                            "Sign Out"
                                        </button>
                                    </div>
                                </Show>
                            }.into_any()
                            },
                            Err(_) => view! {
                                <div class="px-3 py-2 text-sm text-[var(--color-sidebar-foreground-muted)]">"Not signed in"</div>
                            }.into_any(),
                        }
                    })}
                </Suspense>
            </div>
        </div>
    }
}

/// Workspace switcher — shows list of workspaces, allows switching.
#[component]
fn WorkspaceSwitcher(set_user_menu_open: WriteSignal<bool>) -> impl IntoView {
    let editor_guard = expect_context::<EditorGuard>();
    let workspaces = LocalResource::new(list_user_workspaces);

    view! {
        <Suspense fallback=|| ()>
            {move || workspaces.get().map(|result| {
                match result {
                    Ok(ref ws_list) if ws_list.len() > 1 => {
                        let items = ws_list.clone();
                        view! {
                            <div class="px-3 py-1 text-xs text-muted-foreground font-medium uppercase tracking-wider">
                                "Workspaces"
                            </div>
                            {items.into_iter().map(|ws| {
                                let ws_id = ws.workspace_id.clone();
                                let name = ws.name.clone();
                                let is_current = ws.is_current;
                                view! {
                                    <button
                                        class="w-full text-left px-4 py-2 text-sm hover:bg-secondary transition-colors flex items-center gap-2"
                                        disabled=is_current
                                        on:click=move |_| {
                                            let ws_id = ws_id.clone();
                                            set_user_menu_open.set(false);
                                            editor_guard.request(EditorDestination::Workspace(ws_id));
                                        }
                                    >
                                        {if is_current {
                                            view! { <span class="w-2 h-2 rounded-full bg-primary flex-shrink-0"/> }.into_any()
                                        } else {
                                            view! { <span class="w-2 h-2 flex-shrink-0"/> }.into_any()
                                        }}
                                        <span class=if is_current { "font-medium text-foreground" } else { "text-foreground" }>
                                            {name}
                                        </span>
                                    </button>
                                }
                            }).collect_view()}
                            <div class="border-t border-border my-1"/>
                        }.into_any()
                    }
                    _ => view! { <span/> }.into_any(),
                }
            })}
        </Suspense>
    }
}
