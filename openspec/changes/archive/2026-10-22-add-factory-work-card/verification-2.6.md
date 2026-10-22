# Task 2.6 — PostgreSQL Issues and reviewed workflow adoption

Scope: persist the task 2.5 Issue contract and add explicit reviewed workflow
adoption through the same application/store authority. Manual drafts remain
independent of providers, repositories and Factory configuration. REST composition,
Console cards, execution association and native mapping publication remain their
subsequent tasks; no completed prerequisite task is reopened.

## Implementation

- Additive migration `0068_issues.sql` retains globally unique Issue IDs,
  transactionally allocated Project-local numbers, current heads, bounded immutable
  revisions and status history. Exact revision references retain the owning Project
  and workflow publication through non-cascading foreign keys. The current pointer
  is checked by a deferred revision foreign key. Reads initialize no state.
- The PostgreSQL adapter implements `IssueStore`, reusing the existing security-
  scoped mutation identity, original replay outcome, actual actor, safe audit and
  outbox transaction. Creation fences Project default selection and number
  allocation; edits/moves/adoption lock their individual Issue heads. Conflict or
  evidence failure leaves no partial revision, history, replay or consumed number.
- Pure `IssueRevision` edit/move/adoption policy and retained-push selection are
  shared with memory. SQL, row locking, allocation and publication remain in their
  adapters. No additional store dependency, executor, journal or outbox is added.
- A changed authorized local field retains its exact `IssuePushIntent` in the
  existing outbox notification. The payload contains Project/Issue/revision,
  immutable mapping and selected fields, with no requirements/reason bodies.
  Replay after mapping replacement preserves the original intent and emits no
  additional record. This slice does not dispatch a Reporter/provider operation.
- `AdoptIssueWorkflowCommand` requires separate Issue `Administer` authority and
  current Issue visibility. It binds the current revision/workflow/status, exact
  target publication, bounded valid status correspondence and reviewed ownership/
  mapping state. The expected policy is compared, never installed by adoption.
  Source-owned status, stale review, foreign/unavailable policy and incompatible
  managed status mapping fail closed. History retains the previous workflow pin.
- Adoption leaves requirements/key/creation time and all Work unchanged, emits
  no native push intent and invalidates old status applicability. Requirements or
  manual status changes also advance the marker; unchanged requirements and
  priority-only edits preserve it. Later accepted-action integration still owns
  the execution-lineage checks; no automatic execution is added here.
- The retained declarative default can be adopted even if only another workflow
  was previously materialized. The exact initial reference is retained inside
  the successful mutation, without read-time initialization or default changes.
- Mutation policy reads reuse the transaction's connection. A one-connection pool
  exercises both movement and adoption without acquiring a second pool slot,
  changing acquire timeouts or locking unrelated Issue heads.
- Memory now rejects a generated Issue ID reused under another Project, matching
  PostgreSQL and preserving its globally keyed retained revisions.

## Behavioral Red/Green evidence

The agreed public seams are authorized Issue application commands, `IssueStore`
with memory/PostgreSQL parity, and durable SQL migration/evidence contracts.
Every implementation slice followed its failing public test. Existing 26
application Issue/workflow tests passed before extracting shared policy.

| Test/filter | Observed Red | Green |
| --- | --- | --- |
| `manual_creation` | PostgreSQL lacked `IssueStore` methods | Manual draft, default/status/key/history, retained reads and original creation replay |
| `edits_and_moves` | PostgreSQL edit returned unavailable | Shared policy, retained text/status revisions, history pagination and original replay after progress |
| application `issue_workflow_adoption_is_reviewed` | Adoption command/intent and applicability marker were absent | Exact reviewed migration, old workflow/history retained, priority edit preserves applicability and original replay |
| PostgreSQL `reviewed_adoption` | Adoption returned unavailable | Memory parity and original receipt/history after adapter restart |
| `retained_source_ownership` | Source-owned description replacement was accepted | Edit and move conflict without partial history; rejected intent can later be applied under valid authority |
| `managed_push_intent` | Existing outbox record had no original typed push intent | Exactly one original immutable mapping/revision/field intent without body or provider dispatch |
| `upgrade_from_workflow_schema` | Adoption of the unmaterialized template returned unavailable | Upgrade from schema 67, repeated migration, valid adoption, unchanged default and restart replay |
| `issue_identity` | Memory accepted one internal Issue ID in two Projects | Both adapters reject duplication and preserve original revisions and foreign-read isolation |
| `moves_and_adoption_complete` | One-connection pool exhausted its own slot; move returned unavailable | Movement/adoption complete using their transaction connection with unchanged pool timeouts |

Additional conformance tests cover concurrent distinct/replayed key allocation,
competing edits and moves, actual actor and operation/security replay partitioning,
invalid/missing status correspondence, stale review, ownership and mapping
conflict, compatible managed mapping, adoption without a native push, and injected
outbox failures for create/edit/move/adoption. The existing mutation mechanism's
transaction is reused; there is no parallel evidence publisher. Fixture corrections
for the outbox fault constraint and generated-ID replay equivalence were not
counted as production Red evidence.

## Commands

Builds reuse the existing target and bounded settings:

```sh
export CARGO_INCREMENTAL=0
export CARGO_PROFILE_DEV_DEBUG=0
export CARGO_PROFILE_TEST_DEBUG=0
export CARGO_BUILD_JOBS=2
```

`OCTACITY_POSTGRES_URL` points to a separately created disposable local PostgreSQL
service. Tests retain their existing explicit opt-in; no credentials from the
Factory stand, external tokens, test serialization or larger timeouts are used.
Each Red filter above was run separately; the final conformance command is:

```sh
cargo test --locked -p octacity-server-store-postgres \
  --test issues --test issue_workflows --test migrations --test factory_discovery -- --ignored
cargo test --locked -p octacity-server-audit -p octacity-server-store \
  -p octacity-server-application --lib
cargo clippy --locked -p octacity-server-audit -p octacity-server-store \
  -p octacity-server-application -p octacity-server-store-postgres --all-targets -- -D warnings
cargo check --locked -p octacity-server-api-rest -p octacity-server
cargo fmt --all -- --check
python3 tools/check_architecture.py
openspec validate add-factory-work-card --strict
```

## Qualification

- PostgreSQL: **50 pass** — 12 Issue, 8 workflow, 6 migration/rollback and 24
  existing Factory discovery/authority/source tests, including manual intake.
- Full library regression: **504 application, 81 store and 3 audit tests pass**.
  This includes the existing long requirements/release journeys and both new
  application adoption/authorization scenarios.
- Clippy: all targets of the four affected packages pass with `-D warnings`.
- Downstream REST and server compilation passes.
- Rust formatting and whitespace checks pass.
- Architecture: 63 packages, 255 edges; no dependency changes.
- OpenSpec strict validation passes.

Free space is checked throughout compilation (approximately 11–13 GiB). Builds
use the existing target with debug information/incremental compilation disabled.
The disposable PostgreSQL container uses bounded temporary storage and is removed
at the end. No Factory-stand data or integration credentials are accessed.

Migration is additive and has no down-migration. Prior binaries still require a
matching pre-upgrade database snapshot; the existing released-schema rollback
rehearsal remains passing. Source/mapping installation, Reporter delivery, accepted
automated producers, REST and Console qualification are not claimed by this slice.

Task 2.6 is complete; implementation progress is **8/56**. Documentation-only
changes have no executable Red/Green cycle and are validated with formatting,
whitespace and OpenSpec checks. No commit or specification archival was performed.
