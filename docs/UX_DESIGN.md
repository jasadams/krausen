# UX design and interaction contract

Status: **user-approved target contract, 2026-11-02**. Mandatory for all coders
and reviewers. Contract changes require independent review before publication;
no implementation, runtime pass, accessibility certification or ticket
completion is implied. Read [information architecture](INFORMATION_ARCHITECTURE.md)
for authority, destinations and reference departures, [DESIGN.md](../DESIGN.md)
for visual/component ownership, and the canonical
[architecture](ARCHITECTURE.md), [data](DATA_MODEL.md) and
[calculation](CALCULATIONS.md) contracts for backend semantics.

MUST/MUST NOT are normative. The stable UX-/INT-/VIS-/ACC- IDs below define
target behavior. Owners are implementation responsibilities, not new services or
automatic ticket assignments. Acceptance statements and matrices describe future
required evidence, **not executed checks**. Implementers MUST preserve every
current capability while migrating its presentation.

## Approved reference and visual contracts

The actual approved light references are the
[desktop recipe](design/previews/recipe-desktop.png),
[mobile recipe](design/previews/recipe-mobile.png) and
[component showcase](design/previews/design-system.png), approved 2026-10-31.
They show useful identity/amount/timing rows, a dominant formulation, nearby
equipment and estimates, quiet row actions and compact mobile composition.
They are not a catalogue/backend promise, numerical oracle or complete field
inventory. Current explicit requirements supersede sample navigation furniture
and the sample's simplified yeast/process model. There is no supplied approved
dark-recipe screenshot; dark acceptance uses semantic tokens and actual state
verification, not an invented pixel-match baseline.

<a id="vis-001"></a>
### VIS-001: Preserve identity, not the broken baseline

**Owner:** shared token/component maintainers. Retain warm ivory/paper surfaces,
charcoal, warm stone navigation, restrained copper primary and sage support;
dark mode uses approved green/charcoal semantic surfaces. Retain Instrument Serif
expressive titles, DM Sans reading/controls, Geist Mono/tabular quantities, local
fallbacks and 15px root. Keep units and readable type. Exact reference values
remain in prototype tokens and their reviewed production mapping, not a second
table here. Current source/captures MUST NOT replace the approved sample as the
design baseline merely because they previously passed a layout check.

**Acceptance:** direct sample comparison in desktop/mobile light, token-based
dark review, loaded/fallback fonts and semantic state contrast measurements.

<a id="vis-002"></a>
### VIS-002: Shared kit must express the real page

**Owner:** shared kit before consuming pages. Provide reusable panel header-action
and footer slots, section/ingredient-row patterns, numeric-field typography,
save-status and metric/summary density variants. The actual recipe-used variants
MUST appear in the actual showcase. Named variants own responsive appearance;
page composition may place regions but MUST NOT restyle component internals,
copy markup, add a recipe-only compact theme or hide kit elements via page
ancestor selectors. Workflow/business state stays with the appropriate page/app,
not a generic presentation component.

**Acceptance:** inspect shared implementations/call sites and render identical
variants in recipe and showcase in normal/error/pending/disabled states. Sampling
32 components or matching palette alone does not prove full equivalence.

<a id="vis-003"></a>
### VIS-003: Clear density and action hierarchy

**Owner:** shared spacing/buttons/action groups and page composition. Use shared
spacing tokens and wrapping action groups for field, row, header/body/footer and
task boundaries. One copper primary per active task surface; Cancel/Back are
quiet, Add secondary, removal subdued but discoverable. Do not solve uneven
spacing with page offsets or shrink legibility to fit a heading. No card around
every label/value, duplicated large recipe title, or decorative chrome taking
priority over normal ingredient editing. Intrinsic button padding and layout
gaps MUST be reviewed separately, including import pending/error states.

**Acceptance:** consistent real component bounds and hierarchy across import,
addition, start, profile and recipe surfaces; no second copper Cancel.

<a id="vis-004"></a>
### VIS-004: Dominant formulation and adaptive estimates

**Owner:** editor composition using shared variants. Desktop MUST give formulation
most reading space with a nearby supporting estimates rail. Mobile MUST use a
short header/context/action area and compact expandable metrics, not nearly a
screen of chrome before an ordinary ingredient control. Full metrics, units,
style text and water plan remain accessible; no fixed summary may cover fields.
Test normal editing reachability with long titles/status and enlarged text, not
only a Fermentables heading inside one 844px viewport. A historical 0.375px
heading clearance is not responsive acceptance. Quiet aligned water rows are
appropriate for secondary values; headline numbers MUST NOT all compete equally.

**Acceptance:** full-page and first-task screenshots plus keyboard/touch access at
320/390/wide/short layouts and real zoom, without subpixel or tiny-type hacks.

<a id="vis-005"></a>
### VIS-005: Shared dialog geometry

**Owner:** actual shared modal/dialog component and its reset/styles. True dialogs
MUST center in the desktop viewport, including after page scroll, with gutters,
bounded usable height, scrollable long content and reachable actions. Narrow/
short/virtual-keyboard layouts MUST adapt within safe edges without horizontal
document overflow. Fix the shared owner, not picker-specific offsets. The
reported top-left Add-dialog geometry and suspected reset interaction require
rendered bounds/computed-style reproduction before claiming a root cause or fix.
Account dropdowns are intentionally not dialogs.

**Acceptance:** open/scroll/resize bounds, long content, short viewport, focus and
reachable footer for every affected ingredient/task variant under ACC-003.

## Recipe organization

<a id="ux-001"></a>
### UX-001: Concise editable identity and stable commands

**Owner:** recipe header/composition. Provide one expressive directly editable
title with proper heading semantics and access to the full long name. Keep batch
**into fermenter**, selected style/No style and selected equipment visible as
formulation context. Boil duration has one editing owner in Boil & whirlpool.
Save, Undo and textual save status have stable placement; Start brew and a named
Recipe actions entry remain discoverable. Use Recipe / Saved versions / Brews
once under IA-006. No separate read-only recipe mode/Edit entrance or second
competing ingredient/process/settings tab hierarchy.

**Acceptance:** new/long-title/clean/dirty/error header, preserved direct editing
and source context, no status-driven command displacement or duplicate history.

The editor's primary action MUST follow state: Save recipe for new work, Save
changes for dirty/saveable work, and Start brew for clean saved Ready content.
Save remains in its stable location as quiet/unchanged when appropriate, not a
second competing copper action. Invalid input exposes Review highlighted fields
and prevents successful Save; calculation failure has its local Retry action.
Start entry always opens INT-005 review, never a one-click commit.

<a id="ux-002"></a>
### UX-002: Four ingredient groups, one addition grammar

**Owner:** shared ingredient/addition presentation and category integration.
Fermentables, Hops, Yeast and Other additions MUST use consistent Add / Cancel /
explicit Add-to-recipe semantics and a coherent row grammar. Browsing/selecting
MUST NOT create a phantom row; Cancel before addition creates none, final Add
creates exactly one stable local row. Fermentable search and custom workspace
library operations remain supported, with starter/library provenance, pagination,
empty/loading/no-results/error states and retry. Other categories use truthful
manual entry where no supported catalogue exists. Consistent presentation MUST
NOT promise universal searchable catalogues, invented specifications or a second
recipe-only custom fermentable persistence path. Keep every existing manual
specification field and distinct library Save/Add/recipe Save.

**Acceptance:** keyboard and pointer Add/Cancel/Add for all four groups, including
search/select/create/edit/archive/restore/error and manual paths; exact row counts,
copied provenance, no unintended persistence and correct focus under ACC-004.

<a id="ux-003"></a>
### UX-003: Rows foreground brewing decisions

**Owner:** actual shared row and field variants. Ingredient identity/specification,
aligned amount/unit and type-appropriate stage/timing MUST read as a formulation,
not repeated generic forms. Hop use and contact duration MUST be directly visible/
editable; show alpha in human percent, not raw storage fractions. Contextual
temperature, fermentable addition timing, yield and colour remain editable;
secondary provenance/specifications may be details. Identity remains editable
without a view/edit switch. Quiet remove actions need adequate hit areas and
unique accessible row context; details MUST NOT hide the ordinary hop schedule.
Preserve native precision, unsupported stages and raw data when a conditional
field becomes irrelevant. Grain totals/shares follow IA-012: authoritative
projection gap, not UI math or a permanent global-failure sentence on Ready data.

**Acceptance:** duplicate-named hops at different stages/times, long names, row
errors, raw fractions↔percent boundary, removal/Undo and all specifications intact.

<a id="ux-004"></a>
### UX-004: Process follows brewing stages

**Owner:** editor process composition. Use:

| Section | Required controls and supporting context |
| --- | --- |
| Mash & water | Ordered mash steps; allocation method, conditional manual mash water, kettle top-up; nearby resolved mash/sparge/kettle/absorption/pre-/post-boil/total water and pre-boil gravity where supplied |
| Boil & whirlpool | Boil duration, whirlpool duration/temperature; hop timing and effective chilling context with the canonical model's limitations |
| Fermentation | Ordered fermentation steps and **Wort attenuation (%)**, the effective whole-wort input; yeast informational attenuation stays separate and labelled |
| Description & notes | Separate optional description and notes, no calculation/equipment controls |

Step names, durations, temperatures and optional volumes retain IDs, order, units,
raw values and add/remove controls. Numbered/compact populated rows MUST remain
understandable; a later reorder interaction needs separate approval. Do not infer
whole-wort attenuation from yeast metadata, automatically infer schedules, or
claim preserved process fields all affect current calculations. Conditional
visibility MUST NOT erase retained values or turn notes into readiness inputs.

**Acceptance:** control migration inventory below; method/stage transitions,
multi-step/save/reload/Undo and absence versus zero; no formula/default changes.

<a id="ux-005"></a>
### UX-005: Equipment selection, overrides and management are distinct

**Owner:** recipe equipment context and reusable Equipment manager. Visible
selection MUST identify profile or “Recipe-only equipment”, with effective source
and override indication near estimates. “Adjust for this recipe” exposes
inherited/effective/overridden states and per-value reset. Reset MUST remove the
override, not copy a profile/default value into an override. Do not merge distinct
efficiency fields or attenuation semantics.

Equipment manager is a separate reusable task/new UI destination under IA-001,
also reachable contextually without losing the recipe. Preserve list/pagination,
create/edit/Save profile/archive/unarchive and every physical/chilling field.
Group efficiency, water/losses, vessel and mode-specific chilling controls;
conditional controls preserve raw drafts. Clearly distinguish “Editing shared
profile X” from “Using profile Y for this recipe”. Profile Save has its own dirty
baseline, guarded Save/Discard/Stay, pending/conflict/retry and explicit effect on
future previews/new brews. It MUST NOT pretend to save recipe content or alter
old frozen brews. Refreshed start context requires fresh acceptance.

**Acceptance:** independent profile dirty/retry navigation, selection/override/
reset/archive, future preview change and unchanged historical bundle; runtime
reproduction precedes claims about existing profile-draft loss.

<a id="ux-006"></a>
### UX-006: Actionable diagnostics without invented policy

**Owner:** shared diagnostic presentation plus every field/row integration.
Preserve stable codes, semantic field/row paths, severity and affected targets.
Render human field/ingredient-specific messages inline with associated controls
and one concise readiness summary. Distinct issues MUST NOT disappear merely
because generic text duplicates. Summary links MUST activate Recipe, open required
ancestors, render then focus the exact stable-ID control and its explanation.
Uneditable preserved source data needs an honest repair explanation, not a link
to a nonexistent input. Selects as well as numeric fields need association.

Separate Invalid-save blockers, missing-start inputs, optional assumptions and
style guidance. Incomplete may save; Ready is required for start. No style and
optional notes are not errors. Do not conceal missing requirements in “advanced”
without an actionable route or show a permanent wall of generic warnings.

**Acceptance:** multiple same-category issues, hidden row/section jump with exact
activeElement/association and retained values; Save-success plus Incomplete is
truthful, partial available targets stay available under shared outcomes.

<a id="ux-007"></a>
### UX-007: Current estimates identify source and availability

**Owner:** formulation preview and shared metric/style presentation. Show current
draft versus selected saved source, effective equipment, units and truthful
calculation availability. Fermenter batch volume is not interchangeable with total
water; keep each clear and water breakdown close to water inputs. Style uses
full-precision service results and textual Below/Within/Above/Awaiting states;
no band without reference values, no stale marker when unavailable. Assumptions,
source edition and effective details remain accessible, not hash-first chrome.
No source estimate/fixture/historical recalculation may masquerade as current
authoritative targets.

**Acceptance:** draft generation changes, independently unavailable targets,
no-hop true zero, no style, boundary precision, equipment and source changes.

## Interaction and state contracts

<a id="int-001"></a>
### INT-001: Exact raw editing and scoped Undo

**Owner:** raw-input/draft shared fields and editor. Retain transient strings,
absence, real zero, fractional/exponent/long-precision entry, caret/selection and
native undo across typing, blur, disclosures, tab changes and recovery. Do not
round/normalize silently on blur, substitute zero/default or recreate the field
on each preview. Distinguish transitional entry from settled malformed input;
show formatting errors on blur or explicit Save/Next, then keep correction
feedback current. Authoritative domain errors remain service-owned. Invalid/
transient input MUST NOT leave stale current targets. Application Undo deliberately
handles row/recipe actions and raw state; Ctrl/Cmd+Z in an input remains native.

**Acceptance:** multi-character typing, selection/caret, blur/refocus and native
versus app Undo; restored order/IDs/precision and unchanged absent values.

<a id="int-002"></a>
### INT-002: Explicit Save is not local retention

**Owner:** recipe draft/save/recovery. Text must distinguish saved revision,
unsaved changes, locally retained draft, saving, failed save and conflict. Recovery
failure says keep the page open; do not promise durable drafts without evidence.
Save unchanged returns existing history; Incomplete save retains absent data;
Invalid is rejected with actionable fields. Freeze submitted payload/generation,
prevent duplicate dispatch, preserve newer edits and identify if an earlier
payload saved while newer work remains. Failed/stale save retains exact local
draft; readable named comparison identifies base/current revisions and local vs
remote changes. Discard/reload or deliberate Reapply then explicit Save remain;
no silent merge/overwrite. Recovery remains installation/account/workspace/recipe/
base scoped and hidden/cleared appropriately on logout/context loss.

**Acceptance:** clean/Incomplete/Invalid/save failure/conflict, edit-during-save,
recovery storage failure, account isolation and same-request recovery.

<a id="int-003"></a>
### INT-003: Leaving covers each real task, not fictitious batch Save

**Owner:** shared guard plus recipe/profile/library/import/evidence task owners.
Recipe in-app exits offer Save and leave / Discard and leave / Stay; Save failure,
newer edits or Stay MUST NOT navigate. Escape/close means Stay. Browser reload/
close uses native warning where applicable. Capture exact destination and original
scope; execute it once after the chosen successful transition. Profile and library
form saves are independent contexts, never bundled into Save recipe. Uncommitted
import source/choice edits need their own discard/stay contract, not an implicit
import. Evidence offers Return to save evidence / Discard local inputs and leave /
Stay as appropriate, not an impossible Save-and-leave that cannot save explicit
evidence writes. Existing evidence APIs MUST NOT be changed to fabricate batch Save.
No recovery claim for an unsaved domain task without implemented recovery evidence.

**Acceptance:** link/logo/settings/logout/workspace/Back/Forward, Save failure,
Stay during request, profile/library/evidence/import dirty and uncertain work.
Source-inferred evidence guard mismatch needs a focused failing repro before fix.

<a id="int-004"></a>
### INT-004: Uncertain commands survive dismissal and navigation

**Owner:** each mutation task and shared scoped recovery/guard integration.
Distinguish unsent local cancellation, confirmed rejection and unknown result.
For save/library/profile/archive/import/start/evidence/scale/duplicate as
applicable, retain the original scoped request identity, semantic payload and
source before a retry. In-flight/unknown source selection MUST be locked or
visibly pinned to the original attempt. Provide Check previous result/Retry using
that same request; dismissal/Back/route changes MUST NOT clear it and offer a
fresh duplicate as “retry”. Recover across navigation/reload within the documented
recovery mechanism and current authorization. Late completions are fenced to
their task session/context/generation, not a newly opened form.

Closing UI cannot cancel an already sent command or undo a commit. Do not say
“not created”, “nothing started” or “cancelled” unless that result is known.
ResultExpired/auth/conflict outcomes remain typed and truthful; do not invent
backend retry semantics. Preserve a named recovery path before enabling a new
independent command. The reviewed picker-dismissal/start-recovery risks require
controlled failing reproduction, not unverified runtime-bug assertions.

**Acceptance:** lost-response/dismiss/reopen/reload/source-change/late-completion
fault cases inspect original identity/payload and exactly one resulting resource.

<a id="int-005"></a>
### INT-005: Start is a source review then one explicit commit

**Owner:** start task and editor/history integration. Discoverable Start opens a
bounded focused review; it MUST NOT merely jump to a permanent protocol form.
Automatically perform **read-only** preparation. Do not require a Prepare button,
raw revision recall or permanent draft/snapshot tutorial. Final Start is never
automatic and MUST follow the source/equipment/target review in DOM/focus order.

| State | Required presentation/action |
| --- | --- |
| New recipe | Explicit Save recipe and continue or Cancel; failure stays; no saved-source start before successful Save |
| Dirty saved recipe | Save changes and continue / Use saved revision N / Cancel; using saved retains the draft; confirmed Save then fresh read-only review |
| Clean saved | Default identified current saved revision; automatically check, then review |
| Selected saved history | Recognizable revision/date/author/summary when available; start that version without Restore or editing history |
| Checking | Checking saved recipe…; no duplicate dispatch; cancel unsent review is not commit cancellation |
| Ready review | Recipe/source revision, effective profile identity/revision or recipe-only equipment, overrides, fermenter batch, actual accepted targets; detail assumptions accessible before one Start brew confirmation |
| Incomplete/Invalid/unavailable | Specific actionable reasons; historical repair creates a new revision; no stale-current target or doomed enabled commit |
| Changed head/full context | Explain saved recipe or equipment/calculation context change; fresh review and explicit renewed acceptance, never silently accept new targets |
| Starting/unknown | Starting brew or result unconfirmed; lock/pin original source/request and use INT-004 recovery |
| Known success | Concise Brew started from revision N and Open brew, with meaningful result focus and dirty-draft/navigation protection |

Selected immutable revision and optional expected-current-head guard remain
distinct. Starting an old recipe version uses freshly resolved current equipment
for the new brew; reading an old brew uses its frozen equipment/targets. Recheck
the **complete** accepted context including profile, engine, schemas, policies,
style/reference and assumptions through the shared app. No UI-only start or
implicit save, restoration, new default or snapshot weakening is authorized.

**Acceptance:** every state above, historical start without restore, Save failure,
context changes and original-request recovery with unchanged frozen old brews.

<a id="int-006"></a>
### INT-006: Retry and context revalidation are truthful

**Owner:** preview/load/context state. Provide explicit Retry calculation for the
same draft, not “try again” requiring fake edits. Show current outputs unavailable
during failure and reject stale-generation responses. Distinguish checking access,
network unavailable, expired credentials, confirmed context mismatch and forbidden;
retain/hide scoped work while uncertain, with Retry check or safe reconnect/sign-in
and original-context guidance as appropriate. Do not display protected old content
to fix an inconvenient redaction. Successful revalidation preserves logical focus
and task return context where safe, including print; otherwise provide a clear
safe heading/action. Refetch MUST NOT silently destroy preview return state or
misreport every network failure as “wrong workspace”.

**Acceptance:** initial load retry, focus revalidation success/failure/auth/context,
delayed/out-of-order previews, retained scoped edits and no stale disclosure.

<a id="int-007"></a>
### INT-007: Improve the existing library importer

**Owner:** existing BeerXML task and shared form/action presentation. Keep the one
`/beerxml` implementation: file or paste → necessary source/interpretation/profile
choices → native preview → explicit Import. Show parsed identity/content and
missing/unsupported information before source hashes/plumbing. Colour/efficiency
choices remain explicit when required; source estimates remain separate from
current targets. Preserve raw XML/original bytes/provenance/warnings, file source
number and equipment paging. A richer parsed multi-recipe selector is deferred,
not a promised new list API; the current bounded selection capability remains.

Use associated source/choice errors, visible pending action names and shared
footer spacing with quiet Cancel. Cancel before commit creates nothing; leaving
edited uncommitted source follows INT-003. Known successful import offers/opens
the imported editor safely; unknown result retains original identity/recovery,
never “import did not complete” as proof nothing was created. Do not duplicate an
importer to fix the inaccurate premise that it is currently embedded in editing.

**Acceptance:** empty/populated library, file/paste/malformed/oversize/non-UTF8,
necessary choices, incomplete native import, cancel/guard, commit/retry/context,
source preservation and actual reported spacing reproduction.

<a id="int-008"></a>
### INT-008: Output and transform actions are source-bound tasks

**Owner:** recipe actions plus existing print/export/scale/duplicate workflows.
Open on demand, not permanent formulation sections. Explain saved-only Scale/
Duplicate prerequisites locally when invoked; no unexplained disabled mystery,
implicit Save or backend bypass. Preserve target-volume preview, fixed-loss
semantics, accepted context/head checks, explicit apply and independent duplicate
identity without historical markers.

Print/Export MUST identify the source: chosen saved revision, explicitly labelled
Unsaved draft, frozen historical brew, or separately labelled **unedited original
import** where available. Offer a short source choice only when needed; selecting
history supplies its known source rather than another raw revision memory test.
Omissions/lossy-export acknowledgement MUST precede edited BeerXML download.
Retained original bytes MUST NOT overwrite current native values. Print uses the
existing genuine preview and paper treatment with either a real dialog or real
page focus/Back contract; return preserves opener/scroll/draft and print cleanup.
Frozen historical printing remains distinct from current-policy recalculation.
Historical A4 duplicate-page/omitted-actuals work is **deferred**, neither fixed
nor passed by this target reorganization.

**Acceptance:** all sources, omissions acknowledgement, scaling/duplicate
preconditions/retry, keyboard preview/return/context, unchanged historical source.

<a id="int-009"></a>
### INT-009: Brews keep evidence and feedback separate from formulation

**Owner:** history/evidence/comparison composition. Keep optional target/actual
entry, notes, observation selection, corrections/retractions, actual-formulation
amendments, lifecycle/archive/reopen, refresh and retained history accessible.
No questionnaire/timeline or measurements are required to complete a brew.
Clear actual remains absent/retracted, not zero. Additional observations MUST NOT
silently replace selected actuals. Compare/feedback preview and guarded explicit
apply create a new recipe revision, not silent equipment tuning or historical
mutation. Simplifying labels to Open brew/Measurements/Compare MUST preserve
operations and source context; use IA-007 links and capability-specific guards.

**Acceptance:** optional entry/clear/correct/select/race, retained evidence,
explicit feedback/new head, archive/reopen and readable original frozen values.

## Accessibility contracts

<a id="acc-001"></a>
### ACC-001: Names, fields and local states

**Owner:** shared fields and all callers. Use persistent labels, explicit units,
stable DOM IDs derived from stable row IDs, associated hints/errors, correct
invalid/disabled/read-only semantics and visible focus. Name repeated actions by
type/name/stage or position so duplicate ingredient names are distinguishable.
Explain unavailable prerequisites adjacent to the action, not only hover text.
Keep action names while pending and prevent duplicate submission visibly as
well as in handlers. Hidden panels/disclosures MUST exclude inactive controls
from Tab/AT. Native disclosures keep native toggle/focus; diagnostics are the
deliberate jump exception. Actual tabs need roving Arrow/Home/End keyboard support.

**Acceptance:** DOM associations plus keyboard and real AT on complete tasks;
stable diagnostic focus after row insertion/removal and conditional rendering.

<a id="acc-002"></a>
### ACC-002: Chosen account/actions popup pattern

**Owner:** shared anchored popover. Use a native anchored popover/disclosure with
ordinary links/buttons and normal Tab order. Trigger is a native button with
meaningful account/actions name, expanded/controlled relationship; Enter/Space/
click opens. Escape dismisses and returns to trigger; Tab exits naturally without
a trap; outside pointer dismisses without swallowing or triggering an unintended
underlying account action. Selection is textual/semantic, not colour alone.
Constrain popup to the viewport, scroll long memberships and keep focus visible.
Do **not** stamp `role="menu"`, `menuitemradio` or arrow-key promises on this
pattern. A future full ARIA menu would require a documented approved pattern
change and complete roving/menu keyboard implementation, not a role-only patch.

**Acceptance:** full open/Tab/Shift+Tab/Escape/outside/return sequence on desktop
and inside native mobile Navigation, including long lists and disposed openers.

<a id="acc-003"></a>
### ACC-003: True modal and preview boundaries

**Owner:** shared dialogs and task integration. True modals need accessible title/
description, intentional initial focus, contained Tab/Shift+Tab, inert background,
Escape/close/outside behavior appropriate to command certainty, and focus return
to opener or safe visible successor. Native dialog is a foundation, not proof.
Never leave a visually full-screen scale/duplicate/print overlay with keyboard
focus behind it. A real print page instead moves focus to its heading and gives
an explicit Back/return contract. No global Enter-to-save/start shortcuts; Enter
in search selects only deliberately, final Add is explicit, textarea Enter stays
newline. Uncertain sent commands obey INT-004 even on Escape/backdrop dismissal.

**Acceptance:** activeElement traces, background isolation, Enter/Escape, return
after success/cancel/guard/context change and VIS-005 bounds/actions.

<a id="acc-004"></a>
### ACC-004: Add, remove and Undo retain orientation

**Owner:** shared rows and task completion handlers. Open additions with focus on
search/first needed manual field. Committed addition returns focus to the new
row/quantity and announces one concise outcome. Removal focuses next row action,
previous row or Add as appropriate; Undo remains reachable and restores IDs,
order and raw precision with logical viewport context. No mandatory confirmation
for each reversible row removal. Group/action names identify duplicate rows.

**Acceptance:** first/middle/last/only add/remove/Undo, long recipes and keyboard;
no document-body focus fallback or phantom row on Cancel.

<a id="acc-005"></a>
### ACC-005: Announce meaningful settled outcomes

**Owner:** shared save/readiness/task status and callers. Announce meaningful
settled Save/readiness/command success/failure, not the entire diagnostic list on
every keystroke. Detailed diagnostics remain accessible without all being live;
persistent explanation is not automatically `role="status"`. Preview results
MUST NOT steal focus or continually open/collapse sections. Focus a failed Save/
conflict heading or field only as a deliberate action response, not a background
calculation. Error repair remains usable while typing.

**Acceptance:** native screen-reader listen-through with observed speech timing
and editing/caret trace, not merely presence of ARIA attributes.

<a id="acc-006"></a>
### ACC-006: Reachability, enlargement and honest evidence

**Owner:** shared responsive components and affected pages. Prefer at least 44px
touch targets, including quiet row/disclosure actions; verify actual whole targets
and focus rings, not glyph size. Support long names/units/errors, short viewports,
320/390px layouts, desktop, light/dark, reduced motion, larger text and genuine
200%/400% browser zoom. Controls/actions must reflow/scroll safely without focus
occlusion or horizontal document overflow; virtual keyboard must not hide the
only commit/cancel path. Test keyboard-only and real browser/platform AT; record
actual versions and limitations. A screenshot, CSS resize, source attribute or
sampled component check is not functional/native zoom/AT certification.

**Acceptance:** the bounded verification matrix below with actual evidence,
including any not-run combinations, not a universal accessibility claim.

## Complete control migration inventory

Before implementation, workers MUST reconcile this inventory against their actual
base, add newly found controls and link each to destination/tests. This inventory
was curated from all six reviews at `f66a62d`; it is not a runtime certification
or permission to delete fields absent from the simple sample. Paths below are
navigation/reuse points, not a claim their implementation already meets target.

| Existing family / all retained controls | Required destination / scope |
| --- | --- |
| Breadcrumb, heading/name, style/No style, batch litres, boil minutes | UX-001 context; one title owner; boil duration UX-004; no duplicate independently editable value |
| Save/Undo/status, recovery restore/discard/separate candidate, conflict comparison/discard/reapply | Stable bar and bounded notices, INT-001–004; installation/account/workspace/base isolation retained |
| Toolbar Brew history, History/Brews panels, full history, inspect/paging/restore/brew markers | IA-006/007 Saved versions and Brews; one canonical presentation, full URL retained |
| Fermentable name/producer/type, kg, stage, yield %, colour EBC, boil-addition minutes, remove | UX-002/003 row + relevant details; preserve copied specs/provenance |
| Catalogue search, All/Your library, results/select/amount/paging, Create/Edit entry, Archive/Restore | Existing fermentable addition/library subtask; no fake other-category catalogue |
| Custom library name/producer/type/EBC/yield, Save to library/Save changes, Back/Cancel/Add ingredient | Independent persistent library save, explicit local add, command-safe dismissal |
| Hop name/g, stage/contact minutes, alpha %, temperature, remove | Routine schedule directly visible; specifications/details retained |
| Yeast/other name/producer/unit/amount/remove; informational yeast attenuation | Same addition/row grammar; yeast attenuation NOT effective wort attenuation |
| Mash and fermentation step add/remove/name/duration/temperature/optional volume | Respective ordered brewing-stage groups with stable IDs; movement deferred |
| Whole-wort apparent attenuation | Effective Wort attenuation (%) in Fermentation; no automatic yeast inference |
| Description and Notes | Separate optional content fields, not merged or readiness requirements |
| Mash water unspecified/manual/profile-thickness, manual mash litres, kettle top-up | Mash & water with conditional inputs and complete supplied water outputs |
| Whirlpool minutes/temperature; boil duration | Boil & whirlpool, preserving supported flameout/hold semantics and unsupported metadata |
| Equipment select/recipe settings/load more | Visible selector independent of manager; effective identity near metrics |
| Managed profile select/new/name, chilling mode, Save, Archive/Unarchive | Dedicated Equipment manager, independent dirty/retry/revision state |
| Profile efficiency and mash efficiency where separately represented, boil-off, transfer loss, pre-boil deadspace, absorption, thickness, capacity, diameter, opening, boil temperature | Manager efficiency/water-loss/vessel groups; preserve distinct source fields and units |
| Profile natural/forced/counterflow mode, cooling rate/plateau/counterflow rate | Conditional chilling group; retain nonvisible raw values |
| Recipe efficiency override AND separately represented mash efficiency; boil-off/loss/deadspace/absorption/thickness/capacity/diameter/opening/boil temperature/chilling mode/rate/plateau/flow | Adjust for this recipe, inherited/effective/override/reset; never accidentally Save profile |
| OG/FG/ABV/IBU/EBC, fermenter batch, total water, style bands/edition/status | UX-007 and VIS-004 shared metric variants; no stale/current substitution |
| Mash/sparge/kettle/pre-/post-boil/absorption/total water, pre-boil SG; equipment summary/assumptions | Water outputs beside water decisions, effective details/assumptions accessible |
| Every structured diagnostic | UX-006 stable field/row messages and actionable summary, no lost distinct issue |
| Start revision/prepare/commit/retry, targets/assumptions/context digest/conflicts/success | INT-005 review; preparation automatic read-only; technical context details retained |
| Print saved/draft/preview/Print-PDF/Back and frozen print | On-demand source-bound task, INT-008; cleanup/focus/return retained |
| Export saved/draft/original, omission acknowledgement/download | INT-008 explicit source task; unedited original separate, no omission bypass |
| Import file/paste/source number/colour-efficiency choices/equipment-paging/preview/import/open/cancel | Existing library `/beerxml` staged task; all source/interpretation capabilities retained |
| Scale/Duplicate target preview/apply/cancel/retry | Recipe actions, original clean-saved/context/revision semantics retained |
| Library search/active-archived/paging/new/import/open/history/brews/archive/unarchive/stored-source recovery | Recipes task surface, bounded query and recovery preserved |
| Brew readings/clear/correct/retract/select, notes/reasons, actual formulation, lifecycle/archive/reopen/history/retry | Measurements with INT-009/003/004; no fictitious batch Save |
| Compare/refresh/feedback preview/apply/retry/frozen print | Same brew Compare URL, explicit new recipe revision and frozen source |
| Account/profile/theme/security/sessions/invitations/workspace/team/ownership/logout and incoming ceremonies | IA-002/003/008/010; same shared permissions/proof and Light/Dark/System |
| Mobile nav and developer showcase | IA-004/005, one modal owner and deliberate internal direct access |

Reuse points at the reviewed baseline: `crates/krausen-ui/src/pages/recipe_editor.rs`,
`recipe_draft.rs`, `brew_start.rs`, `equipment_profiles.rs`, `catalogue_picker.rs`,
`beerxml.rs`, `recipes.rs`, `brew_history.rs`, `brew_evidence.rs` in that pages
directory; shared `crates/krausen-ui/src/components/brewery_kit.rs`, `layout.rs`,
`recipe_revision_history.rs`, `printable_recipe.rs`, `modal.rs`, and
`crates/krausen-ui/style/kit.css`. Consult graphify and current imports before raw
source exploration. No stale line-number assertion is required for future work.

## Review findings disposition

This portable summary records **all six** report inventories and additional
findings without requiring local reports/captures or exposing workstation data.
Report codes: E1 workflow (F1–F12), E2 visual system (V1–V10 plus additional
items A1–A9 in their listed order), E3 interaction (H1–H6, M7–M11), N1 comparative
inheritance (P1 items 1–3, P2 items 4–8, P3 items 9–11 and departure matrix),
N2 task architecture (task table/routes/journeys), N3 wayfinding (prioritized
findings/departure matrix). Supporting initial Start review is subsumed by
INT-005/UX-006/IA-005. These reviews were advice before approval; only the
consolidated normative target above is authoritative.

Evidence baseline: accepted Krausen source
[`f66a62d`](https://github.com/jasadams/krausen/commit/f66a62d90515b9cc8d81547a9576d62c40cf09be),
read in an equivalent reviewed UI tree, approved repository samples and saved
historical populated images/DOM traces. Those historical states are distinct
from current source findings and hypotheses. No fresh full runtime/AT/zoom/
production acceptance was run by these reports. “Adopt” accepts a target contract,
not a diagnosis or proof that its implementation is absent on every later base.

| Complete finding cluster / report references | Disposition and target |
| --- | --- |
| Generic repeated/missing-field diagnostics; lost paths/select associations; hidden repair route (E1 F1; E2 V3; E3 H1; initial Start 1) | Adopt UX-006/ACC-001: preserve distinct structured reasons and exact-field jumps; no inferred missing fields in Jason's recipe |
| Saved/draft Start mismatch, raw revision/Prepare, target order, missing effective equipment (E1 F2; E2 V6; E3 H2; N2 start; initial Start 2–6) | Adopt INT-005; no one-click commit; historical current equipment is distinct from frozen brew equipment |
| Mixed process/notes, effective vs informational attenuation, sequence labels (E1 F3/F11; E2 V5; E3 issue table) | Adopt UX-004; reorder controls deferred; no automatic schedule/attenuation inference |
| Equipment selector hidden, shared manager conflated with overrides (E1 F4; E2 V4/V5; E3 H6; N1 P2.4; N2 equipment) | Adopt UX-005/IA-001 new UI manager; profile-draft loss is source-inferred, needs failing repro before fix claim |
| Category-add inconsistency, hidden hop stage/time and raw fractions (E1 F5; E2 V2/A2; E3 M8/M10; N2 ingredient task) | Adopt UX-002/003/ACC-004; manual nonfermentables truthful; no universal catalogue promise |
| Mobile chrome dominates; title repetition/long-name clipping; shifting save status (E1 F6; E2 V1/A3/A4/A7; E3 M11) | Adopt UX-001/VIS-003/004/INT-002; long-title/zoom runtime remains to verify; no 0.375px robustness claim |
| Duplicate history entrances/count context (E1 F6; E2 A5; N1 P2.5; N2 histories) | Adopt IA-006/007; Saved versions chosen over alternative History/Saved revisions labels; exact revision detail retained |
| Import hash-first/pre-parse cognitive load and spacing/hierarchy (E1 F7; E2 V9; E3 M10; N1 P2.8; N2 import) | Adopt INT-007/VIS-003; exact reported geometry needs repro; existing separate importer means embedded-import premise is not accurate |
| Permanent export/print utility disclosure, source protocols and preview boundaries (E1 F8; E2 V9; E3 H5; N1/N2 output tasks) | Adopt IA-009/INT-008/ACC-003; existing disclosure is not always expanded; old top-of-editor pictures are not current placement proof |
| Calculation Retry absent and conflict comparison too technical (E1 F9; E3 M9) | Adopt INT-002/006; named changes, original raw entries and no silent merge; network vs context mismatch must be truthful |
| Grain total/share limitation on Ready formulation (E1 F10; E2 V2) | Desired capability gap, deferred shared authoritative projection under IA-012; reject suggested UI calculation shortcut, no invented values |
| Account modal, prominent/single-membership switch, customer Design system (E1 F12; E2 V7; E3 issue table; N1 P1.1–3; N2 account/showcase; N3 P1 first and clutter) | Adopt IA-002/003/005; no redundant workspace outside multi-membership dropdown; retain direct authenticated showcase by explicit policy |
| Dialog placement/reset hypothesis (E2 V8; E1/E3 issue tables) | Adopt VIS-005/ACC-003; user-reported, actual bounds/CSS cause need reproduction; not proven shared reset defect |
| Shared slots/variants vs recipe/showcase mismatch (E2 V10) | Adopt VIS-002; actual shared recipe variants, no page-specific hidden portrait/restyling |
| Numeric text-field typography mismatch (E2 A1) | Adopt VIS-001/002 shared semantic numeric variant; do not switch to browser number inputs merely to match a selector |
| Flattened summary heading hierarchy and oversized secondary water metrics (E2 A6/A9) | Adopt deliberate summary/metric variants VIS-002/004 and UX-007; no global all-panels-serif change |
| Error-height baseline/row alignment shifts (E2 A8) | Adopt shared field-row error layout VIS-002/UX-003; earlier image + current-source risk, not a newly measured runtime failure |
| Uncertain start dismissal/source changes/reload (E3 H3) | Adopt INT-004/005; source-inferred lifecycle risk needs controlled failing repro, original request remains pinned |
| Library Save/Archive pending erased by Back/Escape; late picker completion (E3 H4) | Adopt INT-003/004/UX-002; reproduce before product fix; local Cancel ≠ rollback of sent library command |
| Scale/duplicate/print visually modal without complete boundary (E3 H5) | Adopt ACC-003/INT-008; actual focus behavior still needs verification, not source-only accessibility certification |
| Transient formatting errors/full-list live updates (E3 M7) | Adopt INT-001/ACC-005; native speech noise remains unverified until real AT evidence |
| Add/remove/Undo orientation and duplicated row action names (E3 M8) | Adopt ACC-001/004; deterministic successor focus and native input undo preserved |
| Revalidation hides valid work/mislabels network; no retry (E3 M9) | Adopt INT-006; retain redaction, truthful status/retry and print return context; no unsafe stale display |
| Disabled actions/pending import lack reason/busy semantics (E3 M10) | Adopt ACC-001/INT-007/008; associated prerequisites, known/unknown outcomes distinct |
| Mobile closure/top-layer/duplicate copies/stacked focus (N1 P2.7; N2 mobile; N3 P1 mobile) | Adopt IA-004/ACC-002/003; source-inferred closure/focus risks need repro, no nested account modal |
| Evidence guard offers Save it cannot perform; uncertain discard (N3 P1 evidence) | Adopt INT-003/004/009; failing repro before fix, do not add backend batch save or claim unsupported evidence recovery |
| Workspace deep-link reload/pending races (N3 P1 workspace; N2 switch journey) | Adopt IA-003/INT-003/004; original scope first, pending lock, Recipes on success, failure truthful; runtime effect not certified |
| Weak brew parent/back links, active section falsely current page, import lacks ancestry (N1 P2.6/8; N2 orientation/routes; N3 P2 current page) | Adopt IA-007/010/011; authorized returned parent only; keep existing evidence/analysis URLs |
| Settings misleading workspace copy/duplicates/unsafe return/loading role links (N1 P3.9/10; N2 settings; N3 P2 settings) | Adopt IA-002/008/011; retain appearance System, existing URLs/shared permission/proof; no speculative admin options |
| Missing arrival/skip/landmarks; Back/Stay history and unavailable recovery gaps (N3 P2 continuity; N2 return journeys) | Adopt IA-010/011; browser history implementation needs explicit test, no source-only guarantee |
| Global brew index for ongoing work (N2 preferred fuller IA; N1 P3.11; N3 optional) | Deferred IA-012: separate approved bounded shared-query/parity scope; no fan-out or fake menu stub |
| Standalone Ingredients manager/new catalogues, richer import selection, local custom fermentable path (E1 F5/F7/F11 choices; N1/N2 optional tools) | Deferred IA-012; current manual/library capabilities preserved; do not treat review suggestions as approval |
| Inheritance matrix: domain/home/account/workspaces/settings/theme/showcase/sidebar/mobile/active state/library/import/equipment/ingredients/history/deep links/actions/guards/personal-commercial model (N1 complete matrix; N2/N3 departure matrices) | All disposed by IA-001–012 and reference-departure table: restore mechanisms, not analytics domains or anonymous/commercial policy; chosen ordinary popover rejects N3's optional full-menu recommendation without losing keyboard contract |
| Historical A4 duplicate-page/omitted-actuals printing | Deferred INT-008; not repaired or passed here; old unrelated print/density test failures are not new regression verdicts |
| All screenshots/source review vs runtime/AT/native zoom/contrast/uncertain state limits (all six reports) | Adopt ACC-006 and matrix below; no palette-match, sampled-showcase or prior acceptance as complete conformance proof |

### Existing user-ticket crosswalk

This is a requirements mapping, not tracker writes or status changes. At authoring,
KRA-33–43 were read in full with current comments; their descriptions predate this
approved target. Refine/deduplicate their scope after documentation merge; retain
the user's explicit pause of product work until coordinated resumption. Older
Done design/feature tickets are not automatically reopened.

| Existing ticket | Target links / correction |
| --- | --- |
| KRA-33 Start | INT-005/004, UX-007, ACC-003; review before one commit, not one-click start |
| KRA-34 Diagnostics | UX-006, ACC-001/005; preserve distinct issues and exact-field association |
| KRA-35 Showcase | IA-005; internal direct route policy explicit, link hiding not security |
| KRA-36 Account/workspace | IA-002/003/004, ACC-002, INT-003/004; paused candidate is not accepted baseline |
| KRA-37 Ingredient dialog | VIS-005/ACC-003/006; actual geometry failing repro required |
| KRA-38 Process | UX-004, migration inventory; no fields lost |
| KRA-39 Equipment | IA-001/009, UX-005/INT-003/004; new UI manager approved, no new backend policy |
| KRA-40 Import spacing | VIS-003/INT-007; reproduce exact surface/bounds, coordinate shared action owner |
| KRA-41 Import task | IA-009/010, INT-007; existing standalone `/beerxml`, no duplicate importer |
| KRA-42 Addition consistency | UX-002/003/ACC-004; manual/catalogue availability truthful |
| KRA-43 Print/export | INT-008/ACC-003; sources/acknowledgement/return retained, A4 work deferred |

## Verification and review gate

Every implementation ticket/PR MUST cite applicable stable IDs, actual base,
owned component/page seams, migration controls, dependency/capability gaps and
the precise acceptance subset. Link IDs with these explicit anchors, for example
`docs/UX_DESIGN.md#int-005` or `docs/INFORMATION_ARCHITECTURE.md#ia-003`.
Unassigned gap rows are not implemented; after this documentation merges the
delivery lead maps gaps to existing tickets first and may create only the
explicitly user-authorized implementation-gap tickets. Optional/deferred feature
expansions still require their separate scope approval. No new audit pipeline,
telemetry, unrelated security hardening or generic framework is implied.

Before fixes for reported/source-inferred defects, capture a focused failing
regression and root cause on current code. A source suspicion is not a reproduced
runtime bug. For approved new presentation, assert target behavior through the
real transition. Preserve original safety assertions when updating menu/test
setup; no force-click workaround, weakened guards or test-only copied model.
Follow repository test cadence: focused iteration and one appropriate final
acceptance executor; retain valid unchanged evidence, no duplicate full builds.

### Screen/state verification matrix

All rows below are **required future verification, not PASS claims**. Each owning
ticket executes applicable rows and records excluded/not-run cells; the delivery
lead owns combined coverage. Shared operations/database changes require affected
SQLite/PostgreSQL and UI/MCP checks under ADOPTION, not a UI-only workaround.

| Screen / state coverage | Evidence beyond screenshots |
| --- | --- |
| Recipes empty/populated/search/filter/archived/loading/error/paging; direct/deep return | Actual bounded results, source recovery/archive/unarchive, preserved authorized list position and usable retry |
| Recipe new/empty/clean Ready/dirty Incomplete/Invalid/calculating/unavailable | Actual Save/readiness semantics, stable status, no stale targets, Retry same draft, first normal edit reachable |
| Hidden field/row/multiple same-category diagnostics | Associated descriptions, stable IDs, open ancestors/selected panel and exact focused control; each distinct issue retained |
| Raw `-`, empty, `0`, decimal/exponent/long precision; blur/disclosure/tab/revalidation | Multi-character input/caret/selection/native undo trace, no rounding/default/zero substitution, stale-generation rejection |
| All four ingredient additions, supported search/manual/library, no-results/error/custom/archive | Cancel no row, explicit Add one row, copied provenance, necessary fields, loading/retry and focus; no unsupported catalogue |
| Add/remove first/middle/last/only rows and steps; Undo | Deterministic focus and concise announcement; IDs/order/raw values/viewport restored |
| Mash water methods/conditional fields, boil/whirlpool/fermentation/notes | Full migration control access, no lost conditional values, distinct effective wort vs yeast attenuation and no changed calculation policy |
| Profile select/create/edit/save/dirty switch/archive/unarchive; recipe override/reset | Correct independent target/profile, real dirty/pending guard, reset removes override, current preview changes and old frozen brew does not |
| Save failure/conflict/edit-during-save/local retention failure/recovery/logout | Original scoped draft/request and readable compare retained; earlier saved payload vs newer edits truthful; no cross-account recovery leak |
| Recipe/profile/library/import/evidence leave via link/logo/settings/Back/Forward/logout/switch | Correct task-specific actions, failed Save/Stay no navigation, destination executes once, native unload observed separately |
| Start clean/dirty/new/historical/Incomplete/Invalid/unavailable | Actual reviewed source/equipment/targets precede one final commit; explicit Save choice, no implicit Restore/save, valid selected old revision |
| Start changed head/profile/engine/policy/style/reference/assumptions | Full accepted-context conflict/fresh acceptance; immutable complete bundle unchanged after later edits |
| Unknown save/library/profile/import/start/evidence/transform, Escape/Back/reopen/reload/source change | Network-controlled original identity/payload capture, exactly one result, generation/context fences and truthful cancellation/recovery |
| Import file/paste/source index/required interpretation/preview/commit/cancel/pending/error | Explicit creation only, source bytes/native/diagnostics preserved, associated errors, measured action spacing and unknown outcome copy |
| Scale/Duplicate/Print/Export saved/draft/original/frozen | Explained preconditions, unchanged shared apply semantics, omissions acknowledgement, real preview boundary/focus/return and print cleanup; deferred A4 not counted |
| Saved versions/Brews/Measurements/Compare, archived parent and feedback | Real authorized breadcrumb/deep links, distinct current/saved/frozen sources, history/paging/restore/feedback/evidence/lifecycle preserved |
| Account desktop/mobile 0/1/>1/loading/error/revoked memberships | No external workspace furniture/one-member chooser, >1 choices inside only, text/semantic current state and unchanged policy |
| Account popup keyboard and mobile Navigation → account → guard/destination | Enter/Space/Tab/Shift+Tab/Escape/outside focus traces, one modal owner/top-layer operability, committed closure, Stay and resize safe |
| Workspace slow/double switch/failure/logout/context change from another tab | Original-scope guard and pending lock, no premature current selection; success Recipes, failure/revalidation truthful redaction and recoverable work |
| Settings member/admin/owner/loading/direct URL/safe origin return | Existing operations/proofs/URLs, no speculative management links, labelled navigation/current leaf and validated local return/fallback |
| Navigation direct/Back/Forward/Stay/new route/404/absent/forbidden/expired | Correct title/landmarks/skip/heading arrival versus restored list focus/scroll, no foreign names or auto workspace selection |
| Preview/focus revalidation/network/auth mismatch while print/task open | Original scoped state hidden safely, explicit retry/reconnect, no stale output and retained authorized task return |
| Desktop 1440 and wider; 320/390; short landscape; long title/names/units/errors/many memberships; light/dark | Full bounds, no document overflow, real click target/focus ring/occlusion and dialog/footer reachability, fallback fonts included |
| Genuine browser 200%/400% zoom, enlarged text, virtual keyboard, reduced motion | Actual browser/OS/device evidence, not CSS resize relabelled native zoom; reachable editing/actions and safe reflow |
| Keyboard-only and real screen-reader completion through representative tasks | Tested browser/AT/platform versions, spoken name/role/state/current page/status, live-announcement frequency and modal isolation; limitations explicit |

### Coder and reviewer checklist

- [ ] Read both authoritative target documents and current ticket/project/ledger;
  record base and instruction paths. Current source status rechecked, not assumed
  from old narrative or historical captures.
- [ ] Every applicable ID/control has a destination and real acceptance evidence;
  deferred/capability gaps remain explicit, no invented values/routes/services.
- [ ] Compare actual recipe against approved desktop/mobile samples and relevant
  pinned inheritance; document reason, user impact and Jason's approval for any
  material departure. Old layout approval is not exemption.
- [ ] Shared recipe/showcase components and token mapping verified in actual
  variants; page composition does not restyle internals or duplicate utilities.
- [ ] Preserve UI/MCP shared application, equal permissions, complete context,
  native units/precision, explicit saves, independent task guards, immutable
  history and original-request unknown-outcome recovery.
- [ ] Reported risks have failing reproduction before fixes; screenshots/source
  hypotheses and not-run AT/zoom/state checks are labelled honestly.
- [ ] Three independent written reviews cover correctness/acceptance,
  lifecycle/state, and integration/reuse at the exact candidate; material changes
  get affected-lens re-review. Author self-review is not independent approval.
- [ ] Record exact commit or dirty diff/content fingerprint, commands/results and
  artifact locations without secrets. No claim of overall conformance from
  sampled components, one viewport, a screenshot or old unrelated green tests.

Documentation publication checks are Markdown/link/anchor/ID and authority/
coverage review, not product builds. Implementation verification remains separate;
this document does not certify the target has been delivered.
