# Krausen documentation

Current phase: local design/prototype work. Confirmed product behavior is
distinguished from remaining technical design. The compact visual design and
component-system reference were approved on 2026-10-31. KRA-5 remains the combined
architecture/adoption gate before production implementation.

## Read first

| Document | Purpose and status |
| --- | --- |
| [Agent guidance](../AGENTS.md) | Required reading, checkout/delegation/review rules and current scope |
| [Architecture](ARCHITECTURE.md) | KRA-1 shared UI/MCP contracts and proposed application boundary; adoption pending |
| [Data model](DATA_MODEL.md) | KRA-2 history/snapshot requirements and illustrative records; schema details pending |
| [Calculations](CALCULATIONS.md) | KRA-3 units, equipment and interchange contracts; formula/scaling refinements pending |
| [Design system](../DESIGN.md) | Living component/token specification, confirmed interactions and approved visual direction |
| [Prototype guidance](design/prototype/AGENTS.md) | Scoped ownership rules for the reusable kit, tokens, page composition and real calculation bridge |
| [Prototype README](design/prototype/README.md) | Build/run instructions, review checklist and browser-local limitations |
| [Coding standards](CODING_STANDARDS.md) | Existing Rust/Leptos conventions; architecture-related rules need KRA-5 reconciliation |
| [E2E test plan](E2E_TEST_PLAN.md) | Existing starter test guidance; does not establish new recipe/brew/MCP acceptance |

The executable prototype provides recipe and component-showcase routes using the
same kit. Its assets live under `docs/design/prototype/`; approved reference tokens remain
separate from production `crates/krausen-ui/style/main.css` until adoption.

## Design adoption

The KRA-1–3 documents capture agreed product contracts and proposed technical
design. Each identifies unresolved choices; examples are illustrative rather
than finalized wire or database schemas. KRA-4's compact visual reference is
approved, with production integration still pending.

KRA-5 remains the combined consistency/adoption review. It must reconcile older
guidance, settle foundational open questions and refine downstream tickets before
implementation. Documentation publication does not close those tickets or claim
that the shared application services already exist.

Repository documents are the durable design reference. Tracker records provide
decision history and progress; later accepted changes must update the applicable
documents and agent guidance together.

## Independent reviews

The [2026-10-31 boundary review](reviews/2026-10-31-boundaries.md) consolidates
three independent reviews of all 14 Markdown documents at baseline `d4afe9a`.
It records prioritized adoption findings and later refinements; it does not
itself resolve them or reopen completed design-delivery tickets.

## Historical context

[KRAUSEN.md](../KRAUSEN.md) records earlier product/calculation reasoning. Its
broader learning/sensor vision and some equipment-adoption rules predate the
current decisions and need KRA-5 reconciliation. It does not authorize scope
expansion. [README.md](../README.md) primarily describes the starter foundation.
Existing plans under `docs/plans/` are historical context, not a replacement for
the current design and adopted architecture.
