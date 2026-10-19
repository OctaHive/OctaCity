# Task 4.1: canonical primary native links at the application boundary

## Scope and shared mechanisms

The optional `IssueSourceHandlers` uses the existing `WorkSourceObservations`
port for one bounded installed read, normal management authorization and a
narrow `IssueSourceStore`. Manual Issue handlers gain no Source dependency.
`IssueSourceSelection` derives its key through the existing native Issue,
security-partition and native-correlation helpers. The identity contains only
Project, accepted security partition, installation identity and native
resource/item. Installed versions, selectors, pipeline identities, credentials,
display keys, names and text cannot merge or split canonical tasks.

The in-memory adapter shares the Project transaction lock, ordinary Issue
creation policy/default workflow and atomic revision/replay/audit publisher.
Import creates an ordinary initial draft or returns its canonical link without
refreshing requirements. Each import retains its own replay receipt. Explicit
attachment checks current Issue/ownership review and preserves all task content,
workflow/status, field policy and execution state. Conflicts expose only a
currently visible canonical association. Initial imported placement records
Source provenance, not a native-state-derived business transition.

No PostgreSQL migration, periodic refresh consumer, field/status mapping,
Reporter, admission, HTTP route, Console control or provider SDK is added here.
Those retain their separate tasks. The application seam is qualified with a
normalized fake Source reader; it does not claim live provider qualification.

## Observed TDD cycles

All tests use authorized application commands/queries, ordinary Issue reads and
the existing mutation-evidence probe. The concurrency test starts four callers
against the same memory authority. Tests do not inspect private maps or SQL.

All Cargo invocations used:

```sh
CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_BUILD_JOBS=2
```

The pre-change baseline passed:

```sh
cargo test -p octacity-server-application --lib issue_tests::issue_manual_draft_uses_default_without_execution_or_provider
```

Each filter below used `cargo test -p octacity-server-application --lib FILTER`.

| Filter | Observed Red | Observed Green |
| --- | --- | --- |
| `issue_source_explicit_attach` | Required application command/link contract absent | One primary link, unchanged manual Issue, read-only query and exact replay |
| `issue_source_concurrent_imports` | Required import command absent | Four independent profiles resolve one Issue; manual task with equal text remains distinct; 53 Issue regressions passed |
| `issue_source_existing_import` | A second importer could reuse its receipt key to create another native task | Existing-link resolution seals that importer's selection; changed reuse conflicts |
| `issue_source_hidden_canonical` | Hidden canonical import failed only after publishing an extra replay/audit/outbox receipt | Current Issue visibility reaches the transaction and denial publishes nothing |
| `issue_source_read_can_finish` | A trusted read starting after command ingress was rejected as future data | The shared clock is read after asynchronous access; publication retains that time and an expired/future observation fails |
| `issue_source_initial_history` | Imported initial placement was labeled Manual | Initial placement is Source, with no preceding status or transition edge |
| `issue_source_observation_expiring` | The required shared-clock dependency was absent from the constructor | A reader advances trusted time to the observation's deadline; the post-read check rejects publication without replay/audit/outbox changes |

A missing test import during history-test setup was corrected before observing
its behavioral Red; that setup failure was not counted as Red.

## Qualification

`cargo test -p octacity-server-application --lib issue_tests::source` passed all
11 tests after the final clock refinement. Additional contract coverage checks
Project/security/installation/container/item partitioning, compatible installed
version/reference replacement, separate manual tasks with equal text, zero-or-one
primary source, visible versus hidden duplicate conflicts, stale ownership/version
review, original replay after manual editing, denied/expired/unmatched reads and
manual creation during Source failure. Import and attachment roll back at each
of the five existing mutation failure points and retry without consuming an
extra Issue number or leaving a partial link.

Regression and check results:

- `cargo test -p octacity-server-store -p octacity-server-application --all-features --lib`: 560 application + 81 store tests passed, including existing manual Issue/workflow/planning, execution/status actions, ordinary admission, Source/Reporter and shared publication contracts. This broad run began before the last clock refinement.
- `cargo test -p octacity-server-application --lib issue_`: 79 passed after the final clock refinement, including all 11 native-link tests, manual tasks, execution/publication and guarded status actions.
- `cargo clippy -p octacity-server-application -p octacity-server-store-postgres --all-features --all-targets -- -D warnings`: passed after the clock refinement, including the unchanged PostgreSQL consumer's compatibility with the shared store contracts.
- `cargo fmt --all -- --check`, `git diff --check`: passed.
- `python3 tools/check_architecture.py`: passed, 63 workspace packages and 256 dependency edges.
- `openspec validate add-factory-work-card --strict`: passed.

No UI/API schema, PostgreSQL behavior or runtime composition changes require
additional browser, migration or live-provider qualification in this task.
Disk checks stayed around 9.9–10 GiB available; builds disabled incremental
artifacts/debug information and used two build jobs. No service/image downloads,
extra target directory or unrelated cleanup was needed. Task 4.1 is complete;
the change has 23 of 56 tasks complete. No commit was created.

Documentation changes add no executable behavior; diff/format and strict
OpenSpec validation apply instead of TDD.
