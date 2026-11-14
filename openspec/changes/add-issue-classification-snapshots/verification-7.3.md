# Task 7.3 — Rust and Console regression qualification

## Scope and environment

The qualification starts from commit `293de8e0` and exercises the existing
Issue, classification, snapshot admission/recovery, REST, Source-worker and
PostgreSQL contracts, plus the affected Console journeys. Qualification exposed
two stale test fixtures, corrected through their existing failing behavioral
contracts below. Production code and contracts are unchanged. Earlier
implementation and review Red/Green evidence remains in the preceding
verification reports, including `verification-7.1-7.2-review-fixes.md`.

All PostgreSQL and object-storage traffic uses new task-owned Docker services,
labelled `octacity.qualification=issue-snapshot-73`, bound to loopback dynamic
ports with tmpfs data. PostgreSQL uses the cached `postgres:18.6` image; MinIO
uses `octacity/minio:release-2025-10-15-9e49d5e7a648`. The required
`octacity-artifacts` bucket is explicitly provisioned. No production stand,
repository, provider connection or credential is used.

Rust uses `rustc 1.99.0` / `cargo 1.99.0`. Console's final checks use the exact
repository-selected Node.js `24.21.0` and pnpm `12.8.1`, existing locked installed
dependencies and the installed Playwright Chromium. The official Darwin ARM64
Node archive was downloaded into `/tmp/octacity-73/toolchain` and checked against
the release's `SHASUMS256.txt`:

`6239d4cf92d864487ec8cd3615038f7b67e7f58b77b21cd2f09ea9fbd68065fe`

The cached pnpm entry point is run through that Node binary; the task-owned
`/tmp/octacity-73/tool-bin/pnpm` wrapper and Node bin directory precede the
global tools in `PATH`. Initial Console checks also passed under global Node
`22.17.0` / pnpm `9.12.1`; those preliminary runs are not counted again as
additional tests or presented as pinned-toolchain qualification.

Rust commands below use:

```sh
export CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0
export CARGO_BUILD_JOBS=4 CARGO_NET_OFFLINE=true
# OCTACITY_POSTGRES_URL and OCTACITY_MINIO_* select only the disposable services.
```

## Observed results

| Suite | Passing cases | Retained log under `/tmp/octacity-73/` |
| --- | ---: | --- |
| Application Issues | 214 | `rust-app-issues.log` |
| Ticket delivery | 19 | `rust-app-ticket-delivery.log` |
| Application admission | 153 | `rust-admission.log` |
| Application managed intake | 27 | `rust-intake-application.log` |
| REST v1 handlers and OpenAPI | 288 | `rust-rest.log` |
| Ordinary server-lib | 238 | `rust-server-lib.log` |
| PostgreSQL Source workers | 66 | `rust-source-postgres.log` |
| Other ignored server runtime cases | 6 | `rust-runtime-postgres.log`, `rust-integration-runtime-green.log` |
| PostgreSQL store, including ignored tests | 315 | `rust-postgres-store.log` |
| PostgreSQL intake | 45 | `rust-intake-postgres-ready.log` |
| Configured-flow contracts | 130 | `rust-configured-flow.log`, `rust-configured-fixtures-green.log` |
| Manual real HTTP/browser pilot | 1 | `rust-pinned-console.log` |
| **Rust target/name pairs** | **1502** | Repeated focused checks are not added again |
| Console generator/architecture tests | 46 | `ui-pinned-unit.log` |
| Console unit tests | 1246 | `ui-pinned-unit.log` |
| Affected Console browser scenarios | 56 | `ui-pinned-browser.log` |

All listed cases were observed passing; this is not a claim that the initial
aggregate commands exited successfully. The broad configured-flow run finished
after 1480.61 seconds with 128 passing cases and the two failures described
below. Both failing cases then passed in the three-case focused run after
environment/fixture correction. The broad 130-case command was not repeated
after the fixture-only correction. Similarly, five of the six extra server
runtime cases passed initially; the integration case passed after its policy
fixture correction. The application filters have disjoint test-name sets
(413 cases), and the 66 Source-worker cases are disjoint from the other six
ignored server runtime cases. All 310 server-lib cases were therefore exercised.
The complete store command executed 315 tests across its test targets with
zero ignored cases; zero-test/doc targets contribute nothing to that count.

The Rust-owned browser pilots were repeated with the pinned Console toolchain:
manual Issue HTTP/browser (1 passed, 10.39 seconds) and
`issue_journal::original_issue_job_events_resume_after_restart_without_private_payloads`
(1 passed, 7.59 seconds). The journal case is already included in the 130
configured-flow cases, and the initial global-toolchain pilot is not counted
again. Commands/durations are retained in `rust-pinned-pilots.json`.

Pinned Console API generation, typecheck, lint, format and unit checks all
exited 0 (`ui-pinned-results.json`); the final 56 browser tests passed in
64.52 seconds (`ui-pinned-browser-result.json`). No generated schema or
authorization-inventory changes remained after regeneration.

## Rust commands

```sh
cargo test --locked --offline -p octacity-server-application --lib issues
cargo test --locked --offline -p octacity-server-application --lib ticket_delivery
cargo test --locked --offline -p octacity-server-api-rest --test v1_handlers
cargo test --locked --offline -p octacity-server --lib
cargo test --locked --offline -p octacity-server --lib \
  runtime::sources::tests::worker -- --ignored --test-threads=2
cargo test --locked --offline -p octacity-server-store-postgres \
  -- --include-ignored --test-threads=4

cargo test --locked --offline -p octacity-server-application --lib factory::admission
cargo test --locked --offline -p octacity-server-application --lib \
  management::tests::project_text_intake_tests
cargo test --locked --offline -p octacity-server --test project_text_intake \
  -- --include-ignored --test-threads=2
cargo test --locked --offline -p octacity-server --test factory_configured_flow \
  -- --include-ignored --test-threads=2
cargo test --locked --offline -p octacity-server --test issue_console_pilot \
  -- --ignored --test-threads=1
```

The six remaining server-lib ignored contracts are selected separately from
the 66 Source-worker tests. `OCTACITY_SOURCE_OBJECT_URL` selects the same
task-owned MinIO endpoint for the integration-publisher contract:

```sh
cargo test --locked --offline -p octacity-server --lib -- --ignored \
  runtime::issues::issue_tests runtime::issues::issue_workflow_tests \
  runtime::sources::tests::source_import_tests \
  runtime::integrations::integration_management_tests --test-threads=2
```

The REST suite includes OpenAPI route/DTO drift and authorization contracts;
regeneration leaves no tracked generated-schema or authorization-inventory
changes. PostgreSQL tests are executed with their ignored cases enabled, rather
than counted as covered by compilation or by a default all-ignored test run.

## Console commands

From `ui`, with the pinned toolchain on `PATH`:

```sh
pnpm api:generate
pnpm typecheck
pnpm lint
pnpm format
pnpm test:unit
pnpm exec playwright test \
  tests/browser/issue-classification.spec.ts \
  tests/browser/issue-properties.spec.ts \
  tests/browser/issue-executions.spec.ts \
  tests/browser/issue-execution-snapshots.spec.ts \
  tests/browser/project-issue-fields.spec.ts \
  tests/browser/issue-documents.spec.ts \
  tests/browser/issue-comment-materials.spec.ts \
  tests/browser/issue-activity.spec.ts \
  tests/browser/issue-sources.spec.ts \
  tests/browser/factory-inputs.spec.ts \
  tests/browser/factory-triage-results.spec.ts \
  tests/browser/factory-ticket-delivery.spec.ts \
  tests/browser/factory-definitions.spec.ts \
  tests/browser/project-documents.spec.ts \
  --output=/tmp/octacity-73/playwright-pinned-results
```

The unit command executes 46 Node generator/architecture tests and 1246 Vitest
tests in 140 files. The 14 selected browser files execute 56 tests, including
EN/RU, keyboard/narrow layouts, Project field administration, applied/stale
classification, exact snapshot review, historical inputs, Issue-owned documents,
shared Project document selection, comment selection and direct producer inputs.
They use controlled API fixtures; the separately listed PostgreSQL HTTP/browser
pilots exercise the real runtime boundary.

## Environment failures and recovery

The first `project_text_intake` invocation passed 43 tests and failed two:
`native_resources_publish_and_read_project_inputs_through_the_shared_mechanism`
returned `Store(Unavailable)`, and
`issue_execution_http_resumes_original_inputs_after_runtime_restart` returned
pending HTTP 202 instead of the expected admitted/replayed HTTP 200. The
configured-flow cleanup contract also failed before bucket provisioning.

An authenticated GET to the task-owned bucket returned `404 NoSuchBucket`.
Provisioning it returned 200 and a subsequent GET returned 200; the complete
intake suite then passed **45/45**, with no implementation or test expectation
changes. This is an environment repair, not a missing-behavior Red or a code fix.
The initial failures remain in `/tmp/octacity-73/rust-intake-postgres.log`;
the successful rerun is `/tmp/octacity-73/rust-intake-postgres-ready.log`.
Bucket diagnostics are in `/tmp/octacity-73/bucket-provisioning.log`.

## Fixture corrections: observed Red/Green

### Source target policies

The full runtime integration test failed again in isolation with no Source
admission. Temporary phase diagnostics established
`/admission_policy: unmapped_legacy_policy` before preparation or S3 publication:
the direct-store target fixture stored legacy `admission`/`permissions` aliases
without a mapping. Existing production admission correctly rejected that target.

The fixture now explicitly initializes the existing Project policy owner and
pins its exact launch/access references in both the target Configuration and
retained draft. It uses the provisioning command, not an assumption that this
test's optional runtime installation already initialized policies, and does not
add a production legacy fallback. The whole existing integration contract then
passes, including installed credentials, independently enabled Source, lost
admission acknowledgement, restart and replay checks.

```sh
cargo test --locked --offline -p octacity-server --lib \
  runtime::integrations::integration_management_tests::runtime_publishes_connection_credentials_and_project_access_without_enabling_inputs \
  -- --ignored --exact --test-threads=1
```

- Red: `/tmp/octacity-73/rust-integration-runtime-isolated.log`, 0 passed,
  1 failed in 38.42 seconds. Phase evidence is in
  `/tmp/octacity-73/rust-integration-runtime-probe.log`.
- Green: `/tmp/octacity-73/rust-integration-runtime-green.log`, 1 passed in
  60.58 seconds. Temporary `[DEBUG-73]` probes were removed; no production
  instrumentation is retained.

### Historical schema writer

The schema-v51 migration contract failed when its setup called today's admission
writer against the old database. The current writer requires newer schema; this
was a fixture error, not a reason to relax persistence or state constraints.

The existing metadata/request preparation is shared by current admission and
the migration case. Only the migration case uses the already available frozen
pre-upgrade admission writer before applying current migrations. The original
public snapshot/restart equality and accepted/rejected technical-state checks
remain intact. Existing current-admission/replay coverage remains unchanged.

```sh
cargo test --locked --offline -p octacity-server --test factory_configured_flow \
  the_unreleased_factory_schema_accepts_only_technical_execution_states \
  -- --ignored --exact --test-threads=1
```

- Red: `/tmp/octacity-73/rust-schema-51-red.log`, 0 passed, 1 failed in
  0.63 seconds at the modern admission write.
- Green: `/tmp/octacity-73/rust-schema-51-green.log`, 1 passed in 0.94 seconds;
  the already compiled current test binary was invoked directly with the same
  exact filter and disposable PostgreSQL URL.
- The schema test, restored S3 cleanup test and current
  `phase_pool_observability_correlates_one_committed_selection_with_audit_and_replay`
  control also passed together: `/tmp/octacity-73/rust-configured-fixtures-green.log`,
  3 passed in 57.93 seconds.

Strict Clippy of the changed targets passed:

```sh
cargo clippy --locked --offline -p octacity-server --lib \
  --test factory_configured_flow -- -D warnings
cargo fmt --all -- --check
```

Logs: `/tmp/octacity-73/clippy.log`, `/tmp/octacity-73/rustfmt-final.log`.
The local Darwin linker emitted its `__eh_frame` compact-unwind size warning
during large test-binary linking; this did not fail execution or the strict
Clippy check.

## Other checks and teardown

```sh
python3 tools/check_architecture.py
openspec validate add-issue-classification-snapshots --strict
git diff --check
```

Architecture passed with 64 workspace packages and 264 dependency edges.
Rust formatting, strict OpenSpec validation and whitespace checks passed.
Only test fixtures and verification/task documentation are changed. Task-owned
PostgreSQL/MinIO containers are removed after checking their qualification labels;
the local evidence logs and verified Node toolchain remain outside tracked code.

## Native qualification boundary

This task exercises real PostgreSQL, MinIO, HTTP and browser boundaries. It does
not claim a new Microsandbox/ordinary-Job/trusted-PR qualification run. The
separately observed tasks 7.1–7.2 packaged journeys and retained PR/snapshot proof
are documented in `verification-7.1.md`, `verification-7.2.md` and
`verification-7.1-7.2-review-fixes.md`. That report explicitly distinguishes the
last malformed-JSON CLI subprocess guard from its preceding native runs.
The full document/comment/material packaged journey remains task 7.5; operator
rollout instructions remain task 7.4.
