# Krausen documentation

The compact visual system was approved on 2026-10-31. Architecture, native data,
calculation and workflow contracts are consolidated for KRA-5; the exact adoption
status and independent changed-contract verdicts are in [ADOPTION.md](ADOPTION.md).
Production implementation and its acceptance tests remain separate work.

## Read first

| Document | Purpose and status |
| --- | --- |
| [Agent guidance](../AGENTS.md) | Required reading, checkout/delegation/review rules and current scope |
| [Adoption record](ADOPTION.md) | Review resolutions, delivery dependencies and required implementation acceptance |
| [Architecture](ARCHITECTURE.md) | Shared UI/MCP operations, transaction/auth/tenant and crate boundaries |
| [Data model](DATA_MODEL.md) | Versioned native records, immutable plans, readings/corrections and lifecycle |
| [Calculations](CALCULATIONS.md) | Versioned formula/input/context, equipment, scaling and preservation contracts |
| [Design system](../DESIGN.md) | Living component/token specification, confirmed interactions and approved visual direction |
| [Prototype guidance](design/prototype/AGENTS.md) | Scoped ownership rules for the reusable kit, tokens, page composition and real calculation bridge |
| [Prototype README](design/prototype/README.md) | Build/run instructions, review checklist and browser-local limitations |
| [Coding standards](CODING_STANDARDS.md) | Reconciled Rust/Leptos, transaction, shared-service and adapter conventions |
| [E2E test plan](E2E_TEST_PLAN.md) | Starter regressions plus adopted boundary acceptance scope; cases are not test results |

The executable prototype provides recipe and component-showcase routes using the
same kit. Its assets live under `docs/design/prototype/`; approved reference tokens remain
separate from production `crates/krausen-ui/style/main.css` until adoption.

## Design adoption

The canonical contracts define the implementation target. Their presence does
not mean corresponding services are implemented. ADOPTION.md records resolutions
of B1–B8, ownership of deferred work, acceptance scenarios and delivery order.
KRA-1–4 design delivery remains complete. KRA-5 adopts the independently approved
combined contracts and maps them to implementation tickets; KRA-6 is the next
delivery slice.

Repository documents are the durable contract. Tracker records link delivery and
evidence; later accepted changes update both documents and applicable guidance.

## Independent reviews

The [2026-10-31 boundary review](reviews/2026-10-31-boundaries.md) consolidates
three independent reviews of all 14 Markdown documents at baseline `d4afe9a`.
It records prioritized adoption findings and later refinements; it does not
itself resolve them or reopen completed design-delivery tickets.

## Historical context

[KRAUSEN.md](../KRAUSEN.md) records earlier product/calculation reasoning. Its
broader learning/sensor vision is explicitly superseded by current contracts.
It does not authorize scope expansion. [README.md](../README.md) distinguishes
existing foundation code from implementation targets.
Existing plans under `docs/plans/` are historical context, not a replacement for
the current design and adopted architecture.
