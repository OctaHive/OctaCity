# Worker log capture: TDD verification

## Behavior and diagnosis

`factory_worker_errors_emit_only_safe_classification_and_never_provider_text`
must capture the actual worker failure event, retain its safe `unavailable`
classification, and exclude every private error substring. The check must work
when Tokio schedules the worker on another runtime thread and when unrelated
tests run concurrently.

The original full server library run with 128 test threads reproduced the
missing-event assertion: 109 passed and one failed. Running only the six worker
tests with the same parallelism also failed. The logging test passed alone,
but failed when paired with either backoff/cooldown test. Tracing caches callsite
interest across the process, while the test installed only a thread-local
subscriber; neighboring workers could initialize the shared callsite without
that subscriber.

## Red and Green

Before changing capture setup, the test was extended to use a two-thread Tokio
runtime. This makes cross-thread worker scheduling part of the existing public
logging check. The targeted command below failed with `captured 0 log bytes`:

```sh
cargo test --locked --jobs 2 -p octacity-server --lib \
  factory_worker_errors_emit_only_safe_classification_and_never_provider_text -- --nocapture
```

Green: run this check in an isolated invocation of the test executable and
install a global subscriber there, matching server logging setup. The process
is bounded by a 30-second timeout with kill-on-drop. Other tests retain their
tracing state and can continue running concurrently. The original positive
event/classification assertions and all private-substring assertions remain.
The targeted test then passed, followed by all six worker tests with 128 test
threads. No production code, dependencies, manual Factory admission, or provider
interfaces changed.

## Regression checks

All Cargo commands use `CARGO_TARGET_DIR=/tmp/octacity-source-35-build.PRI4PX`,
`CARGO_INCREMENTAL=0`, `CARGO_PROFILE_DEV_DEBUG=0`, and
`CARGO_PROFILE_TEST_DEBUG=0`.

```sh
cargo test --locked --jobs 2 -p octacity-server --lib runtime::workers::tests -- --test-threads=128
cargo test --locked --jobs 2 -p octacity-server --lib
cargo test --locked --jobs 2 -p octacity-server --lib -- --test-threads=128
cargo clippy --locked --jobs 2 -p octacity-server --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
python3 tools/check_architecture.py
git diff --check
```

All 110 server tests pass with default parallelism and in three consecutive
128-thread runs. These include the Source regressions from the preceding
fixture correction. Documentation records observed evidence and has no
executable behavior requiring a separate TDD cycle.

Clippy passes across all server targets/features with warnings denied. Rust
formatting, the architecture check (63 packages, 252 dependency edges), and
whitespace checks also pass.
