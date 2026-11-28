// SPDX-License-Identifier: AGPL-3.0-or-later
//! Workspace equipment profile management backed by shared application operations.
//!
//! The manager is independent of any recipe draft: it owns its own raw/name/settings
//! baseline, guarded navigation, request identity and revision handling. The recipe
//! editor consumes only [`ProfileLibrary`] (paged read access) for selection.
use crate::{
    components::{
        BreweryActionGroup, BreweryDraftArea, BreweryDraftField, BreweryDraftSelect,
        BreweryNotice, BreweryPanel, BrewerySection, Button, ButtonVariant, Switch,
        layout::{
            EditorGuard, GuardLeaveAttempt, GuardSaveResolution, GuardScope, GuardTaskCallbacks,
            GuardTaskKind, GuardTaskState,
        },
    },
    pages::recipe_editor::current_context_matches,
    server_fns::equipment::equipment_command_shared,
};
use krausen_types::{
    brewery::{
        CoolingMethodV1, EquipmentAboutV1, EquipmentProfile, EquipmentProvenance,
        EquipmentSettingsV1,
    },
    equipment::{
        ArchiveEquipment, EquipmentCommand, EquipmentListQuery, EquipmentOutcome, EquipmentQuery,
        SaveEquipment,
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

/// The workspace default profile: `Ok(None)` only when none is set. A failed
/// lookup is an error, never an unseeded recipe.
pub async fn load_default_profile(
    identity: &OperationIntent,
) -> Result<Option<EquipmentProfile>, ServerFnError> {
    match equipment_command_shared(EquipmentCommand::List(EquipmentListQuery {
        intent: identity.clone(),
        cursor: None,
        limit: 1,
        include_archived: false,
    }))
    .await
    {
        Ok(EquipmentOutcome::Profiles(page)) => Ok(page.default_profile),
        Ok(_) => Err(ServerFnError::new("Unable to load the default equipment profile.")),
        Err(error) => Err(error),
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

/// Raw, user-visible profile fields, in the order of the equipment spec
/// (`docs/EQUIPMENT_PROFILE.md`). Numeric values are kept as typed text.
#[derive(Clone, Debug, PartialEq)]
pub struct ProfileForm {
    pub is_default: bool,
    pub name: String,
    pub brand: String,
    pub description: String,
    pub equipment_type: String,
    pub controller_type: String,
    pub recirculation_system: String,
    pub sparge_water_alert: bool,
    pub sparge_progress_counter: bool,
    pub boil_power_control: bool,
    pub cooling_method: String,
    pub notes: String,
    /// One entry per [`FIELDS`] row.
    pub raw: Vec<String>,
}

/// Numeric fields: element id, spec label, unit.
pub const FIELDS: [(&str, &str, &str); 17] = [
    ("profile-batch-size", "Batch size", "L"),
    ("profile-vessel-count", "Number of vessels", ""),
    ("profile-mash-volume", "Mash tun volume", "L"),
    ("profile-mash-efficiency", "Mash efficiency", "%"),
    ("profile-mash-deadspace", "Mash tun deadspace", "L"),
    ("profile-mash-thickness", "Mash thickness", "L/kg"),
    ("profile-mash-loss", "Mash tun loss", "L"),
    ("profile-mash-absorption", "Grain absorption", "L/kg"),
    ("profile-sparge-volume", "Sparge vessel volume", "L"),
    ("profile-sparge-absorption", "Grain absorption", "L/kg"),
    ("profile-sparge-deadspace", "Sparge deadspace volume", "L"),
    ("profile-boil-volume", "Boil volume", "L"),
    ("profile-shrinkage", "Wort shrinkage", "%"),
    ("profile-boil-loss", "Boil loss", "L/hr"),
    ("profile-trub-loss", "Trub and chiller loss", "L"),
    ("profile-kettle-diameter", "Kettle diameter", "cm"),
    ("profile-cooling-flow", "Flow rate", "L/min"),
];
const BATCH: usize = 0;
const VESSELS: usize = 1;
const MASH_VOLUME: usize = 2;
const MASH_EFFICIENCY: usize = 3;
const MASH_DEADSPACE: usize = 4;
const MASH_THICKNESS: usize = 5;
const MASH_LOSS: usize = 6;
const MASH_ABSORPTION: usize = 7;
const SPARGE_VOLUME: usize = 8;
const SPARGE_ABSORPTION: usize = 9;
const SPARGE_DEADSPACE: usize = 10;
const BOIL_VOLUME: usize = 11;
const SHRINKAGE: usize = 12;
const BOIL_LOSS: usize = 13;
const TRUB_LOSS: usize = 14;
const KETTLE_DIAMETER: usize = 15;
const COOLING_FLOW: usize = 16;
/// Entered as percent, stored as a fraction.
const PERCENT: [usize; 2] = [MASH_EFFICIENCY, SHRINKAGE];

const EQUIPMENT_TYPES: [(&str, &str); 1] = [("all_grain", "All grain")];
/// Only a toggle the user changed is sent. The default is workspace state outside the
/// revision checks, so an untouched stale value must not overwrite another session's choice.
fn default_change(baseline: &ProfileForm, current: &ProfileForm) -> Option<bool> {
    (current.is_default != baseline.is_default).then_some(current.is_default)
}
fn optional_text(text: &str) -> Option<String> {
    let trimmed = text.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

impl ProfileForm {
    pub fn blank() -> Self {
        Self {
            is_default: false,
            name: String::new(),
            brand: String::new(),
            description: String::new(),
            equipment_type: "all_grain".into(),
            controller_type: String::new(),
            recirculation_system: String::new(),
            sparge_water_alert: false,
            sparge_progress_counter: false,
            boil_power_control: false,
            cooling_method: String::new(),
            notes: String::new(),
            raw: vec![String::new(); FIELDS.len()],
        }
    }
    pub fn from_profile(profile: Option<&EquipmentProfile>) -> Self {
        let Some(profile) = profile else {
            return Self::blank();
        };
        let (s, about) = (&profile.settings, &profile.about);
        let numbers = {
            let mut numbers = [None; FIELDS.len()];
            numbers[BATCH] = about.batch_size_l;
            numbers[VESSELS] = about.vessel_count.map(f64::from);
            numbers[MASH_VOLUME] = s.mash_tun_volume_l;
            numbers[MASH_EFFICIENCY] = s.mash_efficiency_fraction;
            numbers[MASH_DEADSPACE] = s.mash_tun_deadspace_l;
            numbers[MASH_THICKNESS] = s.mash_thickness_l_per_kg;
            numbers[MASH_LOSS] = s.mash_tun_loss_l;
            numbers[MASH_ABSORPTION] = s.mash_grain_absorption_l_per_kg;
            numbers[SPARGE_VOLUME] = s.sparge_vessel_volume_l;
            numbers[SPARGE_ABSORPTION] = s.sparge_grain_absorption_l_per_kg;
            numbers[SPARGE_DEADSPACE] = s.sparge_deadspace_l;
            numbers[BOIL_VOLUME] = s.boil_volume_l;
            numbers[SHRINKAGE] = s.wort_shrinkage_fraction;
            numbers[BOIL_LOSS] = s.boil_loss_l_per_hour;
            numbers[TRUB_LOSS] = s.trub_and_chiller_loss_l;
            numbers[KETTLE_DIAMETER] = s.kettle_diameter_cm;
            numbers[COOLING_FLOW] = s.cooling_flow_rate_l_per_minute;
            numbers
        };
        let raw = numbers
            .into_iter()
            .enumerate()
            .map(|(index, value)| {
                value
                    .map(|v| {
                        if PERCENT.contains(&index) {
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
            is_default: profile.is_default,
            name: profile.name.clone(),
            brand: about.brand.clone().unwrap_or_default(),
            description: about.description.clone().unwrap_or_default(),
            equipment_type: about.equipment_type.clone().unwrap_or_default(),
            controller_type: about.controller_type.clone().unwrap_or_default(),
            recirculation_system: about.recirculation_system.clone().unwrap_or_default(),
            sparge_water_alert: about.sparge_water_alert,
            sparge_progress_counter: about.sparge_progress_counter,
            boil_power_control: about.boil_power_control,
            cooling_method: s.cooling_method.map_or("", CoolingMethodV1::key).into(),
            notes: about.notes.clone().unwrap_or_default(),
            raw,
        }
    }
    /// Per-field validation message, or `None` when the raw text is empty or numeric.
    pub fn field_error(&self, index: usize) -> Option<&'static str> {
        let text = self.raw.get(index)?.trim();
        if text.is_empty() {
            return None;
        }
        if index == VESSELS {
            return match text.parse::<u32>() {
                Ok(count) if count > 0 => None,
                _ => Some("Enter a whole number of vessels."),
            };
        }
        match text.parse::<f64>() {
            Ok(value) if value.is_finite() => None,
            Ok(_) => Some("Enter a finite number."),
            Err(_) => Some("Enter a number."),
        }
    }
    /// Convert to native settings (percentages as fractions) and the about block,
    /// preserving unmanaged extensions of the saved settings.
    pub fn parse(
        &self,
        base: &EquipmentSettingsV1,
    ) -> Result<(EquipmentSettingsV1, EquipmentAboutV1), &'static str> {
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
        for index in PERCENT {
            if let Some(value) = parsed[index].as_mut() {
                *value /= 100.0;
            }
        }
        let mut settings = base.clone();
        settings.mash_tun_volume_l = parsed[MASH_VOLUME];
        settings.mash_efficiency_fraction = parsed[MASH_EFFICIENCY];
        settings.mash_tun_deadspace_l = parsed[MASH_DEADSPACE];
        settings.mash_thickness_l_per_kg = parsed[MASH_THICKNESS];
        settings.mash_tun_loss_l = parsed[MASH_LOSS];
        settings.mash_grain_absorption_l_per_kg = parsed[MASH_ABSORPTION];
        settings.sparge_vessel_volume_l = parsed[SPARGE_VOLUME];
        settings.sparge_grain_absorption_l_per_kg = parsed[SPARGE_ABSORPTION];
        settings.sparge_deadspace_l = parsed[SPARGE_DEADSPACE];
        settings.boil_volume_l = parsed[BOIL_VOLUME];
        settings.wort_shrinkage_fraction = parsed[SHRINKAGE];
        settings.boil_loss_l_per_hour = parsed[BOIL_LOSS];
        settings.trub_and_chiller_loss_l = parsed[TRUB_LOSS];
        settings.kettle_diameter_cm = parsed[KETTLE_DIAMETER];
        settings.cooling_method = CoolingMethodV1::from_key(&self.cooling_method);
        settings.cooling_flow_rate_l_per_minute = parsed[COOLING_FLOW];
        let about = EquipmentAboutV1 {
            brand: optional_text(&self.brand),
            description: optional_text(&self.description),
            measurement: Some("metric".into()),
            equipment_type: optional_text(&self.equipment_type),
            controller_type: optional_text(&self.controller_type),
            recirculation_system: optional_text(&self.recirculation_system),
            vessel_count: parsed[VESSELS].map(|v| v as u32),
            batch_size_l: parsed[BATCH],
            sparge_water_alert: self.sparge_water_alert,
            sparge_progress_counter: self.sparge_progress_counter,
            boil_power_control: self.boil_power_control,
            notes: optional_text(&self.notes),
        };
        Ok((settings, about))
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
    let conflict = RwSignal::new(false);
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
        conflict.set(false);
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
                is_default: save.is_default.unwrap_or(false),
                settings: save.settings.clone(),
                about: save.about.clone(),
                provenance: save.provenance.clone(),
                saved_by: String::new(),
                saved_at: 0,
            };
            let next = ProfileForm::from_profile(Some(&profile));
            baseline.set(next.clone());
            form.set(next);
            if save.expected_revision > 0 {
                editing.set(Some(profile));
            }
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
        recovery_retained: pending.get_untracked().is_some() && !blocked.get_untracked(),
        context_visible: context_visible.get_untracked(),
    };
    Effect::new(move |_| {
        let _ = (
            dirty.get(),
            form_generation.get(),
            busy.get(),
            pending.get(),
            blocked.get(),
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
        } else if library.loaded.get() && !library.loading.get() && library.error.get().is_empty() && library.next_cursor.get().is_some() {
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
                if busy.try_set(false).is_none() {
                    context_visible.set(false);
                }
                return;
            }
            let outcome = equipment_command_shared(command).await;
            if busy.try_set(false).is_some() {
                return;
            }
            if !current_context_matches(&expected).await {
                // Disposed or not, there is nothing further to apply.
                let _hidden = context_visible.try_set(false);
                return;
            }
            if form_generation.try_get_untracked().is_none() {
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
                    library.profiles.update(|list| match list.iter_mut().find(|p| p.profile_id == profile.profile_id) {
                        Some(item) => *item = profile.clone(),
                        None => list.push(profile.clone()),
                    });
                    conflict.set(false);
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
                            // Wait for the reloaded list to render before moving focus.
                            #[cfg(target_arch = "wasm32")]
                            request_animation_frame(move || crate::utils::focus::focus_element("equipment-profile-edit"));
                        }
                        AfterSave::Leave(attempt) => {
                            // `apply` advanced the generation; the saved state is the current one.
                            submitted_generation.set_value(form_generation.get_untracked());
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
                    conflict.set(true);
                    status.set(
                        "Profile changed elsewhere. Your edits are kept; load the latest version to review them against it.".into(),
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
        after_save.set_value(after.clone());
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
        let (settings, about) = match current_form.parse(&base) {
            Ok(parsed) => parsed,
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
        let is_default = default_change(&baseline.get_untracked(), &current_form);
        execute.run(EquipmentCommand::Save(SaveEquipment {
            intent: identity.get_untracked(),
            profile_id,
            name: current_form.name,
            settings,
            about,
            is_default,
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

    let load_latest = move |_| {
        let Some(current) = editing.get_untracked() else {
            return;
        };
        if busy.get_untracked() || pending.get_untracked().is_some() {
            return;
        }
        // A clean form (archive conflict) simply adopts the latest; edits are kept otherwise.
        let had_edits = dirty.get_untracked();
        busy.set(true);
        let expected = identity.get_untracked();
        leptos::task::spawn_local(async move {
            if !current_context_matches(&expected).await {
                if busy.try_set(false).is_none() {
                    context_visible.set(false);
                }
                return;
            }
            let outcome = equipment_command_shared(EquipmentCommand::Get(EquipmentQuery {
                intent: expected.clone(),
                profile_id: current.profile_id,
            }))
            .await;
            if busy.try_set(false).is_some() {
                return;
            }
            if !current_context_matches(&expected).await {
                context_visible.set(false);
                return;
            }
            match outcome {
                Ok(EquipmentOutcome::Profile(latest)) => {
                    if had_edits {
                        // Keep the raw edits; they are now measured against the latest revision.
                        baseline.set(ProfileForm::from_profile(Some(&latest)));
                        editing.set(Some(latest));
                        conflict.set(false);
                        status.set("Loaded the latest profile. Your edits are kept; review and save them.".into());
                    } else {
                        apply.run(Some(latest));
                        status.set("Loaded the latest profile.".into());
                    }
                }
                _ => status.set("Unable to load the latest profile. Try again.".into()),
            }
        });
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
            // The choice is rendered by this effect, so focus it in the next frame.
            #[cfg(target_arch = "wasm32")]
            request_animation_frame(move || crate::utils::focus::focus_element("profile-switch-choice"));
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
        #[cfg(target_arch = "wasm32")]
        request_animation_frame(move || crate::utils::focus::focus_element("equipment-profile-edit"));
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
    let equipment_type_options: Vec<(String, String)> = EQUIPMENT_TYPES
        .iter()
        .map(|(key, label)| ((*key).to_owned(), (*label).to_owned()))
        .collect();
    let cooling_options: Vec<(String, String)> = std::iter::once((String::new(), "Not specified".to_owned()))
        .chain(CoolingMethodV1::ALL.iter().map(|method| (method.key().to_owned(), method.label().to_owned())))
        .collect();
    let text = move |id: &'static str, label: &'static str, get: fn(&ProfileForm) -> &String, set: fn(&mut ProfileForm, String)| {
        let value = Signal::derive(move || form.with(|f| get(f).clone()));
        let change = Callback::new(move |v: String| edit(&|f| set(f, v.clone())));
        view! { <BreweryDraftField id=id label=label value=value on_change=change/> }
    };
    let area = move |id: &'static str, label: &'static str, get: fn(&ProfileForm) -> &String, set: fn(&mut ProfileForm, String)| {
        let value = Signal::derive(move || form.with(|f| get(f).clone()));
        let change = Callback::new(move |v: String| edit(&|f| set(f, v.clone())));
        view! { <BreweryDraftArea id=id label=label value=value on_change=change/> }
    };
    let toggle = move |id: &'static str, label: &'static str, get: fn(&ProfileForm) -> bool, set: fn(&mut ProfileForm, bool)| {
        let checked = Signal::derive(move || form.with(|f| get(f)));
        let change = Callback::new(move |v: bool| edit(&|f| set(f, v)));
        view! { <div id=id class="k-field"><Switch checked=checked on_change=change label=label/></div> }
    };
    view! {
        <BreweryPanel title="Equipment profiles" footer=std::sync::Arc::new(move || view! {
                <BreweryActionGroup label="Profile actions">
                <Button disabled=Signal::derive(move || busy.get() || blocked.get() || (pending.get().is_none() && !dirty.get() && editing.with(|p| p.is_some()))) on:click=move |_| submit_save.run(AfterSave::Stay)>{move || if pending.get().is_some() { "Retry previous request" } else { "Save profile" }}</Button>
                <Button variant=ButtonVariant::Secondary disabled=Signal::derive(move || locked.get() || !dirty.get()) on:click=move |_| { form.set(baseline.get_untracked()); form_generation.update(|n| *n += 1); status.set(String::new()); }>"Discard changes"</Button>
                <Show when=move || editing.with(|p| p.is_some())>
                    <Button variant=ButtonVariant::Secondary disabled=Signal::derive(move || busy.get() || blocked.get() || pending.get().is_some() || dirty.get()) on:click=archive>{move || if editing.with(|p| p.as_ref().is_some_and(|p| p.archived)) { "Unarchive" } else { "Archive" }}</Button>
                </Show>
            </BreweryActionGroup>
            }.into_any()) description="Reusable workspace equipment. Saving a profile updates future previews and new brews; it does not change any recipe's content or earlier brews.">
            <BrewerySection title="Profile">
            <p class="k-field__hint">{move || heading.get()}</p>
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
            </BrewerySection>
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
                        <Button variant=ButtonVariant::Secondary on:click=move |_| {
                            switch_target.set(None);
                            // The notice that held focus is removed; move to its successor after render.
                            #[cfg(target_arch = "wasm32")]
                            request_animation_frame(move || crate::utils::focus::focus_element("equipment-profile-edit"));
                        }>"Stay on profile"</Button>
                    }.into_any())/>
                </div>
            </Show>
            <fieldset disabled=move || locked.get()>
                <BrewerySection title="About">
                    <div class="recipe-editor__fields">
                        {toggle("profile-default", "Default equipment", |f| f.is_default, |f, v| f.is_default = v)}
                        <BreweryDraftField id="equipment-profile-name" label="Equipment name" value=Signal::derive(move || form.with(|f| f.name.clone())) on_change=Callback::new(move |v: String| edit(&|f| f.name = v.clone()))/>
                        {text("profile-brand", "Equipment brand", |f| &f.brand, |f, v| f.brand = v)}
                        {area("profile-description", "Description", |f| &f.description, |f, v| f.description = v)}
                        <BreweryDraftSelect id="profile-measurement" label="Measurement" value=Signal::derive(|| "metric".to_owned()) on_change=Callback::new(|_: String| ()) options=vec![("metric".into(), "Metric".into())]/>
                        <BreweryDraftSelect id="profile-equipment-type" label="Equipment type" value=Signal::derive(move || form.with(|f| f.equipment_type.clone())) on_change=Callback::new(move |v: String| edit(&|f| f.equipment_type = v.clone())) options=equipment_type_options.clone()/>
                        {text("profile-controller-type", "Controller type", |f| &f.controller_type, |f, v| f.controller_type = v)}
                        {field(BATCH)}
                        {text("profile-recirculation", "Recirculation system", |f| &f.recirculation_system, |f, v| f.recirculation_system = v)}
                        {field(VESSELS)}
                    </div>
                </BrewerySection>
                <BrewerySection title="Mash">
                    <div class="recipe-editor__fields">
                        {field(MASH_VOLUME)}{field(MASH_EFFICIENCY)}{field(MASH_DEADSPACE)}{field(MASH_THICKNESS)}{field(MASH_LOSS)}{field(MASH_ABSORPTION)}
                    </div>
                </BrewerySection>
                <BrewerySection title="Sparge">
                    <div class="recipe-editor__fields">
                        {toggle("profile-sparge-alert", "Sparge water alert", |f| f.sparge_water_alert, |f, v| f.sparge_water_alert = v)}
                        {toggle("profile-sparge-counter", "Sparge progress counter", |f| f.sparge_progress_counter, |f, v| f.sparge_progress_counter = v)}
                        {field(SPARGE_VOLUME)}{field(SPARGE_ABSORPTION)}{field(SPARGE_DEADSPACE)}
                    </div>
                </BrewerySection>
                <BrewerySection title="Boil">
                    <div class="recipe-editor__fields">
                        {toggle("profile-boil-power", "Boil power control", |f| f.boil_power_control, |f, v| f.boil_power_control = v)}
                        {field(BOIL_VOLUME)}{field(SHRINKAGE)}{field(BOIL_LOSS)}{field(TRUB_LOSS)}{field(KETTLE_DIAMETER)}
                    </div>
                </BrewerySection>
                <BrewerySection title="Cooling">
                    <div class="recipe-editor__fields">
                        <BreweryDraftSelect id="profile-cooling-method" label="Cooling method" value=Signal::derive(move || form.with(|f| f.cooling_method.clone())) on_change=Callback::new(move |v: String| edit(&|f| f.cooling_method = v.clone())) options=cooling_options.clone()/>
                        {field(COOLING_FLOW)}
                    </div>
                </BrewerySection>
                <BrewerySection title="Notes">
                    {area("profile-notes", "Notes", |f| &f.notes, |f, v| f.notes = v)}
                </BrewerySection>
            </fieldset>
            <Show when=move || blocked.get()><Button variant=ButtonVariant::Secondary on:click=move |_| {
                pending.set(None);
                blocked.set(false);
                after_save.set_value(AfterSave::Stay);
                library.reload();
                status.set("Profiles reloaded. Review the list before saving again.".into());
            }>"Reload profiles"</Button></Show>
            <Show when=move || conflict.get()><Button variant=ButtonVariant::Secondary disabled=busy on:click=load_latest>"Load latest and keep my edits"</Button></Show>
            <Show when=move || dirty.get()><p class="k-field__hint">"Unsaved profile changes. Save or discard them before archiving."</p></Show>
            <p role="status">{move || status.get()}</p>
        </BreweryPanel>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(settings: EquipmentSettingsV1, about: EquipmentAboutV1) -> EquipmentProfile {
        EquipmentProfile {
            workspace_id: "w".into(),
            profile_id: "p".into(),
            created_at: 0,
            name: "Garage".into(),
            revision: 1,
            metadata_revision: 1,
            archived: false,
            is_default: true,
            settings,
            about,
            provenance: EquipmentProvenance {
                source: "manual".into(),
                assumptions: vec![],
            },
            saved_by: "u".into(),
            saved_at: 0,
        }
    }

    #[test]
    fn the_form_lists_exactly_the_spec_numeric_fields_in_spec_order() {
        let labels: Vec<_> = FIELDS.iter().map(|(_, label, _)| *label).collect();
        assert_eq!(
            labels,
            [
                "Batch size",
                "Number of vessels",
                "Mash tun volume",
                "Mash efficiency",
                "Mash tun deadspace",
                "Mash thickness",
                "Mash tun loss",
                "Grain absorption",
                "Sparge vessel volume",
                "Grain absorption",
                "Sparge deadspace volume",
                "Boil volume",
                "Wort shrinkage",
                "Boil loss",
                "Trub and chiller loss",
                "Kettle diameter",
                "Flow rate",
            ]
        );
        let ids: std::collections::BTreeSet<_> = FIELDS.iter().map(|(id, _, _)| *id).collect();
        assert_eq!(ids.len(), FIELDS.len(), "every field has its own id");
        assert!(
            FIELDS.iter().all(|(_, label, _)| !label.contains("Brewhouse")),
            "Brewhouse efficiency is not a profile field"
        );
    }

    #[test]
    fn a_loaded_profile_is_clean_and_an_edit_is_dirty_against_its_baseline() {
        let loaded = ProfileForm::from_profile(Some(&profile(
            EquipmentSettingsV1 {
                mash_efficiency_fraction: Some(0.57),
                ..Default::default()
            },
            EquipmentAboutV1::default(),
        )));
        assert_eq!(loaded.raw[MASH_EFFICIENCY], "57");
        assert!(loaded.is_default);
        let mut edited = loaded.clone();
        assert_eq!(edited, loaded);
        edited.raw[BOIL_LOSS] = "3".into();
        assert_ne!(edited, loaded);
        let mut toggled = loaded.clone();
        toggled.sparge_water_alert = true;
        assert_ne!(toggled, loaded);
    }

    #[test]
    fn an_untouched_default_toggle_is_not_sent_so_a_stale_form_cannot_overwrite_it() {
        let loaded = ProfileForm::from_profile(Some(&profile(Default::default(), Default::default())));
        assert!(loaded.is_default);
        let mut edited = loaded.clone();
        edited.notes = "changed".into();
        assert_eq!(default_change(&loaded, &edited), None);
        edited.is_default = false;
        assert_eq!(default_change(&loaded, &edited), Some(false));
        assert_eq!(default_change(&ProfileForm::blank(), &loaded), Some(true));
    }

    #[test]
    fn every_spec_field_round_trips_through_the_form() {
        let settings = EquipmentSettingsV1 {
            mash_tun_volume_l: Some(30.0),
            mash_efficiency_fraction: Some(0.78),
            mash_tun_deadspace_l: Some(3.5),
            mash_thickness_l_per_kg: Some(2.8),
            mash_tun_loss_l: Some(0.0),
            mash_grain_absorption_l_per_kg: Some(0.8),
            sparge_vessel_volume_l: Some(19.0),
            sparge_grain_absorption_l_per_kg: Some(0.0),
            sparge_deadspace_l: Some(0.0),
            boil_volume_l: Some(30.0),
            wort_shrinkage_fraction: Some(0.04),
            boil_loss_l_per_hour: Some(3.0),
            trub_and_chiller_loss_l: Some(2.0),
            kettle_diameter_cm: Some(30.0),
            cooling_method: Some(CoolingMethodV1::CounterflowChiller),
            cooling_flow_rate_l_per_minute: Some(1.0),
            ..Default::default()
        };
        let about = EquipmentAboutV1 {
            brand: Some("Grainfather".into()),
            description: Some("All in one".into()),
            measurement: Some("metric".into()),
            equipment_type: Some("all_grain".into()),
            controller_type: Some("Grainfather Bluetooth Controller".into()),
            recirculation_system: Some("RIMS".into()),
            vessel_count: Some(1),
            batch_size_l: Some(23.0),
            sparge_water_alert: true,
            sparge_progress_counter: true,
            boil_power_control: true,
            notes: Some("Calibrated".into()),
        };
        let form = ProfileForm::from_profile(Some(&profile(settings.clone(), about.clone())));
        let (parsed_settings, parsed_about) = form.parse(&EquipmentSettingsV1::default()).unwrap();
        assert_eq!(parsed_settings, settings);
        assert_eq!(parsed_about, about);
    }

    #[test]
    fn percentages_are_entered_as_percent_and_stored_as_fractions_with_extensions_kept() {
        let mut base = EquipmentSettingsV1::default();
        base.extensions.insert("x".into(), serde_json::json!(1));
        let mut form = ProfileForm::blank();
        form.raw[MASH_EFFICIENCY] = "78".into();
        form.raw[SHRINKAGE] = "4".into();
        let (parsed, _) = form.parse(&base).unwrap();
        assert_eq!(parsed.mash_efficiency_fraction, Some(0.78));
        assert_eq!(parsed.wort_shrinkage_fraction, Some(0.04));
        assert_eq!(parsed.efficiency_override_fraction, None, "never a profile field");
        assert_eq!(parsed.extensions, base.extensions);
    }

    #[test]
    fn malformed_and_non_finite_text_is_reported_per_field_and_blocks_parsing() {
        let mut form = ProfileForm::blank();
        form.raw[BOIL_LOSS] = "3,5".into();
        form.raw[TRUB_LOSS] = "inf".into();
        form.raw[VESSELS] = "1.5".into();
        assert_eq!(form.field_error(BOIL_LOSS), Some("Enter a number."));
        assert_eq!(form.field_error(TRUB_LOSS), Some("Enter a finite number."));
        assert_eq!(
            form.field_error(VESSELS),
            Some("Enter a whole number of vessels.")
        );
        assert_eq!(form.field_error(MASH_LOSS), None);
        assert!(form.parse(&Default::default()).is_err());
    }
}
