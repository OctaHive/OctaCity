# Reporter deadline regression verification

## Correction

Managed Reporter authorization now shares the projection's original absolute
deadline. A stalled store read expires before provider dispatch. Immediately
before executing the verified process, the runtime recomputes the remaining
budget instead of reusing the duration calculated before authorization. Exhausted
authorization is classified as unavailable with no dispatched effect.

The existing process host still owns cancellation and child cleanup. No timeout,
retry policy, dependency, protocol, provider branch or production interface was
added or changed. Manual Factory admission requires no managed connection.

## Test-first evidence

Cargo commands used `--locked`, one build at a time, and:

```sh
export CARGO_INCREMENTAL=0
export CARGO_PROFILE_DEV_DEBUG=0
export CARGO_PROFILE_TEST_DEBUG=0
export CARGO_BUILD_JOBS=2
```

Baseline: `cargo test --locked -p octacity-server --lib managed_reporter_revocation`
passed its existing regression before implementation.

| Command | Observed Red before implementation | Observed Green |
| --- | --- | --- |
| `cargo test --locked -p octacity-server --lib managed_reporter_deadline_stops` | Store authorization remained pending beyond the original deadline | 1 passed after bounding authorization by `timeout_at(end, ...)` |
| `cargo test --locked -p octacity-server --lib managed_reporter_deadline_passes` | After authorization consumed 1.5 seconds, a provider response after the original 2-second window did not enter retry (`retrying` was 0 instead of 1) | Combined `cargo test --locked -p octacity-server --lib reporting::deadline`: 2 passed after recomputing the remaining duration |

Both tests enter through normal Factory admission, action dispatch and the outbox
worker, using managed connection metadata and the verified Reporter process.
Only the existing store port is delayed, after observable protocol progress.
Each test covers GitHub, GitLab, Jira and a custom provider without live accounts.
Tokio time is paused and explicitly advanced; independent per-test blocking
guards prevent automatic time advancement while the real process performs IO.
There is no shared lock, test serialization, new sleep or timeout increase.

The stalled-write case proves that no Apply request is sent after expiry. The
remaining-budget case proves that a late observation cannot qualify delivery.
Both assert that historical Work, Run and Flow remain unchanged.

## Regression and scope

- `cargo test --locked -p octacity-server --lib`: 147 passed; 8 PostgreSQL tests
  ignored because they require a separate service. No database behavior changed;
  PostgreSQL qualification was not rerun or claimed here.
- `cargo test --locked -p octacity-server-application --lib factory_admission_tests`:
  60 passed, including manual intake and provider-neutral admission.
- Ten repeated runs of the built server test binary with `reporting::deadline`:
  20 test executions passed, covering 80 provider scenarios with normal parallel
  test execution.
- `python3 tools/check_architecture.py`: passed, 63 packages and 253 dependency
  edges. `python3 tools/security_matrix.py validate`: passed inventory validation.
- `cargo clippy --locked -p octacity-server --all-targets --all-features -- -D warnings`:
  passed.
- `cargo fmt --all -- --check`, `git diff --check`,
  `openspec validate add-github-work-source --strict` and
  `openspec validate add-factory-work-card --strict`: passed.

The fix stays in the existing Reporter invocation path, with two behavioral
regressions in a child test module. SHA-256 comparison confirms that all earlier
dirty files except the two intentionally extended runtime files remain unchanged.
Completed task entries and the future internal work-card plans are preserved.
Work remains on `main`; no commit was requested or created. Builds retained
approximately 18 GiB free space and a 5.4 GiB `target` directory.

This verification document has no executable behavior; TDD applies to the runtime
fix above, while documentation is validated with the specification checks.
