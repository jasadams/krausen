# Krausen recipe and component prototype

Status: **approved visual reference and local design prototype**, part of KRA-4.
The compact design/component system was accepted on 2026-10-31. Production
application services, Leptos migration and KRA-5 architecture adoption remain
separate deliverables.

Read [`../../../DESIGN.md`](../../../DESIGN.md) for the component ownership rules,
confirmed product interactions, approved visual direction and production Leptos
mapping. Read `docs/CODING_STANDARDS.md` from the repository root before changing
the design system.

## Build and run

Requires Rust/Cargo with the `wasm32-unknown-unknown` target, `wasm-bindgen` CLI
matching the bridge's pinned dependency (currently 0.2.114), and Python 3. The
bridge lives in its own small Cargo workspace and uses the repository's local
`krausen-calc` crate. Font files are included locally with their licence files.

From this directory:

```sh
./build.sh
python3 -m http.server 18040 --bind 127.0.0.1
```

Open:

- [Recipe prototype](http://127.0.0.1:18040/#recipe)
- [Component showcase](http://127.0.0.1:18040/#system)

Serve the directory over HTTP. Opening `index.html` as a `file:` URL does not
provide the module/WebAssembly loading environment used by the prototype.
`build.sh` builds the local calculation bridge; inspect its output for toolchain
requirements and failures. Do not treat an unavailable bridge as permission to
replace calculations with invented fixture figures.

Stop the foreground Python server with Ctrl+C. The server is deliberately bound
to loopback for local review. This does not publish the prototype.

For review without a server, package the built preview into one HTML file:

```sh
python3 package.py /tmp/krausen-design-preview.html
```

That generated file embeds the same components, styles, fonts and Rust engine.
Open it directly in a modern browser. It is a generated review artifact, not a
second implementation; regenerate it after source changes. Some browser privacy
settings restrict local-file storage, in which case use the HTTP preview.

## What is real and what is demonstrated

- Current calculated outputs use the repository's Rust `krausen-calc` engine via
  a WebAssembly bridge. This demonstrates a shared deterministic calculation
  implementation; it does not certify all formula/input semantics.
- Recipe examples begin as fixtures. Explicit save, automatic draft retention
  and recovery, unsaved navigation guards, recipe history and simple brew
  snapshot records with optional actuals operate locally in the browser. They
  are not production recipe revisions or shared records.
- The fermentable picker searches starter examples and a browser-local custom
  library. Create a malt/grain with name, producer, category, EBC and yield, save
  it to the library, then select its amount for the recipe. Custom entries
  survive reload in this browser; another device does not share this storage.
  The production workspace database and catalogue service are not implemented.
  Starter examples are illustrative, not a complete or independently verified
  manufacturer database. Recipes copy resolved specifications and source/version
  provenance instead of following mutable library data.
- There is no account authentication, database, hosted workspace, permission
  enforcement, shared application API or MCP tool execution in this prototype.
- Actual-measurement correction history is not implemented. Local history and
  snapshot demonstrations are not the production immutable-snapshot service;
  durable persistence must follow the adopted architecture/data contracts.
- Calculations use the existing engine's semantics. Proposed calculation
  changes have not been adopted simply because a real engine is running here.
- The approved reference visual system is separate from current production tokens and
  Leptos components. No production theme migration is implied.

Use a separate browser profile if you want an independent local review state.
Browser storage can be cleared through developer tools; it is not a backup.
Do not enter secrets or rely on prototype data as a brewing record.

The initial sample is the repository's Munich Helles reference formulation.
Its preview equipment uses the reference fixture's near-instant chilling
assumption to show boil-only bitterness; it is illustrative, not a validated
manufacturer equipment preset. Mash/fermentation temperatures are saved recipe
metadata here, not inputs modelled by the existing gravity/bitterness engine.

## File responsibilities

| File | Owns |
| --- | --- |
| `index.html` | Prototype entry and loading the shared assets |
| `tokens.css` | Approved reference semantic colour, typography, spacing, shape, layout and state values |
| `components.css` | Shared component appearance, variants and responsive/state rules |
| `layout.css` | Page composition and responsive placement only; no component appearance overrides |
| `components.js` | Shared component markup, variants and interaction semantics |
| `app.js` | Fixture state, recipe/showcase composition, routing and application-level interactions |
| `catalogue.js` | Starter ingredient examples, catalogue search and specification copying/input conversion |
| `build.sh` and calculation bridge sources | Compiling the real Rust engine for browser use |

Both routes must use the same kit. Add a reusable component or variant when a
new pattern is needed; do not copy its markup into `app.js`, introduce a recipe
override stylesheet, or hand-style the showcase separately. Concrete exports
and token names live in the code, not a competing list in this README.

## Review checklist

### Style reference data

`styles.js` contains the prototype's three selectable styles from the official
BJCP 2021 Beer Style Guidelines: [Munich Helles 4A](https://www.bjcp.org/style/2021/4/4A/),
[German Pils 5D](https://www.bjcp.org/style/2021/5/5D/) and
[Märzen 6A](https://www.bjcp.org/style/2021/6/6A/marzen/).
The numeric facts were checked on 2026-10-31; guideline descriptions are not
copied. Colour bounds retain source SRM values and convert to EBC using the
existing engine's 1.97 factor. The catalogue is deliberately limited to the
styles this prototype offers.

Live comparisons use unrounded estimates and inclusive reference boundaries.
Out-of-style values are guidance and can be saved. Missing calculations have
no marker; recipes without a selected style have no range. The same
`StyleRange` component appears in the recipe, mobile metrics and showcase.

### Interaction checks

- Recipe information is readable and directly editable without entering a
  separate edit mode.
- Ingredient/process changes affect calculated outputs; errors/incomplete
  inputs do not leave stale figures presented as current.
- Save remains explicit; automatic draft retention is distinct from saved
  recipe history. Internal dirty navigation offers Save/Discard/Stay. Browser
  reload/close warnings use the browser's native behavior where applicable.
- The same controls, fields, panels, metrics and statuses appear on the recipe
  and in the component showcase.
- Check keyboard focus, labels, dialogs, disabled/pending/error states, dark
  mode, reduced motion and narrow/wide layouts. A rendered example alone is
  not proof of accessibility.
- Equipment context and metric units are visible. Historical example targets
  are not confused with a current calculation using current equipment.

Record review findings and unresolved design decisions. Screenshots are useful
evidence, but do not make prototype interactions production-ready or close the
architecture gate.

## Browser verification

With the preview server running, `verify.cjs` uses the repository's existing
Playwright dependency from `e2e/node_modules`:

```sh
node verify.cjs
```

If dependencies are available in another checkout, set `PLAYWRIGHT_MODULE` to
that checkout's `e2e/node_modules/@playwright/test` directory. The checks use an
isolated browser context and cover live calculations, incomplete inputs,
explicit save/discard/recovery, frozen brew previews, actuals, profile changes,
component showcase, dialogs, dark mode and responsive widths.

Review screenshots are in `../previews` from this directory
(`docs/design/previews` from the repository root).
