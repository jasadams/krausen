// SPDX-License-Identifier: AGPL-3.0-or-later
//! Recipe process composed by brewing stage (UX-004): Mash & water, Boil &
//! whirlpool, Fermentation, and Description & notes.
//!
//! Every control keeps its existing draft path, so raw text, absence versus
//! zero, native undo and the shared preview are unchanged. Conditional controls
//! are only hidden while they hold nothing; a retained value is always shown.
//!
//! Stable DOM ids (published for diagnostic jumps; every section is always
//! open, so no ancestor needs opening beyond the Recipe tab):
//!
//! | Field | Id |
//! | --- | --- |
//! | sections | `recipe-section-mash`, `-boil`, `-fermentation`, `-notes` |
//! | mash method / water / top-up | `recipe-mash-method`, `recipe-mash-water`, `recipe-top-up` |
//! | boil / whirlpool minutes / temperature | `recipe-boil`, `recipe-whirlpool`, `recipe-whirlpool-temp` |
//! | wort attenuation | `recipe-attenuation` |
//! | description / notes | `recipe-description`, `recipe-notes` |
//! | step name/time/temperature/volume | `process.<mash_steps\|fermentation_steps>-<row_id>-<name\|time_minutes\|temperature_c\|volume_l>` |
//! | step remove / add | `process.<kind>-<row_id>-remove`, `process.<kind>-add` |
use crate::{
    components::{
        BreweryActionGroup, BreweryDraftArea, BreweryDraftField,
        BreweryDraftSelect, BreweryFields, BreweryMetricList, BreweryPanel, BrewerySection, BreweryStale, BreweryStaleCue, BreweryStepGroup, BreweryStepRow, Button, ButtonVariant,
        MetricDensity::Quiet,
    },
    pages::{
        ingredient_addition::{Restore, UNDO_DEPTH},
        recipe_draft::{RecipeDraft, RemovedInputs},
        recipe_editor::{NumberField, preview_metric},
    },
};
use krausen_types::{
    brewery::{CoolingMethodV1, ProcessStep, RecipeDocumentV1},
    formulation::Preview,
};
use leptos::prelude::*;

/// The estimate that matches the current draft generation, shared by the
/// formulation preview so water rows beside the inputs never run a second
/// producer or outlive their generation.
#[derive(Clone, Copy)]
pub(crate) struct AcceptedPreview {
    pub(crate) preview: Signal<Option<Preview>>,
    /// Why there is no estimate for the current draft right now.
    pub(crate) phase: Signal<EstimatePhase>,
    /// Whether the editor is shown. Nothing is announced about an estimate the
    /// person cannot see.
    pub(crate) visible: Signal<bool>,
}

/// The preview's state for the current draft when it has no accepted estimate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EstimatePhase {
    /// A request is in flight or about to be sent.
    Calculating,
    /// Settled without an estimate because the inputs are invalid.
    InvalidInput,
    /// Settled without an estimate for another reason (network, profile, style, context).
    Unavailable,
}

impl EstimatePhase {
    fn cue(self) -> &'static str {
        match self {
            Self::Calculating => "Out of date, recalculating",
            Self::InvalidInput => "Estimate unavailable until the inputs are valid",
            Self::Unavailable => "Estimate unavailable",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StepKind {
    Mash,
    Fermentation,
}

impl StepKind {
    /// Draft path and DOM-id prefix; unchanged from the previous editor.
    fn prefix(self) -> &'static str {
        match self {
            Self::Mash => "process.mash_steps",
            Self::Fermentation => "process.fermentation_steps",
        }
    }
    fn noun(self) -> &'static str {
        match self {
            Self::Mash => "mash step",
            Self::Fermentation => "fermentation step",
        }
    }
    fn rows(self, doc: &RecipeDocumentV1) -> &Vec<ProcessStep> {
        match self {
            Self::Mash => &doc.process.mash_steps,
            Self::Fermentation => &doc.process.fermentation_steps,
        }
    }
    fn rows_mut(self, doc: &mut RecipeDocumentV1) -> &mut Vec<ProcessStep> {
        match self {
            Self::Mash => &mut doc.process.mash_steps,
            Self::Fermentation => &mut doc.process.fermentation_steps,
        }
    }
    fn add_button_id(self) -> String {
        format!("{}-add", self.prefix())
    }
    fn undo_button_id(self) -> String {
        format!("{}-undo", self.prefix())
    }
}

fn step_field_id(kind: StepKind, id: &str, suffix: &str) -> String {
    format!("{}-{id}-{suffix}", kind.prefix())
}

/// Visible label of a step measurement, by its path tail.
pub(crate) fn step_field_label(tail: &str) -> Option<&'static str> {
    Some(match tail {
        "time_minutes" => fields::STEP_TIME,
        "temperature_c" => fields::STEP_TEMPERATURE,
        "volume_l" => fields::STEP_VOLUME,
        _ => return None,
    })
}

/// Stable ids and labels of the process controls, declared once: the controls
/// below and the diagnostic registry both read these, so a rename cannot leave a
/// jump pointing at nothing.
pub(crate) mod fields {
    /// `(DOM id, visible label)` of a control addressed by one draft path.
    pub type Field = (&'static str, &'static str);
    pub const MASH_METHOD: Field = ("recipe-mash-method", "Mash water method");
    pub const MASH_WATER: Field = ("recipe-mash-water", "Mash water");
    pub const TOP_UP: Field = ("recipe-top-up", "Kettle top-up");
    pub const BOIL: Field = ("recipe-boil", "Boil time");
    pub const WHIRLPOOL: Field = ("recipe-whirlpool", "Whirlpool time");
    pub const WHIRLPOOL_TEMP: Field = ("recipe-whirlpool-temp", "Whirlpool temperature");
    pub const ATTENUATION: Field = ("recipe-attenuation", "Wort attenuation");
    pub const STEP_TIME: &str = "Time";
    pub const STEP_TEMPERATURE: &str = "Temperature";
    pub const STEP_VOLUME: &str = "Volume";
}

/// The control a structured step path addresses, for the diagnostic registry.
pub(crate) struct StepDiagnostic {
    pub dom_id: String,
    pub row_label: String,
    pub label: &'static str,
}

/// `None` when the path is not a step-field path or its row no longer exists, so
/// a stale diagnostic never becomes a link to nothing.
pub(crate) fn diagnostic_step(doc: &RecipeDocumentV1, path: &str) -> Option<StepDiagnostic> {
    for kind in [StepKind::Mash, StepKind::Fermentation] {
        let Some(rest) = path.strip_prefix(kind.prefix()).and_then(|r| r.strip_prefix('[')) else { continue };
        let (id, tail) = rest.split_once("].")?;
        if !kind.rows(doc).iter().any(|row| row.row_id == id) {
            return None;
        }
        let label = step_field_label(tail)?;
        return Some(StepDiagnostic { dom_id: step_field_id(kind, id, tail), row_label: step_label(doc, kind, id), label });
    }
    None
}

fn remove_button_id(kind: StepKind, id: &str) -> String {
    step_field_id(kind, id, "remove")
}

fn step_ids(doc: &RecipeDocumentV1, kind: StepKind) -> Vec<String> {
    kind.rows(doc).iter().map(|row| row.row_id.clone()).collect()
}

/// Position and name, so repeated actions are distinguishable: "Mash step 2 of
/// 3, Saccharification". An unnamed step is identified by position alone.
fn step_label(doc: &RecipeDocumentV1, kind: StepKind, id: &str) -> String {
    let rows = kind.rows(doc);
    let Some(position) = rows.iter().position(|row| row.row_id == id) else {
        return capitalized(kind.noun());
    };
    let base = format!("{} {} of {}", capitalized(kind.noun()), position + 1, rows.len());
    match rows[position].name.as_deref().filter(|name| !name.is_empty()) {
        Some(name) => format!("{base}, {name}"),
        None => base,
    }
}

fn capitalized(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map(|first| first.to_uppercase().chain(chars).collect()).unwrap_or_default()
}

/// A new step is empty: no name, no invented time/temperature/volume.
fn add_step(draft: &mut RecipeDraft, kind: StepKind, row_id: String) {
    draft.edit(|doc| {
        kind.rows_mut(doc).push(ProcessStep {
            row_id,
            name: None,
            time_minutes: None,
            temperature_c: None,
            volume_l: None,
            extensions: Default::default(),
        })
    });
}

/// Everything needed to put one removed step back exactly where it was.
#[derive(Clone, Debug, PartialEq)]
struct UndoStep {
    kind: StepKind,
    id: String,
    index: usize,
    label: String,
    epoch: u64,
    row: ProcessStep,
    inputs: RemovedInputs,
}

fn remove_step(draft: &mut RecipeDraft, kind: StepKind, id: &str) -> Option<UndoStep> {
    let doc = draft.document();
    let index = kind.rows(doc).iter().position(|row| row.row_id == id)?;
    let label = step_label(doc, kind, id);
    let row = kind.rows(doc)[index].clone();
    let epoch = draft.epoch();
    let prefix = format!("{}[{id}].", kind.prefix());
    let owned = id.to_owned();
    let inputs = draft.edit_removing(&prefix, |doc| kind.rows_mut(doc).retain(|r| r.row_id != owned));
    Some(UndoStep { kind, id: owned, index, label, epoch, row, inputs })
}

/// Restores ID, position, units and every raw string. A step that has since
/// reappeared under the same ID is never duplicated.
fn restore_step(draft: &mut RecipeDraft, undo: UndoStep) -> Restore {
    if undo.epoch != draft.epoch() {
        return Restore::Stale;
    }
    if step_ids(draft.document(), undo.kind).contains(&undo.id) {
        return Restore::AlreadyPresent;
    }
    let UndoStep { kind, index, row, inputs, .. } = undo;
    draft.edit_restoring(inputs, |doc| {
        let rows = kind.rows_mut(doc);
        let at = index.min(rows.len());
        rows.insert(at, row);
    });
    Restore::Restored
}

/// Focus candidates after removing the step at `position` of `ids`: the next
/// step's Remove action, else the previous step's, else the group's Add.
fn removal_successors(kind: StepKind, ids: &[String], position: usize) -> Vec<String> {
    let mut successors = Vec::new();
    if let Some(next) = ids.get(position + 1) {
        successors.push(remove_button_id(kind, next));
    }
    if position > 0 {
        successors.push(remove_button_id(kind, &ids[position - 1]));
    }
    successors.push(kind.add_button_id());
    successors
}

/// A mash-water control stays visible while it can apply or while it holds
/// anything (a value or typed text), so a conditional field never hides data.
fn holds_value(draft: &RecipeDraft, path: &str, value: Option<f64>) -> bool {
    value.is_some() || draft.raw_numbers().contains_key(path)
}

pub(crate) fn show_manual_mash_water(draft: &RecipeDraft) -> bool {
    let process = &draft.document().process;
    process.mash_water_method.as_deref() == Some("manual")
        || holds_value(draft, "process.mash_water_l", process.mash_water_l)
}

pub(crate) fn show_kettle_top_up(draft: &RecipeDraft) -> bool {
    let process = &draft.document().process;
    process.mash_water_method.is_some()
        || holds_value(draft, "process.kettle_top_up_l", process.kettle_top_up_l)
}

/// The canonical model's chilling behaviour for the effective equipment,
/// from `mapping/calculation.rs` and CALCULATIONS.md ("Bitterness, timing and
/// chilling"): natural cooling is an explicit choice; an unspecified choice
/// leaves bitterness unavailable when the recipe has hops; a hop first added
/// after the wort has cooled is not modelled.
fn chilling_context(cooling: &Option<CoolingMethodV1>, has_hops: bool) -> String {
    let choice = match cooling {
        Some(CoolingMethodV1::Natural) => "Effective chilling: natural cooling after flameout.".to_owned(),
        Some(CoolingMethodV1::ImmersionChiller) => "Effective chilling: immersion chiller after flameout, using the published immersion-chiller curve.".to_owned(),
        Some(CoolingMethodV1::IceBath) => "Effective chilling: ice bath after flameout, using the published ice-bath curve.".to_owned(),
        Some(CoolingMethodV1::CounterflowChiller) => "Effective chilling: counterflow chiller after flameout, using the flow rate from Equipment.".to_owned(),
        None if has_hops => "Effective chilling: not specified. A bitterness estimate needs a cooling method in Equipment.".to_owned(),
        None => "Effective chilling: not specified.".to_owned(),
    };
    format!("{choice} Whirlpool hops are charged at flameout and stand for the whirlpool time; a hop first added after the wort has cooled is not modelled.")
}

fn preview_chilling(preview: &Preview) -> Option<String> {
    let context = preview.context.as_ref()?;
    Some(chilling_context(&context.effective_equipment.cooling_method, !context.draft.hops.is_empty()))
}

fn has_unsupported_process(doc: &RecipeDocumentV1) -> bool {
    !doc.process.unsupported.is_empty()
}

/// Last accepted estimate and whether it is out of date. Values only ever come
/// from an accepted preview; keeping the last one mounted while the next is
/// calculated stops the sections below from jumping under a focused input.
#[derive(Clone, Copy)]
struct Shown {
    preview: RwSignal<Option<Preview>>,
    stale: Signal<bool>,
    cue: Signal<Option<String>>,
    announce: Signal<Option<String>>,
    /// Show the numbers (current, or old and dimmed while recalculating); otherwise dashes.
    values: Signal<bool>,
}

/// How long an estimate must stay out of date before it is announced.
const STALE_ANNOUNCE_DELAY_MS: u64 = 800;

/// How long "Estimate updated" stays in the status after an announced episode.
const UPDATED_ANNOUNCE_MS: u64 = 3000;

/// What the polite status says about an out-of-date estimate, as a plain state
/// machine so it is unit-tested without a browser.
///
/// One *episode* runs from the estimate going out of date to an estimate being
/// accepted again. Nothing is said for a quick recalculation. Once the episode has
/// lasted the debounce (and there are old numbers to talk about) the current
/// outcome is announced; after that the text only changes when the outcome
/// SETTLES (invalid input / unavailable) to different wording, so a failure that
/// follows a long wait is announced once, and flips between calculating and an
/// outcome already announced are silent. When the estimate returns, "Estimate
/// updated" follows an announced episode.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Announcement {
    in_episode: bool,
    announced: Option<&'static str>,
    updated: bool,
}

impl Announcement {
    fn begin(&mut self) {
        self.in_episode = true;
        self.updated = false;
    }

    /// The debounce ran out while still out of date.
    fn debounce_elapsed(&mut self, has_plan: bool, outcome: EstimatePhase) {
        if self.in_episode && has_plan && self.announced.is_none() {
            self.announced = Some(outcome.cue());
        }
    }

    /// The outcome for the current draft changed. Only a settled, different
    /// outcome is announced, and only inside an announced episode.
    fn outcome_changed(&mut self, outcome: EstimatePhase) {
        if self.announced.is_some() && outcome != EstimatePhase::Calculating && self.announced != Some(outcome.cue()) {
            self.announced = Some(outcome.cue());
        }
    }

    /// An estimate was accepted. True when an episode had been announced, so the
    /// caller should keep "Estimate updated" for a moment.
    fn end(&mut self) -> bool {
        self.in_episode = false;
        let announced = self.announced.take().is_some();
        self.updated = announced;
        announced
    }

    fn clear_updated(&mut self) {
        self.updated = false;
    }

    fn text(&self) -> Option<String> {
        self.announced.map(str::to_owned).or_else(|| self.updated.then(|| "Estimate updated".to_owned()))
    }
}

#[component]
pub fn ProcessSections(draft: RwSignal<RecipeDraft>) -> impl IntoView {
    let AcceptedPreview { preview: accepted, phase, visible } = expect_context::<AcceptedPreview>();
    let preview = RwSignal::new(None::<Preview>);
    // Out of date at once: the old numbers are dimmed, hidden from assistive
    // technology and named by a visible cue the moment the draft changes. While a
    // new estimate is being calculated the old numbers stay (dimmed); once the
    // draft settles without one (invalid input, unavailable) they become dashes.
    let stale = Memo::new(move |_| accepted.with(Option::is_none));
    let phase = Memo::new(move |_| phase.get());
    let cue = Signal::derive(move || stale.get().then(|| phase.get().cue().to_owned()));
    let values = Signal::derive(move || !stale.get() || phase.get() == EstimatePhase::Calculating);
    // Only the announcement is debounced (see `Announcement`). `stale` is a Memo so
    // an episode starts and ends exactly when the estimate goes out of date or comes
    // back: re-notifications while it stays out of date (every keystroke clears the
    // result again) neither restart the timer nor change the text.
    let state = RwSignal::new(Announcement::default());
    let pending = StoredValue::new(0_u64);
    Effect::new(move |_| {
        if let Some(next) = accepted.get() {
            preview.set(Some(next));
        }
    });
    // A retained estimate describes one draft document. A restore or a saved-recipe
    // replacement changes the draft epoch, so the old document's water plan must not
    // be shown for the new draft (UX-007). Same fence as the summary's `last_preview`.
    let draft_epoch = Memo::new(move |_| draft.with(|d| d.epoch()));
    Effect::new(move |previous: Option<u64>| {
        let epoch = draft_epoch.get();
        if previous.is_some_and(|before| before != epoch) {
            preview.set(None);
        }
        epoch
    });
    Effect::new(move |_| {
        let is_stale = stale.get();
        let is_visible = visible.get();
        let mut ticket = 0;
        pending.update_value(|n| {
            *n += 1;
            ticket = *n;
        });
        if !is_visible {
            // An episode that starts while the editor is hidden is never announced:
            // forget it, so returning access does not end it with "Estimate updated" (ACC-005).
            state.set(Announcement::default());
        } else if is_stale {
            state.update(|announcement| announcement.begin());
            #[cfg(target_arch = "wasm32")]
            set_timeout(
                move || {
                    if pending.try_get_value() == Some(ticket) {
                        let has_plan = preview.with_untracked(Option::is_some);
                        let outcome = phase.get_untracked();
                        state.update(|announcement| announcement.debounce_elapsed(has_plan, outcome));
                    }
                },
                std::time::Duration::from_millis(STALE_ANNOUNCE_DELAY_MS),
            );
        } else if state.try_update(|announcement| announcement.end()).unwrap_or(false) {
            #[cfg(target_arch = "wasm32")]
            set_timeout(
                move || {
                    if pending.try_get_value() == Some(ticket) {
                        state.update(|announcement| announcement.clear_updated());
                    }
                },
                std::time::Duration::from_millis(UPDATED_ANNOUNCE_MS),
            );
        }
        #[cfg(not(target_arch = "wasm32"))]
        let _ = ticket;
    });
    Effect::new(move |_| {
        let outcome = phase.get();
        state.update(|announcement| announcement.outcome_changed(outcome));
    });
    let announce = Signal::derive(move || state.with(Announcement::text));
    let shown = Shown { preview, stale: stale.into(), cue, announce, values };
    view! {
        <MashAndWater draft=draft shown=shown/>
        <BoilAndWhirlpool draft=draft shown=shown/>
        <Fermentation draft=draft/>
        <DescriptionAndNotes draft=draft/>
    }
}

#[component]
fn MashAndWater(draft: RwSignal<RecipeDraft>, shown: Shown) -> impl IntoView {
    let has_plan = Memo::new(move |_| shown.preview.with(Option::is_some));
    let method = Signal::derive(move || draft.with(|d| d.document().process.mash_water_method.clone().unwrap_or_default()));
    let change_method = Callback::new(move |value: String| {
        draft.update(|d| d.edit(|doc| doc.process.mash_water_method = (!value.is_empty()).then_some(value)))
    });
    let method_error = Signal::derive(move || draft.with(|d| d.validation_error("process.mash_water_method").map(str::to_owned)));
    let method_hint = Signal::derive(move || draft.with(|d| d.validation_hint("process.mash_water_method").map(str::to_owned)));
    let manual = Memo::new(move |_| draft.with(show_manual_mash_water));
    let top_up = Memo::new(move |_| draft.with(show_kettle_top_up));
    view! {
        <BreweryPanel id="recipe-section-mash" title="Mash & water" description="Mash steps and how the mash water is allocated." stacked=true>
            <StepGroup draft=draft kind=StepKind::Mash/>
            <BreweryFields>
                <BreweryDraftSelect id=fields::MASH_METHOD.0 label=fields::MASH_METHOD.1 value=method on_change=change_method error=method_error hint=method_hint options=vec![("".into(), "Not specified".into()), ("manual".into(), "Manual volume".into()), ("profile_thickness".into(), "From mash thickness".into())]/>
                <Show when=move || manual.get()>
                    <NumberField id=fields::MASH_WATER.0 label=fields::MASH_WATER.1 unit="L" path="process.mash_water_l" draft=draft read=|d: &RecipeDocumentV1| d.process.mash_water_l write=|d: &mut RecipeDocumentV1, n| d.process.mash_water_l = n/>
                </Show>
                <Show when=move || top_up.get()>
                    <NumberField id=fields::TOP_UP.0 label=fields::TOP_UP.1 unit="L" path="process.kettle_top_up_l" draft=draft read=|d: &RecipeDocumentV1| d.process.kettle_top_up_l write=|d: &mut RecipeDocumentV1, n| d.process.kettle_top_up_l = n/>
                </Show>
            </BreweryFields>
            {move || has_plan.get().then(|| view! {
                <BrewerySection title="Water plan" actions=ChildrenFn::to_children(move || view! { <BreweryStaleCue text=shown.cue announce=shown.announce testid="estimate-stale" announce_testid="estimate-announce"/> })><BreweryStale stale=shown.stale><BreweryMetricList total=true>
                    {move || {
                        // Numbers while current or recalculating (old, dimmed); dashes once the draft
                        // settles without an estimate. Same rows either way, so nothing moves.
                        let targets = shown.values.get().then(|| shown.preview.get()).flatten().map(|preview| preview.targets);
                        let value = |pick: fn(&krausen_types::formulation::Targets) -> Option<f64>| targets.as_ref().and_then(pick);
                        view! {
                            {preview_metric("Mash water", value(|t| t.mash_water_l), None, None, "L", 1, Quiet)}
                            {preview_metric("Sparge water", value(|t| t.sparge_water_l), None, None, "L", 1, Quiet)}
                            {preview_metric("Kettle water", value(|t| t.kettle_water_l), None, None, "L", 1, Quiet)}
                            {preview_metric("Grain absorption", value(|t| t.grain_absorption_l), None, None, "L", 1, Quiet)}
                            {preview_metric("Pre-boil volume", value(|t| t.pre_boil_l), None, None, "L", 1, Quiet)}
                            {preview_metric("Post-boil volume", value(|t| t.post_boil_l), None, None, "L", 1, Quiet)}
                            {preview_metric("Pre-boil gravity", value(|t| t.pre_boil_sg), None, None, "SG", 3, Quiet)}
                            {preview_metric("Total water", value(|t| t.total_water_l), None, None, "L", 1, Quiet)}
                        }
                    }}
                </BreweryMetricList>
                {move || {
                    // A plan that overfills a vessel is flagged; it never blocks Save or start.
                    let warnings = shown.values.get().then(|| shown.preview.get()).flatten().map(|preview| preview.targets.vessel_warnings).unwrap_or_default();
                    (!warnings.is_empty()).then(|| view! { <div data-testid="vessel-warnings">{warnings.iter().map(|warning| { let (title, body) = vessel_warning_text(warning); view! { <crate::components::BreweryNotice title=title body=body warning=true/> } }).collect_view()}</div> })
                }}
                </BreweryStale></BrewerySection>
            })}
        </BreweryPanel>
    }
}

/// One line for an overfilled vessel. The mash tun figure is the mash water plus
/// the grain's displacement (a fixed 0.67 L/kg assumption of the equipment spec).
fn vessel_warning_text(warning: &krausen_types::formulation::VesselWarning) -> (String, String) {
    use krausen_types::formulation::Vessel;
    let (name, what) = match warning.vessel {
        Vessel::MashTun => ("Mash tun", "mash water and grain"),
        Vessel::SpargeVessel => ("Sparge vessel", "sparge water"),
        Vessel::Kettle => ("Kettle", "pre-boil wort at boiling volume"),
    };
    (
        format!("{name} overfilled"),
        format!("{what} is {:.2} L against a volume of {:.2} L.", warning.required_l, warning.capacity_l),
    )
}

#[component]
fn BoilAndWhirlpool(draft: RwSignal<RecipeDraft>, shown: Shown) -> impl IntoView {
    let unsupported = Memo::new(move |_| draft.with(|d| has_unsupported_process(d.document())));
    view! {
        <BreweryPanel id="recipe-section-boil" title="Boil & whirlpool" description="Boil duration and the whirlpool stand after flameout." stacked=true>
            <BreweryFields>
                <NumberField id=fields::BOIL.0 label=fields::BOIL.1 unit="min" path="boil_minutes" draft=draft read=|d: &RecipeDocumentV1| d.boil_minutes write=|d: &mut RecipeDocumentV1, n| d.boil_minutes = n/>
                <NumberField id=fields::WHIRLPOOL.0 label=fields::WHIRLPOOL.1 unit="min" path="process.whirlpool_minutes" draft=draft read=|d: &RecipeDocumentV1| d.process.whirlpool_minutes write=|d: &mut RecipeDocumentV1, n| d.process.whirlpool_minutes = n/>
                <NumberField id=fields::WHIRLPOOL_TEMP.0 label=fields::WHIRLPOOL_TEMP.1 unit="°C" path="process.whirlpool_temperature_c" draft=draft read=|d: &RecipeDocumentV1| d.process.whirlpool_temperature_c write=|d: &mut RecipeDocumentV1, n| d.process.whirlpool_temperature_c = n/>
            </BreweryFields>
            // Same rule as the Water plan: dimmed and hidden from assistive technology while the
            // estimate it was read from is out of date, gone once the draft settles without one.
            {move || shown.values.get().then(|| shown.preview.get()).flatten().and_then(|preview| preview_chilling(&preview)).map(|text| view! { <BreweryStale stale=shown.stale><p class="note-caption" data-testid="chilling-context">{text}</p></BreweryStale> })}
            {move || unsupported.get().then(|| view! { <p class="note-caption" data-testid="unsupported-process">"Imported process details that Krausen does not model are kept with this recipe and do not affect estimates."</p> })}
        </BreweryPanel>
    }
}

#[component]
fn Fermentation(draft: RwSignal<RecipeDraft>) -> impl IntoView {
    view! {
        <BreweryPanel id="recipe-section-fermentation" title="Fermentation" description="Fermentation steps and the whole-wort attenuation." stacked=true>
            <StepGroup draft=draft kind=StepKind::Fermentation/>
            <BreweryFields>
                <NumberField id=fields::ATTENUATION.0 label=fields::ATTENUATION.1 unit="%" scale=100.0 described_by="recipe-attenuation-note" path="process.apparent_attenuation_fraction" draft=draft read=|d: &RecipeDocumentV1| d.process.apparent_attenuation_fraction write=|d: &mut RecipeDocumentV1, n| d.process.apparent_attenuation_fraction = n/>
            </BreweryFields>
            <p id="recipe-attenuation-note" class="note-caption">"Effective apparent attenuation of the whole wort, used for final gravity. The yeast attenuation under Yeast is informational and is never copied here."</p>
        </BreweryPanel>
    }
}

#[component]
fn DescriptionAndNotes(draft: RwSignal<RecipeDraft>) -> impl IntoView {
    let description = Signal::derive(move || draft.with(|d| d.document().description.clone().unwrap_or_default()));
    let notes = Signal::derive(move || draft.with(|d| d.document().notes.clone().unwrap_or_default()));
    let change_description = Callback::new(move |value: String| {
        draft.update(|d| d.edit(|doc| doc.description = (!value.is_empty()).then_some(value)))
    });
    let change_notes = Callback::new(move |value: String| {
        draft.update(|d| d.edit(|doc| doc.notes = (!value.is_empty()).then_some(value)))
    });
    view! {
        <BreweryPanel id="recipe-section-notes" title="Description & notes" description="Optional text; neither affects estimates or readiness." stacked=true>
            <BreweryDraftArea id="recipe-description" label="Description" value=description on_change=change_description/>
            <BreweryDraftArea id="recipe-notes" label="Notes" value=notes on_change=change_notes/>
        </BreweryPanel>
    }
}

#[component]
fn StepGroup(draft: RwSignal<RecipeDraft>, kind: StepKind) -> impl IntoView {
    // One concise outcome, tied to the draft generation it describes: the next
    // unrelated edit (including Undo) leaves it behind instead of keeping a
    // stale claim in the live region.
    let status = RwSignal::new((0_u64, String::new()));
    let say = move |text: String| status.set((draft.with_untracked(|d| d.generation), text));
    let undo = RwSignal::new(Vec::<UndoStep>::new());
    // Undo tokens and the message belong to one document; replacing it
    // (reload, recovery, reset) drops both.
    Effect::new(move |previous: Option<u64>| {
        let epoch = draft.with(|d| d.epoch());
        if previous.is_some_and(|before| before != epoch) {
            undo.set(Vec::new());
            status.set((0, String::new()));
        }
        epoch
    });
    let add = move |_| {
        let Some(id) = crate::utils::request_id::new_request_id() else { return };
        draft.update(|d| add_step(d, kind, id.clone()));
        let label = draft.with_untracked(|d| step_label(d.document(), kind, &id));
        say(format!("Added {label}."));
        crate::utils::focus::focus_first_available_unless_moved(vec![kind.add_button_id()], vec![step_field_id(kind, &id, "name")]);
    };
    let remove = Callback::new(move |id: String| {
        let ids = draft.with_untracked(|d| step_ids(d.document(), kind));
        let Some(position) = ids.iter().position(|row| *row == id) else { return };
        let Some(removed) = draft.try_update(|d| remove_step(d, kind, &id)).flatten() else { return };
        say(format!("Removed {}.", removed.label));
        undo.update(|stack| {
            stack.push(removed);
            if stack.len() > UNDO_DEPTH {
                stack.remove(0);
            }
        });
        crate::utils::focus::focus_first_available_unless_moved(Vec::new(), removal_successors(kind, &ids, position));
    });
    let restore = move |_| {
        let mut token = None;
        undo.update(|stack| token = stack.pop());
        let Some(token) = token else { return };
        let (id, label) = (token.id.clone(), token.label.clone());
        match draft.try_update(|d| restore_step(d, token)) {
            Some(Restore::Restored) => {
                let label = draft.with_untracked(|d| step_label(d.document(), kind, &id));
                say(format!("Restored {label}."));
                crate::utils::focus::focus_first_available_unless_moved(vec![kind.undo_button_id()], vec![step_field_id(kind, &id, "name")]);
            }
            Some(Restore::AlreadyPresent) => {
                say(format!("{label} is already in the recipe."));
                crate::utils::focus::focus_first_available_unless_moved(vec![kind.undo_button_id()], vec![kind.add_button_id()]);
            }
            Some(Restore::Stale) | None => {
                undo.set(Vec::new());
                say(format!("{label} can no longer be restored because the recipe was reloaded or replaced."));
                crate::utils::focus::focus_first_available_unless_moved(vec![kind.undo_button_id()], vec![kind.add_button_id()]);
            }
        }
    };
    let title = match kind {
        StepKind::Mash => "Mash steps",
        StepKind::Fermentation => "Fermentation steps",
    };
    view! {
        <BrewerySection title=title>
            <BreweryStepGroup>
            <For each=move || draft.with(|d| step_ids(d.document(), kind)) key=|id| id.clone() children=move |id| view! { <StepRow draft=draft id=id kind=kind on_remove=remove/> } />
            <p class="sr-only" role="status">{move || { let (generation, text) = status.get(); if draft.with(|d| d.generation) == generation { text } else { String::new() } }}</p>
            <BreweryActionGroup>
                <Button id=kind.add_button_id() variant=ButtonVariant::Secondary on:click=add>"Add step"</Button>
                {move || undo.with(|stack| stack.last().map(|token| format!("Undo removal of {}", token.label))).map(|label| view! {
                    <Button id=kind.undo_button_id() variant=ButtonVariant::Ghost aria_label=label on:click=restore>"Undo"</Button>
                })}
            </BreweryActionGroup>
            </BreweryStepGroup>
        </BrewerySection>
    }
}

#[derive(Clone, Copy)]
enum StepNumber {
    Time,
    Temperature,
    Volume,
}

#[component]
fn StepRow(draft: RwSignal<RecipeDraft>, id: String, kind: StepKind, on_remove: Callback<String>) -> impl IntoView {
    let label = {
        let id = id.clone();
        Signal::derive(move || draft.with(|d| step_label(d.document(), kind, &id)))
    };
    let number = {
        let id = id.clone();
        Signal::derive(move || {
            draft.with(|d| step_ids(d.document(), kind).iter().position(|row| *row == id).map(|p| p + 1).unwrap_or_default())
        })
    };
    let name = {
        let id = id.clone();
        Signal::derive(move || {
            draft.with(|d| kind.rows(d.document()).iter().find(|r| r.row_id == id).and_then(|r| r.name.clone()).unwrap_or_default())
        })
    };
    let change_name = {
        let id = id.clone();
        Callback::new(move |text: String| {
            draft.update(|d| {
                d.edit(|doc| {
                    if let Some(row) = kind.rows_mut(doc).iter_mut().find(|r| r.row_id == id) {
                        row.name = (!text.is_empty()).then_some(text)
                    }
                })
            })
        })
    };
    let remove_id = id.clone();
    let name_field_id = step_field_id(kind, &id, "name");
    let remove_field_id = remove_button_id(kind, &id);
    let measurement_id = id.clone();
    let remove_label = Signal::derive(move || format!("Remove step, {}", label.get()));
    view! {
        <BreweryStepRow label=label number=number
            actions=ChildrenFn::to_children(move || {
                let remove_id = remove_id.clone();
                view! { <Button id=remove_field_id.clone() variant=ButtonVariant::GhostDestructive aria_label=remove_label on:click=move |_| on_remove.run(remove_id.clone())>"Remove step"</Button> }
            })
            measurements=ChildrenFn::to_children(move || view! {
                <StepNumberField draft=draft id=measurement_id.clone() kind=kind field=StepNumber::Time label=fields::STEP_TIME unit="min"/>
                <StepNumberField draft=draft id=measurement_id.clone() kind=kind field=StepNumber::Temperature label=fields::STEP_TEMPERATURE unit="°C"/>
                <StepNumberField draft=draft id=measurement_id.clone() kind=kind field=StepNumber::Volume label=fields::STEP_VOLUME unit="L"/>
            })>
            <BreweryDraftField id=name_field_id label="Step name" value=name on_change=change_name/>
        </BreweryStepRow>
    }
}

#[component]
fn StepNumberField(
    draft: RwSignal<RecipeDraft>,
    id: String,
    kind: StepKind,
    field: StepNumber,
    label: &'static str,
    unit: &'static str,
) -> impl IntoView {
    let suffix = match field {
        StepNumber::Time => "time_minutes",
        StepNumber::Temperature => "temperature_c",
        StepNumber::Volume => "volume_l",
    };
    let path = format!("{}[{id}].{suffix}", kind.prefix());
    let input_id = step_field_id(kind, &id, suffix);
    let read_id = id.clone();
    let read_path = path.clone();
    let error_path = path.clone();
    let blur_path = path.clone();
    let blur = Callback::new(move |()| draft.update(|d| d.settle_number(&blur_path)));
    let value = Signal::derive(move || {
        draft.with(|d| {
            let n = kind.rows(d.document()).iter().find(|r| r.row_id == read_id).and_then(|r| match field {
                StepNumber::Time => r.time_minutes,
                StepNumber::Temperature => r.temperature_c,
                StepNumber::Volume => r.volume_l,
            });
            d.number_text(&read_path, n)
        })
    });
    let error = Signal::derive(move || draft.with(|d| d.number_error(&error_path).map(str::to_owned)));
    let hint_path = path.clone();
    let hint = Signal::derive(move || draft.with(|d| d.validation_hint(&hint_path).map(str::to_owned)));
    let change = Callback::new(move |text| {
        let id = id.clone();
        let path = path.clone();
        draft.update(|d| {
            d.edit_number(&path, text, |doc, n| {
                if let Some(r) = kind.rows_mut(doc).iter_mut().find(|r| r.row_id == id) {
                    match field {
                        StepNumber::Time => r.time_minutes = n,
                        StepNumber::Temperature => r.temperature_c = n,
                        StepNumber::Volume => r.volume_l = n,
                    }
                }
            })
        });
    });
    view! { <BreweryDraftField id=input_id label=label unit=unit value=value error=error hint=hint on_change=change on_blur=blur numeric=true/> }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft_with_steps() -> RecipeDraft {
        let mut doc = RecipeDocumentV1::default();
        for (id, name) in [("m1", "Mash in"), ("m2", "Saccharification"), ("m3", "Mash out")] {
            doc.process.mash_steps.push(ProcessStep {
                row_id: id.into(),
                name: Some(name.into()),
                time_minutes: Some(10.0),
                temperature_c: Some(65.0),
                volume_l: None,
                extensions: Default::default(),
            });
        }
        RecipeDraft::new(doc)
    }

    #[test]
    fn step_labels_carry_position_and_name_so_duplicates_differ() {
        let mut draft = draft_with_steps();
        draft.edit(|doc| doc.process.mash_steps[2].name = Some("Mash in".into()));
        let doc = draft.document();
        assert_eq!(step_label(doc, StepKind::Mash, "m1"), "Mash step 1 of 3, Mash in");
        assert_eq!(step_label(doc, StepKind::Mash, "m3"), "Mash step 3 of 3, Mash in");
        draft.edit(|doc| doc.process.mash_steps[1].name = None);
        assert_eq!(step_label(draft.document(), StepKind::Mash, "m2"), "Mash step 2 of 3");
    }

    #[test]
    fn add_step_appends_an_empty_step_with_the_given_id() {
        let mut draft = draft_with_steps();
        add_step(&mut draft, StepKind::Fermentation, "f1".into());
        let step = &draft.document().process.fermentation_steps[0];
        assert_eq!(step.row_id, "f1");
        assert_eq!((step.name.as_deref(), step.time_minutes, step.temperature_c, step.volume_l), (None, None, None, None));
        assert_eq!(draft.document().process.mash_steps.len(), 3, "the other group is untouched");
    }

    #[test]
    fn remove_and_undo_restore_id_order_and_raw_strings() {
        let mut draft = draft_with_steps();
        draft.edit_number("process.mash_steps[m2].time_minutes", "30.123456789012345678".into(), |doc, n| doc.process.mash_steps[1].time_minutes = n);
        draft.edit_number("process.mash_steps[m2].temperature_c", "6x".into(), |_, _| {});
        let before = draft.document().clone();
        let removed = remove_step(&mut draft, StepKind::Mash, "m2").expect("step existed");
        assert_eq!(step_ids(draft.document(), StepKind::Mash), ["m1", "m3"]);
        assert!(draft.raw_numbers().get("process.mash_steps[m2].time_minutes").is_none());
        assert_eq!(restore_step(&mut draft, removed.clone()), Restore::Restored);
        assert_eq!(draft.document(), &before);
        assert_eq!(draft.raw_numbers().get("process.mash_steps[m2].time_minutes").map(String::as_str), Some("30.123456789012345678"));
        assert_eq!(draft.unfinished_inputs(), [("process.mash_steps[m2].temperature_c".to_owned(), "6x".to_owned())]);
        assert_eq!(restore_step(&mut draft, removed), Restore::AlreadyPresent);
        assert_eq!(step_ids(draft.document(), StepKind::Mash), ["m1", "m2", "m3"]);
    }

    #[test]
    fn undo_is_refused_after_the_document_was_replaced() {
        let mut draft = draft_with_steps();
        let removed = remove_step(&mut draft, StepKind::Mash, "m2").unwrap();
        let replacement = draft.document().clone();
        draft.replace_saved(replacement);
        assert_eq!(restore_step(&mut draft, removed), Restore::Stale);
    }

    #[test]
    fn removal_focus_prefers_next_then_previous_then_add() {
        let ids: Vec<String> = ["a", "b", "c"].map(String::from).into();
        let id = |x: &str| remove_button_id(StepKind::Mash, x);
        let add = StepKind::Mash.add_button_id();
        assert_eq!(removal_successors(StepKind::Mash, &ids, 0), [id("b"), add.clone()], "first");
        assert_eq!(removal_successors(StepKind::Mash, &ids, 1), [id("c"), id("a"), add.clone()], "middle");
        assert_eq!(removal_successors(StepKind::Mash, &ids, 2), [id("b"), add.clone()], "last");
        assert_eq!(removal_successors(StepKind::Mash, &ids[..1], 0), [add], "only");
    }

    #[test]
    fn a_quick_recalculation_is_never_announced() {
        let mut a = Announcement::default();
        a.begin();
        assert_eq!(a.text(), None);
        assert!(!a.end(), "nothing was announced, so no 'updated' either");
        assert_eq!(a.text(), None);
    }

    #[test]
    fn a_slow_recalculation_is_announced_once_then_updated() {
        let mut a = Announcement::default();
        a.begin();
        a.debounce_elapsed(true, EstimatePhase::Calculating);
        assert_eq!(a.text().as_deref(), Some("Out of date, recalculating"));
        a.outcome_changed(EstimatePhase::Calculating);
        assert_eq!(a.text().as_deref(), Some("Out of date, recalculating"), "constant while still calculating");
        assert!(a.end());
        assert_eq!(a.text().as_deref(), Some("Estimate updated"));
        a.clear_updated();
        assert_eq!(a.text(), None);
    }

    #[test]
    fn a_failure_after_the_announcement_is_announced_once_not_left_stale() {
        let mut a = Announcement::default();
        a.begin();
        a.debounce_elapsed(true, EstimatePhase::Calculating);
        a.outcome_changed(EstimatePhase::Unavailable);
        assert_eq!(a.text().as_deref(), Some("Estimate unavailable"), "the status must not keep claiming 'recalculating'");
        // Flipping back to calculating (a new keystroke) and to the same outcome is silent.
        a.outcome_changed(EstimatePhase::Calculating);
        assert_eq!(a.text().as_deref(), Some("Estimate unavailable"));
        a.outcome_changed(EstimatePhase::Unavailable);
        assert_eq!(a.text().as_deref(), Some("Estimate unavailable"));
        // A different settled outcome is announced once.
        a.outcome_changed(EstimatePhase::InvalidInput);
        assert_eq!(a.text().as_deref(), Some("Estimate unavailable until the inputs are valid"));
        assert!(a.end());
        assert_eq!(a.text().as_deref(), Some("Estimate updated"));
    }

    #[test]
    fn an_outcome_that_is_already_settled_when_the_debounce_ends_is_announced_directly() {
        let mut a = Announcement::default();
        a.begin();
        a.debounce_elapsed(true, EstimatePhase::InvalidInput);
        assert_eq!(a.text().as_deref(), Some("Estimate unavailable until the inputs are valid"));
    }

    #[test]
    fn nothing_is_announced_before_there_is_an_estimate_to_be_out_of_date() {
        let mut a = Announcement::default();
        a.begin();
        a.debounce_elapsed(false, EstimatePhase::Calculating);
        a.outcome_changed(EstimatePhase::Unavailable);
        assert_eq!(a.text(), None);
    }

    #[test]
    fn repeated_debounce_firings_do_not_restart_or_rewrite_an_episode() {
        let mut a = Announcement::default();
        a.begin();
        a.debounce_elapsed(true, EstimatePhase::Calculating);
        a.begin();
        a.debounce_elapsed(true, EstimatePhase::Unavailable);
        assert_eq!(a.text().as_deref(), Some("Out of date, recalculating"), "an episode keeps its first text until an outcome settles");
    }

    #[test]
    fn mash_method_transitions_show_only_what_can_apply() {
        let mut draft = RecipeDraft::new(RecipeDocumentV1::default());
        let shown = |d: &RecipeDraft| (show_manual_mash_water(d), show_kettle_top_up(d));
        assert_eq!(shown(&draft), (false, false), "unspecified");
        draft.edit(|doc| doc.process.mash_water_method = Some("manual".into()));
        assert_eq!(shown(&draft), (true, true), "manual");
        draft.edit(|doc| doc.process.mash_water_method = Some("profile_thickness".into()));
        assert_eq!(shown(&draft), (false, true), "profile thickness derives mash water, top-up still applies");
        draft.edit(|doc| doc.process.mash_water_method = Some("unrecognised".into()));
        assert_eq!(shown(&draft), (false, true), "an unknown retained method keeps top-up reachable");
        draft.edit(|doc| doc.process.mash_water_method = None);
        assert_eq!(shown(&draft), (false, false));
    }

    #[test]
    fn chilling_context_states_the_canonical_model_for_every_choice() {
        let limit = "a hop first added after the wort has cooled is not modelled";
        for (chilling, has_hops, expected) in [
            (Some(CoolingMethodV1::Natural), true, "natural cooling after flameout"),
            (Some(CoolingMethodV1::ImmersionChiller), true, "immersion chiller after flameout"),
            (Some(CoolingMethodV1::IceBath), true, "ice bath after flameout"),
            (Some(CoolingMethodV1::CounterflowChiller), false, "counterflow chiller after flameout"),
            (None, true, "not specified. A bitterness estimate needs a cooling method"),
            (None, false, "not specified."),
        ] {
            let text = chilling_context(&chilling, has_hops);
            assert!(text.contains(expected), "{text}");
            assert!(text.contains(limit), "every state names the unmodelled late-hop case: {text}");
        }
        assert!(!chilling_context(&None, false).contains("needs a cooling method"), "no hops, no bitterness claim");
        assert!(!chilling_context(&Some(CoolingMethodV1::Natural), true).contains("not specified"), "natural is explicit, never 'not specified'");
    }

    #[test]
    fn unsupported_process_metadata_is_reported_only_when_present() {
        let mut doc = RecipeDocumentV1::default();
        assert!(!has_unsupported_process(&doc));
        doc.process.unsupported.insert("imported_step".into(), serde_json::json!({"x": 1}));
        assert!(has_unsupported_process(&doc));
    }

    #[test]
    fn conditional_water_controls_never_hide_a_retained_value() {
        let mut draft = RecipeDraft::new(RecipeDocumentV1::default());
        assert!(!show_manual_mash_water(&draft) && !show_kettle_top_up(&draft), "nothing to show yet");
        draft.edit(|doc| doc.process.mash_water_method = Some("manual".into()));
        assert!(show_manual_mash_water(&draft) && show_kettle_top_up(&draft));
        draft.edit_number("process.mash_water_l", "15".into(), |doc, n| doc.process.mash_water_l = n);
        draft.edit_number("process.kettle_top_up_l", "0".into(), |doc, n| doc.process.kettle_top_up_l = n);
        draft.edit(|doc| doc.process.mash_water_method = None);
        assert!(show_manual_mash_water(&draft), "a stored mash volume stays visible after the method is cleared");
        assert!(show_kettle_top_up(&draft), "a real zero top-up is a value, not absence");
        assert_eq!(draft.document().process.kettle_top_up_l, Some(0.0));
        draft.edit(|doc| { doc.process.mash_water_l = None; doc.process.kettle_top_up_l = None; });
        draft.edit_number("process.mash_water_l", String::new(), |doc, n| doc.process.mash_water_l = n);
        assert!(show_manual_mash_water(&draft), "typed-then-cleared text keeps the field while it is being edited");
    }
}
