# Task 2.10 — PostgreSQL planning persistence

## Scope and boundaries

`PostgresStore` now implements the existing `IssuePropertyStore` contract and
persists planning on creation through `IssueStore`. Manual creation remains
independent of a provider, Factory, repository and person directory. The shared
`IssuePlanning::validate` and `IssueRevision::planned` policies from 2.9 retain
responsibility/choice/ownership/version semantics; no parallel validation engine,
provider integration, execution owner or dependency was added.

Migration `0069_issue_properties.sql` retains immutable Project catalogs. Generated
revision columns bind the original JSON to exact catalog versions and same-Project
parents; foreign keys preserve those references. A parent index supports bounded
visible child reads. Existing 0068 documents retain their original JSON shape, without
injected planning fields. The empty catalog is declarative on reads and only
materialized for foreign keys by migration or accepted mutation.

Catalog publication, creation and planning edits take the existing Project fence
before the Issue head, making catalog preconditions and ancestry validation atomic.
Head edits use `FOR NO KEY UPDATE`, retaining competing-edit exclusion while
allowing stable parent references. All mutation reads use the same connection.
The existing mutation transaction atomically commits revision/catalog, scoped
receipt, audit and safe outbox metadata. Planning creates no status event, Work,
Run, push intent or provider call and preserves requirements applicability.

HTTP/generated exposure and Console controls remain tasks 2.11–2.12. Existing
2.9 changes and the separate uncommitted Docker-context correction were preserved.
No commit was created.

## Test-first evidence

The public seam is the existing provider-neutral store port against a disposable
PostgreSQL 18.6 service; memory outcomes serve as parity expectations. Migration
reference constraints are also tested through the published SQL schema boundary.

All cargo commands use these space-conscious settings:

```sh
export CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
export CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2
# OCTACITY_POSTGRES_URL identifies only the isolated disposable service.
cargo test --locked -p octacity-server-store-postgres --test issues FILTER -- --ignored
```

Each following test preceded its corresponding implementation:

| FILTER | Observed Red | Observed Green |
| --- | --- | --- |
| `planning_catalog` | Required property-store methods absent (`E0599`) | Empty/current/exact immutable catalogs, memory parity, retirement, stale conflict and original replay |
| `manual_planning` | Valid manual due date returned `Unavailable` | Unassigned provider-free creation persists the date; rejected assignment consumes no number/receipt |
| `planning_edits` | Planning edit returned `Unavailable` | Shared policy persists assignment/labels/date/choice values, retains retired values/history, rejects stale Issue/catalog and source-owned changes |
| `planning_parent` | Self-parent edit was accepted | Self/foreign/missing parent rejection on create/edit; concurrent opposite links have one valid winner and no partial revision/status history |
| `planning_children` | Child discovery returned `Unavailable` | Visible direct children are filtered before limit/cursor; wrong-parent/foreign cursors fail and memory pages match |
| `planning_upgrade` | SQL deletion of a referenced catalog version succeeded | 0068 upgrade/repeated migration preserves legacy JSON; catalog and parent foreign keys reject broken references |
| `planning_catalog_reads` | Oversized absent version returned `InvalidInput` while memory returned `NotFound` | Absent versions match memory, reads initialize no rows and work with one pool connection |

An initial foreign-Project fixture name collision and a missing unwrap of the
existing visibility constructor were corrected before recording behavioral Red;
they are not counted as evidence. No environment failure counts as Red.

Existing Issue tests passed (20 scenarios) before the shared safe summary-page
projection was extracted. The final post-refactoring run passed all 24 Issue
scenarios. Additional regression scenarios qualify reused mutation guarantees:
actual pool close/reconnect, operation/security-scoped original receipts after
later edits, one winning concurrent catalog publication, safe audit/outbox facts,
and injected outbox failure with no partial catalog/revision/audit/replay. Retry
then commits exactly once. No timeout was increased and tests run in parallel.

## Final verification

```sh
cargo test --locked -p octacity-server-store-postgres \
  --test issues --test issue_workflows --test migrations --test projects -- --ignored
# 42 passed: 24 Issue, 8 workflow, 6 migration, 4 Project tests.

cargo test --locked -p octacity-server-store --lib
# 81 passed.
cargo test --locked -p octacity-server-application --lib issue_
# 38 passed; unrelated application tests were not rerun.
cargo test --locked -p octacity-server-api-rest --test v1_handlers
# 97 passed.

cargo clippy --locked -p octacity-server-store-postgres --all-targets -- -D warnings
cargo check --locked -p octacity-server
cargo fmt --all -- --check
python3 tools/check_architecture.py
# Passed: 63 packages, 256 dependency edges; unchanged dependency graph.
openspec validate add-factory-work-card --strict
git diff --check
```

All checks above passed. UI/browser and complete container image builds were not
rerun: this slice changes only persistence and documentation. Docker used the
already cached PostgreSQL image, with loopback-only ephemeral port, memory limit
and temporary data on tmpfs. The dedicated container was removed after tests;
user stand containers, volumes, credentials, images and build artifacts were not
removed. Free space stayed approximately 6.7–6.9 GiB during builds, with debug
information and incremental compilation disabled.
