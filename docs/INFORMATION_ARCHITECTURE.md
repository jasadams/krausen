# Information architecture

Status: **user-approved target contract, 2026-11-02**. This document and
[UX design](UX_DESIGN.md) govern presentation and navigation. They do **not**
claim that the target is implemented, tested, deployed or accessible. Changes to
this contract require independent documentation review before publication. See
[adoption](ADOPTION.md) for the separate architecture adoption and delivery map.

## Authority and use

**MUST**, **MUST NOT** and **REQUIRED** are normative. Stable IDs identify
requirements, not tickets or claims of completion. Every coder and reviewer MUST
read both target documents and link applicable IDs in implementation acceptance.
Current explicit user decisions override older documents. These two documents
own task organization and interaction presentation; [DESIGN.md](../DESIGN.md)
owns the approved visual identity, tokens and shared-component ownership.
[Architecture](ARCHITECTURE.md), [data model](DATA_MODEL.md) and
[calculations](CALCULATIONS.md) continue to own permissions, shared operations,
transactions, native values, formulas, revisions and historical integrity.
Presentation MUST NOT redefine those contracts or infer backend authority from
a visible control. A conflict requires a documented resolution, not a worker's
silent choice.

This is reference and rationale for an accepted target, not a tutorial describing
working screens. [UX verification](UX_DESIGN.md#verification-and-review-gate)
defines the evidence required before implementation acceptance.

## Target task tree

```text
Krausen / home -> Recipes
  Recipes
    New recipe
    Import recipes (BeerXML, existing separate task)
    Recipe (always directly editable)
      Recipe | Saved versions | Brews
      Save / Undo / Start brew / Recipe actions
      Equipment selection / Adjust for this recipe / Manage equipment
      Saved version -> inspect / restore / start selected version / output
      Brews -> Brew -> Measurements / Compare
  Equipment (new UI management destination, existing shared capabilities)

Compact account dropdown
  Account identity
  Settings: Account (Profile, Security); Workspace (Workspace, Team)
  Appearance: Light / Dark / System, retained in Profile
  Workspace choices: INSIDE dropdown ONLY if verified memberships > 1
  Sign out

Internal engineering access, not customer navigation
  Design system showcase
```

There is no approved global Ingredients destination or workspace-wide Brews
index in this delivery. Recipe-local Brews is required and is a different task.

## Navigation requirements

<a id="ia-001"></a>
### IA-001: Recipe-first home and truthful destinations

**Owner:** shared shell/router and library composition. Home/logo MUST resolve to
Recipes. Customer navigation MUST contain Recipes and, when its real management
screen lands, Equipment. Equipment is an approved **new UI route/manager target**,
not a route already implemented at the review baseline. It MUST reuse current
equipment application capabilities and MUST NOT require a fake recipe to open.
It is brewing content accessible under existing member policy, not an owner-only
account setting. Do not expose an inert/placeholder destination during migration.
No analytics, chat, billing, sensors, learning, fake ingredient catalogue or
unsupported global brew-list destinations may be added to fill the sidebar.

**Acceptance:** direct and menu arrivals to Recipes and the delivered Equipment
screen; empty library can manage profiles; member/admin/owner policy unchanged.

<a id="ia-002"></a>
### IA-002: One compact account dropdown

**Owner:** shared shell/account component. Routine account actions MUST use an
anchored nonmodal dropdown, not an account modal. Settings, appearance access and
Sign out MUST remain discoverable through this account entry; duplicate shell
utility entries MUST be consolidated, not copied into each destination. Account
identity MUST remain readable and accessible. Appearance's full Light/Dark/System
preference remains available in Profile; a shortcut MUST NOT replace System with
a competing binary preference model. The chosen popup is ordinary links/buttons
with normal Tab behavior under [ACC-002](UX_DESIGN.md#acc-002).

**Acceptance:** desktop/mobile account open, keyboard dismissal/return, long
identity, all appearance choices and guarded Settings/logout journeys.

<a id="ia-003"></a>
### IA-003: Workspace switching is exceptional account context

**Owner:** shell/context transition. Shell workspace identity and switching choices
MUST appear inside the account dropdown only when a successful, current authorized
membership list contains more than one membership. There MUST NOT be a top/header workspace
control, separate workspace chooser/modal, or single-workspace switching entry.
Remove redundant workspace subtitles/header furniture; account identity remains.
This shell rule does not remove the authorized workspace-name settings field.
Loading, zero memberships, error and revoked access MUST NOT be interpreted as a
verified single membership or speculative choices. Keep account actions available
and provide truthful checking/retry/no-workspace guidance without foreign names.
Mark the current choice in text and semantics, not only colour.

Switch MUST resolve local work in the original scope before changing context;
show pending state and prevent conflicting switch/logout transitions. Only after
success may the new workspace be current. Land at `/recipes` in the destination,
not the old workspace's entity URL. Failure retains the original context and
recoverable work; an uncertain transition requires revalidation, not mislabeled
old content. Browser selection MUST NOT retarget an independent MCP request.

**Acceptance:** 0/1/>1/loading/error/revoked memberships; slow/double switch,
failure, dirty recipe/profile/evidence, foreign deep links and stale completions.

<a id="ia-004"></a>
### IA-004: Mobile navigation has one modal owner

**Owner:** shared mobile shell. Mobile MUST use the same destination names/order
in one accessible Navigation drawer. Its account dropdown is nonmodal within
that drawer's usable native top-layer subtree; a body portal outside an active
native dialog MUST NOT make it inert. Two simultaneously interactive desktop and
mobile copies MUST NOT exist. Navigation closes on **committed** route arrival.
If a dirty-work guard intervenes, transition from transient navigation to that
single guard while retaining the exact intended destination. Stay/failed Save
MUST NOT navigate behind it. Escape closes the popup first, then the drawer on a
later Escape; resize/rotation MUST NOT leave focus trapped on an invisible opener.

**Acceptance:** clean and guarded drawer → account → Settings/switch/logout;
Stay, Escape, committed arrival and breakpoint change focus traces. The reviewed
mobile-closure risk requires failing reproduction before a claimed bug fix.

<a id="ia-005"></a>
### IA-005: Internal showcase policy

**Owner:** shell and shared-kit maintainers. Design system MUST NOT appear in
desktop/mobile customer navigation. The existing `/design-system` direct route
is deliberately retained as an authenticated engineering review surface under
existing route policy. This decision adds no environment gate, role or security
boundary. Hiding its link is **not** access control. The showcase MUST continue
to render the same kit/components/variants as the recipe, not a visual imitation.

**Acceptance:** no customer nav entry; explicit direct-route check and actual
shared-component reuse evidence. Do not delete or fork the showcase.

<a id="ia-006"></a>
### IA-006: Object-local vocabulary and source context

**Owner:** recipe/library/history composition. The one local vocabulary MUST be
**Recipe / Saved versions / Brews**. “Revision N” identifies an exact saved source
inside that vocabulary. History MUST NOT ambiguously mean both saved formulations
and brews. Remove duplicate equal-prominence Brew history/header entrances while
preserving the full-history URL and its capabilities. Alternative library entry
points MUST converge on the same components and operations, not fork histories.
Recipe is always directly editable; historical inspection MUST NOT create a
second view/edit mode or overwrite the current dirty draft. Counts are shown
only when known, not fabricated from a sampled page.

**Acceptance:** library → saved version/brews and editor → same objects; historical
inspection/restore/start remain distinct; restore creates a new head, never moves
an existing brew marker.

<a id="ia-007"></a>
### IA-007: Deep links carry authorized parent wayfinding

**Owner:** recipe, brew history, evidence and comparison pages. Provide real
links, not only browser Back:

- Recipes → recipe.
- Recipes → recipe → Brews.
- Recipes → recipe → Brews → selected brew → Measurements or Compare.

Use authorized returned parent identity; do not guess a recipe ID from a brew
ID or reveal a foreign title while loading/denied. Archived authorized parents
need an honest history/archived label and safe links, not a dead end. A saved
version selection MUST be reproducibly addressable using additive local URL
state without breaking existing paths; document/test its chosen encoding in the
implementation, not invent already-supported query syntax here. Current editable
recipe, selected saved version and frozen brew MUST remain visibly different.
Preserve authorized library query/filter/pagination position and scroll on return.

**Acceptance:** direct bookmarks, refresh and Back/Forward; saved selection,
archived parent, missing/denied/expired context and dirty current draft.

<a id="ia-008"></a>
### IA-008: Settings is scoped route navigation with safe return

**Owner:** settings shell and shared navigation. Keep existing URLs. Group
Account: Profile/Security and Workspace: Workspace/Team with accurate scope copy
and labelled secondary navigation/current leaf. These are links, not ARIA tabs
unless a complete tab interaction is implemented. Show management options only
after verified applicable policy; grouping MUST NOT broaden permissions or hide
ordinary member reads authorized by the shared service. Preserve all existing
profile/security/session/invitation/ownership actions and proof requirements.
Return MUST use a validated same-origin in-app location if available, else
“Back to recipes”; never trust an external return URL or unconditional
`history.back()`. Every exit/logout uses the relevant work/pending guard.

**Acceptance:** member/admin/owner/loading/direct URL; context return from recipe
and brew, external/invalid origin rejection, preserved auth ceremony links.

<a id="ia-009"></a>
### IA-009: Objects own operations, not permanent utility forms

**Owner:** library/editor/task composition. Library owns New recipe and Import
recipes (BeerXML). Recipe owns Scale, Duplicate, Print and Export in a named
Recipe actions surface; Start remains discoverable near Save, not buried there.
Print/export/start MUST NOT remain permanent content sections of formulation.
The existing standalone importer is reused, not duplicated. Equipment selection
is recipe context, reusable profile editing is Equipment management, and
recipe-only overrides remain local formulation inputs.

**Acceptance:** library import and on-demand recipe actions follow
[INT-005](UX_DESIGN.md#int-005), [INT-007](UX_DESIGN.md#int-007) and
[INT-008](UX_DESIGN.md#int-008), preserving all source choices and guards.

<a id="ia-010"></a>
### IA-010: Route compatibility and recovery

**Owner:** router and affected page owners. Preserve these existing route
families, exact incoming proof/callback parameters and associated operations:

| Existing route family at review baseline | Target ownership / compatibility |
| --- | --- |
| `/`, `/recipes` | Recipe-first home, search/filter/pagination/archive/source recovery |
| `/recipe-editor/new`, `/recipe-editor/:id` | Direct editing; additive saved-version/local-section URL state only |
| `/recipes/:id/brews` | Recipe-specific full retained brew history |
| `/brews/:id`, `/brews/:id/analysis` | Measurements/evidence and Compare for the same frozen brew |
| `/beerxml` | Existing library-level import task; Recipes containing section active |
| `/settings`, `/settings/profile`, `/settings/security`, `/settings/workspace`, `/settings/team` | Existing redirects/routes, new grouping and safe return |
| `/accept-ownership/:transfer_id`, `/onboarding` | Existing incoming/setup tasks, not primary clutter |
| `/login`, `/signup`, `/signup/complete`, `/account/recover`, `/account/recover/complete` | Existing account entry/recovery |
| `/auth/google/callback`, `/auth/passkey-signup`, `/auth/recover-passkey`, `/auth/recover-passkey/complete`, `/oauth-complete` | Existing proof/callback/ceremony flows |
| `/design-system` | Deliberately retained direct engineering route; no customer entry |

`/equipment` is the approved additive UI destination, **not present at the review
baseline**. `/brews` list and `/ingredients` are not approved implemented routes.
Unknown/missing/denied pages MUST provide neutral typed outcomes and safe
recovery, never auto-select a workspace from an inaccessible entity link.

**Acceptance:** route inventory/deep-link regression checks; no deleted capability
or broken incoming ceremony due to navigation cleanup.

<a id="ia-011"></a>
### IA-011: Location, arrival and browser history

**Owner:** shell/router and page headings. Provide skip-to-main, named primary,
settings and breadcrumb landmarks, a meaningful page title and arrival heading.
Exact destination uses `aria-current="page"`; containing-section styling MUST NOT
falsely call Recipes the current page on a brew. Use an appropriate ancestor
state plus the real breadcrumb. Committed new navigation focuses the arrival
heading; authorized Back restores prior context where safe. Background refetch
MUST NOT repeatedly move focus. Rejected navigation retains draft, URL, selection,
scroll and logical focus. Repeated Back/Forward/Stay MUST remain usable; a
`pushState` technique or source comment alone is not evidence of history safety.

**Acceptance:** keyboard/AT arrival and repeated guarded history traversal,
including direct links, refetch and missing/denied states.

<a id="ia-012"></a>
### IA-012: Capability gaps are not sidebar features

**Owner:** delivery lead plus shared application owner for any approved expansion.
A global workspace Brews index is **optional/deferred**, not implied by this IA
approval. It needs a separately approved bounded shared query and UI/MCP/backend
parity; fan-out over all recipes, direct database reads and fake results are not
acceptable shortcuts. A global Ingredients manager/new catalogue breadth, richer
multi-recipe import selector, step reordering and recipe-only custom fermentable
creation are proposals, not silently accepted deliverables. Grain bill totals/
shares visible in the sample are a desired capability gap: until authoritative
shared projection scope is approved and delivered, do not compute UI brewing
math, invent values or label an otherwise Ready recipe globally failed.

**Acceptance:** each gap has a truthful deferred/capability disposition in delivery
planning; no unsupported route or manufactured metric is shipped. User permission
to create implementation-gap tickets after documentation merge is not approval
of these deferred feature expansions.

## Reference inheritance and approved departures

Pinned comparative sources are Tane
[`2a923e817582c0f2aaf70d37f6a9848095a2d8c0`](https://github.com/kyomi-ai/tane/commit/2a923e817582c0f2aaf70d37f6a9848095a2d8c0)
and Kyomi
[`bd4c667376cef20914ea5068e9bee7386ed65ae8`](https://github.com/kyomi-ai/kyomi/commit/bd4c667376cef20914ea5068e9bee7386ed65ae8).
These are source-pattern references, not requirements to access another product's
private deployment or a claim of rendered accessibility. Relevant original paths
are `crates/tane-ui/src/components/layout.rs` and
`crates/kyomi-ui/src/components/layout.rs`. Do not modify those products.

| Pattern | Approved decision and user impact |
| --- | --- |
| Anchored account utility; choices only for multiple memberships | Restore interaction class and threshold; fewer interruptions and no pointless single-choice switcher. Use tested shared positioning, not copied accessibility omissions. |
| Starter/settings or analytics navigation | Depart: Recipes home plus Equipment and object-local brews. Brewing tasks replace analytics nouns; no commercial/personal-mode bypass or agent-specific restrictions. |
| Profile/security/workspace/team separation | Retain underlying scope and permissions; simplify duplicate navigation and misleading all-workspace framing. |
| Mobile modal navigation | Retain one accessible drawer; a justified mobile mechanism, not permission for modal account utilities or nested traps. |
| Recipe drafts, guarded exits and immutable history | Brewing-specific addition retained even where references lack it; protects unsaved work and historical evidence. |
| Shared live equipment vs frozen brew equipment | Brewing-specific distinction retained; manager independent of recipe, selected/effective context visible before start. |
| Kyomi recents/collapse/personalized home/tier badges | Not imported: no approved task need or backend scope; reduced chrome and unchanged authenticated one-member workspace policy. |
| Original sample workspace box, public Design system and global Brews link | Superseded by current explicit direction: conditional in-account workspaces, internal showcase, deferred global list. Equipment sample entry is now an approved new UI target, not existing functionality. |

Any material departure from this target, the approved visual samples or relevant
Tane/Kyomi patterns MUST record the affected requirement, reason, user impact,
alternative and Jason's approval before implementation. Existing source,
historical screenshot approval, a paused worker candidate or previous layout
acceptance is not permission to diverge. Do not reopen settled backend contracts
to justify a presentation shortcut.

## Current-state boundary

The six 2026-11-02 reviews inspected accepted source
[`f66a62d90515b9cc8d81547a9576d62c40cf09be`](https://github.com/jasadams/krausen/commit/f66a62d90515b9cc8d81547a9576d62c40cf09be)
and saved captures, not a full fresh runtime matrix. That baseline has a separate
`/beerxml` importer, recipe-local histories, an embedded profile manager and no
Equipment route/global brew index. Review observations and source-inferred risks
are curated in [the disposition appendix](UX_DESIGN.md#review-findings-disposition).
Re-inventory actual current source/tickets before implementation; old narrative
status text is not a release manifest. No ticket is completed or reopened here.
