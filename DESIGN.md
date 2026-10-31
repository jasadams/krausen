# Krausen design system

Review the [recipe mockup](docs/design/previews/recipe-desktop.png),
[component showcase](docs/design/previews/design-system.png) and
[mobile recipe](docs/design/previews/recipe-mobile.png). The
[prototype guide](docs/design/prototype/README.md) explains running the shared
components and generating a single-file offline preview. Screenshots and the
generated file are review artifacts; edit the shared source, then regenerate.

Status: **visual design and component-system reference approved on
2026-10-31**, following the compact-density review. This is the agreed visual
target for implementation. Production Leptos migration, database/MCP integration
and combined architecture adoption under KRA-5 remain separate work. The working
prototype does not establish production backend or historical guarantees.

Krausen is a brewing recipe workspace: formulate a recipe, understand its
calculated targets, save its history, and record what happened when it was
brewed. UI and MCP are interfaces to the same application and user permissions.
The interface makes brewing information readable and editing deliberate.

## Read this before changing the interface

Read this document, `docs/CODING_STANDARDS.md`, and the applicable architecture,
data-model and calculation documents before changing UI components. Follow
explicit current user decisions when older repository guidance conflicts.
`KRAUSEN.md` contains useful calculation/history principles, but its broader
product scope and some profile-adoption guidance predate the current decisions.
Architecture adoption and reconciliation belong to KRA-5.

| Source | Authority in this design work |
| --- | --- |
| Confirmed interactions below | Product behavior confirmed during design review |
| `DESIGN.md` | Approved visual/component reference; production integration pending |
| `docs/design/prototype/tokens.css` | Single source of approved prototype token values |
| `docs/design/prototype/components.css` | Shared prototype component appearance, variants and state styling |
| `docs/design/prototype/layout.css` | Page composition and responsive placement only; no component appearance overrides |
| `docs/design/prototype/components.js` | Prototype component implementations and interaction semantics |
| Prototype `/#system` | Executable component showcase using the same kit as the recipe |
| Prototype `/#recipe` | Recipe composition and interaction demonstration |
| `crates/krausen-ui/style/main.css` | Current production tokens; unchanged by the prototype |
| `crates/krausen-ui/src/components/` | Existing production Leptos components; future adoption target |

The design system is code and guidance together. A screenshot is an example,
not a second source of component values. The showcase must render the actual
components; never maintain a visually similar but separate demonstration.

## Confirmed product interactions

- A recipe is directly editable. There are no separate view/edit modes and no
  Edit button required to enter formulation.
- Save is explicit. Automatically retained drafts protect work; they do not
  publish a saved recipe revision or move a historical brew marker.
- Leaving a changed recipe inside the app offers **Save**, **Discard** and
  **Stay**. A failed save keeps the draft open. Reload/close uses the browser's
  native unsaved-changes warning when applicable; its wording and presentation
  are controlled by the browser.
- Independent drafts and revision-checked saves handle concurrent edits. No
  live collaborative editing, CRDTs or remote keystrokes are required. Refetch
  current data and show conflicts without replacing a dirty draft.
- Metric inputs use litres, kilograms/grams and degrees Celsius with explicit
  labels. Missing actual measurements remain absent.
- Selected equipment is a live reference for recipe previews and new brews.
  Equipment edits change future calculations without creating a recipe-content
  revision solely for that change. A brew freezes its effective equipment,
  original inputs and targets. Changed calculation context must be checked
  before starting it.
- Saved recipe history is linear. Restore creates a new current revision.
  Brews pin a selected saved revision; their markers never move.
- Brew actuals appear beside targets, with notes. Recording and correcting an
  actual are simple actions; authorship, timestamps and retained correction
  history happen underneath. Actual ingredient/process deviations belong to
  the brew. Applying them back is explicit and creates a recipe revision.
- UI and MCP have identical application permissions and business rules.
  Components must not invent an agent-specific permission or feature gate.

The prototype demonstrates a subset of these interactions. Its fixture history,
drafts and saved state are browser-local demonstrations, not the production
revision, account, permission or immutable-snapshot implementation.

## Visual direction

### Searchable ingredients and a personal workspace library

Adding a fermentable opens a searchable catalogue rather than a fixed select.
Search matches name, producer and type. Clearly distinguish starter entries
from the workspace's own library, with an explicit **Create fermentable** action
when an ingredient is missing. A personal workspace is the user's own library;
the production workspace service must apply the same rules through UI and MCP.

The creation form uses shared fields for name, producer, type, colour in EBC
and potential yield. Required numerical specifications must not silently default
to zero. **Save to library** persists the specification; **Add ingredient** copies
it into the recipe draft with an explicit quantity. Recipe Save remains a
separate action. Keep selected identity and quantity visible, and use the same
`CatalogueItem` for results and component-showcase examples.

A recipe keeps resolved ingredient specifications plus catalogue identity/version
provenance. Later library changes never silently rewrite saved formulations or
brew history. Changing equipment settings remains the separately agreed live
profile behavior.

The prototype demonstrates this using a small illustrative starter list and
browser-local custom entries. It currently offers malt/grain types supported by
the existing calculation model. Production catalogue sourcing, database services
and broader fermentable semantics are implementation/design work under the
existing ingredient and calculation tickets.

### Style references while formulating

Calculated OG, FG, ABV, bitterness and colour show a shared `StyleRange` below
the current value when the selected style has reference statistics. The band
marks its minimum/maximum; a marker shows where the current estimate falls.
Show a textual **Below style**, **Within style** or **Above style** state and
an accessible equivalent; do not rely on colour. These are guidance, never
validation errors, save restrictions or a judgement of beer quality.

Use the exact same component in desktop metrics, compact mobile metrics and
the design-system showcase. Changing ingredients, quantities, equipment or
style updates the comparison immediately. Missing/invalid calculations show
**Awaiting estimate** with no stale marker. A recipe without a style has no
invented limits. Volume and other non-style quantities do not acquire ranges.

The prototype catalogue in `docs/design/prototype/styles.js` contains its three
selectable styles from the official BJCP 2021 guidelines, with source links and
edition/category provenance. It is not a complete style catalogue. BJCP colour
limits are published in SRM; the prototype converts them to EBC with the existing
engine convention of 1.97 EBC per SRM. Keep calculations and comparisons at full
precision, format only for display, and expose numeric ranges/status in the
future shared UI/MCP contract. Future historical records must retain the style
identity/edition they were formulated against.

This interaction is a confirmed user request; reference-data and numeric
semantics remain subject to the overall KRA-3/KRA-5 review.

Use a warm ivory canvas, clear paper-like surfaces, charcoal text/brand details,
warm stone navigation, copper for the primary action, and restrained sage for
supportive context. Dark mode uses deep green/charcoal surfaces. This palette is
the approved visual reference; production token migration remains pending.

The character comes from typography, proportion and useful brewing information.
Use Instrument Serif for expressive recipe/page headings, DM Sans for controls
and reading, and Geist Mono for quantities and aligned numerical comparisons.
Provide local/system fallbacks; a font-loading failure must not hide content.
The root is 15px. Component sizing must account for that rather than assuming
Tailwind's default 16px root.

Keep recipe content dominant: recipe name, style, batch volume, ingredients and
headline calculated figures. Import source, internal IDs and engine details
belong in provenance/assumption disclosures. Delete is a secondary or overflow
action, not the visual centre of a recipe.

Use restrained borders and elevation to group content. Avoid repeating a card
around every label/value. A coherent ingredients panel should read as one
composition. Copper is an action colour, not a universal decoration; green is
not the only indication of success. Colour samples are accompanied by named
units and values.

## Tokens

### Compact default density

The user requested a more compact overall presentation while retaining the
approved visual character. Density is a system-level default: tighten shared
spacing, panel padding, ingredient rows, headers and decorative proportions in
the tokens/components, rather than applying a recipe-only compact stylesheet
or adding another visual mode. The recipe, catalogue and showcase must all
reflect the same defaults.

Keep the 15px root and readable body text. Preserve comfortable mobile controls,
visible focus rings, clear units and style-range labels. Reduce whitespace
before reducing legibility; maintain hierarchy between title, ingredients,
supporting metadata and calculations. Exact dimensions remain in `tokens.css`
and shared component definitions rather than a duplicate value table here.

Approved visual token values live in prototype `tokens.css`. Component CSS consumes
tokens; pages do not duplicate hex values, numeric type scales or shadow/radius
recipes. The following are token **roles**; use the exact property names defined
in that file rather than creating a second token vocabulary in pages.

| Group | Roles to represent |
| --- | --- |
| Surfaces | Canvas, panel, inset, raised/overlay and inverse navigation |
| Text | Primary, secondary, muted, inverse and disabled |
| Actions | Primary/rest/hover/pressed, secondary and destructive |
| State | Focus ring; selected; validation/error; warning; success; pending |
| Borders | Subtle grouping, strong control boundary, active and invalid |
| Typography | Display, section title, body, control, label, metadata, numeric |
| Spacing | Shared density scale, panel padding, row gap, field gap, page gutters |
| Shape | Control, panel and overlay radius; border widths and elevation |
| Layout | Navigation width, readable page width, calculation rail and breakpoints |
| Motion | Short feedback, standard transition and reduced-motion behavior |

Typography roles include font family, size, weight, line height and, where
needed, letter spacing. A page must not recreate a heading by setting those
properties independently. Numbers use tabular alignment when compared in rows.
Units remain close to values but visually subordinate.

Dark mode changes semantic token assignments rather than inverting a screenshot.
Every normal/hover/selected/disabled/error state must remain legible on its dark
surface. Do not hardcode light surfaces in component variants.

## Component ownership

Every styled visual or interactive pattern has one implementation in the shared
kit. Recipe and showcase pages compose it through properties, content and event
handlers. Control markup belongs in `components.js`; its appearance belongs in
`components.css`. `tokens.css` owns values. `app.js` owns fixture data, routing
and composition, not a second implementation of buttons or ingredient rows.

This rule includes repeated domain patterns, not only buttons. Use shared
layout/section/row primitives instead of making each screen invent the same
spacing and grouping. `layout.css` may arrange page regions and responsive
placement; it must not recolour, resize or otherwise override component
appearance. Plain structural composition markup is acceptable, but controls,
fields, panels and repeated domain patterns remain kit responsibilities.

Allowed caller configuration:

- Documented semantic variants and sizes.
- Content, labels, values, IDs, accessible descriptions and event callbacks.
- Composition and slot content using other shared components.
- Documented layout placement/visibility options.

Not allowed in pages:

- Recolouring or resizing a component with ad hoc classes/inline styles.
- Per-page descendants such as `.recipe-page button` that override the kit.
- A copied component with a small visual variation under a new name.
- A showcase-only mock that looks like the real component.

When a valid need is missing, add a named variant or compose a new shared
component, add its states to the showcase, and reuse it from the page. A
page-specific exception is not the default implementation strategy.

### Component families and production mapping

Names in this table describe responsibilities; `components.js` defines the
prototype's actual exports. Existing Leptos components are reuse points, not a
claim that the proposed behavior is already implemented in production.

| Family | Responsibility | Production mapping |
| --- | --- | --- |
| Shell/navigation | Brand, workspace context, navigation, mobile menu and page frame | Extend `Layout` and shared navigation components |
| Typography/layout | Page title, section heading, text roles, stack/grid and panel composition | Shared Leptos presentation/layout components |
| Button/icon action | Primary, secondary, quiet, destructive; sizing, focus and pending states | Existing `Button`, `ButtonLink`, `ToggleButton`; extend variants deliberately |
| Field/number/textarea | Persistent label, input, optional unit, hint, validation and disabled/read-only state | Build typed components above existing `Label`/`INPUT_CLASS`; the constant alone is not the final field API |
| Select/search | Labelled selection and ingredient search with empty/loading states | Existing `StyledSelect`, `DynSelect`, `SearchInput`; finish domain composition |
| Panel/section | Heading, description, action slot and consistent content spacing | Existing `Card` family plus shared section primitives |
| Badge/status | Short descriptive state, selected context and save feedback | Existing `Badge`, `StatusBadge`, `ActionStatus` |
| Metric/summary | Label, value, unit, supporting context and calculation availability | New reusable calculation/metric components |
| Ingredient row/group | Editable ingredient identity/specification, quantity, units and row actions | New domain components using shared field/action primitives |
| History/actual row | Saved revision/brew provenance and planned/actual comparison | New shared domain components; preserve immutable-history semantics |
| Alert/empty/loading | Recoverable error, incomplete data, empty state, skeleton and pending feedback | Existing `Alert`, `EmptyState`, `Skeleton`, `Spinner`, toast support |
| Dialog/popover | Title, description, action arrangement, keyboard/focus and dismissal | Existing `Modal`, `ConfirmDialog`, `Popover`, `Tooltip`; verify semantics and focus before adoption |

Do not move prototype JavaScript into the production UI as its final component
framework. Port the reviewed component contracts into Leptos, keeping the shared
token and variant ownership model.

## Interaction and accessibility contracts

Every component owns its states rather than relying on page-specific fixes.

| State or concern | Required behavior |
| --- | --- |
| Keyboard/focus | Visible `:focus-visible` treatment; logical tab order; no hover-only essential action |
| Labels | Persistent visible field labels; icon actions have meaningful accessible names |
| Validation | Associate errors/hints with fields; use `aria-invalid` where relevant; explain what needs fixing |
| Disabled | Native disabled semantics where appropriate; distinguish from read-only; explain unavailable actions when needed |
| Pending | Prevent duplicate submission; retain the action's name; announce meaningful completion/failure |
| Selection | Show selected state through shape/text/semantics as well as colour |
| Dialog | Accessible title/description, initial focus, contained tab navigation, Escape behavior and focus return |
| Mobile navigation | Treat an overlay menu as an accessible modal interaction; restore focus on close |
| Touch | Adequate action hit area, including row actions; no precision-only hover affordances |
| Numerical input | Label units, permit transient draft entry, and distinguish empty/invalid from a real zero |
| Motion | Use brief feedback transitions; respect `prefers-reduced-motion` |
| Status | Save/error/pending feedback is text as well as colour; announce changes without repeatedly interrupting typing |

Prototype demonstrations are not an accessibility certification. Verify actual
keyboard, screen-reader semantics, contrast and responsive behavior before
adoption; document any limitations rather than treating a visual state as proof.

## Recipe composition

The recipe page is the editor. The normal composition is recipe identity and
context, directly editable ingredient/process sections, and a persistent
calculation summary. Save state is easy to find without obscuring the recipe.
Equipment context is visible alongside the figures it affects. History and
brew actions expose deliberate operations rather than introducing a second
read-only recipe mode.

On a wide screen, give ingredients most of the reading space and keep the
calculation rail nearby. On narrow screens, use a coherent single-column order
and concise summary; avoid a fixed panel that covers fields or steals most of
the viewport. Page and overlay layouts must support long ingredient names,
many additions and larger text without document-width overflow.

Raw XML is part of import/export, not the recipe's primary content. Calculated
figures must come from the deterministic Rust engine. Fixture targets/history
must be visibly distinguishable from current calculated outputs. When inputs
are incomplete or calculation fails, explain that state; never keep stale
figures labelled as current.

Saving a recipe and retaining a draft are separate states. The intended
production language differentiates saved revision, unsaved changes, retained
draft, saving, save failed and conflict. A draft-retained message must not imply
that collaborators or agents can see a new saved revision.

## Living-system workflow

For each component addition or material change:

1. Define the component's responsibility, props, variants and keyboard behavior.
2. Reuse existing tokens; add a semantic token only when the role is missing.
3. Implement it once in the shared kit/component module.
4. Add realistic examples and supported states to `/#system`.
5. Use that exact implementation in the recipe composition.
6. Check light/dark, keyboard, reduced motion and narrow/wide layouts.
7. Update this document when the contract or system direction changes.

The showcase should make component reuse reviewable. It should include controls,
typography, surfaces, calculated figures, ingredient patterns and feedback; state
examples must use the same variants and semantics as product composition.

Before production integration, reconcile this approved visual reference with the agreed
architecture/data/calculation documents, record remaining limitations, and port
approved contracts into Leptos. Production token changes belong in
`crates/krausen-ui/style/main.css`, not a competing page stylesheet.

## Prototype and its limits

See [`docs/design/prototype/README.md`](docs/design/prototype/README.md) for build
and launch instructions. The prototype is a local design review surface with
fixture recipes, browser-local state and the real Rust calculation core compiled
to WebAssembly. Local explicit save, draft recovery, unsaved navigation guards,
recipe history and simple brew snapshot records with optional actuals demonstrate
interactions. Measurement correction history is not implemented. These local
records do not implement the production history/snapshot guarantees.

It is not a deployed product or a substitute for shared application/MCP
operations, identity, database migrations or durable historical persistence.
Figures use the existing engine's semantics; displaying its real outputs does
not adopt proposed calculation changes or resolve known model issues.

The approved visual reference and pending production architecture remain
distinct. Visual approval does not complete unimplemented application services
or resolve the remaining technical design. Do not mark broader planning tickets
complete from prototype existence or screenshots alone.
