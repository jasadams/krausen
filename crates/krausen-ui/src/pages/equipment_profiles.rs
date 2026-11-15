// SPDX-License-Identifier: AGPL-3.0-or-later
//! Workspace equipment profile management backed by shared application operations.
//!
//! The manager is independent of any recipe draft: it owns its own raw/name/settings
//! baseline, guarded navigation, request identity and revision handling. The recipe
//! editor consumes only [`ProfileLibrary`] (paged read access) for selection.
use crate::{
    components::{
        BreweryActionGroup, BreweryDraftField, BreweryDraftSelect, BreweryNotice, BreweryPanel,
        BrewerySection, Button, ButtonVariant,
        layout::{
            EditorGuard, GuardLeaveAttempt, GuardSaveResolution, GuardScope, GuardTaskCallbacks,
            GuardTaskKind, GuardTaskState,
        },
    },
    pages::recipe_editor::current_context_matches,
    server_fns::equipment::equipment_command_shared,
};
use krausen_types::{
    brewery::{ChillingV1, EquipmentProfile, EquipmentProvenance, EquipmentSettingsV1},
    equipment::{
        ArchiveEquipment, EquipmentCommand, EquipmentListQuery, EquipmentOutcome, SaveEquipment,
    },
    request_context::OperationIntent,
};
use leptos::prelude::*;

/// Paged, workspace-scoped read access to equipment profiles (active and archived).
#[derive(Clone, Copy)]
pub struct ProfileLibrary {
    pub profiles: RwSignal<Vec<EquipmentProfile>>,
    pub next_cursor: RwSignal<Option<String>>,
    pub loading: RwSignal<bool>,
    pub loaded: RwSignal<bool>,
    pub error: RwSignal<String>,
    reload: RwSignal<u64>,
    generation: RwSignal<u64>,
    identity: RwSignal<OperationIntent>,
    context_visible: RwSignal<bool>,
}

impl ProfileLibrary {
    pub fn reload(self) {
        self.reload.update(|n| *n += 1);
    }
    pub fn load_more(self) {
        if self.loading.get_untracked() {
            return;
        }
        if let Some(cursor) = self.next_cursor.get_untracked() {
            self.load(Some(cursor));
        }
    }
    fn load(self, cursor: Option<String>) {
        if cursor.is_none() {
            self.generation.update(|n| *n += 1);
        }
        let generation = self.generation.get_untracked();
        let append = cursor.is_some();
        self.loading.set(true);
        let expected = self.identity.get_untracked();
        let Self {
            profiles,
            next_cursor,
            loading,
            loaded,
            error,
            generation: list_generation,
            context_visible,
            ..
        } = self;
        leptos::task::spawn_local(async move {
            if !current_context_matches(&expected).await {
                if list_generation.try_get_untracked() == Some(generation) {
                    loading.set(false);
                    context_visible.set(false);
                }
                return;
            }
            let outcome = equipment_command_shared(EquipmentCommand::List(EquipmentListQuery {
                intent: expected.clone(),
                cursor,
                limit: 100,
                include_archived: true,
            }))
            .await;
            if list_generation.try_get_untracked() != Some(generation) {
                return;
            }
            if !current_context_matches(&expected).await {
                if list_generation.try_get_untracked() == Some(generation) {
                    loading.set(false);
                    context_visible.set(false);
                }
                return;
            }
            if list_generation.try_get_untracked() != Some(generation) {
                return;
            }
            loading.set(false);
            match outcome {
                Ok(EquipmentOutcome::Profiles(page)) => {
                    if append {
                        profiles.update(|items| items.extend(page.items));
                    } else {
                        profiles.set(page.items);
                    }
                    next_cursor.set(page.next_cursor);
                    loaded.set(true);
                    error.set(String::new());
                }
                Ok(EquipmentOutcome::ContextChanged | EquipmentOutcome::Forbidden) => {
                    context_visible.set(false)
                }
                _ => error.set("Unable to load equipment profiles.".into()),
            }
        });
    }
}

/// Load the workspace profile library and reload whenever its context becomes visible.
pub fn use_profile_library(
    identity: RwSignal<OperationIntent>,
    context_visible: RwSignal<bool>,
) -> ProfileLibrary {
    let library = ProfileLibrary {
        profiles: RwSignal::new(Vec::new()),
        next_cursor: RwSignal::new(None),
        loading: RwSignal::new(false),
        loaded: RwSignal::new(false),
        error: RwSignal::new(String::new()),
        reload: RwSignal::new(0),
        generation: RwSignal::new(0),
        identity,
        context_visible,
    };
    Effect::new(move |_| {
        let _ = library.reload.get();
        if !context_visible.get() {
            return;
        }
        library.load(None);
    });
    library
}

/// Raw, user-visible profile fields. Conditional chilling values stay in `raw`
/// while their mode is hidden, so changing the mode never discards typed text.
#[derive(Clone, Debug, PartialEq)]
pub struct ProfileForm {
    pub name: String,
    pub raw: Vec<String>,
    pub chilling: String,
}

impl ProfileForm {
    pub fn blank() -> Self {
        Self {
            name: String::new(),
            raw: vec![String::new(); FIELDS.len()],
            chilling: String::new(),
        }
    }
    pub fn from_profile(profile: Option<&EquipmentProfile>) -> Self {
        let Some(profile) = profile else {
            return Self::blank();
        };
        let raw = values(&profile.settings)
            .into_iter()
            .enumerate()
            .map(|(index, value)| {
                value
                    .map(|v| {
                        if index < 2 {
                            ((v * 100.0) * 1e9).round() / 1e9
                        } else {
                            v
                        }
                        .to_string()
                    })
                    .unwrap_or_default()
            })
            .collect();
        Self {
            name: profile.name.clone(),
            raw,
            chilling: match &profile.settings.chilling {
                Some(ChillingV1::Natural) => "natural",
                Some(ChillingV1::Forced { .. }) => "forced",
                Some(ChillingV1::Counterflow { .. }) => "counterflow",
                None => "",
            }
            .into(),
        }
    }
    /// Per-field validation message, or `None` when the raw text is empty or numeric.
    pub fn field_error(&self, index: usize) -> Option<&'static str> {
        let text = self.raw.get(index)?.trim();
        if text.is_empty() {
            return None;
        }
        match text.parse::<f64>() {
            Ok(value) if value.is_finite() => None,
            Ok(_) => Some("Enter a finite number."),
            Err(_) => Some("Enter a number."),
        }
    }
    /// Convert to native settings (efficiencies as fractions), preserving unmanaged extensions.
    pub fn parse(&self, base: &EquipmentSettingsV1) -> Result<EquipmentSettingsV1, &'static str> {
        let mut parsed = Vec::with_capacity(FIELDS.len());
        for index in 0..FIELDS.len() {
            if let Some(message) = self.field_error(index) {
                return Err(message);
            }
            let text = self.raw[index].trim();
            parsed.push(if text.is_empty() {
                None
            } else {
                text.parse::<f64>().ok()
            });
        }
        for value in parsed.iter_mut().take(2).flatten() {
            *value /= 100.0;
        }
        let mut settings = base.clone();
        assign_values(&mut settings, &parsed);
        settings.chilling = match self.chilling.as_str() {
            "natural" => Some(ChillingV1::Natural),
            "forced" => Some(ChillingV1::Forced {
                rate_per_minute: parsed[11],
                plateau_c: parsed[12],
            }),
            "counterflow" => Some(ChillingV1::Counterflow {
                litres_per_minute: parsed[13],
            }),
            _ => None,
        };
        Ok(settings)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum AfterSave {
    Stay,
    Switch(String),
    Leave(GuardLeaveAttempt),
}

fn pending_key(scope: &OperationIntent) -> String {
    format!(
        "krausen-equipment-pending-v1:{}",
        serde_json::to_string(&(
            GuardScope::installation(),
            &scope.account_id,
            &scope.workspace_id
        ))
        .unwrap_or_default()
    )
}

#[cfg(target_arch = "wasm32")]
fn read_pending(key: &str) -> Option<EquipmentCommand> {
    let storage = web_sys::window()?.local_storage().ok().flatten()?;
    serde_json::from_str(&storage.get_item(key).ok().flatten()?).ok()
}
#[cfg(not(target_arch = "wasm32"))]
fn read_pending(_: &str) -> Option<EquipmentCommand> {
    None
}
#[cfg(target_arch = "wasm32")]
fn write_pending(key: &str, command: Option<&EquipmentCommand>) -> bool {
    let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) else {
        return false;
    };
    match command.and_then(|c| serde_json::to_string(c).ok()) {
        Some(json) => storage.set_item(key, &json).is_ok(),
        None => storage.remove_item(key).is_ok(),
    }
}
#[cfg(not(target_arch = "wasm32"))]
fn write_pending(_: &str, _: Option<&EquipmentCommand>) -> bool {
    true
}

#[cfg(target_arch = "wasm32")]
fn focus_element(id: &str) {
    use wasm_bindgen::JsCast;
    let id = id.to_owned();
    // Wait for the replacement control to exist before choosing its successor.
    request_animation_frame(move || {
        if let Some(element) = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.get_element_by_id(&id))
            .and_then(|e| e.dyn_into::<web_sys::HtmlElement>().ok())
        {
            if element.focus().is_err() {
                tracing::warn!("Unable to move focus to {id}");
            }
        }
    });
}
#[cfg(not(target_arch = "wasm32"))]
fn focus_element(_: &str) {}

#[cfg(target_arch = "wasm32")]
fn set_select_value(id: &str, value: &str) {
    use wasm_bindgen::JsCast;
    if let Some(select) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id(id))
        .and_then(|e| e.dyn_into::<web_sys::HtmlSelectElement>().ok())
    {
        select.set_value(value);
    }
}
#[cfg(not(target_arch = "wasm32"))]
fn set_select_value(_: &str, _: &str) {}

/// The reusable Equipment manager. It never reads or writes a recipe draft.
#[component]
pub fn EquipmentManager(
    identity: RwSignal<OperationIntent>,
    context_visible: RwSignal<bool>,
    /// Called after a profile save/archive is confirmed, e.g. to refresh a preview.
    #[prop(optional)]
    on_changed: Option<Callback<()>>,
    /// Profile to open for editing once it has been loaded (contextual Manage).
    #[prop(optional)]
    initial_profile: String,
) -> impl IntoView {
    let library = use_profile_library(identity, context_visible);
    let editing = RwSignal::new(None::<EquipmentProfile>);
    let form = RwSignal::new(ProfileForm::blank());
    let baseline = RwSignal::new(ProfileForm::blank());
    let form_generation = RwSignal::new(0_u64);
    let pending = RwSignal::new(None::<EquipmentCommand>);
    let busy = RwSignal::new(false);
    let blocked = RwSignal::new(false);
    let status = RwSignal::new(String::new());
    let switch_target = RwSignal::new(None::<String>);
    let initial = StoredValue::new((!initial_profile.is_empty()).then_some(initial_profile));
    let after_save = StoredValue::new(AfterSave::Stay);
    let submitted_generation = StoredValue::new(0_u64);
    let dirty = Memo::new(move |_| form.with(|f| baseline.with(|b| f != b)));
    let key = StoredValue::new(pending_key(&identity.get_untracked()));

    let apply = Callback::new(move |profile: Option<EquipmentProfile>| {
        let next = ProfileForm::from_profile(profile.as_ref());
        baseline.set(next.clone());
        form.set(next);
        editing.set(profile);
        status.set(String::new());
        form_generation.update(|n| *n += 1);
    });
    let edit = move |change: &dyn Fn(&mut ProfileForm)| {
        if pending.get_untracked().is_some() || busy.get_untracked() {
            return;
        }
        form.update(change);
        form_generation.update(|n| *n += 1);
    };

    // Recover one unresolved request for this account/workspace; same identity and payload.
    if let Some(command) = read_pending(&key.get_value()) {
        if let EquipmentCommand::Save(save) = &command {
            let profile = EquipmentProfile {
                workspace_id: save.intent.workspace_id.clone(),
                profile_id: save.profile_id.clone(),
                created_at: 0,
                name: save.name.clone(),
                revision: save.expected_revision,
                metadata_revision: save.expected_metadata_revision,
                archived: false,
                settings: save.settings.clone(),
                provenance: save.provenance.clone(),
                saved_by: String::new(),
                saved_at: 0,
            };
            let next = ProfileForm::from_profile(Some(&profile));
            baseline.set(next.clone());
            form.set(next);
        }
        status.set("A previous profile request was not confirmed. Retry it to check the result.".into());
        pending.set(Some(command));
    }

    let editor_guard = expect_context::<EditorGuard>();
    let guard_save = StoredValue::new(None::<Callback<GuardLeaveAttempt>>);
    let source = identity.get_untracked();
    let task = editor_guard.register_task(
        GuardTaskKind::Equipment,
        GuardScope {
            installation: GuardScope::installation(),
            account_id: source.account_id,
            workspace_id: source.workspace_id,
            resource_id: None,
            base_revision: None,
        },
        GuardTaskCallbacks {
            save_and_leave: Some(Callback::new(move |attempt| {
                if let Some(save) = guard_save.get_value() {
                    save.run(attempt);
                }
            })),
            return_to_actions: None,
            discard_unsent: Callback::new(move |()| {
                if busy.get_untracked() {
                    return false;
                }
                if pending.get_untracked().is_some() {
                    // The unresolved request is retained for recovery; leaving cannot cancel it.
                    return true;
                }
                let original = baseline.get_untracked();
                form.set(original);
                form_generation.update(|n| *n += 1);
                true
            }),
        },
    );
    let guard_state = move || GuardTaskState {
        dirty: dirty.get_untracked(),
        generation: form_generation.get_untracked(),
        in_flight: busy.get_untracked(),
        uncertain: pending.get_untracked().is_some(),
        recovery_retained: pending.get_untracked().is_some(),
        context_visible: context_visible.get_untracked(),
    };
    Effect::new(move |_| {
        let _ = (
            dirty.get(),
            form_generation.get(),
            busy.get(),
            pending.get(),
            context_visible.get(),
        );
        task.update(guard_state());
    });
    on_cleanup(move || task.unregister());
    Effect::new(move |_| editor_guard.message.set(status.get()));
    #[cfg(target_arch = "wasm32")]
    {
        let location = leptos_router::hooks::use_location();
        let origin = web_sys::window()
            .and_then(|w| w.location().origin().ok())
            .unwrap_or_default();
        editor_guard.current_url.set(format!(
            "{}{}{}{}",
            origin,
            location.pathname.get_untracked(),
            location.search.get_untracked(),
            location.hash.get_untracked()
        ));
    }

    // Open the requested profile once it appears in the paged library.
    Effect::new(move |_| {
        let Some(id) = initial.get_value() else {
            return;
        };
        let found = library
            .profiles
            .with(|list| list.iter().find(|p| p.profile_id == id).cloned());
        if let Some(profile) = found {
            initial.set_value(None);
            if !dirty.get_untracked() && pending.get_untracked().is_none() {
                apply.run(Some(profile));
            }
        } else if library.loaded.get() && !library.loading.get() && library.next_cursor.get().is_some() {
            library.load_more();
        } else if library.loaded.get() && !library.loading.get() {
            initial.set_value(None);
        }
    });

    let execute = Callback::new(move |command: EquipmentCommand| {
        if busy.get_untracked() {
            return;
        }
        if !write_pending(&key.get_value(), Some(&command)) {
            status.set("Profile saving is unavailable because this browser cannot retain the request identity.".into());
            return;
        }
        pending.set(Some(command.clone()));
        busy.set(true);
        submitted_generation.set_value(form_generation.get_untracked());
        let expected = identity.get_untracked();
        leptos::task::spawn_local(async move {
            if !current_context_matches(&expected).await {
                if busy.try_set(false).is_some() {
                    context_visible.set(false);
                }
                return;
            }
            let outcome = equipment_command_shared(command).await;
            if busy.try_set(false).is_none() {
                return;
            }
            if !current_context_matches(&expected).await {
                context_visible.set(false);
                return;
            }
            let clear = || {
                if !write_pending(&key.get_value(), None) {
                    tracing::warn!("Unable to clear the retained equipment request");
                }
            };
            match outcome {
                Ok(EquipmentOutcome::Profile(profile)) => {
                    clear();
                    pending.set(None);
                    let archived = profile.archived;
                    let unchanged = form_generation.get_untracked() == submitted_generation.get_value();
                    let action = after_save.get_value();
                    after_save.set_value(AfterSave::Stay);
                    let message = if archived { "Profile archived." } else { "Profile saved." };
                    if unchanged {
                        apply.run(Some(profile.clone()));
                    } else {
                        baseline.set(ProfileForm::from_profile(Some(&profile)));
                        editing.set(Some(profile.clone()));
                    }
                    status.set(message.into());
                    library.reload();
                    if let Some(changed) = on_changed {
                        changed.run(());
                    }
                    match action {
                        AfterSave::Switch(id) if unchanged => {
                            let item = library.profiles.with_untracked(|l| {
                                l.iter().find(|p| p.profile_id == id).cloned()
                            });
                            apply.run(item);
                            status.set(message.into());
                            set_select_value("equipment-profile-edit", &id);
                            focus_element("equipment-profile-edit");
                        }
                        AfterSave::Leave(attempt) => {
                            task.update(guard_state());
                            task.complete_save(
                                attempt,
                                if unchanged {
                                    GuardSaveResolution::Saved {
                                        submitted_generation: submitted_generation.get_value(),
                                    }
                                } else {
                                    GuardSaveResolution::NewerEdits
                                },
                            );
                        }
                        _ => {}
                    }
                }
                Ok(EquipmentOutcome::ContextChanged | EquipmentOutcome::Forbidden) => {
                    context_visible.set(false)
                }
                Ok(EquipmentOutcome::Unavailable) => {
                    status.set("The result is uncertain. Retry the same request.".into());
                }
                Ok(EquipmentOutcome::ResultExpired | EquipmentOutcome::RequestIdentityConflict) => {
                    clear();
                    blocked.set(true);
                    status.set("The previous request cannot be confirmed. Review the profile list before continuing.".into());
                }
                Ok(EquipmentOutcome::Conflict { .. }) => {
                    clear();
                    pending.set(None);
                    after_save.set_value(AfterSave::Stay);
                    status.set(
                        "Profile changed elsewhere. Your edits are kept; copy them, then reload the latest version.".into(),
                    );
                    library.reload();
                }
                Ok(_) => {
                    clear();
                    pending.set(None);
                    after_save.set_value(AfterSave::Stay);
                    status.set("Profile could not be saved. Check the fields and try again.".into());
                }
                Err(_) => {
                    status.set("Connection interrupted. Retry to confirm the same request.".into())
                }
            }
        });
    });

    let submit_save = Callback::new(move |after: AfterSave| {
        if blocked.get_untracked() || busy.get_untracked() {
            return;
        }
        if let Some(command) = pending.get_untracked() {
            execute.run(command);
            return;
        }
        let current = editing.get_untracked();
        let base = current
            .as_ref()
            .map(|p| p.settings.clone())
            .unwrap_or_default();
        let current_form = form.get_untracked();
        let settings = match current_form.parse(&base) {
            Ok(settings) => settings,
            Err(message) => {
                status.set(message.into());
                return;
            }
        };
        let Some(request_id) = crate::utils::request_id::new_request_id() else {
            status.set("Unable to create request identity.".into());
            return;
        };
        let profile_id = match &current {
            Some(p) => p.profile_id.clone(),
            None => match crate::utils::request_id::new_request_id() {
                Some(id) => id,
                None => {
                    status.set("Unable to create profile identity.".into());
                    return;
                }
            },
        };
        after_save.set_value(after);
        execute.run(EquipmentCommand::Save(SaveEquipment {
            intent: identity.get_untracked(),
            profile_id,
            name: current_form.name,
            settings,
            provenance: current.as_ref().map(|p| p.provenance.clone()).unwrap_or(
                EquipmentProvenance {
                    source: "manual".into(),
                    assumptions: vec![],
                },
            ),
            expected_revision: current.as_ref().map(|p| p.revision).unwrap_or(0),
            expected_metadata_revision: current.as_ref().map(|p| p.metadata_revision).unwrap_or(0),
            request_id,
        }));
    });
    guard_save.set_value(Some(Callback::new(move |attempt| {
        submit_save.run(AfterSave::Leave(attempt))
    })));
    let archive = move |_| {
        if blocked.get_untracked() || dirty.get_untracked() {
            return;
        }
        if let Some(command) = pending.get_untracked() {
            execute.run(command);
            return;
        }
        let Some(profile) = editing.get_untracked() else {
            return;
        };
        let Some(request_id) = crate::utils::request_id::new_request_id() else {
            status.set("Unable to create request identity.".into());
            return;
        };
        after_save.set_value(AfterSave::Stay);
        execute.run(EquipmentCommand::Archive(ArchiveEquipment {
            intent: identity.get_untracked(),
            profile_id: profile.profile_id,
            archived: !profile.archived,
            expected_metadata_revision: profile.metadata_revision,
            request_id,
        }));
    };

    let change_profile = Callback::new(move |id: String| {
        let current = editing.get_untracked().map(|p| p.profile_id).unwrap_or_default();
        if id == current {
            return;
        }
        // The select is not the source of truth; put the DOM back before any rejection.
        set_select_value("equipment-profile-edit", &current);
        if pending.get_untracked().is_some() || busy.get_untracked() {
            status.set("Retry the previous request before changing profiles.".into());
            return;
        }
        if dirty.get_untracked() {
            switch_target.set(Some(id));
            return;
        }
        let item = library
            .profiles
            .with_untracked(|list| list.iter().find(|p| p.profile_id == id).cloned());
        apply.run(item);
        set_select_value("equipment-profile-edit", &id);
    });
    Effect::new(move |previous: Option<bool>| {
        let open = switch_target.get().is_some();
        if open && previous != Some(true) {
            focus_element("profile-switch-choice");
        }
        open
    });
    let finish_switch = move |id: String| {
        switch_target.set(None);
        let item = library
            .profiles
            .with_untracked(|list| list.iter().find(|p| p.profile_id == id).cloned());
        apply.run(item);
        set_select_value("equipment-profile-edit", &id);
        focus_element("equipment-profile-edit");
    };

    let locked = Signal::derive(move || busy.get() || pending.get().is_some());
    let heading = Signal::derive(move || match editing.get() {
        Some(p) => format!("Editing shared profile “{}”", p.name),
        None => "New equipment profile".to_owned(),
    });
    let field = move |index: usize| {
        let (id, label, unit) = FIELDS[index];
        let value = Signal::derive(move || form.with(|f| f.raw[index].clone()));
        let error = Signal::derive(move || {
            form.with(|f| f.field_error(index).map(str::to_owned))
        });
        let change = Callback::new(move |text: String| edit(&|f| f.raw[index] = text.clone()));
        view! { <BreweryDraftField id=id label=label unit=unit value=value error=error on_change=change numeric=true/> }
    };
    view! {
        <BreweryPanel title="Equipment profiles" description="Reusable workspace equipment. Saving a profile updates future previews and new brews; it does not change any recipe's content or earlier brews.">
            <p class="recipe-editor__provenance">{move || heading.get()}</p>
            <div class="recipe-editor__fields">
                {move || {
                    let mut options = vec![(String::new(), "New profile".to_owned())];
                    options.extend(library.profiles.get().into_iter().map(|p| {
                        let label = if p.archived { format!("{} (archived)", p.name) } else { p.name.clone() };
                        (p.profile_id, label)
                    }));
                    let selected = Signal::derive(move || editing.get().map(|p| p.profile_id).unwrap_or_default());
                    view! { <BreweryDraftSelect id="equipment-profile-edit" label="Manage profile" value=selected on_change=change_profile options=options disabled=locked.get()/> }
                }}
            </div>
            <Show when=move || library.next_cursor.get().is_some()>
                <Button variant=ButtonVariant::Secondary disabled=library.loading on:click=move |_| library.load_more()>"Load more profiles"</Button>
            </Show>
            <Show when=move || !library.error.get().is_empty()>
                <BreweryNotice title="Profiles unavailable" body=library.error.get() warning=true/>
            </Show>
            <Show when=move || switch_target.get().is_some()>
                <div id="profile-switch-choice" tabindex="-1" role="group" aria-label="Unsaved profile changes">
                    <BreweryNotice title="Unsaved profile changes" body="Save this shared profile, discard the edits, or stay on it before choosing another." warning=true actions=std::sync::Arc::new(move || view! {
                        <Button disabled=Signal::derive(move || busy.get() || blocked.get()) on:click=move |_| {
                            if let Some(id) = switch_target.get_untracked() {
                                switch_target.set(None);
                                submit_save.run(AfterSave::Switch(id));
                            }
                        }>"Save profile and switch"</Button>
                        <Button variant=ButtonVariant::Secondary disabled=locked on:click=move |_| {
                            if let Some(id) = switch_target.get_untracked() { finish_switch(id); }
                        }>"Discard changes and switch"</Button>
                        <Button variant=ButtonVariant::Secondary on:click=move |_| { switch_target.set(None); focus_element("equipment-profile-edit"); }>"Stay on profile"</Button>
                    }.into_any())/>
                </div>
            </Show>
            <fieldset class="recipe-editor__fieldset" disabled=move || locked.get()>
                <BrewerySection title="Name">
                    <BreweryDraftField id="equipment-profile-name" label="Profile name" value=Signal::derive(move || form.with(|f| f.name.clone())) on_change=Callback::new(move |v: String| edit(&|f| f.name = v.clone()))/>
                </BrewerySection>
                <BrewerySection title="Efficiency">
                    <div class="recipe-editor__fields">{field(0)}{field(1)}</div>
                </BrewerySection>
                <BrewerySection title="Water and losses">
                    <div class="recipe-editor__fields">{field(2)}{field(3)}{field(4)}{field(5)}{field(6)}</div>
                </BrewerySection>
                <BrewerySection title="Vessel">
                    <div class="recipe-editor__fields">{field(7)}{field(8)}{field(9)}{field(10)}</div>
                </BrewerySection>
                <BrewerySection title="Chilling">
                    <div class="recipe-editor__fields">
                        <BreweryDraftSelect id="profile-chilling" label="Profile chilling" value=Signal::derive(move || form.with(|f| f.chilling.clone())) on_change=Callback::new(move |v: String| edit(&|f| f.chilling = v.clone())) options=vec![("".into(), "Not specified".into()), ("natural".into(), "Natural".into()), ("forced".into(), "Forced".into()), ("counterflow".into(), "Counterflow".into())]/>
                        <Show when=move || form.with(|f| f.chilling == "forced")>{field(11)}{field(12)}</Show>
                        <Show when=move || form.with(|f| f.chilling == "counterflow")>{field(13)}</Show>
                    </div>
                </BrewerySection>
            </fieldset>
            <BreweryActionGroup label="Profile actions">
                <Button disabled=Signal::derive(move || busy.get() || blocked.get() || (pending.get().is_none() && !dirty.get() && editing.with(|p| p.is_some()))) on:click=move |_| submit_save.run(AfterSave::Stay)>{move || if pending.get().is_some() { "Retry previous request" } else { "Save profile" }}</Button>
                <Button variant=ButtonVariant::Secondary disabled=Signal::derive(move || locked.get() || !dirty.get()) on:click=move |_| { form.set(baseline.get_untracked()); form_generation.update(|n| *n += 1); status.set(String::new()); }>"Discard changes"</Button>
                <Show when=move || editing.with(|p| p.is_some())>
                    <Button variant=ButtonVariant::Secondary disabled=Signal::derive(move || busy.get() || blocked.get() || pending.get().is_some() || dirty.get()) on:click=archive>{move || if editing.with(|p| p.as_ref().is_some_and(|p| p.archived)) { "Unarchive" } else { "Archive" }}</Button>
                </Show>
            </BreweryActionGroup>
            <Show when=move || dirty.get()><p class="k-field__hint">"Unsaved profile changes. Save or discard them before archiving."</p></Show>
            <p role="status">{move || status.get()}</p>
        </BreweryPanel>
    }
}

pub const FIELDS: [(&str, &str, &str); 14] = [
    ("profile-efficiency-override", "Brewhouse efficiency", "%"),
    ("profile-mash-efficiency", "Mash efficiency", "%"),
    ("profile-boil-off", "Boil-off", "L/h"),
    ("profile-transfer-loss", "Transfer loss", "L"),
    ("profile-deadspace", "Pre-boil deadspace", "L"),
    ("profile-absorption", "Grain absorption", "L/kg"),
    ("profile-thickness", "Mash thickness", "L/kg"),
    ("profile-capacity", "Kettle capacity", "L"),
    ("profile-diameter", "Kettle diameter", "cm"),
    ("profile-opening", "Kettle opening", "cm"),
    ("profile-boil-temperature", "Boil temperature", "°C"),
    ("profile-cooling-rate", "Forced cooling rate", "per min"),
    ("profile-cooling-plateau", "Cooling plateau", "°C"),
    ("profile-counterflow-rate", "Counterflow rate", "L/min"),
];
fn values(s: &EquipmentSettingsV1) -> [Option<f64>; 14] {
    [
        s.efficiency_override_fraction,
        s.mash_efficiency_fraction,
        s.boil_off_l_per_hour,
        s.transfer_loss_l,
        s.pre_boil_deadspace_l,
        s.grain_absorption_l_per_kg,
        s.mash_thickness_l_per_kg,
        s.kettle_capacity_l,
        s.kettle_diameter_cm,
        s.kettle_opening_cm,
        s.boil_temperature_c,
        match &s.chilling {
            Some(ChillingV1::Forced {
                rate_per_minute, ..
            }) => *rate_per_minute,
            _ => None,
        },
        match &s.chilling {
            Some(ChillingV1::Forced { plateau_c, .. }) => *plateau_c,
            _ => None,
        },
        match &s.chilling {
            Some(ChillingV1::Counterflow { litres_per_minute }) => *litres_per_minute,
            _ => None,
        },
    ]
}
fn assign_values(s: &mut EquipmentSettingsV1, v: &[Option<f64>]) {
    s.efficiency_override_fraction = v[0];
    s.mash_efficiency_fraction = v[1];
    s.boil_off_l_per_hour = v[2];
    s.transfer_loss_l = v[3];
    s.pre_boil_deadspace_l = v[4];
    s.grain_absorption_l_per_kg = v[5];
    s.mash_thickness_l_per_kg = v[6];
    s.kettle_capacity_l = v[7];
    s.kettle_diameter_cm = v[8];
    s.kettle_opening_cm = v[9];
    s.boil_temperature_c = v[10];
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(settings: EquipmentSettingsV1) -> EquipmentProfile {
        EquipmentProfile {
            workspace_id: "w".into(),
            profile_id: "p".into(),
            created_at: 0,
            name: "Garage".into(),
            revision: 1,
            metadata_revision: 1,
            archived: false,
            settings,
            provenance: EquipmentProvenance {
                source: "manual".into(),
                assumptions: vec![],
            },
            saved_by: "u".into(),
            saved_at: 0,
        }
    }

    #[test]
    fn a_loaded_profile_is_clean_and_an_edit_is_dirty_against_its_baseline() {
        let loaded = ProfileForm::from_profile(Some(&profile(EquipmentSettingsV1 {
            mash_efficiency_fraction: Some(0.57),
            ..Default::default()
        })));
        assert_eq!(loaded.raw[1], "57");
        let mut edited = loaded.clone();
        assert_eq!(edited, loaded);
        edited.raw[2] = "3".into();
        assert_ne!(edited, loaded);
    }

    #[test]
    fn hidden_chilling_values_survive_mode_changes_but_only_the_active_mode_is_saved() {
        let mut form = ProfileForm::blank();
        form.chilling = "forced".into();
        form.raw[11] = "0.5".into();
        form.raw[12] = "20".into();
        form.raw[13] = "4".into();
        let forced = form.parse(&Default::default()).unwrap();
        assert_eq!(
            forced.chilling,
            Some(ChillingV1::Forced {
                rate_per_minute: Some(0.5),
                plateau_c: Some(20.0)
            })
        );
        form.chilling = "counterflow".into();
        assert_eq!(form.raw[11], "0.5");
        assert_eq!(
            form.parse(&Default::default()).unwrap().chilling,
            Some(ChillingV1::Counterflow {
                litres_per_minute: Some(4.0)
            })
        );
        form.chilling = "forced".into();
        assert_eq!(form.parse(&Default::default()).unwrap(), forced);
    }

    #[test]
    fn efficiency_is_entered_as_percent_and_stored_as_fraction_with_extensions_kept() {
        let mut base = EquipmentSettingsV1::default();
        base.extensions.insert("x".into(), serde_json::json!(1));
        let mut form = ProfileForm::blank();
        form.raw[0] = "70".into();
        form.raw[1] = "75".into();
        let parsed = form.parse(&base).unwrap();
        assert_eq!(parsed.efficiency_override_fraction, Some(0.7));
        assert_eq!(parsed.mash_efficiency_fraction, Some(0.75));
        assert_eq!(parsed.extensions, base.extensions);
    }

    #[test]
    fn malformed_and_non_finite_text_is_reported_per_field_and_blocks_parsing() {
        let mut form = ProfileForm::blank();
        form.raw[2] = "3,5".into();
        form.raw[3] = "inf".into();
        assert_eq!(form.field_error(2), Some("Enter a number."));
        assert_eq!(form.field_error(3), Some("Enter a finite number."));
        assert_eq!(form.field_error(4), None);
        assert!(form.parse(&Default::default()).is_err());
    }
}
