# Task 5.14 verification

The subsequent [review follow-up](verification-review-5.14.md) records extraction
of the independent read profile and early input/stage-effect compatibility checks.

## Scope and result

The optional application `WorkSourceObservations` port and runtime
`HostedWorkSourceObservations` expose one bounded explicitly requested installed
Source read. There is no Factory target/input identity in the request, no task
store, subscription, second poller, admission or Reporter dependency. Existing
Source v1 scan/window/continuation, verified process execution, normalized snapshot
translation and finite `SourcePollingPolicy` validation are reused.

Private observations retain trusted Project/security/installation/native identity,
exact plugin/configuration/selector provenance, complete snapshot digest and
original read/expiry times. Fresh/unchanged/stale classification uses content and
trusted time, not ordered provider update identifiers. Current managed task-read
grants are checked before dispatch and response handoff under one deadline.

Seven process-boundary contracts prove bounded reads, native correlation without
input keys, fresh data after admission/selector mismatch, two independent original
Work/Run records, unchanged frozen inputs, closed-task reads with every execution
input disabled, grant revocation before/during reads, safe failures, cancellation
and no extra reads without a consumer. The common process peer exercises both
GitHub and a fake provider identity. The actual GitHub adapter separately proves
the reused HTTPS scan contract with an independently installed unrestricted
selector for a closed task excluded by its execution selector.

No native GitLab/Jira adapter, live GitHub access or persistent internal Issue is
claimed. The card change still owns import/link-refresh enablement, durable task
progress, association uniqueness, revision ordering, field ownership, execution
requests and additional command producers. Empty/incomplete pages do not mean
deletion, and opaque update ids do not establish a remote revision order. See the
[observation reference](../../../docs/reference/work-source-observations.md).
Completed earlier tasks and their uncommitted implementation remain preserved.

## Observed TDD cycles

| Public boundary | Red observed before implementation | Green observed |
| --- | --- | --- |
| Optional observation read | `optional_observations_carry_trusted_native_scope` could not compile because the required public port/request/runtime types did not exist | One verified bounded read returns native provenance without Run, Artifact or scan scheduling mutations |
| Freshness | `observation_freshness_uses` could not compile because the required freshness API did not exist | First/unchanged/changed content, equal opaque update ids, expiry, future dates and out-of-order observations pass |
| Current read authority | `observation_revocation_before` failed because an in-flight response was handed off after grant revocation | Before-dispatch revocation makes zero provider calls; in-flight revocation withholds the snapshot |
| Finite installed bounds | `observation_installation_rejects` failed because zero retention was accepted | Shared existing polling policy rejects invalid bounds; duplicate exact profiles fail before activity |

The initial admission/process suite passed before shared snapshot extraction
(67 tests after the first three cycles). Existing polling-policy characterization
passed before extracting its validator (3 tests). The native GitHub selector test
characterizes reused behavior; no GitHub production behavior was changed.
Environment failures were not used as Red. No timeout increase or test
serialization was introduced.

## Commands and qualification

All Cargo commands used the existing locked dependency graph and bounded build
settings (no new dependency):

```sh
export CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_BUILD_JOBS=2

# Each ran first as Red and then passed after its corresponding implementation.
cargo test -p octacity-server --lib optional_observations_carry_trusted_native_scope
cargo test -p octacity-server --lib observation_freshness_uses
cargo test -p octacity-server --lib observation_revocation_before
cargo test -p octacity-server --lib observation_installation_rejects

cargo test --locked -p octacity-server --lib work_source
# 71 passed; 9 existing explicitly opt-in PostgreSQL tests ignored.
cargo test --locked -p octacity-server-application --lib factory_admission_tests
# 82 passed, including independent manual admission, replay and bounded reconciliation.
cargo test --locked -p octacity-server --lib work_reporter_tests
# 4 passed, including the existing absolute-deadline regression.
cargo test --locked -p octacity-github -p octacity-server-work-source -p octacity-work-source-protocol
# 142 passed: native HTTPS Source/Reporter, credentials, process host and protocol contracts.
cargo clippy --locked -p octacity-server -p octacity-server-application --all-targets --all-features -- -D warnings
cargo clippy --locked -p octacity-github --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
python3 tools/check_architecture.py
python3 tools/security_matrix.py validate
python3 tools/verify_local_stand_context.py
openspec validate add-github-work-source --strict
git diff --check
```

There are no UI, REST, protocol-shape, persistence or migration changes in this
delta. Broad UI/PostgreSQL/live composed qualification remains in task 6. Existing
5.12/5.13 evidence is retained without claiming it was rerun here. Documentation
has no executable behavior; OpenSpec and whitespace checks validate these edits.

Work stayed on `main`; no commit was requested or created. Disk space was checked
throughout and remained approximately 13 GiB free. Existing caches and the user's
stand were preserved; no image download, container or server was needed.
