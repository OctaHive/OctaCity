# Task 2.9 — Optional planning application and memory contracts

## Scope

Provider-neutral Project planning definitions, optional task planning and
same-Project relationships are specified and implemented through authorized
application commands and the existing in-memory transactional authority.
Ordinary manual creation remains independent of providers, Factory, repositories
and a person directory. PostgreSQL planning persistence, HTTP/generated contracts
and Console properties remain tasks 2.10–2.12; their checkboxes are unchanged.

The implementation adds no dependency, execution owner, provider SDK, evaluator,
permission system, scheduler or status roll-up. Existing content/status commands
retain their original roles. Shared pure catalog/value/planning policy can be
reused by durable persistence in the next slice. The current PostgreSQL adapter
refuses unsupported nonempty planning rather than accepting discarded metadata;
ordinary empty creation and old retained JSON/replay shapes remain compatible.

## Public seams and behavior

- `PublishIssuePropertiesCommand` publishes immutable catalogs under independent
  Project administration, strong current catalog version and scoped replay.
  Current/exact reads require visible Project scope; version zero is an empty
  declarative template, with no first-creator administration or read-time write.
- Catalogs bound counts, UTF-8 bytes and supported values. Type/component/size are
  ordinary configurable fields. Text, one finite choice, boolean, bounded integer
  and calendar date are supported; formulas, nested executable objects, duplicate
  identities and reserved requirements/status/execution/authority setters are not.
  Stable keys/types remain retained; retirement replaces identity deletion.
- `CreateIssueCommand` accepts optional planning. `EditIssuePlanningCommand`
  replaces that snapshot under exact Issue/current catalog preconditions, using
  ordinary Issue edit rights. Requirements-only edits preserve it. Source-owned
  planning fields reject changed replacement while unrelated local fields remain
  editable. Old revisions, retired values and original replay receipts survive.
- An optional server-authorized `IssueAssigneeDirectory` supplies bounded visible
  organizational people. Without it, availability is explicit and Unassigned
  creation remains usable. New selection requires a current eligible identity;
  unchanged responsibility remains retainable during directory loss. A service
  principal is never fabricated as a person and assignment grants no rights.
- Parent visibility is checked before lookup/replay, without inheriting edit or
  execution authority. Same-Project identity, self-reference and current graph
  checks execute atomically with the revision. Competing opposite links have one
  winner. Child discovery filters visible direct children before paging/cursors;
  hidden children cannot create a count or continuation hint.
- Planning changes retain requirements/status applicability and business state,
  append no status event, start no execution and dispatch no native effect. They
  use the existing revision/history/replay/audit/outbox publisher and safe Debug
  boundaries. Definition publication uses the same five fault points and commits
  no partial catalog or evidence on failure. No frozen Work type is modified.

## Red/Green evidence

Focused commands use the public authorized application seam:

```sh
export CARGO_INCREMENTAL=0
export CARGO_PROFILE_DEV_DEBUG=0
export CARGO_PROFILE_TEST_DEBUG=0
export CARGO_BUILD_JOBS=2

cargo test --locked -p octacity-server-application --lib issue_planning_manual_creation
cargo test --locked -p octacity-server-application --lib issue_planning_definitions
cargo test --locked -p octacity-server-application --lib issue_planning_create_edit
cargo test --locked -p octacity-server-application --lib issue_planning_assignment
cargo test --locked -p octacity-server-application --lib issue_planning_parent_links
cargo test --locked -p octacity-server-application --lib issue_planning_children
cargo test --locked -p octacity-server-application --lib issue_planning_retirement
cargo test --locked -p octacity-server-application --lib issue_planning_invalid_values
cargo test --locked -p octacity-server-application --lib issue_planning_empty_snapshots
```

Each test preceded its implementation. Required contract/API compilation errors
are distinguished from environment failures. Two intermediate implementation
compiler errors (missing field documentation and a lock-guard dereference) were
corrected and are not counted as behavioral Red evidence.

| Slice | Observed Red | Green |
| --- | --- | --- |
| Ordinary unassigned creation | Planning snapshot/choice query contracts absent | No directory, empty catalog, no read-time mutation and ordinary draft remain usable |
| Project definitions | Publication/retained-read contracts absent | Exact immutable versions, original replay, stale conflict and bounded closed definitions |
| Planned create/edit | Optional creation/planning-edit contracts absent | Local labels/type/component/size/date/scalars, invalid-value rejection and original revision/replay |
| Responsibility | Authorized directory seam absent | Visible selectable person, hidden/service reference rejection, directory loss and separate edit rights |
| Parent link guards | Self-parent was accepted | Self/foreign/missing/cycle and hidden replay/create rejection, no parent status roll-up |
| Children | Bounded child query absent | Visibility before limit/cursor and one valid concurrent opposite-link mutation |
| Failure recovery | Injected catalog DomainState failure still committed | Both publication and planning edit roll back all five fault points, with exact retry/replay |
| Mutation diagnostics | Invalid task dates were classified as catalog publication | Creation and planning edits identify their actual store operation |
| Legacy serialization/privacy | An ordinary revision gained a new empty planning member | Empty snapshots preserve the old JSON shape and nonempty values stay out of generic Debug |

An additional characterization checks independent administration, hidden Project
choices, identity/type retention, duplicate/oversized catalogs, unavailable or
oversized directories, unsupported fractional/nested values and shared child-page
bounds. The existing 28 Issue/workflow scenarios passed before implementation;
the focused selection initially reached 36 passing tests, including eight planning tests.
Two final boundary corrections increased the passing selection to 38, including
ten planning tests.

PostgreSQL compatibility received its own observed Red/Green:

```sh
# OCTACITY_POSTGRES_URL identifies a separately created disposable service.
cargo test --locked -p octacity-server-store-postgres --test issues \
  unsupported_planning -- --ignored
```

Without the guard, PostgreSQL acknowledged a nonempty planning request but
returned empty metadata. With the guard it refuses the whole request, and the
same key can subsequently create an ordinary task as ISS-1. The first failed
request consumes neither a number nor a replay receipt. Durable planning behavior
is not claimed by this compatibility check and belongs to task 2.10.

## Qualification

The full library regression passed 512 application tests and all 81 store tests
before the final operation-diagnostic and empty-JSON corrections. After those
narrow corrections, all 38 current Issue/workflow tests (including ten planning
tests), all 81 store tests, all 97 HTTP tests and all 30 PostgreSQL tests passed.
The unrelated long Factory journeys were not repeated after those corrections;
the focused Issue/serialization and actual persistence/HTTP boundaries were.

| Check | Observed result |
| --- | --- |
| Final Issue/workflow application selection | 38 pass |
| Full store library after final corrections | 81 pass |
| HTTP application/authorization/OpenAPI regressions | 97 pass |
| Disposable PostgreSQL Issue/workflow/migration/rollback regressions | 16 + 8 + 6 pass |
| Clippy, all targets of store/application/REST/PostgreSQL packages | Pass with `-D warnings` |
| Released server compilation | Pass |
| Console generation/lint/unit/build/format | Pass; 501 Vitest cases, 30 Node script tests |
| Architecture | Pass: 63 packages, 256 edges; no dependency changes |
| Rust formatting, OpenSpec strict validation and whitespace | Pass |

```sh
cargo test --locked -p octacity-server-store -p octacity-server-application --lib
cargo test --locked -p octacity-server-application --lib issue_
cargo test --locked -p octacity-server-store --lib
cargo test --locked -p octacity-server-api-rest --test v1_handlers
cargo test --locked -p octacity-server-store-postgres \
  --test issues --test issue_workflows --test migrations -- --ignored
cargo clippy --locked -p octacity-server-store -p octacity-server-application \
  -p octacity-server-api-rest -p octacity-server-store-postgres --all-targets -- -D warnings
cargo check --locked -p octacity-server
cargo fmt --all --check
python3 tools/check_architecture.py
openspec validate add-factory-work-card --strict
git diff --check

# ui/: installed Node 24 and pinned pnpm 12.8.1
pnpm api:generate
pnpm lint
pnpm test:unit
pnpm build
pnpm format
```

Console routes, request bodies and presentation were not extended in this slice;
the existing generated client and unit/build boundary remain qualified. Final
API generation after the model corrections also passed; hashes of every generated
client file remained unchanged. No new
browser journey, live provider, Docker image build or complete Factory-stand
pilot is claimed. Existing unrelated Docker-context fixes are preserved.

Compilation
reuses the existing target with debug and incremental output disabled. Free
space is checked throughout (approximately 6.8–7.1 GiB). Disposable PostgreSQL
uses bounded temporary storage and is removed after qualification; the user's
Factory stand and integration credentials are not accessed. No timeout increase,
test serialization, production rollout or commit is part of this task.

Task 2.9 is complete: **11/56** implementation tasks. Other pending tasks remain
unchanged. Documentation-only edits have no executable Red/Green cycle and use
the stated formatting, whitespace and strict OpenSpec checks.
