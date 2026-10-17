# Reporter review fixes: TDD evidence

Latest PostgreSQL stability results are in the
[deterministic time follow-up](#deterministic-postgresql-time-under-load), which
supersedes the earlier load-related qualification limit.

This follow-up fixes the four review findings in completed tasks 4.1–4.5. It
adds no integration administration, provider implementation or subsequent task.
Manual Factory admission and installation-scoped admission identities remain
unchanged. Shared Factory code uses opaque provider identities and fields.

## Behavioral Red / Green

Each regression test was run before its implementation change and failed on the
behavior below. Fixture validation failures during setup were corrected first
and are not counted as Red. The same test then passed with the minimal fix.

| Command / public boundary | Observed Red | Observed Green |
| --- | --- | --- |
| `cargo test --locked -p octacity-server --lib a_confirmed_prewrite_conflict` | A new source projection delivered 0 instead of 1 after the old typed Conflict and original deadline expiry. | A valid definitive failure releases its write permit; the later intent delivers. The test runs GitHub/GitLab/Jira/custom protocol peers and keeps accepted Work/Flow unchanged. |
| `cargo test --locked -p octacity-server --lib late_writes_from_separate_installations` | The late old operation succeeded (`failed == 0` instead of 1), bypassing the newer installation's order. | Both installations share native-item ordering. A process barrier reproduces the late native write; the newer intent reconciles it before delivery. |
| `cargo test --locked -p octacity-github --test issue_reporting managed_fields_namespace` | Different HTTPS provider instances with coincident numeric container/item/label ids produced overlapping fields. | API instance/container/item namespace separates unrelated fields; existing rename/recreation overlap remains covered. |
| `cargo test --locked -p octacity-server --lib migration_bootstraps_retained -- --ignored` with the disposable PostgreSQL URL | After switching the technical key, the old migration key left retained accepted history unable to deliver (0 instead of 1). | Migration and live publication use the same length-framed native-item digest; subsequent acceptance continues the seeded sequence. |
| `cargo test --locked -p octacity-server --lib migration_ignores_status_actions -- --ignored` with the disposable PostgreSQL URL | An unrelated configured status adapter became an unknown newer intent and blocked the valid original Reporter (0 deliveries instead of 1). | Migration resolves the exact accepted node and includes only the retained source Reporter's capability. Both Runs keep their accepted Flow history. |
| `cargo test --locked -p octacity-github --test issue_reporting a_single_change_in_a_large_managed_group` | Two read-only plans already required 134 requests, exceeding the test ceiling of 8. | The full inspect/inspect/apply/inspect cycle uses 19 requests for one change in a 64-label group, under the ceiling of 20 (formerly 271). One native mutation preserves the human-owned label. |

Typed Conflict, Permanent, InvalidRequest and Unsupported responses distinguish a
completed failure from an unfinished remote request. Missing/invalid responses,
Transient and Cancelled completion remain uncertain; existing loss, expiry and
late-write contracts continue to cover those hazards. Retry classification,
original deadlines and accepted Flow authority are unchanged.

The projection bucket is Project plus opaque native container/item ids, shared
across installations. Adapter-owned field digests additionally namespace the
canonical API instance. Unknown newer metadata conservatively blocks an older
operation even for coincident opaque ids; known disjoint fields are independent.
Admission and source correlation keep their original installation scope.

GitHub v2 reads one bounded 100-label catalog page per invocation and uses exact
name lookup for selected labels absent from that page. Existing 128-label scope
coverage exercises that fallback. There is no cross-invocation cache, new package,
arbitrary pagination or relaxation of fresh Issue/conditional-step checks.

## Refactoring without behavior changes

Before extracting the common accepted-action proof, the existing
`cargo test --locked -p octacity-server-application --lib factory_flow_action_tests`
baseline passed all 18 tests. Application dispatch and store ordering now share
one immutable provenance check for records, attempts, nested definitions,
capability, receipt and permissions. Live claim/deadline/retry checks remain with
the dispatcher. The unused duplicate claim-end argument was removed. The same
18 tests passed after the refactor.

## Regression verification

```sh
cargo test --locked -p octacity-server --lib runtime
# 131 passed; 6 opt-in PostgreSQL cases run separately
cargo test --locked -p octacity-server-application --lib factory_flow_action_tests
# 18 passed
cargo test --locked -p octacity-server-application --lib factory_admission_tests
# 60 passed, including manual intake and provider-neutral source admission
cargo test --locked -p octacity-server-store --features test-support --lib factory_run_testing
# 20 passed
cargo test --locked -p octacity-github -p octacity-server-work-source -p octacity-work-source-protocol -p octacity-delivery-protocol
# 101 passed
OCTACITY_POSTGRES_URL=postgres://postgres:reporter-fixture-only@127.0.0.1:32779/postgres cargo test --locked -p octacity-server --lib reporting::ordering::postgres -- --ignored --test-threads=1
# 6 passed
OCTACITY_POSTGRES_URL=postgres://postgres:reporter-fixture-only@127.0.0.1:32779/postgres cargo test --locked -p octacity-server-store-postgres --test factory_discovery source_migration_preserves_existing_manual_admission_and_its_replay -- --ignored
# 1 passed
cargo clippy --locked -p octacity-github -p octacity-server -p octacity-server-application --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
python3 tools/check_architecture.py
openspec validate add-github-work-source --strict
git diff --check
```

All listed final commands passed: 337 behavioral tests, Clippy with warnings
denied, formatting, architecture, strict OpenSpec and whitespace checks.
The first PostgreSQL run, parallel with rebuilds and other database tests, passed
4/6: two late-write assertions received a nonterminal retry instead of the
expected terminal old-operation failure. These journeys have finite wall-time
windows. The final isolated run uses `--test-threads=1` and passed all six; no
production deadline or assertion was relaxed. The subsequent stability fix is
recorded below; the serial execution requirement is removed.

Architecture validation passes with 63 packages and 252 dependency edges.
Documentation has no executable behavior requiring a separate TDD cycle.

## Test environment and limits

PostgreSQL uses only the task-owned `postgres:18.6` container
`dc6454b7a86f9342ebe2ec3b346cfe7b338d2beab7731e834be9dba2d34f3e4a`, name
`octacity-reporter-review-fixes`, label `octacity.task=reporter-review-fixes`,
loopback port 32779. Tests create private UUID databases. Migration 0063 belongs
to this uncommitted, unreleased change; it was edited before release and tested
by the public migration runner over genuine retained history in those private
databases. No deployed migration or user database was modified. After the final checks,
the exact container id/name/ownership label was revalidated and the container
with its owned volume was removed.

Native GitHub behavior uses controlled HTTPS/process peers; other providers use
hermetic protocol peers. These checks do not claim live GitLab/Jira integrations
or live account qualification. No credentials or user services are required, and
no commit is created.


## Parallel PostgreSQL stability follow-up

The two late-write journeys held the old native Apply at a process barrier while
publishing the next input and performing its complete source preparation and
Work admission. That unrelated setup shared the old action's finite wall-time
window. Slow admission could correctly time out the old call, producing Retry
rather than the terminal supersession the test expected. This was a fixture
scheduling problem, not a missing production timeout or a store ordering change.

The new opt-in public-boundary regression
`postgres_slow_admission_does_not_consume_the_late_write_window` installs a
private-database trigger delaying Work admission by 5.1 seconds, longer than the
unchanged 5-second action budget. It still drives the real source worker,
PostgreSQL store, node runner, reporting outbox and verified provider process.
There is no live provider or shared stand dependency.

```sh
OCTACITY_POSTGRES_URL=<owned-disposable-url> cargo test --locked -p octacity-server --lib postgres_slow_admission -- --ignored
```

Red was observed twice before the fixture fix: the old dispatch had
`claimed=1, delivered=0, retrying=1, failed=0`, so the existing `failed == 1`
assertion failed. Temporary diagnostic timings showed the old Apply ready after
0.46 seconds, but the next status action queued only after 6.33 seconds, with the
old worker already finished. Diagnostics were then removed. Green passed with
the same delay and unchanged assertions.

The shared test helper now exposes its two existing phases: admit Work, then
accept its configured status action. The late-write scenario admits both Works
before starting the race. Only the new status action is accepted after the old
Apply reaches its barrier, preserving the required acceptance order. The first
new write still precedes release of the old write; the new operation must stay
unfinished while the old hazard exists and must restore the new value afterward.
Fence-loss, separate-installation, unrelated-label and unchanged-Flow assertions
are retained. No timeout, budget, retry policy, production code, dependency or
migration changed, and no global test lock or forced serialization was added.

Follow-up PostgreSQL service: task-owned `postgres:18.6` container
`9fffbfef46f8991d96444459afc3fbc189fe3fe0bb002c839a52d333e6b71fd4`, name
`octacity-reporter-parallel-tests`, label `octacity.task=reporter-parallel-tests`,
loopback port 32780. Every test uses its own UUID database. Initial ordinary
parallel runs passed. The deterministic Red/Green regression and first full
parallel Green also ran with the service limited to 0.5 CPU, confirmed by Docker
inspection (`NanoCpus=500000000`). The controlled admission delay supplies the
reproducible Red without relying on incidental host load.

An additional overload probe ran two complete seven-test binaries concurrently
on that half CPU: 3 of 4 suites failed, including the unrelated migration test
with its 2-second hosted operation window. Late-write calls could still exhaust
their real I/O budgets in that condition. At that stage this was a qualification
limit; the deterministic-time correction below supersedes it without retries,
larger deadlines or relaxed assertions. The fix
removes unrelated admission setup from the race; it does not disable production
timeouts under arbitrary database starvation.

Final repetition uses the service's normal resources: the Docker engine reports
10 CPUs, and the task container's limit was set to those 10 CPUs (verified as
`NanoCpus=10000000000`). User services and databases are untouched.

Follow-up final checks passed:

```sh
cargo test --locked -p octacity-server --lib runtime
# 131 passed; 7 opt-in PostgreSQL cases run separately
OCTACITY_POSTGRES_URL=<owned-disposable-url> cargo test --locked -p octacity-server --lib reporting::ordering::postgres -- --ignored --test-threads=7
# 7 passed, including slow admission; no serial test execution
cargo clippy --locked -p octacity-server --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
python3 tools/check_architecture.py
openspec validate add-github-work-source --strict
git diff --check
```

The same seven-test PostgreSQL command was then repeated four times, with two
binaries running concurrently against the task service and each test using its
own database: **28/28 passed**, suite durations 14.46, 14.51, 15.18 and 15.13
seconds. The injected 5.1-second admission regression passed in every suite.
The 131 runtime tests retain manual intake, provider-neutral peers, real timeout,
lost-response, hazard-expiry and late-write coverage. Changes are limited to
three test files and this verification document. No production behavior or
OpenSpec task progress changed. Documentation needs no executable TDD cycle.

After checking the exact container id/name/task label again, the task-owned
container and its owned volume were removed. No commit was requested or created.


## Deterministic PostgreSQL time under load

The remaining source of nondeterminism was real I/O consuming Tokio's operation
windows. The previous admission-only fixture correction shortened the race but
could not prevent deadline expiry or extra hazard-poll observations under CPU
starvation. This follow-up controls test time while keeping real PostgreSQL and
content-verified provider processes.

### Red / Green

A new public-boundary regression,
`postgres_slow_confirmation_does_not_spend_the_projection_budget`, holds the
native Apply response at the existing process barrier. It then runs a real
`SELECT pg_sleep(2.1)` before releasing confirmation. The unchanged hosted
operation window is 2 seconds and the original action budget is 5 seconds.

```sh
OCTACITY_POSTGRES_URL=<owned-disposable-url> cargo test --locked -p octacity-server --lib postgres_slow_confirmation -- --ignored
```

Before the controller, the test failed twice: delivered was **0 instead of 1**;
the same reporting operation went to Retry while waiting for the barrier.
An earlier connection-refused setup run used the wrong disposable port and is
not counted as Red. After introducing controlled time, the unchanged delivery,
single native write and unchanged accepted Flow assertions passed with the same
2.1-second database delay.

The test also characterizes the negative case: after the same real delay, Tokio
elapsed time is exactly zero and the worker is unfinished. Advancing the test
clock by the original 2-second window, then releasing the response, yields
`delivered=0, retrying=1, failed=0`. Without that explicit advance it yields
`delivered=1, retrying=0, failed=0`. The timeout remains active; only the test
controls when its clock advances.

### Small test-only controller

All eight PostgreSQL tests use their own `start_paused` runtime. The shared
`controlled_io` helper uses Tokio's documented `spawn_blocking` inhibition of
automatic time advancement, with `Handle::block_on` to drive real asynchronous
I/O. This uses a sleeping blocking executor thread while awaiting I/O, without
a spin loop, a global lock, shared clock, database mock or serialization. It
requires only the already-enabled Tokio `test-util` feature.

Existing provider barriers use a shared blocking file observer with the
unchanged real 30-second `TEST_WAIT` watchdog. Its real polling is separate from
Tokio business time. In the late-write race the newer action is still accepted
after the old Begin, the new native write still precedes the old write release,
and delivery stays blocked by the old hazard. After old Finish commits, the test
advances the existing hazard-poll interval by exactly 10 milliseconds once.
Host/process scheduling therefore consumes neither elapsed time nor extra
hazard-poll output budget. Original fence-loss, separate-installation, native
write-count, unrelated-label and unchanged-Flow assertions remain intact.
The two memory tests sharing this race use the same controller; unrelated real
timeout, cancellation and expiry tests retain their existing execution.

Only three test files and this verification document change. A content-hash
comparison of 926 existing production/configuration files against the start of
this follow-up shows no changes. No timeout value, production branch, migration,
manifest, dependency, OpenSpec task progress or manual Factory path was changed.

The task-owned service is `postgres:18.6`, container
`e972319872de942c5fd5518e6e77aa4fc9e11bf8df3d05648295811f344f75f6`, name
`octacity-reporter-deterministic`, label `octacity.task=reporter-deterministic`,
loopback port 32781. Its **0.5 CPU limit remains in place throughout**; test
cases create independent UUID databases. No user service is involved.


### Final verification of deterministic time

```sh
cargo test --locked -p octacity-server --lib runtime
# 131 passed; all 8 opt-in PostgreSQL tests run separately
OCTACITY_POSTGRES_URL=<owned-disposable-url> cargo test --locked -p octacity-server --lib reporting::ordering::postgres -- --ignored --test-threads=8
# 8 passed with the task PostgreSQL service limited to 0.5 CPU
cargo clippy --locked -p octacity-server --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
python3 tools/check_architecture.py
openspec validate add-github-work-source --strict
git diff --check
```

The PostgreSQL command was repeated **four times with two binaries concurrently**:
16 test cases may execute at once against the same half-CPU service, with separate
UUID databases and independent controlled clocks. **32/32 passed**, with suite
durations **56.97, 56.91, 60.33 and 60.48 seconds**. The unchanged 2- and 5-second
operation/action windows therefore do not depend on finishing real database
work within those wall-time windows. Each suite includes slow admission, held
confirmation with real SQL delay, explicit virtual deadline expiry, late write
repair, claim loss, separate installations and legacy migration compatibility.

Clippy, formatting, architecture (63 packages, 252 dependency edges), strict
OpenSpec and diff checks passed. No debug instrumentation remains. Documentation
has no executable behavior requiring a separate TDD cycle. No commit was created.

After revalidating the exact id/name/task label and unchanged 0.5 CPU limit, the
owned test container and volume were removed. No user stand or database changed.
