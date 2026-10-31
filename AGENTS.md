# Krausen agent guidance

Status: guidance for the current design work and approved visual design system.
KRA-5 architecture adoption remains pending. Explicit current user instructions
control when they conflict with older documents or this guidance.

## Required reading

Before working in this repository, read:

1. [Documentation index](docs/README.md) for document status and scope.
2. [Design system](DESIGN.md) for confirmed interactions, approved visuals,
   shared component ownership and production/prototype boundaries.
3. [Architecture](docs/ARCHITECTURE.md), [data model](docs/DATA_MODEL.md) and
   [calculations](docs/CALCULATIONS.md) for confirmed contracts, draft proposals
   and unresolved adoption questions.
4. [Coding standards](docs/CODING_STANDARDS.md) for current implementation
   conventions, subject to the pending architecture reconciliation.
5. The applicable scoped `AGENTS.md` and build/test guidance for your change.
   For prototype work, read its [guidance](docs/design/prototype/AGENTS.md) and
   [README](docs/design/prototype/README.md).

Read the current Trakkt ticket/project and any repository ticket ledger before
implementation or delegation. Do not treat an old prompt or checkout as current
guidance. Architecture/data/calculation documents distinguish confirmed requirements
from draft technical proposals; their presence does not complete KRA-5.

## Scope and design status

Current work covers the recipe/component design reference and supporting
architecture, data-model and calculation documentation. Production
feature implementation and final architecture adoption remain gated by KRA-5.
The prototype does not implement shared accounts, database history, permissions
or MCP services. Keep these limitations explicit.

Use `DESIGN.md` for the confirmed explicit-save/draft behavior, equal UI/MCP
permissions, independent revision-checked editing and historical snapshot
requirements. Reuse shared components and tokens; pages must not restyle them.
The showcase and recipe use the same kit.

[KRAUSEN.md](KRAUSEN.md) contains useful historical reasoning, but its broader
vision and some profile-adoption rules need reconciliation in KRA-5. It is not
authority to expand the agreed recipe/brew-history scope or override current
user decisions. `README.md` also predates the current product/design direction.

## Checkout and coordination

- Before a new implementation task, fetch the repository's default branch and
  create an isolated task worktree from that fetched commit unless the user
  explicitly requests another base. Preserve the invoking checkout and unrelated
  changes.
- After creating it, read its applicable instructions, build/test guidance and
  current ticket ledger before implementation or delegation. Record the base
  commit and instruction paths, and give delegates the same context. Re-read
  guidance after a rebase changes it.
- Delegate useful independent work with clear ownership and expected results.
  Avoid overlapping edits; the coordinating agent integrates, verifies and
  waits for required delegated work before finishing.
- Review findings and verdicts are written. Material changes need fresh
  independent review where applicable. Cryptographic review signing, signing
  keys and signed approval files are not required.

Verify changes using the scoped guidance and checks appropriate to their impact.
Keep draft/accepted status, limitations and unresolved decisions accurate; do
not mark KRA tickets complete merely because a prototype or screenshot exists.
