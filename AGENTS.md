# Krausen agent guidance

Status: implementation guidance for the shared application contracts and approved
visual system. [ADOPTION.md](docs/ADOPTION.md) records KRA-5 review/adoption status.
Explicit current user instructions control when they conflict with older documents.

## Required reading

Before working in this repository, read:

1. [Documentation index](docs/README.md) for document status and scope.
2. [Design system](DESIGN.md) for confirmed interactions, approved visuals,
   shared component ownership and production/prototype boundaries.
3. [Architecture](docs/ARCHITECTURE.md), [data model](docs/DATA_MODEL.md) and
   [calculations](docs/CALCULATIONS.md) for canonical implementation contracts; read [adoption](docs/ADOPTION.md)
   for boundary decisions, delivery order and acceptance evidence.
4. [Coding standards](docs/CODING_STANDARDS.md) for transaction, mapping,
   adapter and component conventions.
5. The applicable scoped `AGENTS.md` and build/test guidance for your change.
   For prototype work, read its [guidance](docs/design/prototype/AGENTS.md) and
   [README](docs/design/prototype/README.md).

Read the current Trakkt ticket/project and any repository ticket ledger before
implementation or delegation. Do not treat an old prompt or checkout as current
guidance. Canonical documents distinguish implementation targets from existing code.
Use the adoption record for review status; do not infer completion from file existence.

## Scope and design status

Production work follows the adoption record and each ticket's prerequisites.
KRA-1–4 delivered the design pack; KRA-5 governs acceptance of its combined
contracts. The prototype does not implement shared accounts, database history,
permissions or MCP services. Do not use it as production acceptance evidence.

Use `DESIGN.md` for the confirmed explicit-save/draft behavior, equal UI/MCP
permissions, independent revision-checked editing and historical snapshot
requirements. Reuse shared components and tokens; pages must not restyle them.
The showcase and recipe use the same kit.

[KRAUSEN.md](KRAUSEN.md) is historical reasoning, clearly superseded by the canonical
documents. Its learning/sensor vision does not expand current scope. The archived
starter plan is not executable guidance; current README describes implementation
status and development setup.

## Checkout and coordination

- Before a new implementation task, fetch the repository's default branch and
  create an isolated task worktree from that fetched commit unless the user
  explicitly requests another base. Preserve the invoking checkout and unrelated
  changes. Keep temporary worktrees out of the parent repositories directory
  and remove them after integration; do not leave duplicate project folders.
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

## Boundary checks for implementation and review

- Both UI and MCP call `krausen-app`; no direct protected CRUD/calculation bypass.
- App owns each write transaction; helpers share `DbTx`, never commit/reacquire.
- Native data belongs to `krausen-types::brewery`; projections are not second truths.
- Scope linked IDs to workspace and parent; shared catalogue is an explicit exception.
- Keep snapshots/evidence immutable and compare complete accepted calculation context.
- Use the adoption scenario matrix and both database backends for affected contracts.
- Match review scope to material changes; reuse existing accepted decisions and evidence.

## Delivery coordination

Do not create any new Trakkt ticket or sub-ticket without Jason's explicit
approval. Refine existing scope, acceptance and dependencies instead. Each
assigned developer owns one ticket through implementation, tests, review fixes
and an open PR. The delivery lead reviews/merges, records completion and assigns
the next ticket; developers do not merge or start another ticket themselves.
Operational updates belong on the dedicated Krausen Agent Bridge board, with
Needs Jason first and a message input last; leave the main board unchanged.
