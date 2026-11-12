// SPDX-License-Identifier: AGPL-3.0-or-later
//! Task ownership and navigation continuations. Sent commands stay with their
//! feature owner; cancelling a continuation never cancels a server request.
use leptos::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GuardTaskKind {
    Recipe,
    Evidence,
    Equipment,
    Library,
    Import,
    Start,
    Scale,
    Duplicate,
    Feedback,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GuardScope {
    pub installation: String,
    pub account_id: String,
    pub workspace_id: String,
    pub resource_id: Option<String>,
    pub base_revision: Option<i64>,
}
impl GuardScope {
    pub fn installation() -> String {
        #[cfg(target_arch = "wasm32")]
        {
            web_sys::window()
                .and_then(|w| w.location().origin().ok())
                .unwrap_or_default()
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            String::new()
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GuardTaskState {
    pub dirty: bool,
    pub generation: u64,
    pub in_flight: bool,
    pub uncertain: bool,
    pub recovery_retained: bool,
    pub context_visible: bool,
}
impl GuardTaskState {
    pub fn blocks(self) -> bool {
        self.dirty || self.in_flight || self.uncertain
    }
    pub fn may_discard(self, kind: GuardTaskKind) -> bool {
        !self.in_flight
            && (!self.uncertain || (kind == GuardTaskKind::Evidence && self.recovery_retained))
    }
}

#[derive(Clone)]
pub struct GuardTaskCallbacks {
    pub save_and_leave: Option<Callback<GuardLeaveAttempt>>,
    pub return_to_actions: Option<Callback<()>>,
    pub discard_unsent: Callback<(), bool>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GuardLeaveAttempt {
    pub navigation: u64,
    pub registration: u64,
    pub scope: GuardScope,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GuardSaveResolution {
    Saved { submitted_generation: u64 },
    NewerEdits,
    Failed,
}

#[derive(Clone)]
pub(super) struct GuardTask {
    pub id: u64,
    pub kind: GuardTaskKind,
    pub scope: GuardScope,
    pub state: GuardTaskState,
    pub callbacks: GuardTaskCallbacks,
    pub resolved_navigation: Option<u64>,
}

#[derive(Clone, Copy)]
pub struct GuardTaskHandle {
    pub(super) guard: super::EditorGuard,
    pub(super) id: u64,
}

impl GuardTaskHandle {
    pub fn current(self) -> Option<(GuardScope, GuardTaskState)> {
        self.guard
            .tasks
            .try_get_untracked()?
            .into_iter()
            .find(|task| task.id == self.id)
            .map(|task| (task.scope, task.state))
    }
    pub fn update(self, state: GuardTaskState) {
        self.guard.tasks.update(|tasks| {
            if let Some(task) = tasks.iter_mut().find(|task| task.id == self.id) {
                if task.state != state {
                    task.resolved_navigation = None;
                }
                task.state = state;
            }
        });
        self.guard.refresh_tasks();
    }
    pub fn unregister(self) {
        self.guard
            .tasks
            .update(|tasks| tasks.retain(|task| task.id != self.id));
        self.guard.refresh_tasks();
    }
    pub fn complete_save(self, attempt: GuardLeaveAttempt, result: GuardSaveResolution) {
        let navigation = self.guard.navigation.get_untracked();
        if !self.guard.notice.get_untracked() || self.guard.destination.get_untracked().is_none() {
            return;
        }
        let mut accepted = false;
        self.guard.tasks.update(|tasks| {
            if let Some(task) = tasks.iter_mut().find(|task| task.id == self.id) {
                if save_matches(task, &attempt, navigation, result) {
                    task.resolved_navigation = Some(navigation);
                    accepted = true;
                }
            }
        });
        if accepted {
            self.guard.continue_leave();
        }
    }
}

fn save_matches(
    task: &GuardTask,
    attempt: &GuardLeaveAttempt,
    navigation: u64,
    result: GuardSaveResolution,
) -> bool {
    attempt.navigation == navigation
        && attempt.registration == task.id
        && attempt.scope == task.scope
        && matches!(result, GuardSaveResolution::Saved { submitted_generation } if submitted_generation == task.state.generation)
        && !task.state.blocks()
        && task.state.context_visible
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retired_history_cannot_cancel_or_publish_failure_over_a_replacement_navigation() {
        let owner = Owner::new();
        owner.with(|| {
            let guard = super::super::EditorGuard::new();
            let task = guard.register_task(GuardTaskKind::Recipe, GuardScope { installation: "local".into(), account_id: "a".into(), workspace_id: "w".into(), resource_id: None, base_revision: None }, GuardTaskCallbacks { save_and_leave: None, return_to_actions: None, discard_unsent: Callback::new(|()| true) });
            task.update(GuardTaskState { dirty: true, context_visible: true, ..Default::default() });
            guard.request(super::super::EditorDestination::History { key: "A-target".into(), attempt: 7 });
            let original = guard.begin_transition();
            guard.cancel_leave();
            let prechoice_epoch = guard.navigation_epoch();
            guard.request(super::super::EditorDestination::Href("/B".into()));
            let replacement = guard.navigation_epoch();
            guard.navigation_error.set("B owns its message".into());
            assert!(!guard.cancel_leave_if(original));
            guard.history_failure(original, Some((7, "A-target")), "late native A arrival/reject");
            guard.history_failure(prechoice_epoch, None, "late pre-choice restoration failure");
            assert_eq!(guard.navigation_epoch(), replacement);
            assert!(matches!(guard.destination.get_untracked(), Some(super::super::EditorDestination::Href(ref href)) if href == "/B"));
            assert_eq!(guard.navigation_error.get_untracked(), "B owns its message");
            assert!(guard.notice.get_untracked());
        });
    }
    #[test]
    fn actual_document_arrival_does_not_depend_on_a_feature_unload_listener() {
        let owner = Owner::new();
        owner.with(|| {
            let guard = super::super::EditorGuard::new();
            guard
                .destination
                .set(Some(super::super::EditorDestination::Href(
                    "/recipes".into(),
                )));
            let navigation = guard.begin_transition();
            guard.arm_document_departure(navigation, "/recipes".into());
            guard.document_hidden();
            assert_eq!(
                guard.transition.get_untracked(),
                super::super::GuardTransition::Committed(navigation)
            );
            guard
                .destination
                .set(Some(super::super::EditorDestination::Href(
                    "/settings".into(),
                )));
            let next = guard.begin_transition();
            guard.arm_document_departure(next, "/settings".into());
            let retired_issuance = guard.document_issuance.get_untracked();
            guard.arm_document_departure(next, "/settings".into());
            guard.document_aborted(next, retired_issuance);
            assert_eq!(
                guard.transition.get_untracked(),
                super::super::GuardTransition::Pending(next)
            );
            guard.document_aborted(next, guard.document_issuance.get_untracked());
            guard.document_hidden();
            assert_ne!(
                guard.transition.get_untracked(),
                super::super::GuardTransition::Committed(next)
            );
        });
    }
    #[test]
    fn pending_notice_dismissal_cancels_but_owned_history_hide_is_not_user_dismissal() {
        let owner = Owner::new();
        owner.with(|| {
            let guard = super::super::EditorGuard::new();
            let task = guard.register_task(
                GuardTaskKind::Recipe,
                GuardScope {
                    installation: "local".into(),
                    account_id: "a".into(),
                    workspace_id: "w".into(),
                    resource_id: None,
                    base_revision: None,
                },
                GuardTaskCallbacks {
                    save_and_leave: None,
                    return_to_actions: None,
                    discard_unsent: Callback::new(|()| true),
                },
            );
            task.update(GuardTaskState {
                dirty: true,
                context_visible: true,
                ..Default::default()
            });
            guard.request(super::super::EditorDestination::Workspace("other".into()));
            let navigation = guard.begin_transition();
            guard.notice.set(false);
            guard.notice_closed();
            assert!(!guard.history_arrival_owned(navigation));
            assert_eq!(
                guard.transition.get_untracked(),
                super::super::GuardTransition::Cancelled(navigation)
            );
            guard.request(super::super::EditorDestination::History {
                key: "native".into(),
                attempt: 1,
            });
            let next = guard.begin_transition();
            guard.hide_for_history(next);
            guard.notice_closed();
            assert!(guard.history_arrival_owned(next));
            guard.notice.set(true);
            guard.notice.set(false);
            guard.notice_closed();
            assert!(
                !guard.history_arrival_owned(next),
                "a subsequent genuine Close must still cancel"
            );
        });
    }
    #[test]
    fn actual_native_arrival_policy_rejects_cancelled_newer_or_retired_task_leases() {
        let owner = Owner::new();
        owner.with(|| {
            let guard = super::super::EditorGuard::new();
            let scope = GuardScope {
                installation: "local".into(),
                account_id: "a".into(),
                workspace_id: "w".into(),
                resource_id: None,
                base_revision: None,
            };
            let task = guard.register_task(
                GuardTaskKind::Evidence,
                scope,
                GuardTaskCallbacks {
                    save_and_leave: None,
                    return_to_actions: None,
                    discard_unsent: Callback::new(|()| true),
                },
            );
            let state = GuardTaskState {
                dirty: true,
                generation: 2,
                context_visible: true,
                ..Default::default()
            };
            task.update(state);
            guard.request(super::super::EditorDestination::History {
                key: "native-target".into(),
                attempt: 1,
            });
            let navigation = guard.begin_transition();
            assert!(guard.history_arrival_owned(navigation));
            task.update(GuardTaskState {
                generation: 3,
                ..state
            });
            assert!(!guard.history_arrival_owned(navigation));
            task.update(state);
            task.update(GuardTaskState {
                context_visible: false,
                ..state
            });
            assert!(!guard.history_arrival_owned(navigation));
            task.update(state);
            guard.cancel_leave();
            assert!(!guard.history_arrival_owned(navigation));
            guard.request(super::super::EditorDestination::History {
                key: "native-target".into(),
                attempt: 2,
            });
            let next = guard.begin_transition();
            assert!(guard.history_arrival_owned(next));
            task.unregister();
            assert!(!guard.history_arrival_owned(next));
        });
    }
    #[test]
    fn real_handles_keep_each_finite_task_independent_and_old_cleanup_cannot_unregister_a_new_owner()
     {
        let owner = Owner::new();
        owner.with(|| {
            let guard = super::super::EditorGuard::new();
            let scope = GuardScope {
                installation: "local".into(),
                account_id: "a".into(),
                workspace_id: "w".into(),
                resource_id: Some("resource".into()),
                base_revision: Some(2),
            };
            let callbacks = || GuardTaskCallbacks {
                save_and_leave: None,
                return_to_actions: None,
                discard_unsent: Callback::new(|()| true),
            };
            let kinds = [
                GuardTaskKind::Recipe,
                GuardTaskKind::Evidence,
                GuardTaskKind::Equipment,
                GuardTaskKind::Library,
                GuardTaskKind::Import,
                GuardTaskKind::Start,
                GuardTaskKind::Scale,
                GuardTaskKind::Duplicate,
                GuardTaskKind::Feedback,
            ];
            let handles: Vec<_> = kinds
                .into_iter()
                .map(|kind| {
                    let handle = guard.register_task(kind, scope.clone(), callbacks());
                    handle.update(GuardTaskState {
                        dirty: true,
                        context_visible: true,
                        ..Default::default()
                    });
                    handle
                })
                .collect();
            assert_eq!(guard.tasks.get_untracked().len(), 9);
            assert!(guard.dirty.get_untracked());
            let newest = guard.register_task(GuardTaskKind::Recipe, scope.clone(), callbacks());
            newest.update(GuardTaskState {
                dirty: true,
                context_visible: true,
                ..Default::default()
            });
            let stale = handles[0];
            stale.unregister();
            stale.unregister();
            assert!(
                guard
                    .tasks
                    .get_untracked()
                    .iter()
                    .any(|task| task.id == newest.id)
            );
            stale.update(GuardTaskState::default());
            guard.notice.set(true);
            guard
                .destination
                .set(Some(super::super::EditorDestination::Href(
                    "/original".into(),
                )));
            stale.complete_save(
                GuardLeaveAttempt {
                    registration: stale.id,
                    navigation: guard.navigation.get_untracked(),
                    scope,
                },
                GuardSaveResolution::Saved {
                    submitted_generation: 0,
                },
            );
            assert!(guard.notice.get_untracked());
            assert!(guard.destination.get_untracked().is_some());
            for handle in handles {
                handle.unregister();
            }
            assert!(
                guard.dirty.get_untracked(),
                "another task is still independently dirty"
            );
            newest.unregister();
            assert!(!guard.dirty.get_untracked());
        });
    }
    #[test]
    fn unknown_discard_is_not_cancellation_of_sent_recipe_or_unretained_evidence() {
        let mut state = GuardTaskState {
            uncertain: true,
            ..Default::default()
        };
        assert!(!state.may_discard(GuardTaskKind::Recipe));
        assert!(!state.may_discard(GuardTaskKind::Evidence));
        state.recovery_retained = true;
        assert!(state.may_discard(GuardTaskKind::Evidence));
        assert!(!state.may_discard(GuardTaskKind::Recipe));
        state.in_flight = true;
        assert!(!state.may_discard(GuardTaskKind::Evidence));
    }
    #[test]
    fn actual_navigation_keeps_first_destination_and_old_completion_cannot_satisfy_a_new_attempt() {
        let owner = Owner::new();
        owner.with(|| {
            let guard = super::super::EditorGuard::new();
            let scope = GuardScope { installation: "local".into(), account_id: "a".into(), workspace_id: "w".into(), resource_id: None, base_revision: None };
            let callbacks = || GuardTaskCallbacks { save_and_leave: None, return_to_actions: None, discard_unsent: Callback::new(|()| true) };
            let first = guard.register_task(GuardTaskKind::Recipe, scope.clone(), callbacks());
            let second = guard.register_task(GuardTaskKind::Equipment, scope.clone(), callbacks());
            let dirty = GuardTaskState { dirty: true, generation: 3, context_visible: true, ..Default::default() };
            first.update(dirty); second.update(dirty);
            guard.request(super::super::EditorDestination::Href("/first".into()));
            let attempt = GuardLeaveAttempt { navigation: guard.navigation.get_untracked(), registration: first.id, scope };
            guard.request(super::super::EditorDestination::Href("/second".into()));
            assert!(matches!(guard.destination.get_untracked(), Some(super::super::EditorDestination::Href(ref href)) if href == "/first"));
            first.update(GuardTaskState { dirty: false, ..dirty });
            first.complete_save(attempt.clone(), GuardSaveResolution::Saved { submitted_generation: 3 });
            assert!(guard.notice.get_untracked(), "independent equipment edits still block the captured exit");
            guard.cancel_leave();
            guard.request(super::super::EditorDestination::Href("/new-attempt".into()));
            first.complete_save(attempt, GuardSaveResolution::Saved { submitted_generation: 3 });
            assert!(guard.notice.get_untracked());
            assert!(matches!(guard.destination.get_untracked(), Some(super::super::EditorDestination::Href(ref href)) if href == "/new-attempt"));
        });
    }
    #[test]
    fn save_requires_original_owner_scope_navigation_generation_and_known_clean_result() {
        let owner = Owner::new();
        owner.with(|| {
            let scope = GuardScope {
                installation: "local".into(),
                account_id: "a".into(),
                workspace_id: "w".into(),
                resource_id: None,
                base_revision: None,
            };
            let mut task = GuardTask {
                id: 4,
                kind: GuardTaskKind::Recipe,
                scope: scope.clone(),
                state: GuardTaskState {
                    generation: 3,
                    context_visible: true,
                    ..Default::default()
                },
                callbacks: GuardTaskCallbacks {
                    save_and_leave: None,
                    return_to_actions: None,
                    discard_unsent: Callback::new(|()| true),
                },
                resolved_navigation: None,
            };
            let attempt = GuardLeaveAttempt {
                navigation: 8,
                registration: 4,
                scope,
            };
            let saved = GuardSaveResolution::Saved {
                submitted_generation: 3,
            };
            assert!(save_matches(&task, &attempt, 8, saved));
            assert!(!save_matches(&task, &attempt, 9, saved));
            assert!(!save_matches(
                &task,
                &attempt,
                8,
                GuardSaveResolution::Failed
            ));
            assert!(!save_matches(
                &task,
                &attempt,
                8,
                GuardSaveResolution::NewerEdits
            ));
            task.id = 5;
            assert!(!save_matches(&task, &attempt, 8, saved));
            task.id = 4;
            task.scope.workspace_id = "other".into();
            assert!(!save_matches(&task, &attempt, 8, saved));
            task.scope = attempt.scope.clone();
            task.state.generation += 1;
            assert!(!save_matches(&task, &attempt, 8, saved));
            task.state.generation = 3;
            task.state.uncertain = true;
            assert!(!save_matches(&task, &attempt, 8, saved));
            task.state.uncertain = false;
            task.state.dirty = true;
            assert!(!save_matches(&task, &attempt, 8, saved));
        });
    }
}
