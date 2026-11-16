// SPDX-License-Identifier: AGPL-3.0-or-later

//! App layout — sidebar + content area.

use leptos::prelude::*;
use leptos_router::components::Outlet;

#[cfg(target_arch = "wasm32")]
#[path = "guard_history.rs"]
mod guard_history;
#[path = "guard_tasks.rs"]
mod guard_tasks;
use guard_tasks::GuardTask;
pub use guard_tasks::{
    GuardLeaveAttempt, GuardSaveResolution, GuardScope, GuardTaskCallbacks, GuardTaskHandle,
    GuardTaskKind, GuardTaskState,
};

use crate::components::{Button, ButtonVariant, Spinner};
use crate::utils::nav_state::{current_location, settings_href};
use crate::server_fns::sidebar::{
    SidebarUser, get_sidebar_user, list_user_workspaces, switch_workspace,
};

/// A cross-tab invalidation signal; it contains no account or credential data.
pub const CONTEXT_CHANGED_KEY: &str = "krausen-context-changed";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GuardTransition {
    Idle,
    Pending(u64),
    Committed(u64),
    Cancelled(u64),
    Failed(u64),
}

#[derive(Clone)]
pub enum EditorDestination {
    Href(String),
    Workspace(String),
    Logout,
    History { key: String, attempt: u64 },
}

/// Prevent an in-progress recipe draft from being discarded by workspace switching.
#[derive(Clone, Copy)]
pub struct EditorGuard {
    tasks: RwSignal<Vec<GuardTask>>,
    registration: RwSignal<u64>,
    navigation: RwSignal<u64>,
    pub transition: RwSignal<GuardTransition>,
    history_commit: StoredValue<Option<Callback<(u64, String, u64), bool>>>,
    history_cancel: StoredValue<Option<Callback<()>>>,
    history_restore: StoredValue<Option<Callback<(), bool>>>,
    history_restore_ready: RwSignal<bool>,
    navigation_scopes: StoredValue<Vec<GuardScope>>,
    transition_owners: StoredValue<Vec<(u64, GuardScope, u64, bool)>>,
    unload_allowance: RwSignal<Option<u64>>,
    issued_document: RwSignal<Option<u64>>,
    document_issuance: RwSignal<u64>,
    document_target: StoredValue<Option<String>>,
    #[cfg(target_arch = "wasm32")]
    document_abort: StoredValue<
        Option<
            send_wrapper::SendWrapper<(
                web_sys::EventTarget,
                wasm_bindgen::closure::Closure<dyn FnMut(web_sys::Event)>,
            )>,
        >,
    >,
    same_document_destination: StoredValue<Option<(u64, String)>>,
    #[cfg(target_arch = "wasm32")]
    unload_event: StoredValue<Option<send_wrapper::SendWrapper<(wasm_bindgen::JsValue, bool)>>>,
    interaction_epoch: RwSignal<u64>,
    owned_history_hide: RwSignal<Option<u64>>,
    #[cfg(target_arch = "wasm32")]
    focus_origin: StoredValue<Option<send_wrapper::SendWrapper<web_sys::Element>>>,
    pub dirty: RwSignal<bool>,
    pub notice: RwSignal<bool>,
    pub destination: RwSignal<Option<EditorDestination>>,
    pub pending: RwSignal<bool>,
    pub message: RwSignal<String>,
    pub navigation_error: RwSignal<String>,
    pub current_url: RwSignal<String>,
}

impl EditorGuard {
    pub fn new() -> Self {
        Self {
            tasks: RwSignal::new(Vec::new()),
            registration: RwSignal::new(0),
            navigation: RwSignal::new(0),
            transition: RwSignal::new(GuardTransition::Idle),
            history_commit: StoredValue::new(None),
            history_cancel: StoredValue::new(None),
            history_restore: StoredValue::new(None),
            history_restore_ready: RwSignal::new(false),
            navigation_scopes: StoredValue::new(Vec::new()),
            transition_owners: StoredValue::new(Vec::new()),
            unload_allowance: RwSignal::new(None),
            issued_document: RwSignal::new(None),
            document_issuance: RwSignal::new(0),
            document_target: StoredValue::new(None),
            #[cfg(target_arch = "wasm32")]
            document_abort: StoredValue::new(None),
            same_document_destination: StoredValue::new(None),
            #[cfg(target_arch = "wasm32")]
            unload_event: StoredValue::new(None),
            interaction_epoch: RwSignal::new(0),
            owned_history_hide: RwSignal::new(None),
            #[cfg(target_arch = "wasm32")]
            focus_origin: StoredValue::new(None),
            dirty: RwSignal::new(false),
            notice: RwSignal::new(false),
            destination: RwSignal::new(None),
            pending: RwSignal::new(false),
            message: RwSignal::new(String::new()),
            navigation_error: RwSignal::new(String::new()),
            current_url: RwSignal::new(String::new()),
        }
    }
    pub fn register_task(
        self,
        kind: GuardTaskKind,
        scope: GuardScope,
        callbacks: GuardTaskCallbacks,
    ) -> GuardTaskHandle {
        self.registration.update(|id| *id += 1);
        let id = self.registration.get_untracked();
        self.tasks.update(|tasks| {
            tasks.push(GuardTask {
                id,
                kind,
                scope,
                callbacks,
                state: GuardTaskState::default(),
                resolved_navigation: None,
            })
        });
        GuardTaskHandle { guard: self, id }
    }
    fn refresh_tasks(self) {
        let tasks = self.tasks.get_untracked();
        self.dirty.set(tasks.iter().any(|task| task.state.blocks()));
        self.pending.set(
            tasks.iter().any(|task| task.state.in_flight)
                || matches!(self.transition.get_untracked(), GuardTransition::Pending(_)),
        );
    }
    fn active_task(self) -> Option<GuardTask> {
        let navigation = self.navigation.get();
        self.tasks
            .get()
            .into_iter()
            .rev()
            .find(|task| task.state.blocks() && task.resolved_navigation != Some(navigation))
    }
    fn continue_leave(self) {
        if self.active_task().is_none() && self.destination.get_untracked().is_some() {
            self.finish();
        }
    }
    fn save_active(self) {
        if self.destination.get_untracked().is_none() {
            return;
        }
        if let Some(task) = self.active_task() {
            if task.state.in_flight || !task.state.context_visible {
                return;
            }
            if let Some(save) = task.callbacks.save_and_leave {
                save.run(GuardLeaveAttempt {
                    navigation: self.navigation.get_untracked(),
                    registration: task.id,
                    scope: task.scope,
                });
            }
        }
    }
    fn discard_active(self) {
        if self.destination.get_untracked().is_none() {
            return;
        }
        if let Some(task) = self.active_task() {
            if !task.state.may_discard(task.kind) {
                return;
            }
            if task.callbacks.discard_unsent.run(()) {
                let navigation = self.navigation.get_untracked();
                self.tasks.update(|tasks| {
                    if let Some(current) = tasks
                        .iter_mut()
                        .find(|current| current.id == task.id && current.scope == task.scope)
                    {
                        current.resolved_navigation = Some(navigation);
                    }
                });
                self.continue_leave();
            }
        }
    }
    fn cancel_leave(self) {
        self.history_restore_ready.set(false);
        self.issued_document.set(None);
        self.document_target.set_value(None);
        self.same_document_destination.set_value(None);
        self.owned_history_hide.set(None);
        self.unload_allowance.set(None);
        if let Some(cancel) = self.history_cancel.get_value() {
            cancel.run(());
        }
        self.transition
            .set(GuardTransition::Cancelled(self.navigation.get_untracked()));
        self.navigation.update(|id| *id += 1);
        self.destination.set(None);
        self.notice.set(false);
        self.refresh_tasks();
    }
    #[cfg(any(target_arch = "wasm32", test))]
    fn cancel_leave_if(self, navigation: u64) -> bool {
        if self.navigation.try_get_untracked() != Some(navigation) {
            return false;
        }
        self.cancel_leave();
        true
    }
    #[cfg(any(target_arch = "wasm32", test))]
    fn history_shell_owned(self, navigation: u64, native: Option<(u64, &str)>) -> bool {
        if self.navigation.try_get_untracked() != Some(navigation) {
            return false;
        }
        match (native, self.destination.get_untracked()) {
            (
                Some((expected_attempt, expected_key)),
                Some(EditorDestination::History { key, attempt }),
            ) => attempt == expected_attempt && key == expected_key,
            (None, None) => true,
            _ => false,
        }
    }
    #[cfg(any(target_arch = "wasm32", test))]
    fn history_failure(self, navigation: u64, native: Option<(u64, &str)>, message: &str) {
        if self.history_shell_owned(navigation, native) {
            self.transition_failed(navigation, message);
        }
    }
    fn hide_for_history(self, navigation: u64) {
        self.owned_history_hide.set(Some(navigation));
        self.notice.set(false);
    }
    fn notice_closed(self) {
        let navigation = self.navigation.get_untracked();
        let owned = self.owned_history_hide.get_untracked() == Some(navigation)
            && self.transition.get_untracked() == GuardTransition::Pending(navigation);
        self.owned_history_hide.set(None);
        if !owned
            && (self.destination.get_untracked().is_some()
                || (!self.navigation_error.get_untracked().is_empty()
                    && self.history_restore.get_value().is_some()))
        {
            self.cancel_leave();
        }
    }
    /// Install before the router so a dirty form cannot be unmounted by its
    /// earlier popstate handler before the user chooses how to leave.
    #[cfg(target_arch = "wasm32")]
    pub fn install_history_guard(self) {
        guard_history::install(self);
        use wasm_bindgen::{JsCast, closure::Closure};
        let window = web_sys::window().expect("browser window");
        let interaction = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
            self.interaction_epoch.update(|epoch| *epoch += 1)
        });
        let _ = window
            .add_event_listener_with_callback("pointerdown", interaction.as_ref().unchecked_ref());
        let _ = window
            .add_event_listener_with_callback("keydown", interaction.as_ref().unchecked_ref());
        let hide = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
            self.document_hidden();
        });
        let unload = Closure::<dyn FnMut(web_sys::BeforeUnloadEvent)>::new(
            move |event: web_sys::BeforeUnloadEvent| {
                if self.should_warn_unload(&event) {
                    event.prevent_default();
                    event.set_return_value("");
                }
            },
        );
        let hash = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
            if let Some((navigation, href)) = self.same_document_destination.get_value() {
                if web_sys::window()
                    .and_then(|w| w.location().href().ok())
                    .as_deref()
                    == Some(&href)
                {
                    self.history_arrived(navigation);
                }
            }
        });
        let _ = window
            .add_event_listener_with_callback("beforeunload", unload.as_ref().unchecked_ref());
        let _ =
            window.add_event_listener_with_callback("hashchange", hash.as_ref().unchecked_ref());
        if window
            .add_event_listener_with_callback("pagehide", hide.as_ref().unchecked_ref())
            .is_err()
        {
            self.navigation_error
                .set("Document navigation completion reporting is unavailable.".into());
        }
        let listener = send_wrapper::SendWrapper::new((window, hide, interaction, unload, hash));
        on_cleanup(move || {
            self.document_abort.with_value(|binding| {
                if let Some(binding) = binding {
                    let (signal, callback) = &**binding;
                    let _ = signal.remove_event_listener_with_callback(
                        "abort",
                        callback.as_ref().unchecked_ref(),
                    );
                }
            });
            let (window, hide, interaction, unload, hash) = &*listener;
            let _ = window.remove_event_listener_with_callback(
                "beforeunload",
                unload.as_ref().unchecked_ref(),
            );
            let _ = window
                .remove_event_listener_with_callback("hashchange", hash.as_ref().unchecked_ref());
            let _ = window
                .remove_event_listener_with_callback("pagehide", hide.as_ref().unchecked_ref());
            let _ = window.remove_event_listener_with_callback(
                "pointerdown",
                interaction.as_ref().unchecked_ref(),
            );
            let _ = window.remove_event_listener_with_callback(
                "keydown",
                interaction.as_ref().unchecked_ref(),
            );
        });
    }
    pub fn request(self, destination: EditorDestination) {
        if self.destination.get_untracked().is_some()
            || matches!(self.transition.get_untracked(), GuardTransition::Pending(_))
        {
            return;
        }
        self.navigation.update(|id| *id += 1);
        self.navigation_scopes.set_value(
            self.tasks
                .get_untracked()
                .iter()
                .map(|task| task.scope.clone())
                .collect(),
        );
        self.history_restore_ready.set(false);
        #[cfg(target_arch = "wasm32")]
        self.focus_origin.set_value(
            web_sys::window()
                .and_then(|w| w.document())
                .and_then(|d| d.active_element())
                .map(send_wrapper::SendWrapper::new),
        );
        self.destination.set(Some(destination));
        if self.dirty.get_untracked() {
            self.notice.set(true);
        } else {
            self.finish();
        }
    }
    pub fn should_warn_unload(self, event: &web_sys::BeforeUnloadEvent) -> bool {
        #[cfg(target_arch = "wasm32")]
        if let Some(cached) = self.unload_event.get_value() {
            if js_sys::Object::is(&cached.0, event.as_ref()) {
                return cached.1;
            }
        }
        let warn = self.decide_unload();
        #[cfg(target_arch = "wasm32")]
        {
            let event_value: &wasm_bindgen::JsValue = event.as_ref();
            self.unload_event
                .set_value(Some(send_wrapper::SendWrapper::new((
                    event_value.clone(),
                    warn,
                ))));
        }
        let _ = event;
        warn
    }
    fn decide_unload(self) -> bool {
        if let Some(navigation) = self.unload_allowance.get_untracked() {
            self.unload_allowance.set(None);
            if self.navigation.get_untracked() == navigation
                && self.transition.get_untracked() == GuardTransition::Pending(navigation)
                && self.lease_current(navigation)
            {
                return false;
            }
        }
        if self.issued_document.get_untracked().is_some()
            || self.same_document_destination.get_value().is_some()
        {
            self.cancel_leave(); // a NEW native departure, not the old permitted event
        }
        self.dirty.get_untracked()
    }
    fn arm_document_departure(self, navigation: u64, target: String) {
        #[cfg(target_arch = "wasm32")]
        {
            use wasm_bindgen::JsCast;
            self.document_abort.with_value(|binding| {
                if let Some(binding) = binding {
                    let (signal, callback) = &**binding;
                    let _ = signal.remove_event_listener_with_callback(
                        "abort",
                        callback.as_ref().unchecked_ref(),
                    );
                }
            });
            self.document_abort.set_value(None);
        }
        self.document_issuance.update(|id| *id += 1);
        self.document_target.set_value(Some(target));
        self.issued_document.set(Some(navigation));
        self.unload_allowance.set(Some(navigation));
    }
    #[cfg(any(target_arch = "wasm32", test))]
    fn document_hidden(self) {
        if let Some(navigation) = self.issued_document.get_untracked() {
            if self.transition.get_untracked() == GuardTransition::Pending(navigation)
                && self.lease_current(navigation)
            {
                self.commit_navigation(navigation);
            }
        }
    }
    #[cfg(any(target_arch = "wasm32", test))]
    fn document_aborted(self, navigation: u64, issuance: u64) {
        if self.issued_document.get_untracked() == Some(navigation)
            && self.document_issuance.get_untracked() == issuance
            && self.navigation.get_untracked() == navigation
            && self.transition.get_untracked() == GuardTransition::Pending(navigation)
        {
            self.cancel_leave();
            self.navigation_error.set("Document navigation was canceled. Any server action already sent is not canceled; your original work remains retained.".into());
        }
    }
    fn issue_href(self, href: &str, navigation: u64) -> Option<&'static str> {
        #[cfg(target_arch = "wasm32")]
        {
            use wasm_bindgen::JsCast;
            let Some(window) = web_sys::window() else {
                return Some("Browser window unavailable.");
            };
            let Some(anchor) = window
                .document()
                .and_then(|d| d.create_element("a").ok())
                .and_then(|e| e.dyn_into::<web_sys::HtmlAnchorElement>().ok())
            else {
                return Some("Could not resolve the original destination.");
            };
            anchor.set_href(href);
            let target = anchor.href();
            let Ok(source) = window.location().href() else {
                return Some("Could not read the source address.");
            };
            if source.split('#').next() == target.split('#').next() {
                self.same_document_destination
                    .set_value(Some((navigation, target.clone())));
                if source == target {
                    self.history_arrived(navigation);
                    return None;
                }
            } else {
                self.arm_document_departure(navigation, target.clone());
            }
            window
                .location()
                .set_href(&target)
                .is_err()
                .then_some("Could not open that destination.")
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = (href, navigation);
            Some("Document departure requires a browser.")
        }
    }
    pub fn interaction_epoch(self) -> u64 {
        self.interaction_epoch.get_untracked()
    }
    pub fn navigation_epoch(self) -> u64 {
        self.navigation.get_untracked()
    }
    #[cfg(target_arch = "wasm32")]
    pub fn focus_origin(self) -> Option<web_sys::Element> {
        self.focus_origin
            .get_value()
            .map(|element| (*element).clone())
    }
    fn lease_current(self, navigation: u64) -> bool {
        if self.navigation.try_get_untracked() != Some(navigation) {
            return false;
        }
        let current = self.tasks.get_untracked();
        let owners = self.transition_owners.get_value();
        current.len() == owners.len()
            && owners.iter().all(|(id, scope, generation, _)| {
                current.iter().any(|task| {
                    task.id == *id
                        && task.scope == *scope
                        && task.state.generation == *generation
                        && !task.state.in_flight
                })
            })
    }
    #[cfg(any(target_arch = "wasm32", test))]
    fn history_arrived(self, navigation: u64) {
        if self.history_arrival_owned(navigation) {
            self.commit_navigation(navigation);
        }
    }
    #[cfg(any(target_arch = "wasm32", test))]
    fn commit_navigation(self, navigation: u64) {
        if self.transition.get_untracked() == GuardTransition::Pending(navigation)
            && self.lease_current(navigation)
        {
            self.owned_history_hide.set(None);
            self.issued_document.set(None);
            self.document_target.set_value(None);
            self.same_document_destination.set_value(None);
            self.unload_allowance.set(None);
            self.destination.set(None);
            self.transition.set(GuardTransition::Committed(navigation));
            self.notice.set(false);
            self.refresh_tasks();
        }
    }
    #[cfg(any(target_arch = "wasm32", test))]
    fn history_arrival_owned(self, navigation: u64) -> bool {
        let current = self.tasks.get_untracked();
        self.transition.try_get_untracked() == Some(GuardTransition::Pending(navigation))
            && self.lease_current(navigation)
            && self
                .transition_owners
                .get_value()
                .iter()
                .all(|(id, _, _, visible)| {
                    current
                        .iter()
                        .any(|task| task.id == *id && task.state.context_visible == *visible)
                })
    }
    fn transition_failed(self, navigation: u64, message: &str) {
        if self.navigation.try_get_untracked() == Some(navigation) {
            self.unload_allowance.set(None);
            self.issued_document.set(None);
            self.document_target.set_value(None);
            self.same_document_destination.set_value(None);
            self.navigation_error.set(message.into());
            self.transition.set(GuardTransition::Failed(navigation));
            self.notice.set(true);
            self.refresh_tasks();
        }
    }
    fn begin_transition(self) -> u64 {
        let navigation = self.navigation.get_untracked();
        self.transition_owners.set_value(
            self.tasks
                .get_untracked()
                .iter()
                .map(|task| {
                    (
                        task.id,
                        task.scope.clone(),
                        task.state.generation,
                        task.state.context_visible,
                    )
                })
                .collect(),
        );
        self.transition.set(GuardTransition::Pending(navigation));
        self.refresh_tasks();
        navigation
    }
    fn finish(self) {
        self.navigation_error.set(String::new());
        let Some(destination) = self.destination.get_untracked() else {
            return;
        };
        let navigation = self.begin_transition();
        let scopes = self.navigation_scopes.get_value();
        leptos::task::spawn_local(async move {
            let contextual = matches!(
                &destination,
                EditorDestination::Workspace(_) | EditorDestination::Logout
            );
            let original_account = if !contextual {
                None
            } else if let Some(scope) = scopes.first() {
                Some(scope.account_id.clone())
            } else {
                crate::server_fns::context::get_user_context()
                    .await
                    .ok()
                    .map(|context| context.user_id)
            };
            if !self.lease_current(navigation) {
                self.transition_failed(navigation, "Newer task state prevents the original navigation; your work remains retained.");
                return;
            }
            for scope in scopes.into_iter().filter(|_| contextual) {
                let expected = krausen_types::request_context::OperationIntent {
                    account_id: scope.account_id,
                    workspace_id: scope.workspace_id,
                };
                let verified =
                    crate::pages::recipe_editor::current_context_matches(&expected).await;
                if !self.lease_current(navigation) {
                    self.transition_failed(navigation, "Newer task state prevents the original navigation; your work remains retained.");
                    return;
                }
                if !verified {
                    self.transition_failed(navigation, "The account or workspace changed before navigation. The original destination was not retargeted; your work remains retained.");
                    return;
                }
            }
            if !self.lease_current(navigation) {
                self.transition_failed(navigation, "Newer task state prevents the original navigation; your work remains retained.");
                return;
            }
            if let EditorDestination::History { key, attempt } = destination {
                self.hide_for_history(navigation);
                if !self
                    .history_commit
                    .get_value()
                    .is_some_and(|commit| commit.run((attempt, key, navigation)))
                {
                    self.transition_failed(navigation, "The original history traversal could not start; your work remains held open.");
                }
                return;
            }
            if matches!(
                &destination,
                EditorDestination::Workspace(_) | EditorDestination::Logout
            ) && original_account.is_none()
            {
                self.transition_failed(navigation, "The original account could not be verified. No workspace or sign-out action was sent.");
                return;
            }
            let failure = match destination {
                EditorDestination::History { .. } => unreachable!(),
                EditorDestination::Href(href) => self.issue_href(&href, navigation),
                EditorDestination::Workspace(id) => {
                    let target_workspace = id.clone();
                    if switch_workspace(id).await.is_err() {
                        Some(
                            "Could not switch workspaces. The result is unconfirmed; check the current context before trying again. Your work remains retained.",
                        )
                    } else if !self.lease_current(navigation) {
                        Some(
                            "Workspace change completed, but newer work prevents the old page reload. Your local work remains retained.",
                        )
                    } else {
                        // The browser's access token still carries the old
                        // workspace until the existing refresh endpoint re-mints
                        // it from the server-side choice. Without this the new
                        // context can never be verified.
                        #[cfg(target_arch = "wasm32")]
                        let refreshed = crate::utils::auth_refresh::try_refresh().await;
                        #[cfg(not(target_arch = "wasm32"))]
                        let refreshed = false;
                        if !refreshed {
                            self.transition_failed(navigation, "The workspace choice was saved, but this session could not be refreshed. The result is unconfirmed; check the current context before trying again. Your work remains retained.");
                            return;
                        }
                        let verified = if let Some(account_id) = original_account.clone() {
                            crate::pages::recipe_editor::current_context_matches(
                                &krausen_types::request_context::OperationIntent {
                                    account_id,
                                    workspace_id: target_workspace,
                                },
                            )
                            .await
                        } else {
                            false
                        };
                        if !verified || !self.lease_current(navigation) {
                            self.transition_failed(navigation, "The workspace choice was saved and this session was refreshed, but the new context could not be confirmed here. Reload to continue in the current workspace; your local work remains retained.");
                            return;
                        }
                        // Land on the destination workspace's Recipes, never the
                        // old workspace's entity URL (IA-003).
                        let target = web_sys::window()
                            .and_then(|w| w.location().origin().ok())
                            .map(|origin| format!("{origin}/recipes"))
                            .unwrap_or_default();
                        if target.is_empty() {
                            self.transition_failed(
                                navigation,
                                "Could not identify the destination workspace's recipes.",
                            );
                            return;
                        }
                        self.arm_document_departure(navigation, target.clone());
                        web_sys::window()
                            .is_none_or(|w| w.location().set_href(&target).is_err())
                            .then_some("Workspace changed, but the page could not open its recipes.")
                    }
                }
                EditorDestination::Logout => {
                    if crate::server_fns::security::logout().await.is_err() {
                        Some(
                            "Could not sign out. The result is unconfirmed; check the current session before trying again. Your work remains retained.",
                        )
                    } else if !self.lease_current(navigation) {
                        Some(
                            "Sign-out completed, but newer work prevents the old navigation. Your local work remains retained.",
                        )
                    } else {
                        let session_active =
                            crate::server_fns::context::get_user_context().await.is_ok();
                        if session_active || !self.lease_current(navigation) {
                            self.transition_failed(navigation, "A newer session or task state prevents the old sign-out navigation. Your local work remains retained.");
                            return;
                        }
                        #[cfg(target_arch = "wasm32")]
                        crate::utils::list_position::clear_all();
                        self.issue_href("/login", navigation)
                    }
                }
            };
            if let Some(message) = failure {
                tracing::warn!("Unable to complete a requested navigation or account action");
                self.transition_failed(navigation, message);
            }
        });
    }
}

/// Focuses the page's own heading (else the main landmark) once it has
/// rendered. Pages load their content asynchronously, so this polls for a bounded
/// time (not a frame count) and gives up as soon as the user focuses something.
#[cfg(target_arch = "wasm32")]
fn focus_arrival_heading() {
    fn deadline_from_now() -> f64 {
        js_sys::Date::now() + 4_000.0
    }
    fn attempt(deadline: f64) {
        use wasm_bindgen::JsCast;
        let Some(document) = web_sys::window().and_then(|w| w.document()) else { return };
        let Some(main) = document.get_element_by_id("main-content") else { return };
        // Any deliberate focus the user has taken anywhere (sidebar, account
        // popover, a field) wins; only the main landmark/body means "still ours".
        if let Some(active) = document.active_element() {
            let tag = active.tag_name();
            if tag != "BODY" && tag != "HTML" && active.id() != "main-content" {
                return;
            }
        }
        let heading = main
            .query_selector("h1")
            .ok()
            .flatten()
            .and_then(|el| el.dyn_into::<web_sys::HtmlElement>().ok());
        match heading {
            Some(heading) => {
                if heading.tab_index() < 0 && !heading.has_attribute("tabindex") {
                    if heading.set_attribute("tabindex", "-1").is_err() {
                        tracing::warn!("Unable to make the arrival heading focusable");
                    }
                }
                if let Err(error) = heading.focus() {
                    tracing::warn!(?error, "Unable to focus the arrival heading");
                }
            }
            None if js_sys::Date::now() < deadline => {
                request_animation_frame(move || attempt(deadline));
            }
            None => {
                if let Ok(main) = main.dyn_into::<web_sys::HtmlElement>() {
                    if let Err(error) = main.focus() {
                        tracing::warn!(?error, "Unable to focus the main landmark");
                    }
                }
            }
        }
    }
    let deadline = deadline_from_now();
    request_animation_frame(move || attempt(deadline));
}

/// Main authenticated layout with sidebar and content area.
#[component]
pub fn Layout() -> impl IntoView {
    let user_info = LocalResource::new(get_sidebar_user);
    let mobile_sidebar_open = RwSignal::new(false);
    let editor_guard = expect_context::<EditorGuard>();
    let location = leptos_router::hooks::use_location();
    // Close transient navigation only when a route has COMMITTED.
    Effect::new(move |previous: Option<String>| {
        let path = location.pathname.get();
        if previous.as_ref().is_some_and(|previous| *previous != path) {
            mobile_sidebar_open.set(false);
        }
        path
    });
    // Committed NEW navigation moves focus to the arrival heading (IA-011).
    // Back/Forward restores context instead, same-route query changes and
    // background refetches never move focus, and a control the user already
    // focused inside the page is never taken over.
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::{JsCast, closure::Closure};
        // Millisecond stamp of the latest Back/Forward; only a very recent one counts,
        // so a traversal the draft guard cancelled can never leak into a later arrival.
        let traversal = StoredValue::new(None::<f64>);
        // Back/Forward is detected by the Navigation API's `traverse` type where
        // available and by `popstate` otherwise; both in the capture phase so the
        // flag is set before the router's own handling schedules effects.
        let mark = Closure::<dyn FnMut(web_sys::Event)>::new(move |event: web_sys::Event| {
            let kind = js_sys::Reflect::get(event.as_ref(), &"navigationType".into())
                .ok()
                .and_then(|value| value.as_string());
            if kind.is_none() || kind.as_deref() == Some("traverse") {
                traversal.set_value(Some(js_sys::Date::now()));
            }
        });
        let mut targets: Vec<web_sys::EventTarget> = Vec::new();
        if let Some(window) = web_sys::window() {
            if let Ok(navigation) = js_sys::Reflect::get(window.as_ref(), &"navigation".into()) {
                if let Ok(target) = navigation.dyn_into::<web_sys::EventTarget>() {
                    targets.push(target);
                }
            }
            targets.push(window.into());
        }
        let mut bound: Vec<(web_sys::EventTarget, &'static str)> = Vec::new();
        for target in targets {
            let name = if bound.is_empty() && js_sys::Reflect::has(target.as_ref(), &"currentEntry".into()).unwrap_or(false) {
                "navigate"
            } else {
                "popstate"
            };
            if target
                .add_event_listener_with_callback_and_bool(name, mark.as_ref().unchecked_ref(), true)
                .is_ok()
            {
                bound.push((target, name));
            }
        }
        let listener = send_wrapper::SendWrapper::new((bound, mark));
        on_cleanup(move || {
            let (bound, mark) = &*listener;
            for (target, name) in bound {
                let _ = target.remove_event_listener_with_callback_and_bool(
                    name,
                    mark.as_ref().unchecked_ref(),
                    true,
                );
            }
        });
        Effect::new(move |previous: Option<String>| {
            let path = location.pathname.get();
            // Any committed location change consumes the traversal flag, so a
            // same-route Back never leaks into the next real arrival.
            let _ = location.search.get();
            let arrived = previous.as_ref().is_some_and(|previous| *previous != path);
            let restored = traversal
                .get_value()
                .is_some_and(|stamp| js_sys::Date::now() - stamp < 750.0);
            traversal.set_value(None);
            if arrived && !restored {
                focus_arrival_heading();
            }
            path
        });
    }
    // A dirty-work guard replaces the drawer as the single modal; the exact
    // destination stays captured by the guard, never navigated behind it.
    let guard_was_open = StoredValue::new(false);
    Effect::new(move |_| {
        if editor_guard.notice.get() {
            guard_was_open.set_value(true);
            mobile_sidebar_open.set(false);
        } else if guard_was_open.get_value() {
            guard_was_open.set_value(false);
            // The guard's opener may have been inside the closed drawer or popup,
            // so nothing visible would receive focus on Stay. Restore the
            // visible shell opener unless focus went somewhere deliberate.
            #[cfg(target_arch = "wasm32")]
            request_animation_frame(move || {
                use wasm_bindgen::JsCast;
                let Some(document) = web_sys::window().and_then(|w| w.document()) else {
                    return;
                };
                let lost = document
                    .active_element()
                    .is_none_or(|el| matches!(el.tag_name().as_str(), "BODY" | "HTML"));
                if !lost {
                    return;
                }
                for selector in ["#mobile-navigation-open", "[aria-controls=\"account-popover\"]"] {
                    if let Some(opener) = document
                        .query_selector(selector)
                        .ok()
                        .flatten()
                        .and_then(|el| el.dyn_into::<web_sys::HtmlElement>().ok())
                        && opener.get_client_rects().length() > 0
                    {
                        if let Err(error) = opener.focus() {
                            tracing::warn!(?error, "Unable to restore shell opener focus");
                        }
                        return;
                    }
                }
            });
        }
    });
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::{JsCast, closure::Closure};
        // Crossing to the desktop shell must not leave a modal drawer (a second
        // interactive copy) open over it.
        if let Some(query) = web_sys::window()
            .and_then(|w| w.match_media("(min-width: 768px)").ok().flatten())
        {
            let change = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
                // The copy in the other layout becomes invisible; its popup must
                // not stay logically open (and reappear) across the seam.
                if let Some(document) = web_sys::window().and_then(|w| w.document()) {
                    for id in ["account-popover", "mobile-account-popover"] {
                        if let Some(popup) = document.get_element_by_id(id) {
                            if popup.matches(":popover-open").unwrap_or(false) {
                                let _ = js_sys::Reflect::get(popup.as_ref(), &"hidePopover".into())
                                    .and_then(|method| method.dyn_into::<js_sys::Function>())
                                    .and_then(|method| method.call0(popup.as_ref()));
                            }
                        }
                    }
                }
                if web_sys::window()
                    .and_then(|w| w.match_media("(min-width: 768px)").ok().flatten())
                    .is_some_and(|query| query.matches())
                {
                    mobile_sidebar_open.try_set(false);
                }
            });
            let _ = query.add_event_listener_with_callback("change", change.as_ref().unchecked_ref());
            let listener = send_wrapper::SendWrapper::new((query, change));
            on_cleanup(move || {
                let (query, change) = &*listener;
                let _ = query.remove_event_listener_with_callback("change", change.as_ref().unchecked_ref());
            });
        }
    }
    let notice_was_open = StoredValue::new(false);
    let notice_open_navigation = StoredValue::new(0_u64);
    Effect::new(move |_| {
        if editor_guard.notice.get() {
            if !notice_was_open.get_value() {
                notice_open_navigation.set_value(editor_guard.navigation.get_untracked());
            }
            notice_was_open.set_value(true);
        } else if notice_was_open.get_value() {
            notice_was_open.set_value(false);
            if editor_guard.navigation.get_untracked() != notice_open_navigation.get_value()
                && editor_guard.destination.get_untracked().is_none()
            {
                return;
            }
            editor_guard.notice_closed();
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
            <a class="brewery-skip-link print-hide" href="#main-content"
                on:click=move |event| {
                    event.prevent_default();
                    #[cfg(target_arch = "wasm32")]
                    {
                        use wasm_bindgen::JsCast;
                        if let Some(main) = web_sys::window()
                            .and_then(|w| w.document())
                            .and_then(|d| d.get_element_by_id("main-content"))
                            .and_then(|el| el.dyn_into::<web_sys::HtmlElement>().ok())
                        {
                            if let Err(error) = main.focus() {
                                tracing::warn!(?error, "Unable to focus the main landmark");
                            }
                        }
                    }
                }
            >"Skip to main content"</a>
            <div class="app-shell h-dvh bg-background">
                // Desktop sidebar
                <div class="app-shell__desktop print-hide">
                    <Sidebar user_info=user_info/>
                </div>

                <Show when=move || editor_guard.notice.get()>
                {move || {
                let recipe = editor_guard.active_task().is_some_and(|task| task.kind == GuardTaskKind::Recipe);
                view! { <crate::components::BreweryDialog id="workspace-draft-warning" title=if recipe { "Unsaved recipe changes" } else if editor_guard.active_task().is_some_and(|task| task.kind == GuardTaskKind::Import && task.state.uncertain) { "Import result unconfirmed" } else { "Unsaved task changes" } description="Resolve this task, or stay with your work." open=editor_guard.notice return_focus_fallback="mobile-navigation-open".to_string()>
                    {move || if editor_guard.active_task().is_some_and(|task| task.kind == GuardTaskKind::Import && task.state.uncertain) {
                        view! { <p>"An import was sent and its result is unconfirmed. The original request is retained in this account and workspace; leaving does not cancel it, and you can check it later."</p> }.into_any()
                    } else {
                        view! { <p>"Your draft remains in this account and workspace until you choose what to do."</p> }.into_any()
                    }}
                    <p role="status">{move || editor_guard.message.get()}</p>
                    <p role="status">{move || editor_guard.navigation_error.get()}</p>
                    <Show when=move || !editor_guard.navigation_error.get().is_empty() && editor_guard.history_restore_ready.get()>
                    <Button on:click=move |_| { if let Some(restore) = editor_guard.history_restore.get_value() { restore.run(()); } }>"Retry original history restoration"</Button>
                    </Show>
                    <Show when=move || editor_guard.active_task().is_some_and(|task| task.callbacks.save_and_leave.is_some())>
                    <crate::components::Button disabled=Signal::derive(move || editor_guard.pending.get() || editor_guard.destination.get().is_none()) on:click=move |_| editor_guard.save_active()>"Save and leave"</crate::components::Button>
                    </Show>
                    <Show when=move || editor_guard.active_task().is_some_and(|task| task.callbacks.return_to_actions.is_some())>
                    <crate::components::Button on:click=move |_| {
                        let callback = editor_guard.active_task().and_then(|task| task.callbacks.return_to_actions);
                        editor_guard.cancel_leave();
                        if let Some(callback) = callback { callback.run(()); }
                    }>"Return to save evidence"</crate::components::Button>
                    </Show>
                    <crate::components::Button variant=crate::components::ButtonVariant::Secondary disabled=Signal::derive(move || editor_guard.pending.get() || editor_guard.destination.get().is_none() || editor_guard.active_task().is_none_or(|task| !task.state.may_discard(task.kind))) on:click=move |_| editor_guard.discard_active()>
                        {move || match editor_guard.active_task().map(|task| (task.kind, task.state.uncertain)) {
                            Some((GuardTaskKind::Evidence, _)) => "Discard local inputs and leave",
                            Some((GuardTaskKind::Import, true)) => "Leave and keep the retained request",
                            _ => "Discard and leave",
                        }}
                    </crate::components::Button>
                    <crate::components::Button variant=crate::components::ButtonVariant::Secondary on:click=move |_| editor_guard.cancel_leave()>"Stay"</crate::components::Button>
                </crate::components::BreweryDialog> }
                }}
                </Show>

                <crate::components::BreweryDialog id="mobile-navigation" title="Navigation" description="Choose a destination in your workspace." open=mobile_sidebar_open return_focus_fallback="sidebar-recipes-link".to_string()>
                    <Sidebar user_info=user_info drawer=mobile_sidebar_open/>
                </crate::components::BreweryDialog>

                <div class="app-shell__content flex-1 flex flex-col overflow-hidden">
                    // Mobile header bar
                    <div class="app-shell__mobile flex items-center gap-3 px-4 py-3 border-b border-border bg-background print-hide">
                        <Button variant=ButtonVariant::GhostMuted
                            attr:id="mobile-navigation-open"
                            on:click=move |_| mobile_sidebar_open.set(true)
                            aria_label="Toggle menu"
                            attr:aria-controls="mobile-navigation"
                            attr:aria-expanded=move || mobile_sidebar_open.get().to_string()
                        >
                            <svg class="w-5 h-5 text-foreground" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 6h16M4 12h16M4 18h16"/>
                            </svg>
                        </Button>
                        <crate::components::BreweryBrand/>
                    </div>
                    <main id="main-content" tabindex="-1" class="app-shell__main flex-1 overflow-y-auto">
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

/// Sidebar component: destinations and the compact account popover. Rendered
/// once for desktop and once inside the mobile Navigation dialog; only one is
/// ever interactive (the other is hidden or in a closed dialog).
#[component]
fn Sidebar(
    user_info: LocalResource<Result<SidebarUser, ServerFnError>>,
    /// The mobile Navigation dialog's state; `None` on the desktop sidebar.
    #[prop(optional)]
    drawer: Option<RwSignal<bool>>,
) -> impl IntoView {
    let mobile = drawer.is_some();
    let editor_guard = expect_context::<EditorGuard>();
    let account_open = RwSignal::new(false);
    let location = leptos_router::hooks::use_location();
    // Dismiss only after navigation commits; the document-capture draft guard
    // can still intercept Settings and keep this popup/draft intact on Stay.
    Effect::new(move |_| {
        location.pathname.get();
        account_open.set(false);
    });
    // A dirty-work guard becomes the single modal: the account popup yields to it.
    Effect::new(move |_| {
        if editor_guard.notice.get() {
            account_open.set(false);
        }
    });
    // A hidden drawer must not leave a popup open that reappears with it.
    if let Some(drawer) = drawer {
        Effect::new(move |_| {
            if !drawer.get() {
                account_open.set(false);
            }
        });
    }
    let pending_status = move || match (
        editor_guard.transition.get(),
        editor_guard.destination.get(),
    ) {
        (GuardTransition::Pending(_), Some(EditorDestination::Workspace(_))) => {
            Some("Switching workspace…")
        }
        (GuardTransition::Pending(_), Some(EditorDestination::Logout)) => Some("Signing out…"),
        _ => None,
    };
    let suffix = if mobile { "mobile-" } else { "" };
    // One path for every destination: a same-route arrival commits no route
    // change, so the transient drawer would otherwise stay open. A guarded click
    // never reaches link handlers; real route changes close it in Layout.
    let close_if_current = move |target: &'static str| {
        if let Some(drawer) = drawer {
            if location.pathname.get_untracked() == target {
                drawer.set(false);
            }
        }
    };
    let query = leptos_router::hooks::use_query_map();
    let return_to = Signal::derive(move || {
        let path = location.pathname.get();
        if path.starts_with("/settings") {
            // Already in Settings: keep the original in-app return, never nest.
            query.read().get("return").unwrap_or_default()
        } else {
            current_location(&path, &location.search.get())
        }
    });
    let arrived = Callback::new(move |()| {
        account_open.set(false);
        close_if_current("/settings/profile");
    });

    view! {
        <div class=if mobile { "brewery-sidebar brewery-sidebar--mobile" } else { "brewery-sidebar" }>
            <div class="brewery-sidebar__brand">
                <crate::components::BreweryBrand/>
            </div>

            <nav class="brewery-sidebar__nav" aria-label="Your brewery">
                <p class="brewery-sidebar__label">"Your brewery"</p>
                <a href="/recipes" id=format!("{suffix}sidebar-recipes-link") class="brewery-sidebar__link"
                    on:click=move |_| close_if_current("/recipes")
                    aria-current=move || (location.pathname.get() == "/recipes").then_some("page")
                    data-ancestor=move || {
                        // The Recipes section contains these pages; only the exact
                        // destination is the current page (IA-011).
                        let path = location.pathname.get();
                        (path.starts_with("/recipes/") || path.starts_with("/recipe-editor/") || path.starts_with("/brews/") || path == "/beerxml").then_some("true")
                    }>
                    <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 5h16v14H4zM8 9h8M8 13h8M8 17h5"/></svg>
                    "Recipes"
                </a>
                <a href="/equipment" class="brewery-sidebar__link" on:click=move |_| close_if_current("/equipment") aria-current=move || location.pathname.get().starts_with("/equipment").then_some("page")>
                    <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M7 3h10M9 3v4l-4 11a2 2 0 002 2h10a2 2 0 002-2L15 7V3M8 14h8"/></svg>
                    "Equipment"
                </a>
            </nav>
            <div class="brewery-sidebar__detail" aria-hidden="true"><p>"A little curiosity."<br/>"A better next batch."</p></div>

            <div class="brewery-sidebar__footer">
                {move || pending_status().map(|text| view! { <p class="brewery-sidebar__pending" role="status">{text}</p> })}
                <div class="brewery-sidebar__account">
                    <Suspense fallback=|| view! {
                        <div class="px-3 py-2 text-sm text-[var(--color-sidebar-foreground-muted)]">"Loading..."</div>
                    }>
                        {move || user_info.get().map(|result| match result {
                            Ok(user) => match drawer {
                                Some(drawer) => view! { <AccountPopover user=user open=account_open drawer=drawer arrived=arrived return_to=return_to/> }.into_any(),
                                None => view! { <AccountPopover user=user open=account_open arrived=arrived return_to=return_to/> }.into_any(),
                            },
                            Err(_) => view! {
                                <div class="px-3 py-2 text-sm text-[var(--color-sidebar-foreground-muted)]">"Not signed in"</div>
                            }.into_any(),
                        })}
                    </Suspense>
                </div>
            </div>
        </div>
    }
}

/// Compact, nonmodal account popup (IA-002, ACC-002): ordinary links/buttons in
/// normal Tab order. Workspace choices exist only for a verified >1 membership
/// list (IA-003); every other state keeps the account actions available.
#[component]
fn AccountPopover(
    user: SidebarUser,
    open: RwSignal<bool>,
    #[prop(optional)] drawer: Option<RwSignal<bool>>,
    /// Same-route arrival commits no route change, so nothing else would close
    /// the popup/drawer. A guarded click never reaches the link handlers.
    arrived: Callback<()>,
    /// The in-app location Settings returns to (validated before use; empty or
    /// non-app locations fall back to Recipes there).
    return_to: Signal<String>,
) -> impl IntoView {
    let mobile = drawer.is_some();
    let editor_guard = expect_context::<EditorGuard>();
    let display_name = user.name.clone().unwrap_or_else(|| user.email.clone());
    let avatar = display_name
        .chars()
        .next()
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_else(|| "?".into());
    let show_email = user.name.is_some();
    let email = user.email.clone();
    let accessible_name = format!("Account menu, {display_name}");
    let identity_name = display_name.clone();
    let trigger_name = display_name.clone();
    let trigger = ViewFn::from(move || {
        view! {
            <span class="brewery-account__avatar" aria-hidden="true">{avatar.clone()}</span>
            <span class="brewery-account__name">{trigger_name.clone()}</span>
            <svg class="w-4 h-4 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24" aria-hidden="true">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7"/>
            </svg>
        }
        .into_any()
    });

    view! {
        <crate::components::BreweryPopover
            id=if mobile { "mobile-account-popover" } else { "account-popover" }
            label="Account menu"
            open=open
            placement=crate::components::popover::Placement::TOP_START
            trigger_variant=ButtonVariant::GhostMuted
            trigger_class="brewery-account__trigger"
            trigger_aria_label=accessible_name
            trigger_content=trigger
        >
            <div class="brewery-account__identity">
                <p class="brewery-account__identity-name">{identity_name}</p>
                {show_email.then(|| view! { <p class="brewery-account__identity-email">{email.clone()}</p> })}
            </div>
            <Show when=move || open.get()>
                <WorkspaceChoices open=open mobile=mobile/>
            </Show>
            <a href=move || settings_href("/settings/profile", Some(&return_to.get())) class="brewery-sidebar__link" on:click=move |_| arrived.run(())>"Settings"</a>
            <a href=move || format!("{}#appearance", settings_href("/settings/profile", Some(&return_to.get()))) class="brewery-sidebar__link" on:click=move |_| arrived.run(())>"Appearance"</a>
            <Button variant=ButtonVariant::GhostMuted class="w-full"
                disabled=Signal::derive(move || editor_guard.pending.get())
                on:click=move |_| {
                    open.set(false);
                    editor_guard.request(EditorDestination::Logout);
                }
            >
                "Sign Out"
            </Button>
        </crate::components::BreweryPopover>
    }
}

/// Workspace identity and choices inside the account popup. Mounted fresh each
/// time the popup opens so choices are never stale. Only a successful list with
/// more than one membership shows names; zero/loading/error never infer a choice.
#[component]
fn WorkspaceChoices(open: RwSignal<bool>, mobile: bool) -> impl IntoView {
    let editor_guard = expect_context::<EditorGuard>();
    let heading_id = if mobile { "mobile-account-workspaces-heading" } else { "account-workspaces-heading" };
    let workspaces = LocalResource::new(list_user_workspaces);

    view! {
        <Suspense fallback=|| view! { <p class="brewery-account__status" role="status">"Checking workspaces…"</p> }>
            {move || workspaces.get().map(|result| match result {
                Ok(list) if list.len() > 1 => view! {
                    <section class="brewery-account__workspaces" aria-labelledby=heading_id>
                        <h3 id=heading_id class="brewery-account__heading">"Workspaces"</h3>
                        <ul class="brewery-account__list">
                            {list.into_iter().map(|workspace| {
                                let id = workspace.workspace_id.clone();
                                if workspace.is_current {
                                    view! { <li><span class="brewery-account__current" aria-current="true">{workspace.name}" (current)"</span></li> }.into_any()
                                } else {
                                    view! { <li>
                                        <Button variant=ButtonVariant::GhostMuted class="w-full"
                                            disabled=Signal::derive(move || editor_guard.pending.get())
                                            on:click=move |_| {
                                                open.set(false);
                                                editor_guard.request(EditorDestination::Workspace(id.clone()));
                                            }
                                        >
                                            <span class="min-w-0 truncate">{workspace.name}</span>
                                        </Button>
                                    </li> }.into_any()
                                }
                            }).collect_view()}
                        </ul>
                    </section>
                }.into_any(),
                Ok(list) if list.is_empty() => view! {
                    <p class="brewery-account__status" role="status">"No workspace is available for this account."</p>
                }.into_any(),
                // Exactly one verified membership: no switching entry.
                Ok(_) => ().into_any(),
                Err(_) => view! {
                    <crate::components::BreweryNotice title="Unable to load workspaces" body="Workspace choices could not be checked. Your account actions still work." warning=true announcement=crate::components::NoticeAnnouncement::Settled/>
                    <Button variant=ButtonVariant::Secondary on:click=move |_| workspaces.refetch()>"Retry"</Button>
                }.into_any(),
            })}
        </Suspense>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user(name: Option<&str>) -> SidebarUser {
        SidebarUser {
            user_id: "u".into(),
            workspace_id: Some("w".into()),
            name: name.map(str::to_owned),
            email: "brewer@example.test".into(),
            workspace_name: Some("Foreign workspace name".into()),
            is_personal_mode: false,
            is_self_hosted: true,
            theme_preference: "system".into(),
        }
    }

    // IA-002/ACC-002: ordinary native popover; no modal, menu roles or shell
    // workspace furniture. Workspace names come only from a verified list.
    #[test]
    fn account_entry_is_an_ordinary_nonmodal_popover_without_workspace_furniture() {
        let owner = Owner::new();
        owner.with(|| {
            provide_context(EditorGuard::new());
            let open = RwSignal::new(false);
            let html = view! { <AccountPopover user=user(Some("Ada Brewer")) open=open arrived=Callback::new(|()| {}) return_to=Signal::derive(String::new)/> }.to_html();
            assert!(html.contains("aria-label=\"Account menu, Ada Brewer\""));
            assert!(html.contains("popovertarget=\"account-popover\""));
            assert!(html.contains("popover=\"auto\""));
            assert!(html.contains("href=\"/settings/profile\""));
            assert!(html.contains("href=\"/settings/profile#appearance\""));
            assert!(html.contains("Sign Out"));
            assert!(html.contains("brewer@example.test"));
            assert!(!html.contains("role=\"menu"));
            assert!(!html.contains("<dialog"));
            assert!(!html.contains("Foreign workspace name"));
            assert!(!html.contains("Switch workspace"));
            let mobile = view! { <AccountPopover user=user(None) open=open drawer=RwSignal::new(true) arrived=Callback::new(|()| {}) return_to=Signal::derive(String::new)/> }.to_html();
            assert!(mobile.contains("id=\"mobile-account-popover\""));
            assert!(mobile.contains("aria-label=\"Account menu, brewer@example.test\""));
        });
    }
}
