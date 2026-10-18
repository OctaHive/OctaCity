# Verified Reporter outbox composition — task 4.2

Completed on 2026-10-17. OpenSpec progress: 16/29 tasks. No live provider, operator
credentials, paid model calls, deployment changes or commits.

## Scope and preserved authority

`HostedFactoryWorkStatusProjector` implements the existing application port using
the verified Work Reporter process host. The Flow interpreter, shared Work wire
format and manual admission do not acquire provider-specific types or behavior.
GitHub/GitLab/Jira/custom names in the composed journeys are hermetic protocol
peers, not live-account qualification claims.

`FactoryReporterProfile::new` pins the entire trusted Project/installation/
capability/resource selection, executable, provider profile, mapping, write handle,
secret profile and network ceiling into its immutable configuration reference.
Raw token bytes remain in the provider's private grant files. The retained source
input selects that exact Reporter binding. Ordinary profile/input replacement
cannot redirect an existing Work projection; keep original installed profile
versions available for pending operations.

Memory and PostgreSQL implement `source_preparation_for_work` over the existing
settled journal. This scope-checked read returns the original input and native
item association, never the current input version. The original retained snapshot
supplies its display key; missing retained data fails closed rather than querying
mutable ticket content. Manual Work has no installed-source association and
cannot be mistaken for an external ticket. No schema migration or second outbox,
identity/deduplication or lifecycle authority was added.

The application shares one accepted `set_status` materialization between the
existing worker and `validate_accepted_factory_status_request`. The latter checks
the current claimed outbox state and exact accepted Flow record, plugin, status,
Work/subject, permissions, budget and deadline. `FactoryWorkStatusRequest` now
carries the current outbox claim separately from its stable operation identity;
synchronous projectors receive `None`. An old claim cannot borrow a replacement
worker's fence even when both share the same original action deadline.

The bridge verifies executable/network/secret-profile ceilings before exposing a
write handle, observes the original mapping first, and projects only missing
effects after rechecking accepted authority. Operation id, native item, display
key, mapping and provider profile remain frozen. Metadata reads and both process
operations share one finite window narrowed by original action/claim and budget
bounds. Observation usage is checked before permitting projection. Existing outbox
policy owns retries; provider retry hints are lower bounds without renewed
deadlines or attempts. Reporting failures never rewrite accepted Flow data.

The trusted installer can explicitly construct
`FactoryRuntimeResources::work_reporter_dispatcher` and install it using the
existing `FactoryRuntimeInstallation.outbox_dispatchers` extension. Construction
performs no database, credential or provider I/O. Absence starts no Reporter
worker. The hosted bridge authorizes outbox dispatch only; synchronous action
cancellation is deliberately unsupported, while the verified host supplies actual
operation cancellation, timeout and process-tree cleanup.

## Observed TDD cycles

| Public boundary | Observed Red | Observed Green |
| --- | --- | --- |
| Original Work/source lookup, memory and PostgreSQL | Required `source_preparation_for_work` operation missing (E0599) | Original settled input resolves by exact Project/Work; foreign Project gets no association |
| Accepted-action authorization | Required public validator missing (E0425) | Live accepted request passes; changed value/plugin, expired and terminal requests fail |
| Dispatch fencing proof | Request lacked independently retained outbox claim (E0609) | Original logical operation cannot reuse another claim's fence |
| Immutable Reporter profile | Required public profile API missing (E0425/E0422) | Replacing mapping/provider/container/write-handle changes the selected configuration identity for every provider name |
| Profile validation | Read grant accepted as Reporter authority | Read-class substitution, invalid/duplicate/empty hosts, invalid mapping and locator rejected |
| Composed Source/action/outbox/Reporter | Required hosted projector API missing (E0425/E0433) | Ordinary admission and accepted action dispatch observe/project through the real verified process; pending rows cannot authorize writes |
| Retry lower bounds and restart | 2000ms provider delay lost behind configured 100ms retry | Durable retry at 2120, no claim at 2119, same original operation after restart, finite exhaustion at the second dispatch |
| Installation bounds | Zero timeout and ambiguous profile inventory accepted | Zero time/grace, duplicate bindings and oversized inventories rejected before dependency I/O |
| Native runtime composition | Required resource dispatcher method missing (E0599) | Native outbox binding constructs without database, grant or provider calls |
| Metadata deadline | Deliberately stalled store exceeded the test's 2-second watchdog despite a 100ms original deadline | Metadata resolution returns a bounded failure around the original 100ms window |
| Observation output budget | Provider write started after observation had exhausted its output budget | Projection is refused before a write; accepted Flow data stays unchanged |

Focused Red/Green commands:

```sh
cargo test -p octacity-server-application --lib staged_prepared_admission_keeps_one_run --locked
cargo test -p octacity-server-application --lib optional_status_reporting_is_queued --locked
cargo test -p octacity-server --lib reporter_profiles_freeze --locked
cargo test -p octacity-server --lib reporter_profiles_reject --locked
cargo test -p octacity-server --lib accepted_source_status_reaches --locked
cargo test -p octacity-server --lib reporter_retry_lower_bounds --locked
cargo test -p octacity-server --lib hosted_reporter_installation_rejects --locked
cargo test -p octacity-server --lib native_reporting_worker_binds --locked
cargo test -p octacity-server --lib reporter_metadata_reads_share --locked
cargo test -p octacity-server --lib an_observation_that_exhausts --locked
```

The PostgreSQL command below was also run before implementation and observed the
missing read operation, then passed after memory/PostgreSQL implementations.
Fixture setup/typing mistakes were corrected before relying on behavioral Red.

Additional passing public contracts exercise missing grant/network/executable
authority before process activity, disabled/replaced current input while the
original projection succeeds, timeout followed by original action expiry, and
manual admission during a Reporter outage. Snapshot comparisons prove retained
Flow/Run state is unchanged. The real process peer captures original operation,
item/display ids, mapping and independent write class; provider-independent host
correlation checks remain in force.

The first broad runtime run exposed an invalid timing assumption in the new
timeout test: a loaded host can time out before Python creates its request file.
The corrected assertion permits zero or one incomplete observation and requires
no writes and no new calls after original expiry. No product check was weakened.
The complete runtime suite then passed.

## Regressions and checks

Before changing behavior, the existing 15 Flow action tests, 7 verified host
tests and absent-input runtime test passed. Before extending the shared Source
journey fixture, all 23 existing Source runtime tests passed.

Final commands and observed results:

```sh
cargo test --locked -p octacity-server --lib runtime
# 114 passed, including all Source, Reporter, Delivery, supervisor and native installation tests
cargo test --locked -p octacity-server-application --lib factory
# 358 passed, including manual/source admission and accepted Flow/outbox behavior
cargo test --locked -p octacity-github -p octacity-server-work-source -p octacity-work-source-protocol -p octacity-delivery-protocol
# 88 passed: 66 GitHub, 7 verified host, 11 Work protocol and 4 Delivery protocol
OCTACITY_POSTGRES_URL=postgres://postgres:reporter-fixture-only@127.0.0.1:32775/postgres cargo test --locked -p octacity-server-store-postgres --test factory_discovery source_publication_is_atomic -- --ignored
# Passed; both stores and all four provider identities, including the new original-Work lookup
cargo clippy --locked -p octacity-server -p octacity-server-application -p octacity-server-store -p octacity-server-store-postgres --all-targets --all-features -- -D warnings
# Passed
cargo fmt --all -- --check
python3 tools/check_architecture.py
openspec validate add-github-work-source --strict
git diff --check
```

Architecture check: 63 workspace packages, 252 dependency edges. Dependencies and
lockfiles are unchanged. Documentation/formatting-only edits have no executable
behavior requiring a separate TDD cycle.

PostgreSQL used the task-owned `postgres:18.6` container
`86e88dc6fdadaa246a3afcabd3228f2a7c5848f12f9b1d48c82bdcec11ba580b`, loopback port
32775 and label `octacity.task=reporter-4.2`. Each contract created/dropped its own
database. Exact id/name/ownership label were inspected before removing that
container and its owned volume after verification. No user service was touched.
The metadata timeout test owns a separate local TCP peer that deliberately stalls
PostgreSQL negotiation; it does not depend on a failing external environment.

## Remaining scope

This task connects the verified capability to existing accepted actions/outbox.
Controlled HTTPS response-loss/partial-write qualification, cross-input managed
field ordering and stage-local effect authoring remain tasks 4.3–4.5. Multi-request
provider updates are not atomic. No new management/Console activation or
live-account qualification is implied by this composition boundary.
