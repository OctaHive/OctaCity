# Source review corrections

Scope: the four findings from the review of tasks 3.1–3.5. Tasks 4–6 remain open.
No provider API, paid model, real repository or user service is involved.

## Behavioral boundaries and Red/Green evidence

Tests use the public `FactorySourceWorker::run_once` boundary and the shared
`FactorySourceStore` contract, with the existing immutable configuration,
admission, Artifact and verified-process fixtures. No private worker methods are
tested. The existing cross-store source contract passed before SQL extraction.

- `disabled_members_pending_work_does_not_block_the_current_group`: Red returned
  `(completed, deferred) = (0, 1)` instead of `(1, 0)`. Green processes the enabled
  member while leaving the disabled member's original snapshot unchanged and
  unpublished. The shared memory/PostgreSQL contract additionally covers removal
  from a group, rejects the old owner, and permits checkpointing without deleting
  retired private preparations. Its PostgreSQL Red retained the disabled pending
  member in the current queue.
- `pending_work_follows_its_stable_input_into_an_exclusive_group`: Red completed
  without recovering any Run. Green recovers one original Run for both raw and
  already-mapped preparations even when the provider returns no candidates.
  Snapshot, input, original scan provenance and frozen intent are preserved;
  prepared bytes are published once. The cross-store version first returned an
  empty PostgreSQL pending queue. It also verifies pending work still blocks
  uncovered checkpoints and denies old standalone owners and unrelated groups.
- `source_attempt_budget_is_independent_of_the_supervisor_circuit_threshold`:
  the new explicit setting first failed to compile because the API was missing.
  After adding the field, behavioral Red exhausted a Source on the first failure
  despite its four-attempt budget. Green exhausts exactly after four failures for
  circuit thresholds of both one and six, including worker restarts. The existing
  installation validation test separately went Red when zero attempts were
  accepted; Green rejects zero before provider activity.
- Duplicated PostgreSQL member resolution is a behavior-preserving extraction,
  covered by the existing cross-store selection, ownership and fencing contracts.
  Documentation changes have no executable behavior and are checked with the
  repository's formatting, rustdoc and specification validation checks.

For the first worker regression, an incidental assertion originally assumed three
Artifact publications. The established mapping contract publishes two documents;
the assertion now checks that no disabled snapshot is published. The required
group-progress assertion established Red before the implementation change.

## Commands

All Cargo commands use `CARGO_TARGET_DIR=/tmp/octacity-source-35-build.PRI4PX`,
`CARGO_INCREMENTAL=0`, `CARGO_PROFILE_DEV_DEBUG=0`, and
`CARGO_PROFILE_TEST_DEBUG=0`. Cargo builds run sequentially.

```sh
# Each worker regression was run alone for its Red/Green cycle.
cargo test --locked --jobs 2 -p octacity-server --lib \
  disabled_members_pending_work_does_not_block_the_current_group -- --nocapture
cargo test --locked --jobs 2 -p octacity-server --lib \
  pending_work_follows_its_stable_input_into_an_exclusive_group -- --nocapture
cargo test --locked --jobs 2 -p octacity-server --lib \
  source_attempt_budget_is_independent_of_the_supervisor_circuit_threshold -- --nocapture
cargo test --locked --jobs 2 -p octacity-server --lib \
  source_installations_reject_unbounded_workers_and_ambiguous_or_foreign_profiles_before_activity -- --nocapture
# Shared source behavior in memory and a real, owned PostgreSQL fixture:
cargo test --locked --jobs 2 -p octacity-server-store-postgres --test factory_discovery \
  source_publication_is_atomic_owned_and_retains_exact_versions_in_both_stores -- --ignored --nocapture
```

The PostgreSQL fixture is a new `postgres:18.6` container bound only to loopback,
labelled `octacity.task=source-review-fixes`; `OCTACITY_POSTGRES_URL` selects it.
Each passing contract removes its own database. Fixture container and volume
cleanup was limited to that exact owned container after verification; both were
removed. The correction adds no migration, dependency or provider-specific type.

## Regression verification

```sh
cargo test --locked --jobs 2 -p octacity-server --lib -- --test-threads=2
cargo test --locked --jobs 2 -p octacity-server-application --lib factory_admission_tests -- --test-threads=2
cargo test --locked --jobs 2 -p octacity-server-application --test manual_trigger --test factory_artifact_reader -- --test-threads=2
cargo test --locked --jobs 2 -p octacity-server-store-postgres --test migrations --test factory_discovery -- --ignored --test-threads=2
cargo test --locked --jobs 2 -p octacity-server-store --all-features
cargo clippy --locked --jobs 2 -p octacity-server -p octacity-server-application \
  -p octacity-server-store -p octacity-server-store-postgres --all-targets --all-features -- -D warnings
RUSTDOCFLAGS='-D warnings' cargo doc --locked --jobs 2 -p octacity-server \
  -p octacity-server-store -p octacity-server-store-postgres --all-features --no-deps
cargo fmt --all -- --check
python3 tools/check_architecture.py
openspec validate add-github-work-source --strict
git diff --check
```

308 tests and one compile-fail doctest pass: 109 server tests, 60 Factory admission
tests, 34 manual trigger tests, 5 Artifact reader tests, 71 core store tests,
23 PostgreSQL Factory contracts and 6 migration tests. Clippy across all targets
and features and Rustdoc pass with warnings denied. Rust formatting, architecture
(63 packages, 252 edges), strict OpenSpec validation and whitespace checks pass.
Manual Factory input and ordinary CI/CD remain independent of Source activation;
provider-neutral worker contracts still cover GitHub, GitLab, Jira and a custom
tracker using hermetic process peers. No live provider qualification is claimed.
No commit was requested or created.
