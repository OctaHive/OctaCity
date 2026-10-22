# Task 4.3: bounded execution-neutral Source import and refresh

## Scope and shared mechanisms

`IssueSourceReadHandlers` consumes the existing installed `WorkSourceObservations`
capability. Its explicit finite command selects a Project, exact installed read
profile and optionally a retained Issue link. It has no Factory target,
repository, admission, Artifact publisher, Reporter or credential port. Ordinary
manual Issue handlers retain no Source dependency. Construction starts no
background worker or provider access; REST/Console composition remains task 4.5.

Factory intake and this consumer use one extracted bounded polling driver,
including the existing retained page, continuation, inclusive overlap,
reconciliation fairness, page/item/byte budgets and original scan identity.
The existing scan/claim model is generic over its target while defaulting to the
unchanged Factory target. Shared acquisition/fencing policy and
`WorkSourceProgress`/`WorkSourceSchedule` are reused. The native Factory worker
and Issue reader also share the existing retry/backoff classification instead
of introducing another retry or provider-polling implementation.

PostgreSQL additive migrations 82–83 retain safe freshness/control metadata and
separately scoped bounded private progress. Read publication checks the exact
scan owner in the same transaction as Issue/replay/audit/outbox changes. A
resumed page uses original observations and deadlines. Its expiry is capped by
the earliest original provenance deadline; it cannot renew a private tail by
persisting or retrying it later. Expiry discards the tail and starts a fresh
bounded read through the existing driver.

The installed reader rechecks current read authority independently of listing
another page. This uses the existing managed TaskRead checks and does not fetch
a new ticket page. The consumer reauthorizes before processing a retained item.
Project/Issue visibility precedes reads; existing canonical visibility and
current Create/Update authority still precede Issue mutation.

Freshness retains a last accepted receipt and an unconfirmed changed receipt.
Opaque update identities are never compared as versions. A changed complete
snapshot remains `uncertain` until a later independent read under the same
installed binding confirms it; older/equal-time observations cannot confirm it.
The classified results are fresh, unchanged, duplicate, stale and uncertain.
Only reconciled changes can replace already declared source-owned title or
description. The ordinary requirements revision/applicability policy and
publication writer remain authoritative. Local fields, workflow/status and all
frozen execution inputs remain unchanged. Field/status mapping publication and
activation remain task 4.4; labels/state never infer a business move.

Explicit retained-link pull control has a reviewed monotonic generation and
shared command replay/audit. Disabling it blocks that link's refresh; reenabling
cannot revive a prior read owner or its private page. A separately installed
broader read profile can refresh retained links without matching an execution
input. Execution input enablement does not grant or revoke this link policy.
No native GitLab/Jira adapter, provider SDK or live-provider qualification is
claimed.

## Observed TDD cycles

All Cargo commands used the existing target directory and:

```sh
CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_BUILD_JOBS=2
```

PostgreSQL tests used an explicitly disposable PostgreSQL 18.6 service and
`-- --ignored`. The cached image ran with 512 MiB memory and a 384 MiB data tmpfs;
no image download or new target directory was needed. Assertions use authorized
application commands, ordinary Issue reads or public store contracts. SQL is
used for migration fixtures and controlled failure injection, not as the
expected-result oracle.

Pre-change baselines: 11 application Source scenarios passed. The existing
application test binary passed all 19 `polling::` scenarios before extraction
(two initial Cargo filters matched zero tests and were not counted). The native
`cargo test -p octacity-server --lib work_source` baseline passed 72 scenarios,
with 9 PostgreSQL cases intentionally ignored pending a configured disposable
service.

| Test command/filter | Observed Red | Observed Green |
| --- | --- | --- |
| `cargo test -p octacity-server-application --lib issue_source_refresh_reconciles` | Required refresh command/state API absent | First changed opaque snapshot is uncertain; a later confirming read refreshes owned text once; echo/stale observations preserve revision/status/local description |
| `cargo test -p octacity-server-store-postgres --test issues source_refresh_reconciliation -- --ignored` | PostgreSQL refresh capability absent | Pending reconciliation survives restart; failed publication rolls back watermark and revision; exact retry returns original result |
| `cargo test -p octacity-server-application --lib issue_source_bounded_import` | Required execution-neutral bounded consumer API absent | One retained two-item page settles one item per invocation; restart processes the original tail without listing again or configuring Factory |
| `cargo test -p octacity-server-store-postgres --test issues source_execution_neutral_progress -- --ignored` | Required PostgreSQL read journal API absent | Original position and rate-limit schedule survive restart; takeover increases the shared fence and rejects the old owner |
| `cargo test -p octacity-server-application --lib issue_source_rate_limit` | A new owner retried before the provider's retained not-before time | Shared backoff respects the hint and finite exhaustion across owner restart; manual creation stays usable |
| `cargo test -p octacity-server-application --lib issue_source_retained_link_refresh` | Required explicit pull control absent | Disabled link performs no read; reenabled link uses a separately installed broader profile without execution-selector matches |
| `cargo test -p octacity-server --lib retained_observation_authority` | Independent installed read-authorization API absent | Managed grant recheck succeeds, then fails after revocation, without listing a ticket or admitting Work |
| `cargo test -p octacity-server-application --lib issue_source_owner_expiring` | An owner expiring during reauthorization created ISS-1 before its checkpoint failed | Atomic read-fence validation rejects publication; the next manual task still receives ISS-1 |
| `cargo test -p octacity-server-store-postgres --test issues source_read_takeover -- --ignored` | Import request lacked a retained-read proof | Original owner cannot publish after takeover; its replacement publishes exactly ISS-1 through the common transaction |
| `cargo test -p octacity-server-store-postgres --test issues source_pull_disable -- --ignored` | Disable/enable revived the old owner's checkpoint authority | Captured pull generation rejects that owner while original control replay and a new read remain valid |
| `cargo test -p octacity-server-application --lib issue_source_expired_original` | A retained tail outlived its original observation and failed instead of starting a fresh read | The original observation deadline caps the page; expiry discards the old tail and fetches a fresh page without importing expired native-b |
| `cargo test -p octacity-server-application --lib issue_source_hidden_project` | An empty page succeeded despite hidden Project visibility | Visibility denial precedes provider activity and read progress |
| `cargo test -p octacity-server-application --lib issue_source_discovery_can_pass` | An independently authorized discovery pass stopped at a link with disabled pull | The pass can checkpoint that observation without refreshing the paused Issue |
| `cargo test -p octacity-server-application --lib issue_source_read_pass_identity` | Two accepted security partitions used the same provider pass ID | Original pass identity also binds the accepted partition, while ordinary Factory pass IDs stay unchanged |
| `cargo test -p octacity-server-store-postgres --test issues source_private_read_progress -- --ignored` | Private progress prevented deletion of an otherwise empty Project | Its derived private journal cascades with Project deletion; existing Issue/history reference protection stays authoritative |

Missing API Red runs establish the required new public contract; Green runs
assert behavior, not compilation alone. Missing fixture imports and a missing
Rust documentation comment were setup issues, not behavioral Red.

## Qualification

| Command | Result |
| --- | --- |
| `cargo test -p octacity-server-application --lib issue_` | 88 passed, including all 20 Source scenarios; repeated after the final simplification |
| `cargo test -p octacity-server-store --all-features --lib` | 81 passed |
| `cargo test -p octacity-server-application --lib factory_admission_tests::installed_sources::polling` | All 19 original driver scenarios passed before and after extraction, including continuation, reconciliation fairness, bounds, expiry, ownership and replay |
| `cargo test -p octacity-server-store-postgres --test issues --test issue_execution --test issue_workflows --test factory_discovery -- --ignored --test-threads=2` | 89 passed: 35 Issue, 19 execution, 8 workflow and 27 Factory/Source scenarios, before the final empty-Project cache refinement |
| `cargo test -p octacity-server-store-postgres --test issues source_ -- --ignored --test-threads=2` | 13 passed after the cache cascade and final checkpoint/finish simplification, including all six new PostgreSQL scenarios |
| `cargo test -p octacity-server-store-postgres --test migrations --test projects -- --ignored --test-threads=1` | 6 migration and 4 Project scenarios passed after the cache cascade; both new tables participate in the existing migration/snapshot inventory |
| `cargo test -p octacity-server --lib work_source` | 73 passed; 9 PostgreSQL scenarios then qualified separately |
| `cargo test -p octacity-server --lib work_source -- --ignored --test-threads=1` | All 9 native PostgreSQL scenarios passed, including Source/Reporter restart and original reporting ordering |
| `cargo clippy -p octacity-server -p octacity-server-store-postgres -p octacity-server-application --all-features --all-targets -- -D warnings` | Passed after final simplification |

Upgrade qualification seeds the exact schema-81 link/receipt format, migrates
twice, and reads the original safe provenance and unchanged manual Issue through
public store methods. No provider access, workflow move or execution is created
by migration. The native installed-process regression also retains its existing
closed/nonmatching task and disabled execution-input observation scenario, with
the two original input Runs unchanged.

Final review removed repeated draft normalization across explicit and paged
import/refresh. A retained candidate stores its receipt and original snapshot;
Project/profile/native selection is derived from the original fence/snapshot
instead of storing the same selection again. Clippy identified an unnecessary
large checkpoint enum and a dense replay tuple. The store now uses the existing
checkpoint/finish shape and shared guarded update, with a named replay record;
no lint suppression or extra policy branch was added. Existing behavioral tests
passed before refactoring and the 88 application/13 PostgreSQL Source scenarios
passed again afterward.

`cargo fmt --all -- --check`, `git diff --check`,
`python3 tools/check_architecture.py` (63 packages, 256 dependency edges) and
`openspec validate add-factory-work-card --strict` passed. Documentation and
formatting add no executable behavior, so these checks apply instead of TDD.
No REST/Console schema or generated client changed in task 4.3.

Observed disk availability stayed about 9.7–10 GiB. The dedicated disposable
test container was removed after qualification; no unrelated files, images or
services were cleaned up. Native process fixtures qualify the installed read
and authorization boundary, not live provider access. Live GitHub qualification
and real GitLab/Jira adapters remain outside this result. Task 4.3 is complete,
bringing the change to 25 of 56 tasks. No commit was created.

## Review corrections: pull pause race and attachment documentation

The application regression interleaves a link pause during Update authorization,
after discovery has checked the enabled flag. It exercises the existing
`RunIssueSourceReadCommand` boundary, resumes the original remaining page through
a new owner without another provider read, and verifies ordinary manual creation.

| Test command/filter | Observed Red | Observed Green |
| --- | --- | --- |
| `cargo test --offline -p octacity-server-application --lib issue_source_discovery_pause_during_refresh` | Pausing the link during authorization returned a store conflict instead of settling the item | The paused item settles without changing its Issue; restart imports the other retained item and manual creation receives ISS-3 |
| `cargo test --offline -p octacity-server-store-postgres --test issues source_refresh_paused_publication -- --ignored` | The same publication on an active discovery fence returned an Issue conflict | PostgreSQL retains `pull_disabled`, preserves accepted/pending freshness and the Issue revision, and returns the original skip after restart and reenabling; a new changed read still requires independent confirmation |

Both adapters now classify disabled pull through `IssueSourceRefreshState::observe`
under their existing locks and retain the result using the existing atomic
replay/audit/outbox publisher. There is no consumer-specific error suppression,
new retry owner or provider branch. Claim, generation, native identity and
observation guards still precede classification, so a stale retained-link owner
cannot gain authority from this skip. The shared polling/retry driver is unchanged.

The attachment reference now states the existing behavior precisely: pull starts
enabled for explicitly authorized refresh, while attachment starts no polling and
activates no field ownership, push synchronization or retroactive reporting.
That wording correction adds no executable behavior and needs no TDD cycle.

Review-fix qualification used the existing bounded build environment:
`CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_BUILD_JOBS=2`.
PostgreSQL commands used `OCTACITY_POSTGRES_URL` pointing to a dedicated disposable
PostgreSQL 18.6 container, with 512 MiB memory and a 384 MiB temporary data directory.

| Command | Result |
| --- | --- |
| `cargo test --offline -p octacity-server-application --lib issue_` | 89 passed |
| `cargo test --offline -p octacity-server-application --lib factory_admission_tests::installed_sources::polling` | 19 passed |
| `cargo test --offline -p octacity-server-store --all-features --lib` | 81 passed |
| `cargo test --offline -p octacity-server-store-postgres --test issues -- --ignored --test-threads=2` | 37 passed, including manual Issue behavior and Source takeover/pull-generation guards |
| `cargo clippy --offline -p octacity-server-application -p octacity-server-store -p octacity-server-store-postgres --all-features --all-targets -- -D warnings` | Passed |
| `cargo fmt --all -- --check` | Passed |
| `git diff --check` | Passed |
| `python3 tools/check_architecture.py` | Passed: 63 packages, 256 dependency edges |
| `openspec validate add-factory-work-card --strict` | Passed |

Disk availability stayed about 9.5–9.8 GiB. The dedicated test container was
stopped and automatically removed after PostgreSQL qualification. No unrelated
cleanup, provider dependency, migration or commit was introduced by these fixes.
