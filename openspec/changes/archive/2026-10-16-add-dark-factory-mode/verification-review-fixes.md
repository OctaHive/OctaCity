# Review corrections: configured Factory engine

Scope: the confirmed review findings against the uncommitted generic-engine
migration, plus defects detected by the correction review. No commit, staging or
push is part of this work. Flow and node identities remain configuration data.

## Public boundaries and TDD evidence

All Cargo commands below use `CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0`.
PostgreSQL commands use an explicitly disposable PostgreSQL 18 service through
`OCTACITY_POSTGRES_URL`; runtime checks also use a disposable MinIO through
`OCTACITY_MINIO_ENDPOINT`.

| Behavior                                                              | Public test command (after `cargo test`)                                                                                               | Observed Red                                                                                                                 | Observed Green / log prefix in `/tmp`                                                                                                                                                                        |
| --------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Archived ownership cannot disable snapshot or cancellation            | `-p octacity-server --test factory_configured_flow expired_claim_history_cannot_disable_snapshot_or_cancellation -- --ignored`         | 32,769 expired claims exhausted the working snapshot ceiling                                                                 | Complete paged history and current ownership remain readable; cancellation works (`octacity-review-claim-history`)                                                                                           |
| Current ownership belongs to the requested Run                        | `-p octacity-server --test factory_configured_flow a_current_claim_cannot_belong_to_another_run -- --ignored`                          | A foreign canonical claim was accepted through a corrupted current pointer                                                   | The public snapshot rejects it (`octacity-review-claim-binding`)                                                                                                                                             |
| Archive queries have bounded ordered access paths                     | `-p octacity-server-store-postgres --test factory_discovery representative_factory_discovery_plans -- --ignored`                       | Claim page required sorting; Audit page used global PK filtering; exact control proof filtered the archive                   | Run-scoped ordered pagination and exact indexed proof lookup (`octacity-review-archive-plan`, intermediate `octacity-review-audit-plan-red` and `octacity-review-control-proof-plan-red`)                    |
| Management retry cannot resurrect a cancelled managed operation       | `-p octacity-server --test factory_configured_flow ordinary_build_retry_cannot_restart_a_cancelled_factory_operation -- --ignored`     | Retry created a new ordinary Attempt after root cancellation                                                                 | Conflict returned; original Attempt retained (`octacity-review-factory-retry`)                                                                                                                               |
| A DAG cannot multiply a node output ceiling                           | `-p octacity-server --test factory_configured_flow a_multi_job_build_cannot_multiply_the_node_output_budget -- --ignored`              | Two 2,048-byte Jobs admitted under a 3,000-byte node ceiling                                                                 | Entire DAG rejected before dispatch (`octacity-review-multi-job-budget`)                                                                                                                                     |
| Deletion and direct reservation cannot replenish managed output quota | `-p octacity-server --test factory_configured_flow deleting_a_managed_upload_does_not_replenish_its_signed_output_budget -- --ignored` | Deleting an upload allowed another allocation; direct reservation bypassed quota                                             | Both public admission paths reject excess allocation (`octacity-review-managed-output-deletion`, `octacity-review-managed-reservation`)                                                                      |
| Signature validity and execution deadline are independent and exact   | `-p octacity-server-application factory_job_spec_tests --lib` and `-p octacity-protocol --lib`                                         | Missing independent wire deadline; fractional remaining deadline rejected; expired execution deadline accepted by validation | Exact millisecond deadline survives short signing validity and fractional admission (`octacity-review-deadline-wire-green-v2`, `octacity-review-fractional-deadline`, `octacity-review-deadline-validation`) |
| Capture obeys the absolute deadline and observes cancellation         | `-p octacity-job managed_capture_obeys_the_absolute_deadline_and_drains_cancellation --lib`                                            | Slow capture returned success after deadline                                                                                 | Timeout, cancellation and workspace cleanup (`octacity-review-capture-deadline`)                                                                                                                             |
| Identity preparation drains and revokes late credentials              | `-p octacity-job preparation_timeout_drains_and_revokes_a_late_identity_lease --lib`                                                   | Provisioning exceeded deadline without cancellation                                                                          | Timeout, cancellation, late lease revocation, no runner start (`octacity-review-identity-deadline`)                                                                                                          |
| Materialization obeys the same absolute deadline                      | `-p octacity-job --lib`                                                                                                                | Slow materialization escaped the deadline                                                                                    | Timeout, cancellation and no runner start (`octacity-review-materialization-deadline-red`, `octacity-review-agent-preparation-green`, 39 tests)                                                              |

The private Build insertion origin enum is a behavior-preserving refactoring.
The existing PostgreSQL admission/recovery suite passed 17 tests both before and
after it (`octacity-review-origin-baseline.log`, `octacity-review-origin-green.log`).
It removes contradictory/incomplete combinations of origin fields.

The proposed Job ordering defect was not reproduced: the existing public
materialized-job contract deliberately requires increasing identity order.
A characterization test rejects reversed input and compares fresh/replayed
receipts (`octacity-review-receipt-order-characterization.log`). No ordering
implementation change was made, and that rejected-input experiment is not
counted as a bug-fix Red.

Documentation-only corrections need no executable Red/Green cycle. They update
Build causality, Node Attempt terminology, signed deadlines and the actual retry
surface. The observer supports ordinary retry history, but management retry has
no Factory owner authority. `RetryNode` creates a fresh configured attempt; an
owner-authorized ordinary retry producer remains deferred with worker integration.

## Final regression

Results are recorded after the complete correction slice, separately from the
older generic-engine migration verification. Temporary services and compilation
artifacts contain development/test data only. Two independent correction reviews
(Standards and Spec) found no remaining confirmed defect after the claim-binding,
index and documentation corrections.

Library regression: **568 passed**, no failures or ignored tests, in
`/tmp/octacity-review-final-units.log`. Architecture validation passed for 56
packages and 210 edges; all 24 architecture-check tests passed
(`/tmp/octacity-review-final-architecture.log`).

| Final suite                                                                  | Result                                               | Log in `/tmp`                              |
| ---------------------------------------------------------------------------- | ---------------------------------------------------- | ------------------------------------------ |
| Libraries, eleven packages                                                   | 568 passed                                           | `octacity-review-final-units.log`          |
| REST contracts                                                               | 62 passed                                            | `octacity-review-final-rest.log`           |
| Configured Flow, ordinary suite                                              | 15 passed; the 25 service tests run separately below | `octacity-review-final-flow.log`           |
| Application/Factory doctests                                                 | 4 passed                                             | `octacity-review-final-doctests.log`       |
| PostgreSQL discovery, retention, migrations, ordinary store contracts        | 43 passed                                            | `octacity-review-final-postgres.log`       |
| Configured owner, restart, quotas and deadline authority in PostgreSQL/MinIO | 25 passed                                            | `octacity-review-final-owner-postgres.log` |
| Released HTTP Build/Agent pipeline through PostgreSQL/MinIO                  | 1 passed                                             | `octacity-review-final-http-runtime.log`   |

Clippy passed for twelve packages, all targets/features, with warnings denied
(`/tmp/octacity-review-final-clippy.log`). Public API documentation passed for
the same twelve packages and all features with missing documentation denied
(`/tmp/octacity-review-final-public-docs.log`).

The completed Rust behavioral and documentation-test suites total **718 passing
tests**, without failures. UI behavior was not changed by this correction slice;
its prior migration verification remains recorded separately. Formatting-only
Markdown changes do not require TDD.

Commands:

```sh
cargo test -p octacity-server -p octacity-server-application -p octacity-server-factory -p octacity-server-store -p octacity-server-job -p octacity-job -p octacity-changeset -p octacity-protocol -p octacity-observability -p octacity-server-decision-signal -p octacity-server-trigger --lib
cargo test -p octacity-server-api-rest --tests
cargo test -p octacity-server --test factory_configured_flow
cargo test -p octacity-server-application -p octacity-server-factory --doc
cargo test -p octacity-server-store-postgres --test factory_discovery --test factory_retention --test migrations --test build_discovery --test internal_triggers --test store_contract -- --ignored
cargo test -p octacity-server --test factory_configured_flow -- --ignored
cargo test -p octacity-server --test postgres_runtime -- --ignored
cargo clippy -p octacity-server -p octacity-server-application -p octacity-server-factory -p octacity-server-store -p octacity-server-job -p octacity-job -p octacity-changeset -p octacity-protocol -p octacity-observability -p octacity-server-decision-signal -p octacity-server-api-rest -p octacity-server-store-postgres --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D missing_docs" cargo doc -p octacity-server -p octacity-server-application -p octacity-server-factory -p octacity-server-store -p octacity-server-job -p octacity-job -p octacity-changeset -p octacity-protocol -p octacity-observability -p octacity-server-decision-signal -p octacity-server-api-rest -p octacity-server-store-postgres --all-features --no-deps
python3 tools/check_architecture.py
python3 -m unittest discover -s tools/tests -p 'test_check_architecture.py'
openspec validate add-dark-factory-mode --strict
cargo fmt --all --check
git diff --check
```

Final format, whitespace and strict OpenSpec checks passed. HEAD remained
`3bd12f1377594d3baced74b75f5485062da203a1`; the index was untouched.
Compilation cleanup used `cargo clean --profile dev` after approaching the
free-space threshold; 19 GiB remained after validation. Disposable PostgreSQL
and MinIO services were stopped and removed after their contracts completed.
