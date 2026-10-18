# Task 5.7 verification

Task 5.7 implements protected bounded Source preview and retained Source/Reporter
diagnostics. It preserves the completed 5.6 work already present in the working
tree, ordinary manual Factory admission and existing automatic replay identities.
Work remains in `main`, without a commit.

## Implemented boundary

- Source v1 has an independently advertised optional `preview` operation. Old
  manifests default to unsupported; a Scan capability cannot expose a read handle
  through Preview implicitly. Existing Scan/Authorize/Check/Reporter contracts
  remain compatible.
- The GitHub adapter reuses bounded verified HTTPS metadata/listing, returns
  matching and filtered summaries with exact selector reasons, excludes pull
  requests and truncates titles on UTF-8 boundaries. Preview performs GETs only.
  Raw bodies, mapped private inputs and credentials never enter the response.
- Trusted hosted preview holds verified profiles, current managed read access and
  finite process bounds, with no Artifact, admission, scan or task-write store.
  Authorization and provider execution share one original absolute deadline.
  Installed page ceilings can only be narrowed. No retry is scheduled.
- Exact draft inputs or frozen exclusive-group members are previewed, including
  disabled definitions. The response identifies reviewed configuration and exact
  installed predicates; missing predicate presentation means unknown. Filtered,
  selected and ambiguous results are distinct from authoritative admission.
  Empty reads, throttling and unavailability remain distinct.
- Read-only store projections return retained scan/window/selection/scheduling and
  visible original input/version/Work/Run associations. PostgreSQL does not load
  private preparation documents for correlation queries. Dependency/Run
  visibility precedes ordering, limit and cursor calculation in both backends.
- Native correlation includes Project, installed security partition, installation
  and native container/item, excluding automatic input identity. Existing source
  preparation/admission hashes remain unchanged; the new correlation is not a
  replay, deduplication or task-creation authority.
- Reporting summaries use accepted actions and the existing Run/outbox journal,
  with the original input Reporter's exact capability. Queued/in-flight,
  confirmed and exhausted projections remain separate from business status and
  execution. No second reporting journal or dispatcher is added.
- Optional internal task links come only from an explicitly installed authoritative
  read capability, with current visibility and Project checks. A hermetic enabled
  task/observation owner stand-in exposes only that read capability to Factory;
  its Issue/revision/association/synchronization counters stay unchanged during
  preview. Hidden links disappear while native/Work/Run diagnostics continue.
  Persistent cards and the actual discovery observation handoff remain in their
  designated changes/tasks; preview installs no observation consumer.
- Three protected no-store REST operations expose preview and exact input/group
  diagnostics. Payloads and queries are closed and bounded; diagnostic cursors
  bind Project and target version. There is no credential or internal Issue key
  in preview input. OpenAPI and generated Console types match these operations.

No migration, live provider mutation, production-stand change, paid model call or
arbitrary installed-profile publisher is introduced. GitLab/Jira/custom-provider
qualification uses the same generic contracts and hermetic peers, not live SDKs.
Manual Artifact intake still requires no integration or internal task card.

## TDD: observed Red and Green

Every Cargo command used these resource bounds (including commands run by API
code generation):

```sh
export CARGO_INCREMENTAL=0
export CARGO_PROFILE_DEV_DEBUG=0
export CARGO_PROFILE_TEST_DEBUG=0
export CARGO_BUILD_JOBS=2
```

The following tests were written and executed before their behavior/API was
implemented. Missing newly required public API/operations are explicit Red;
fixture/compiler setup mistakes were corrected and were not counted as Red.
All rows have subsequently observed Green.

| Public boundary / command | Observed Red | Green behavior |
| --- | --- | --- |
| `cargo test --locked -p octacity-work-source-protocol --test preview` | Unknown `preview` operation | Bounded summaries/reasons; wrong scope, private fields and oversized/foreign proofs rejected |
| `cargo test --locked -p octacity-server-work-source --test host preview_requires_independent` | Unknown Preview advertisement | Explicit support required before read-handle exposure, for four provider identities |
| `cargo test --locked -p octacity-github --test issue_preview` | Unsupported operation | Matching/filtered HTTPS summaries, literal reasons, UTF-8 truncation and GET-only reads |
| `cargo test --locked -p octacity-server-application --lib source_preview_explains` | Preview public query/port absent | Match/admission distinction, native identity invariant under input-id changes, zero source/publication mutations |
| `cargo test --locked -p octacity-server-application --lib source_preview_freezes` | Group preview rejected as Invalid | Exact disabled/current members, ambiguity, and stale submitted member rejection |
| `cargo test --locked -p octacity-server-application --lib source_preview_distinguishes` | Rate-limit returned as an application error | Safe classified failures and bounded advisory retry distinct from successful empty reads |
| `cargo test --locked -p octacity-server --lib hosted_preview_reads` | Hosted read capability absent | Verified process read without claims, Work, Artifact or managed-to-legacy credential fallback |
| `cargo test --locked -p octacity-server --lib source_diagnostics_reads` | Diagnostics query absent | Existing scan/admission facts, original versions and repeatable zero-mutation reads |
| `cargo test --locked -p octacity-server-store-postgres --test factory_discovery source_publication_is_atomic_owned -- --ignored` | PostgreSQL diagnostic read capability absent | Same native/partition/visibility/progress contract in memory and a single-connection PostgreSQL pool |
| `cargo test --locked -p octacity-server --lib source_preview_correlates` | Authoritative optional-link capability absent | Common visible Work/Run correlation, optional hidden cards, unchanged task-owner state and grant-revocation protection |
| `cargo test --locked -p octacity-server-api-rest --test integration_administration source_preview_and_diagnostics_rest` | Read service/registered endpoints absent | Protected closed bounded REST, no publication headers, rate-limit versus empty, no secret echo |
| `cargo test --locked -p octacity-server --lib source_reporting_diagnostics_` | Zero reporting rows instead of queued action | Pending, applied and exhausted states from existing accepted outbox facts; diagnostics never dispatch |
| `cargo test --locked -p octacity-server-application --lib source_preview_and_diagnostics_identify` | Exact configuration/selector response fields absent | Reviewed immutable bindings and installed literal predicates in preview and diagnostics |
| `cargo test --locked -p octacity-server --lib hosted_preview_can_only_narrow` | Provider request used 100 items instead of installed ceiling 1 | Installed item/byte ceilings narrow client limits; production cursor remains absent |
| `cargo test --locked -p octacity-server-application --lib source_preview_rejects_duplicate` | Duplicate draft definitions silently reached provider | Duplicate input IDs fail before provider reads |
| `cargo test --locked -p octacity-server-application --lib source_group_preview_does_not_reveal` | Hidden member returned Invalid instead of indistinguishable NotFound | Frozen member loading reuses the existing protected immutable-read boundary |

The final application regression command below includes Green for every new
application scenario (including both validation/privacy corrections). Existing
HTTPS failure behavior and dependency/Run visibility also receive regression
characterizations. No timeout increase, test serialization or production-cursor
write was used to obtain passing tests.

## Regression qualification

```sh
cargo test --locked -p octacity-server-application --lib factory_admission_tests
```

**76 passed**, including new preview/diagnostics queries and existing manual,
provider-neutral Source, retained-intent, distinct-stage and replay behavior.

```sh
cargo test --locked -p octacity-server-factory -p octacity-server-store \
  -p octacity-work-source-protocol -p octacity-server-work-source \
  -p octacity-github -p octacity-server-api-rest --lib --tests
```

**625 passed across 23 suites** (including zero-test targets). Protocol, host,
native HTTPS/CLI, core/store, protected REST, route inventory and OpenAPI checks
pass. Native Preview empty/throttled/unavailable reads remain distinct and never
write provider resources.

```sh
cargo test --locked -p octacity-server --lib
```

**156 passed, 8 existing PostgreSQL-only tests ignored** by the default command.
This was repeated after removing preview's dependency on unused polling-worker
settings and sharing exact managed-input connection resolution. Source/Reporter
absolute-deadline, cancellation, recovery, ordering, revocation and manual-outage
journeys pass under normal parallel test execution.

```sh
OCTACITY_POSTGRES_URL=postgres://postgres@127.0.0.1:32788/postgres \
  cargo test --locked -p octacity-server-store-postgres \
  --test factory_discovery source_ -- --ignored
```

**3 passed**: shared Source publication/recovery/diagnostic contract, managed
configuration/revocation contract, and preservation of existing manual admission
and replay. The diagnostic contract checks native and input lookup, original
versions, wrong Project/security partition, dependency/Run visibility, absence of
hidden pagination hints and unchanged scan progress in both stores. It uses the
existing disposable test databases and a one-connection application pool; no
production schema or timeout change is required.

Final non-behavioral checks pass:

```sh
cargo clippy --locked -p octacity-server-factory -p octacity-server-store \
  -p octacity-server-application -p octacity-server-api-rest \
  -p octacity-server-store-postgres -p octacity-server \
  -p octacity-work-source-protocol -p octacity-server-work-source \
  -p octacity-github --all-targets --all-features -- -D warnings
pnpm --dir ui api:generate
pnpm --dir ui typecheck
cargo fmt --all -- --check
git diff --check
python3 tools/check_architecture.py
python3 -m unittest tools.tests.test_check_architecture tools.tests.test_security_matrix
python3 tools/security_matrix.py validate
node ui/scripts/verify-architecture.mjs
openspec validate add-github-work-source --strict
openspec validate add-factory-work-card --strict
```

Architecture remains **63 packages / 253 dependency edges**; tooling tests:
**37 passed**. Documentation/status changes have no executable behavior and are
qualified by strict OpenSpec validation, formatting and whitespace checks. This
is targeted hermetic qualification, not a live account or full release matrix.

## Resources and status

Disk checks throughout observed 17–18 GiB free; `target` grew from about 6.1 to
6.5 GiB. Cargo builds ran one at a time, without incremental/debug output and with
two build jobs. Test execution remained normally parallel. No target wipe,
package installation or user-data cleanup was performed.

The owned PostgreSQL 18.6 container `octacity-5-7-preview-pg` used a private tmpfs,
random loopback port and uniquely created databases. Its exact container identity
was checked and removed after qualification. No local stand or user service was
modified.

Task 5.7 is complete. OpenSpec reports **26/37** tasks complete; task **5.8** is
next. Earlier completed entries, input-scoped automatic replay and the independent
manual/task-card scope are retained. No commit was created.
