# Task 1.2: baseline verification

## Scope and method

Baseline checked on 2026-10-27 against commit `69f0502f`, on macOS arm64.
This task runs existing behavior before triage implementation. After the first
baseline failure, a controlled overlap reproduces and corrects a request-count
race in the existing HTTP test. The test correction follows Red/Green below; no
production behavior, dependency, configuration or migration is changed. Existing
baseline successes are regression evidence, not a new TDD Green. The initial
failure and its isolated successful rerun remain recorded.

Use the [dependency baseline](dependency-baseline.md) for implementation owners
and later execution prerequisites. Its running-stand observations belong to task
1.1; the current environment observation below supersedes their availability for
this run without changing those historical facts.

## Test results

Rust commands use `CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2` to bound build output and parallelism.

| Command / boundary | Result |
| --- | --- |
| `cargo test -p octacity-server-factory -p octacity-server-application -p octacity-server-api-rest -p octacity-server --lib -j2` | Factory: 300 passed; application: 768 passed; REST unit: 13 passed; server: 213 passed, 47 explicitly ignored pending their environment run below. No failures. |
| `cargo test -p octacity-server-factory --test role_profiles` | 42 passed. Role/Profile publication, templates, compatibility, immutable identity and scope contracts. |
| `cargo test -p octacity-server-application --test manual_trigger --test factory_artifact_reader --test factory_observability` | 42 passed across the three targets. |
| `cargo test -p octacity-server-api-rest --tests` | 249 passed across four targets, including the 13 unit tests above. Covers Source management/preview, Issue actions, integration administration, contracts and handlers. |
| `cargo test -p octacity-github --tests` | 125 passed across library, binary and adapter contracts; fixture HTTP only. Includes initial/incremental reads, predicate selection, retained labels and native effects. |
| `cargo test -p octacity-server --lib -- --ignored --test-threads=2` | Initial run on real disposable PostgreSQL/MinIO: 46 passed, 1 failed, no ignored tests. The preview/import request-count failure is detailed below. |
| `cargo test -p octacity-server --lib runtime::integration_management_tests::runtime_publishes_connection_credentials_and_project_access_without_enabling_inputs -- --ignored --exact --nocapture` | Isolated rerun: 1 passed, 259 filtered. No implementation or assertion changes between runs. |
| `cargo test -p octacity-server-store-postgres --test factory_role_profiles --test factory_runtime_settings --test factory_flow_management --test factory_discovery --test issue_execution --test issues --test integration_project_access --test integration_connections --test integration_administration -- --ignored --test-threads=2` | 156 passed across nine targets against real disposable PostgreSQL. One non-ignored test is filtered by this explicitly ignored-only invocation. |
| `cargo test -p octacity-artifact-s3 --test minio -- --ignored --test-threads=1` | 2 passed against real disposable MinIO, including trusted input publication, exact bytes, integrity, expiration, outage/recovery and deletion. |
| `python3 -m unittest tools.tests.test_factory_local_runtime tools.tests.test_factory_local_stand tools.tests.test_factory_source_installation tools.tests.test_local_stand_launcher tools.tests.test_configure_local_stand_agent` | 76 collected: 72 passed, 4 opt-in checks skipped. Qualified Runtime/login and real stand pilot/bootstrap skips are not execution evidence. |
| `pnpm run test:unit --maxWorkers=2` with Node 24.21.0/pnpm 12.8.1 | 36 structural/schema tests and 836 Vitest tests across 106 files passed. |
| `pnpm exec playwright test tests/browser/factory.spec.ts tests/browser/factory-inputs.spec.ts tests/browser/factory-profiles.spec.ts tests/browser/factory-actions.spec.ts tests/browser/factory-issue-status.spec.ts tests/browser/project-sources.spec.ts tests/browser/issue-sources.spec.ts tests/browser/integrations.spec.ts` with the same pinned versions | 35 passed, none skipped. Includes keyboard/narrow-screen Source setup and English/Russian local status controls. These are browser fixture contracts, not real backend or Agent execution. |

The library baseline includes common Source predicate/mapping/preparation/replay,
Issue status/Reporter authority and recovery, generic Flow/template/Role/Profile
and model-discovery startup regressions. Test output is retained locally under
`/tmp/octacity-triage-1-2-*.log`; the tables preserve durable counts and limitations
without requiring those temporary logs to remain available.


## Red/Green and final checks

The public HTTP test command is the exact integration scenario listed in the
table, using the same disposable services throughout:

- **Red:** extend the existing test to wait for real retained-link polling during
  its preview sequence, leaving the original global count unchanged. The command
  fails with 7 versus 4 calls. A second diagnostic run fails identically and
  confirms that every extra scan belongs to retained-Issue pull.
- **Green:** scope the exact count to `preview` operations. The same extended test
  passes: 1 passed, 259 filtered (57.92 seconds). No production code was changed.
- **Full regression:** `cargo test -p octacity-server --lib -- --include-ignored --test-threads=2` passes: 260 passed, 0 failed, 0 ignored (219.65 seconds), including the corrected original HTTP journey and all 47 environment-gated contracts.
- `cargo clippy -p octacity-server --lib --tests -j2 -- -D warnings` passes.
- `rustfmt --check server/app/src/runtime/source_management_tests.rs` and
  `git diff --check` pass.

## Environment and qualification limits

- Task-owned PostgreSQL 18.6 and the cached MinIO release
  `octacity/minio:release-2025-10-15-9e49d5e7a648` are bound only to dynamically
  allocated loopback ports. PostgreSQL answers `pg_isready`; MinIO answers its
  readiness endpoint with HTTP 200. The integration contracts use only their
  disposable databases/buckets and public test credentials. No personal stand
  database, stored input or provider credential is reused.
- The shared runtime fixtures require the `octacity-artifacts` bucket and their
  existing `octacity`/`octacity-secret` fixture credentials. That bucket was
  provisioned in the disposable MinIO before the real runtime checks. The S3
  contract independently creates, verifies and deletes its own objects/buckets.
- The default personal stand root and CA certificate are currently absent. Its
  HTTPS readiness probe cannot connect, and no Agent or Microsandbox process is
  running. Consequently there is **no current qualified-Agent response or real
  VM/model/tool execution evidence** from 1.2. Do not reuse 1.1's online ordinary
  Agent observation as current evidence or interpret it as Factory qualification.
- Restore the executable Runtime/Agent/MCP installation through
  `enable-local-stand-runtime-mcp` and verify exact capabilities before tasks
  4.3/8.x claim execution. Installing metadata alone cannot supply that evidence.
  No paid model, login snapshot, remote GitHub write, admission or replacement
  personal stand was used by this baseline.
- Initially available Node 22.17.0/pnpm 9.12.1 ran the UI checks successfully, but
  those results alone do not satisfy the pinned-toolchain baseline. Node 24.21.0
  was restored in a task-owned temporary directory from its official archive
  after SHA-256 verification, and the existing pnpm 12.8.1 executable was selected
  explicitly. `pnpm install --frozen-lockfile --ignore-scripts` passed supply-chain
  checks and reported the lockfile/modules already up to date. The suites were
  rerun with those versions; no dependency or lockfile change was made.

## Observed failure and follow-up boundary

The first real PostgreSQL-backed server run reports one failure:

```text
runtime::integration_management_tests::
  runtime_publishes_connection_credentials_and_project_access_without_enabling_inputs
server/app/src/runtime/source_management_tests.rs:250
provider calls during preview: expected 4, observed 5
```

The unmodified test, rerun alone against the same disposable PostgreSQL/MinIO
services, passes (1 passed, 259 filtered). That isolated success is not the fix.

The confirmed cause is independent retained-Issue pull, which remains enabled
even when Project import is disabled. The assertion counted all provider requests
and required all of them to be `preview`, although the same real runtime also
performs legitimate `scan` requests. Slow or differently interleaved runs expose
that invalid assumption.

The test now deliberately waits for a real `scan` between its preview calls,
using a bounded condition rather than a fixed delay. Before changing the count,
the same HTTP command fails twice in about 23 seconds with exactly four `preview`
and three `scan` operations (observed total 7 versus expected 4). Temporary targeted
diagnostics match all three scan IDs to retained-Issue pull journal entries;
there are no duplicate preview calls. This falsifies a preview retry explanation
and identifies the independent consumer as the extra-call owner. All diagnostic
logging and temporary database-inspection plumbing were removed afterward.

The minimal correction counts exactly the four `preview` operations. The controlled
overlap remains in the test, and the existing unchanged-Issue, empty admission,
no published-input history after validation and no Factory Run checks remain.
No background consumer is disabled, no timeout/retry budget is enlarged, and the
count is not relaxed to an inequality.

The macOS server test binary also produces a linker warning that its `__eh_frame`
section exceeds compact-unwind encoding bounds. Compilation succeeds. No linker
flags or safety checks were changed to hide it.

## Group 1 review and next work

Group 1 adds the dependency inventory, this evidence and a correction to one
existing test helper; it introduces no runtime owner, preset registry, production
business constants, schema, migration or duplicated implementation. Shared Source/Reporter, template and Context ownership remains as
recorded in the design. Neighboring changes and their completion states are
unchanged. The recorded environment and qualification limitations must remain visible
when planning execution qualification; pure predicate work in group 2 can proceed
independently through its own Red/Green cycles.

## Resource use and completion

Free disk space was checked before builds (about 21 GiB), during compilation and
regressions (minimum observed about 15 GiB), and after them (about 16 GiB before
removing task-owned services). The 10 GB cleanup threshold was never crossed.
Disposable PostgreSQL/MinIO containers and their anonymous volumes were removed
after successful regressions, as were the temporary Node archive/toolchain and
pnpm archive. Test logs remain in `/tmp`; personal stand data and tagged images
were preserved. No commit was created.

`openspec validate add-factory-triage-pipeline --strict`, local Markdown target
validation and the final diff review pass. Task 1.2 completes baseline verification
and its discovered test-race correction; qualified-Agent execution remains an
explicit later prerequisite, not an implied successful check.
