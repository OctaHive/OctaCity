# Source process fixture timing correction

Only the Source test module and its controlled Python process peer change.
Production worker settings, deadlines, admission, manual Factory input and
provider-neutral integration contracts are unchanged.

## Reproduction and Red/Green

The CI command runs tests in parallel. Earlier qualification used two test
threads, which did not expose the narrow real-time assumptions in these fixtures.
The full server library with `--test-threads=64` reproduced Source failures:
zero completed inputs, missing retained replay and the supervisor watchdog.
Restricting that run to Source tests reduced contention and passed, pointing to
scheduling-sensitive fixture timing rather than shared Project/input state.

Ranked hypotheses were process startup/deadline contention, fixed-delay races,
and shared fixture files. Each test owns its temporary directory and writable
files; only the immutable executable is hard-linked. The following controlled
tests established the two relevant timing failures:

- `admission_contract_fixtures_tolerate_slow_valid_provider_responses`: a valid
  page delayed by 750 ms failed with `(completed, deferred) = (0, 1)` and the
  retained safe classification `Timeout`. The original process allowance was
  only 500 ms. Green admits one Run using the ordinary bounded fixture policy.
  Ordinary process operations now have 10 seconds inside a 60-second claim;
  cancellation has one second, and test watchdogs have 30 seconds. These are
  test settings, not production defaults or a performance contract.
- `disabling_an_input_in_flight_revokes_preparation_and_admission`: the revised
  driver first failed because the provider completed admission before reaching
  the requested revocation barrier. Green uses the peer's `pause_scan` mode:
  it emits readiness, waits for an explicit release file, then returns the
  original candidate after the input has been disabled. Group publication uses
  the same barrier. The concurrency test waits for both process-start records
  before releasing either page, replacing a 150-ms overlap assumption.
- The VCS deadline test now explicitly observes the injected resolver being
  reached after the original intent is frozen. Only then does it advance Tokio
  time past the claim deadline, preserving both timeout and retained-intent
  assertions without making Python startup part of a 1500-ms race. The stalled
  provider test similarly waits for the real peer before advancing the process
  deadline; real time is resumed before waiting for OS child exit/reaping.
  Shutdown still verifies cancellation delivery and zero new admission.

## Commands and results

All Cargo commands use the task-owned target directory
`/tmp/octacity-source-35-build.PRI4PX`, `CARGO_INCREMENTAL=0`,
`CARGO_PROFILE_DEV_DEBUG=0`, and `CARGO_PROFILE_TEST_DEBUG=0`.

```sh
cargo test --locked --jobs 2 -p octacity-server --lib \
  admission_contract_fixtures_tolerate_slow_valid_provider_responses -- --nocapture
cargo test --locked --jobs 2 -p octacity-server --lib \
  disabling_an_input_in_flight_revokes_preparation_and_admission -- --nocapture
cargo test --locked --jobs 2 -p octacity-server --lib
cargo test --locked --jobs 2 -p octacity-server --lib -- --test-threads=64
cargo test --locked --jobs 2 -p octacity-server --lib runtime::work_source_tests -- --test-threads=128
cargo clippy --locked --jobs 2 -p octacity-server --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
python3 tools/check_architecture.py
git diff --check
```

Green: all 110 server tests with default parallelism and twice with 64 test
threads. All 23 Source tests pass with 64 threads and in three consecutive
128-thread runs. The six Source cases reported by CI are included. Python
fixture syntax was checked with `ast.parse` without creating cache files.
Clippy across all server targets/features with warnings denied, Rust formatting,
the architecture check (63 packages, 252 edges), and whitespace checks pass.

A full-library 128-thread stress run additionally failed the unchanged
`factory_worker_errors_emit_only_safe_classification_and_never_provider_text`
test: its capture lacked `durable worker pass failed`. All Source tests in that
run passed. That separate log-capture instability is recorded rather than
silently counted as a passing full stress run; this correction does not modify
the worker logging test or production logging.

The subsequent [worker log-capture correction](verification-worker-log-capture.md)
resolves that remaining failure through a separate Red/Green cycle. Full-library
128-thread verification now passes in three consecutive runs.
