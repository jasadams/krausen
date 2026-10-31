// SPDX-License-Identifier: AGPL-3.0-or-later
//! Workspace equipment profile controls backed by shared application operations.
use crate::{
    components::{BreweryDraftField, BreweryDraftSelect, BreweryNotice, Button, ButtonVariant},
    pages::{recipe_draft::RecipeDraft, recipe_editor::current_context_matches},
    server_fns::equipment::equipment_command_shared,
};
use krausen_types::{
    brewery::{EquipmentProfile, EquipmentProvenance, EquipmentSettingsV1},
    equipment::{
        ArchiveEquipment, EquipmentCommand, EquipmentListQuery, EquipmentOutcome, SaveEquipment,
    },
    request_context::OperationIntent,
};
use leptos::prelude::*;

#[component]
pub fn EquipmentProfiles(
    draft: RwSignal<RecipeDraft>,
    identity: RwSignal<OperationIntent>,
    context_visible: RwSignal<bool>,
    refresh_preview: RwSignal<u64>,
) -> impl IntoView {
    let profiles = RwSignal::new(Vec::<EquipmentProfile>::new());
    let reload = RwSignal::new(0_u64);
    let status = RwSignal::new(String::new());
    let editing = RwSignal::new(None::<EquipmentProfile>);
    let name = RwSignal::new(String::new());
    let settings = RwSignal::new(EquipmentSettingsV1::default());
    let raw = RwSignal::new(vec![String::new(); 14]);
    let chilling = RwSignal::new(String::new());
    let pending = RwSignal::new(None::<EquipmentCommand>);
    let busy = RwSignal::new(false);
    Effect::new(move |_| {
        let _ = reload.get();
        if !context_visible.get() {
            return;
        }
        let expected = identity.get_untracked();
        leptos::task::spawn_local(async move {
            if !current_context_matches(&expected).await {
                context_visible.set(false);
                return;
            }
            let outcome = equipment_command_shared(EquipmentCommand::List(EquipmentListQuery {
                intent: expected.clone(),
                after: String::new(),
                limit: 100,
                include_archived: true,
            }))
            .await;
            if !current_context_matches(&expected).await {
                context_visible.set(false);
                return;
            }
            match outcome {
                Ok(EquipmentOutcome::Profiles(items)) => profiles.set(items),
                Ok(EquipmentOutcome::ContextChanged | EquipmentOutcome::Forbidden) => {
                    context_visible.set(false)
                }
                _ => status.set("Unable to load equipment profiles.".into()),
            }
        });
    });
    let selection = Signal::derive(move || {
        draft.with(|d| {
            d.document()
                .equipment_profile_id
                .clone()
                .unwrap_or_default()
        })
    });
    let choose = Callback::new(move |id: String| {
        draft.update(|d| d.edit(|doc| doc.equipment_profile_id = (!id.is_empty()).then_some(id)));
    });
    let prepare = Callback::new(move |profile: Option<EquipmentProfile>| {
        let initial = profile
            .as_ref()
            .map(|p| p.settings.clone())
            .unwrap_or_default();
        name.set(profile.as_ref().map(|p| p.name.clone()).unwrap_or_default());
        raw.set(
            values(&initial)
                .into_iter()
                .map(|n| n.map(|v| v.to_string()).unwrap_or_default())
                .collect(),
        );
        chilling.set(
            match &initial.chilling {
                Some(krausen_types::brewery::ChillingV1::Natural) => "natural",
                Some(krausen_types::brewery::ChillingV1::Forced { .. }) => "forced",
                Some(krausen_types::brewery::ChillingV1::Counterflow { .. }) => "counterflow",
                None => "",
            }
            .into(),
        );
        settings.set(initial);
        editing.set(profile);
        pending.set(None);
        status.set(String::new());
    });
    let execute = Callback::new(move |command: EquipmentCommand| {
        if busy.get_untracked() {
            return;
        }
        pending.set(Some(command.clone()));
        busy.set(true);
        let expected = identity.get_untracked();
        leptos::task::spawn_local(async move {
            if !current_context_matches(&expected).await {
                context_visible.set(false);
                busy.set(false);
                return;
            }
            let outcome = equipment_command_shared(command).await;
            busy.set(false);
            if !current_context_matches(&expected).await {
                context_visible.set(false);
                return;
            }
            match outcome {
                Ok(EquipmentOutcome::Profile(profile)) => {
                    pending.set(None);
                    let message = if profile.archived {
                        "Profile archived."
                    } else {
                        "Profile saved."
                    };
                    prepare.run(Some(profile));
                    status.set(message.into());
                    reload.update(|n| *n += 1);
                    refresh_preview.update(|n| *n += 1);
                }
                Ok(EquipmentOutcome::ContextChanged | EquipmentOutcome::Forbidden) => {
                    context_visible.set(false)
                }
                Ok(EquipmentOutcome::Conflict { .. }) => {
                    pending.set(None);
                    status.set(
                        "Profile changed elsewhere. Reload and edit the latest version.".into(),
                    );
                    reload.update(|n| *n += 1);
                }
                Ok(_) => {
                    pending.set(None);
                    status
                        .set("Profile could not be saved. Check the fields and try again.".into());
                }
                Err(_) => {
                    status.set("Connection interrupted. Retry to confirm the same request.".into())
                }
            }
        });
    });
    let save = move |_| {
        if let Some(command) = pending.get_untracked() {
            execute.run(command);
            return;
        }
        let texts = raw.get_untracked();
        let mut parsed = Vec::new();
        for text in texts {
            if text.trim().is_empty() {
                parsed.push(None);
            } else if let Ok(value) = text.trim().parse::<f64>() {
                if !value.is_finite() {
                    status.set("Enter finite numbers.".into());
                    return;
                }
                parsed.push(Some(value));
            } else {
                status.set("Enter valid numbers.".into());
                return;
            }
        }
        let mut value = settings.get_untracked();
        assign_values(&mut value, &parsed);
        value.chilling = match chilling.get_untracked().as_str() {
            "natural" => Some(krausen_types::brewery::ChillingV1::Natural),
            "forced" => Some(krausen_types::brewery::ChillingV1::Forced {
                rate_per_minute: parsed[11],
                plateau_c: parsed[12],
            }),
            "counterflow" => Some(krausen_types::brewery::ChillingV1::Counterflow {
                litres_per_minute: parsed[13],
            }),
            _ => None,
        };
        let Some(request_id) = crate::utils::request_id::new_request_id() else {
            status.set("Unable to create request identity.".into());
            return;
        };
        let current = editing.get_untracked();
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
        execute.run(EquipmentCommand::Save(SaveEquipment {
            intent: identity.get_untracked(),
            profile_id,
            name: name.get_untracked(),
            settings: value,
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
    };
    let archive = move |_| {
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
        execute.run(EquipmentCommand::Archive(ArchiveEquipment {
            intent: identity.get_untracked(),
            profile_id: profile.profile_id,
            archived: !profile.archived,
            expected_metadata_revision: profile.metadata_revision,
            request_id,
        }));
    };
    view! {
        <div class="recipe-editor__fields">
            {move || {
                let mut options = vec![(String::new(), "Use draft settings".into())];
                options.extend(profiles.get().into_iter().filter(|p| !p.archived).map(|p| (p.profile_id, p.name)));
                view! { <BreweryDraftSelect id="equipment-profile-select" label="Equipment profile" value=selection on_change=choose options=options/> }
            }}
            {move || { let items = profiles.get(); let selected = selection.get(); (!selected.is_empty() && !items.iter().any(|p| p.profile_id == selected && !p.archived)).then(|| view! { <BreweryNotice title="Equipment profile unavailable" body="Choose an active profile or use draft settings." warning=true/> }) }}
        </div>
        <div class="k-disclosure__body"><p>"Create or edit workspace equipment. Recipe overrides remain separate."</p>
            <fieldset disabled=move || busy.get() || pending.get().is_some()><div class="recipe-editor__fields">
                {move || { let mut options = vec![(String::new(), "New profile".into())]; options.extend(profiles.get().into_iter().map(|p| (p.profile_id.clone(), if p.archived { format!("{} (archived)", p.name) } else { p.name }))); let selected = Signal::derive(move || editing.get().map(|p| p.profile_id).unwrap_or_default()); let change = Callback::new(move |id: String| { if pending.get_untracked().is_some() { status.set("Retry the previous request before changing profiles.".into()); return; } let item = profiles.with_untracked(|list| list.iter().find(|p| p.profile_id == id).cloned()); prepare.run(item); }); view! { <BreweryDraftSelect id="equipment-profile-edit" label="Manage profile" value=selected on_change=change options=options/> } }}
                <BreweryDraftField id="equipment-profile-name" label="Profile name" value=Signal::derive(move || name.get()) on_change=Callback::new(move |v| { if pending.get_untracked().is_none() { name.set(v); } }) />
                <BreweryDraftSelect id="profile-chilling" label="Profile chilling" value=Signal::derive(move || chilling.get()) on_change=Callback::new(move |v| { if pending.get_untracked().is_none() { chilling.set(v); } }) options=vec![("".into(), "Not specified".into()), ("natural".into(), "Natural".into()), ("forced".into(), "Forced".into()), ("counterflow".into(), "Counterflow".into())]/>
                {FIELDS.iter().enumerate().map(|(index, (id, label, unit))| { let value = Signal::derive(move || raw.with(|v| v[index].clone())); let change = Callback::new(move |text| { if pending.get_untracked().is_none() { raw.update(|v| v[index] = text); } }); view! { <BreweryDraftField id=*id label=*label unit=*unit value=value on_change=change numeric=true/> } }).collect_view()}
            </div></fieldset>
            <Button disabled=busy on:click=save>{move || if pending.get().is_some() { "Retry previous request" } else { "Save profile" }}</Button>
            <Show when=move || editing.with(|p| p.is_some())><Button variant=ButtonVariant::Secondary disabled=Signal::derive(move || busy.get() || pending.get().is_some()) on:click=archive>{move || if editing.with(|p| p.as_ref().is_some_and(|p| p.archived)) { "Unarchive" } else { "Archive" }}</Button></Show>
            <p role="status">{move || status.get()}</p>
        </div>
    }
}
const FIELDS: [(&str, &str, &str); 14] = [
    (
        "profile-efficiency-override",
        "Brewhouse efficiency",
        "fraction",
    ),
    ("profile-mash-efficiency", "Mash efficiency", "fraction"),
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
            Some(krausen_types::brewery::ChillingV1::Forced {
                rate_per_minute, ..
            }) => *rate_per_minute,
            _ => None,
        },
        match &s.chilling {
            Some(krausen_types::brewery::ChillingV1::Forced { plateau_c, .. }) => *plateau_c,
            _ => None,
        },
        match &s.chilling {
            Some(krausen_types::brewery::ChillingV1::Counterflow { litres_per_minute }) => {
                *litres_per_minute
            }
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
