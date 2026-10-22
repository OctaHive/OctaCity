# Task 3.1 — Reviewed execution requests and Work associations

## Scope and public boundaries

The authorized application boundary now reserves an explicit execution request for
one exact current Issue revision. `IssueExecutionStore` supplies narrow request,
original replay, exact read and trusted Work-association operations. In-memory
conformance uses the existing Project/Issue lock and shared atomic mutation
publisher. No provider SDK or new package dependency is introduced.

`RequestIssueExecutionCommand` requires separate Issue execution and ordinary
Project-owned Factory admission authority, with current Project/Issue/target and
Repository visibility. It checks the exact immutable definitions and ordinary
base selection policy. Missing/blank acceptance is rejected with the finite
`IssueAcceptanceRequired` diagnostic. Ordinary Issue creation continues through
`IssueHandlers` without Factory, repository, provider or acceptance prerequisites.

The request retains its reviewed revision, immutable target/repository versions,
base policy and finite `TitleDescriptionAcceptanceV1` mapping. The shared mapping
produces title + two LF bytes + description; acceptance bytes remain unchanged.
The snapshot retains exact content digests and mapped byte sizes, with no task
bodies in generic request diagnostics. Original scoped replay is resolved before
current requirements or target lookup. Changed key reuse and new requests against
stale reviewed versions conflict. Later intentional requests get distinct logical
IDs and ordinary manual Work/replay identities.

`AssociateIssueWork` accepts an original trusted `PublishedFactoryAdmission`.
Initial Work/Run/Flow validation is shared with ordinary admission. A reviewed
link must match Project, configuration, repository and mapped input digests/sizes.
One Issue can retain many Work; one Work belongs to at most one same-Project Issue
and its original Run, and one request cannot be retargeted to another Work.
Association and the request's linked projection commit atomically through the
existing mutation publisher. Legacy Artifact lineage can remain unassociated or
be linked without inventing a requirements revision. Reads create no links.

Reservation returns its original receipt, including pending preparation. Exact
request reads expose a committed association after linkage; no admission is claimed
from an unlinked reservation. Input publication, VCS/source freezing, durable
PostgreSQL recovery/composition, HTTP and Console start controls remain tasks
3.2–3.5. No second intake, executor, Run journal, status transition or Reporter is
implemented. The pure text mapping is reusable by the shared publisher in 3.2.

## Red/Green evidence

Repository AGENTS.md selects the public application/store contracts as the test
boundary. Each implementation slice followed its test; API compilation failures
below identify newly required contracts, not environment failures.

All Cargo commands use bounded existing workspace output:

```sh
export CARGO_INCREMENTAL=0
export CARGO_PROFILE_DEV_DEBUG=0
export CARGO_PROFILE_TEST_DEBUG=0
export CARGO_BUILD_JOBS=2
```

| Slice | Observed Red | Observed Green |
| --- | --- | --- |
| Exact reviewed reservation | Required request/handler/query contracts absent | Original version/digests survive edits, original receipt replays, changed reuse and new stale requests conflict |
| Required acceptance | A missing acceptance document was accepted | Missing, empty and whitespace-only acceptance reject without mutation; manual drafts remain valid |
| Ordinary rights and targets | Reservation succeeded without Factory Run authority | Missing admission/visibility, unknown exact versions, forbidden reference and foreign Project reject; original replay survives target-store loss |
| Work association | Required association and manual identity contracts absent | Multiple intentional requests retain distinct ordinary Work/Run; input identity/digests/sizes and original receipts remain exact |
| Association integrity | A foreign Project could accept a Work link | Foreign Project, malformed Run receipt, competing Issue and replacement Work reject without partial association; exact original link replays |
| Actionable acceptance diagnostic | Finite acceptance-error contract absent | Required acceptance reports its own safe diagnostic through the application boundary |

Focused Red/Green commands:

```sh
cargo test --locked -p octacity-server-application --lib issue_execution_request_freezes
cargo test --locked -p octacity-server-application --lib issue_execution_requires_real_acceptance
cargo test --locked -p octacity-server-application --lib issue_execution_checks_admission
cargo test --locked -p octacity-server-application --lib issue_execution_associations
cargo test --locked -p octacity-server-application --lib issue_execution_association_rejects
cargo test --locked -p octacity-server-application --lib issue_execution_
cargo test --locked -p octacity-server-application --lib late_validation_retry_preserves
cargo test --locked -p octacity-server-application --lib factory_development_control_tests
```

The existing actual late-validation retry/cancellation journeys now attach their
original admission as legacy Artifact lineage before processing. Both journeys
passed: retry produces a fresh execution generation with the same immutable
inputs, Work, original Run and retained Issue link. This is actual public control
and owner processing, not a fabricated retry receipt or a second request.

The existing store baseline passed 81 tests before the shared mutation publisher
was extracted. Before ordinary admission authorization extraction, both hidden
Project/dependency application tests passed. The broader application baseline
(including the first new request scenario) passed all 515 tests. Intermediate
compiler mistakes (UUID serialization and fixture/API references) were corrected;
they are not counted as behavioral Red evidence.

## Qualification commands

```sh
cargo test --locked -p octacity-server-store -p octacity-server-application --lib
cargo test --locked -p octacity-server-domain
cargo clippy --locked -p octacity-server-domain -p octacity-server-store \
  -p octacity-server-application --all-targets -- -D warnings
cargo check --locked -p octacity-server
cargo fmt --all --check
python3 tools/check_architecture.py
openspec validate add-factory-work-card --strict
git diff --check
```

Documentation-only edits use formatting, whitespace and strict OpenSpec checks;
they have no executable behavior requiring a separate Red/Green cycle. HTTP,
Console, provider adapters and PostgreSQL persistence are unchanged in this task.
No live provider, paid model, image build, deployment or commit is part of this
qualification.

## Final results

| Check | Result |
| --- | --- |
| Full application library regression | 519 pass |
| Full store library regression | 81 pass |
| Domain value integration tests | 17 pass |
| New execution scenarios after fixture deduplication | 5 pass |
| Actual retry/cancellation journeys with Issue association | 2 pass (also included in application regression) |
| Final Clippy, all targets of domain/store/application | Pass with `-D warnings` |
| Server compilation, including production REST/PostgreSQL composition | Pass |
| Architecture | Pass: 63 packages, 256 edges; dependencies unchanged |
| Rust formatting, OpenSpec strict validation and whitespace | Pass |

The final full library regression includes the safe acceptance diagnostic and
shared admission/transaction validation. The subsequent test-only fixture cleanup
preserves every assertion; the five focused scenarios and all-target Clippy were
rerun successfully after that cleanup. No product behavior changed afterward.

Builds disabled incremental/debug output and used two compile jobs. Free space
was checked throughout, approximately 18–23 GiB, with about 18 GiB available at
completion. Generated Cargo output reused the workspace target directory.

Task 3.1 is complete: **15/56** implementation tasks. Other pending tasks remain
unchanged. No commit was created.
