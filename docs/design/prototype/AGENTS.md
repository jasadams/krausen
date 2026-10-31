# Editing the design prototype

Read root `DESIGN.md` and this directory's `README.md` before changing the prototype.

- `tokens.css` owns approved reference colour, typography, spacing, radius and state tokens.
- `components.js` owns reusable control/component markup and variants.
- `components.css` owns their appearance and responsive states.
- `layout.css` only composes page layout. Do not restyle components here.
- `app.js` composes the kit and handles browser-local preview interactions. Do not
  introduce raw styled buttons, inputs, selects or dialogs into screen markup;
  extend the shared kit and showcase new variants instead.
- All derived brewing figures come from the existing `krausen-calc` crate via
  the small WASM bridge. Do not write another calculator in JavaScript.
- This is a local design review artifact, not a production UI or backend. Keep
  fixture/browser storage limitations explicit. Production implementation and
  architecture adoption remain gated by KRA-5.

Check desktop/mobile, light/dark, keyboard focus, explicit Save and dirty-draft
navigation behavior when changing component behavior or layout.
