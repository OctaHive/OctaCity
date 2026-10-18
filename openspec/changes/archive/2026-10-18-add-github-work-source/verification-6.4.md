# Task 6.4 — Final scoped regression gate

## Result and TDD evidence

The implementation gate passed on 2026-10-18 in the existing `main` checkout.
No new production or test behavior was needed in this verification-only task.
Its changes are this evidence and the completed task entry, so the repository's
documentation-only TDD exemption applies. No artificial Red is claimed.

The preceding test-first slices retain their commands and observed outcomes:

- [6.1 composed pilot](verification-6.1.md): Red after successful management,
  admission and reporting because the downstream input saw no projected labels;
  Green after the small opt-in bridge in the hermetic process peer. Public
  integration tests then retained replay, recovery, observations and manual intake.
- [6.3 editable settings](verification-6.3.md): each required settings example
  first failed its public behavioral test when absent, then passed after its
  configuration was added; the 87-test admission regression remained green.
- Earlier contract, UI and observation Red/Green evidence remains in the existing
  `verification-5.*.md` files. None of their completed tasks was reopened or erased.

The final rerun passed **1,412 Rust tests, including seven documentation tests**,
**453 Vitest tests**, **27 Node script tests**, **82 browser tests** and
**52 Python tool tests**. The nine PostgreSQL runtime contracts initially ignored
by the ordinary library invocation were explicitly run and all passed. There are
no unrun ignored tests in the selected PostgreSQL integration targets.

| Gate | Observed result |
| --- | --- |
| Server/runtime library | 168 passed; nine opt-in PostgreSQL tests run separately below. |
| Application library | 477 passed, including manual admission, credential publication/recovery, current grants, input publication, preview, examples and existing execution/release journeys. |
| Factory/store/REST/GitHub/process/VCS/Delivery scope | 676 passed, including two documentation tests. REST contributes 134 tests; native GitHub contributes 134 controlled-peer tests. |
| Secrets/adapter/decision-signal scope | 38 passed, including five compile-fail documentation tests for secret boundary misuse. |
| PostgreSQL store integration scope | 42 passed: 24 Factory discovery, five administration, five connection, two Project-access and six migration contracts. |
| PostgreSQL runtime Source/Reporter | Nine passed, including ordering, recovery, slow admission/confirmation and diagnostic reads without Run-row lock contention. |
| Composed integration pilot | Both tests passed, covering twelve hermetic variants. |
| Locked UI | 27 script tests, 453 tests in 71 Vitest files and all 82 browser tests passed. API generation, Prettier, ESLint, TypeScript, production build and bundle architecture budgets passed. |
| Repository checks | Rust format and scoped all-target/all-feature Clippy passed with warnings denied. Architecture: 63 packages, 254 dependency edges. Security-matrix validation, local-stand build-context validation and 52 associated tool tests passed. |
| Documentation/planning | Both OpenSpec changes validated strictly; four JSON examples parsed, 36 local documentation links resolved and diff whitespace checks passed. |

## Reproduction commands and environment

All completed Cargo builds reused the existing target with incremental/debug
output disabled and two build jobs:

```sh
export CARGO_INCREMENTAL=0
export CARGO_PROFILE_DEV_DEBUG=0
export CARGO_PROFILE_TEST_DEBUG=0
export CARGO_BUILD_JOBS=2

cargo test --locked --all-features -p octacity-server -p octacity-server-application --lib

cargo test --locked --all-features \
  -p octacity-server-factory -p octacity-server-store -p octacity-server-api-rest \
  -p octacity-github -p octacity-server-work-source -p octacity-work-source-protocol \
  -p octacity-server-delivery -p octacity-delivery-protocol \
  -p octacity-server-vcs -p octacity-vcs-protocol

cargo test --locked --all-features \
  -p octacity-server-secrets -p octacity-adapter-protocol \
  -p octacity-server-decision-signal -p octacity-server-decision-signal-jev \
  -p octacity-server-decision-signal-openai

cargo test --locked --all-features -p octacity-server --test factory_work_source_pilot

cargo clippy --locked --all-targets --all-features \
  -p octacity-server -p octacity-server-application -p octacity-server-factory \
  -p octacity-server-store -p octacity-server-store-postgres -p octacity-server-api-rest \
  -p octacity-github -p octacity-server-work-source -p octacity-work-source-protocol \
  -p octacity-server-delivery -p octacity-delivery-protocol \
  -p octacity-server-vcs -p octacity-vcs-protocol -p octacity-server-secrets \
  -p octacity-adapter-protocol -p octacity-server-decision-signal \
  -p octacity-server-decision-signal-jev -p octacity-server-decision-signal-openai \
  -- -D warnings

cargo fmt --all -- --check
python3 tools/check_architecture.py
python3 tools/security_matrix.py validate
python3 tools/verify_local_stand_context.py
python3 -m unittest tools.tests.test_check_architecture \
  tools.tests.test_security_matrix tools.tests.test_verify_local_stand_context
openspec validate add-github-work-source --strict
openspec validate add-factory-work-card --strict
git diff --check
```

This is the relevant package/security scope above, not a claim to have run the
entire workspace, every release-security matrix group, container-image builds or
platform/backend qualification matrices. The executed tests cover the Source,
Reporter, grant, credential, protocol and existing action authority boundaries;
the separate matrix validator checks the repository's declared security coverage.

PostgreSQL used an owned disposable `postgres:18.6-bookworm` container named
`octacity-64-pg`, labeled `octacity.test=github-work-source-64`, with 120 available
connections and an ephemeral loopback port. Trust authentication was confined to
this local test fixture. No user stand/database was selected. The existing test
support generated separate databases and applied the authoritative migrations.
Tests retained ordinary parallel execution, deadlines and production code.

```sh
docker run --detach --rm --name octacity-64-pg \
  --label octacity.test=github-work-source-64 --publish 127.0.0.1::5432 \
  --env POSTGRES_HOST_AUTH_METHOD=trust postgres:18.6-bookworm -c max_connections=120
docker port octacity-64-pg 5432/tcp
docker exec octacity-64-pg pg_isready -U postgres
# The allocated port in this run was 32794; use the actual fixture port on rerun.
export OCTACITY_POSTGRES_URL=postgres://postgres@127.0.0.1:32794/postgres
cargo test --locked --all-features -p octacity-server-store-postgres \
  --test integration_administration --test integration_connections \
  --test integration_project_access --test migrations --test factory_discovery -- --ignored
cargo test --locked --all-features -p octacity-server --lib work_source -- --ignored
```

After both commands passed, container name, image and ownership label were checked
before stopping that container. Its `--rm` lifecycle removed the test service.

UI used **Node 24.21.0** from `.node-version` and **pnpm 12.8.1** from
`packageManager`. The system pnpm was older, and the application-bundled Node
rejected third-party native bindings under macOS library validation. These were
environment failures, not behavioral Red. A task-owned official Node archive was
verified against its release SHA-256 manifest and used for the final UI rerun;
the temporary runtime was removed afterward. No Node security setting or project
dependency was changed. The pinned pnpm was already in the Corepack cache.

With those binaries selected in `PATH`, the following commands ran in `ui/`:

```sh
CI=true pnpm install --frozen-lockfile --offline --ignore-scripts
pnpm run api:generate
pnpm run test:unit
pnpm run test:browser
pnpm run format
pnpm run lint
pnpm run build
```

The frozen offline installation reported the lockfile up to date. API generation
used the authoritative Rust OpenAPI exporter. Browser tests retained their existing
two-worker configuration; no snapshots, assertions, deadlines or retries changed.
The browser regressions explicitly cover Russian guidance at narrow width,
manual Artifact admission without integration setup and ordinary CI/CD navigation
during Factory provider failure.

Raw command output remains in the task-local `/tmp/octacity-64-*.log` files;
this checked-in evidence retains the commands, counts and limits independently
of those temporary logs. Disk space was checked before and during compilation:
approximately 12 GiB initially, 11–12 GiB during builds and 12 GiB after cleanup.
The target grew from approximately 11 GiB to 12 GiB. No user files or build caches
were deleted, and no additional target directory was created.

## Source scopes, compatibility and remaining qualification

The composed pilot uses isolated installed process peers with GitHub, GitLab,
Jira and custom common-contract identities and independently configured label
vocabularies. Connection/Project/read/write grants, task-container scope and exact
Factory target/code Repository pins remain separate. Public commands publish the
inputs and their immutable settings; no label, provider or pipeline name selects
runtime behavior. Native GitHub tests use controlled HTTPS peers. No live account,
external repository, credential or paid model was used in this gate.

The pilot's retained in-memory authority and reconstructed handlers/workers do not
claim database-process durability; the separately executed PostgreSQL contracts
qualify the selected persistence/recovery boundaries. Its private publisher peer
does not qualify real token bytes or a production protected-service mount. Actual
private-file publication, rotation, recovery and credential rejection remain
covered by the native GitHub/application regressions.

Manual Artifact admission remains integration-independent. Existing application
execution/release journeys, VCS/Delivery protocols and GitHub Delivery process
contracts passed unchanged; Source/Reporter add no new authority to ordinary
CI/CD or Delivery. Optional observations preserve native correlation across
input-scoped Runs and do not mutate frozen Work or implicitly create execution.
The fake consumer is not persistent Issue storage. Manual internal task creation,
durable associations/history, import/refresh, conflict ownership and explicit card
execution remain in `add-factory-work-card`; no integration is made a prerequisite
for those future manual tasks.

**Live GitHub qualification and activation remain blocked / unqualified**, as
recorded in [6.2](verification-6.2.md). No disposable repository, selected live
installation/connection or Project/resource/purpose grants were supplied. Closing
this regression gate does not turn the hermetic pilot into a live pass. The
[live runbook](../../../../docs/reference/github-work-source-live-qualification.md)
retains that separate operator qualification and owned-resource cleanup procedure.

No commit, push or change archival was performed.
