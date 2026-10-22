# Task 1.1 — Existing execution and integration baseline

Verification date: 2026-10-18. Base commit:
`b8a4818a36ae4c8f7af8d76905fd39d85763504d` (`main`).
Status: complete for task 1.1 only; 1 of 42 tasks complete.

## Scope and TDD evidence

Task 1.1 verifies existing public behavior before introducing Issues. Only this
report and the task checklist change. Existing behavioral suites run against
unchanged production code; no behavior correction or uncovered shared-seam
refactoring was required. There is no missing-behavior Red/Green cycle to claim.
Documentation-only changes use validation. Subsequent implementation still
requires an observed behavioral Red before its minimal Green implementation.

## Commands and environment

Commands run from the repository root unless `cd ui` is specified. Rust/Cargo
1.99.0 uses the existing `target`, two compilation jobs, no incremental cache or
debug information. Test parallelism and timeouts remain unchanged.

```sh
export CARGO_INCREMENTAL=0
export CARGO_PROFILE_DEV_DEBUG=0
export CARGO_PROFILE_TEST_DEBUG=0
export CARGO_BUILD_JOBS=2

cargo test --locked -p octacity-server-factory -p octacity-server-store \
  -p octacity-server-application -p octacity-server --lib

cargo test --locked -p octacity-server-api-rest \
  --test v1_handlers --test integration_administration

cargo test --locked -p octacity-server --test factory_work_source_pilot
```

The PostgreSQL fixture uses the locally available `postgres:18.6-bookworm` image,
an ephemeral tmpfs database and a dynamically assigned loopback port. Each
existing test creates its own database. No running stand or persistent user
database is reused.

```sh
docker run --detach --rm --pull never \
  --name octacity-work-card-11-pg --label octacity.test=work-card-11 \
  --publish 127.0.0.1::5432 --env POSTGRES_HOST_AUTH_METHOD=trust \
  --tmpfs /var/lib/postgresql postgres:18.6-bookworm -c max_connections=120
docker exec octacity-work-card-11-pg pg_isready -U postgres
docker port octacity-work-card-11-pg 5432/tcp

# Port allocated in this run; substitute the actual fixture port on rerun.
export OCTACITY_POSTGRES_URL=postgres://postgres@127.0.0.1:32797/postgres
cargo test --locked -p octacity-server-store-postgres \
  --test factory_discovery --test factory_flow_management \
  --test integration_administration --test integration_connections \
  --test integration_project_access --test migrations -- --ignored
cargo test --locked -p octacity-server --lib work_source -- --ignored

# Remove only this task's owned fixture after testing.
docker stop octacity-work-card-11-pg
```

The remaining ignored runtime-management contract was run on a fresh fixture
created with the same command after initial cleanup (allocated port 32799):

```sh
OCTACITY_POSTGRES_URL=postgres://postgres@127.0.0.1:32799/postgres \
  cargo test --locked -p octacity-server --lib integration_management_tests -- --ignored
docker stop octacity-work-card-11-pg
```

Console uses the pinned Node 24.21.0 and pnpm 12.8.1. The initial host Node 22
and Corepack launcher failed before tests (`bin/pnpm.cjs` missing); this was an
environment failure, not Red. Node was restored in the npm execution cache and
the existing pinned pnpm native launcher used directly, with no repository
dependency/configuration changes:

```sh
cd ui
work_card_node=$(npm exec --yes --package=node@24.21.0 -- node -p process.execPath)
export PATH="$(dirname "$work_card_node"):$PATH"
work_card_pnpm="$HOME/.cache/node/corepack/v1/pnpm/12.8.1/pnpm"
"$work_card_pnpm" exec vitest run src/features/factory \
  src/features/integrations src/api/integrationContracts.test.ts
"$work_card_pnpm" exec playwright test tests/browser/factory.spec.ts \
  tests/browser/factory-inputs.spec.ts tests/browser/integrations.spec.ts
```

## Observed results

| Boundary | Result |
| --- | --- |
| Server runtime / application / Factory domain / memory store | 171 / 477 / 296 / 81 passed |
| REST integration administration / v1 handlers | 17 / 80 passed |
| Hermetic composed Source/Reporter pilot | 2 passed |
| PostgreSQL discovery / Flow management | 24 / 2 passed |
| PostgreSQL administration / connections / Project access / migrations | 5 / 5 / 2 / 6 passed |
| PostgreSQL Reporter recovery and ordering | 9 passed; 172 unrelated tests filtered |
| PostgreSQL runtime connection/credential management | 1 passed; 180 unrelated tests filtered |
| Console component/contracts | 147 passed in 35 files |
| Console browser | 14 passed |

All commands exited 0: 1,178 Rust/PostgreSQL tests plus 161 Console tests,
1,339 passing executions in total. The 10 PostgreSQL runtime tests ignored by
the default library command all passed in the two explicit ignored-test runs.
The broad application suite completed in 394.32 seconds; its long-scenario
notices were progress messages, not failures or interrupted results.

Disk was checked before and during compilation: approximately 17 GiB free at
the start and 12 GiB at completion; `target` grew from 1.3 GiB to 4.8 GiB.
No additional Rust target/cache, image build or cache deletion was needed.
Both owned ephemeral PostgreSQL containers were stopped and removed.

## Contract evidence

- **Immutable execution identity:** `WorkEnvelope` retains immutable admitted
  input; migration `0047_dark_factory_authority.sql` enforces scoped native
  identity uniqueness and a unique `factory_runs.work_envelope_id`. Admission
  contracts verify one original Work/Run, replay before mutable base lookup,
  restart with unavailable VCS, conflicting replacement Artifacts, atomic
  capacity and dependency visibility. PostgreSQL discovery/migration contracts
  retain manual admission and its replay across schema upgrade.
- **Recovery and authority:** Source publication, profile/grant revocation,
  retained input versions, interrupted reporting, accepted-order projection,
  fences and unknown/late writes use the existing stores and owner. The composed
  pilot exercises Source → ordinary Factory action → Reporter → Source with
  hermetic peers; it is not live provider qualification.
- **Controls and discovery:** application/REST tests retain exact Run targets,
  candidate/version preconditions, scoped replay and visibility. PostgreSQL
  checks cover restart, concurrent claims, nonblocking management reads and
  bounded discovery. Business card status does not replace these contracts.
- **Manual and provider-neutral Console:** component/browser baselines cover
  manual Artifact intake without an integration, Russian narrow-layout
  guidance, integration failures, credentials/access review and ordinary CI
  navigation during provider outage. Manual internal Issue creation is future
  behavior, not an already implemented capability established by these tests.

## Planning compatibility and remaining qualification

The proposal, design, all three delta specifications and all 42 tasks were
reviewed for the execution/provider boundary. They consistently put a persistent
Issue above zero or many immutable Work Envelopes, preserve one Run per Work and
legacy unassociated Work, and reuse ordinary admission, Flow owner, Build/Agent
execution, typed actions and Reporter recovery. They require neither a new
executor nor mutable Work. Manual Issue/workflow creation requires no provider,
Repository, model or Factory target; explicit execution has its own prerequisites.

Provider SDKs, native status values and network/credential policy remain in
installed adapters. Project business workflows and explicit pipeline moves are
separate from external mappings. GitLab/Jira in conformance fixtures represent
alternate vocabularies, not shipped native adapters or implementation prerequisites.
The detailed shared publication/action/profile inspection belongs to task 1.2;
the related quickstart and role-profile changes are not marked complete here.

Remaining release gates are explicit:

- The archived Source change's [live qualification gate](../2026-10-18-add-github-work-source/verification-6.2.md)
  remains unqualified. No disposable live repository, selected live execution
  installation/profiles, independently scoped grants or owned remote fixtures
  were supplied for this run. The earlier local-stand selection of
  `OctaHive/OctaCity` does not authorize disposable qualification writes there.
  No live credential, provider mutation or paid model was used.
- Local-stand connection management and credential checks do not themselves
  install or activate Source polling, preview/admission or Reporter execution
  profiles. Follow the existing [live runbook](../../../../docs/reference/github-work-source-live-qualification.md)
  for separate supported-adapter qualification.
- Native GitLab/Jira adapters, the future Issue UI/workflows/associations and
  this change's manual/imported end-to-end pilots remain future work. Passing
  this baseline does not qualify those capabilities.
- Existing [Factory release gates](../../../../docs/operations/factory.md), including
  real signed Agent v3, backend-specific native/containerd/microsandbox,
  delivery/promotion and operational qualification, remain separate. They were
  not rerun or waived by this task.

## Documentation and repository checks

```sh
openspec validate add-factory-work-card --strict
# Valid.
cargo fmt --all -- --check
# Exit 0.
python3 tools/check_architecture.py
# Exit 0: 63 workspace packages, 255 dependency edges.
git diff --check
# Exit 0.
```

Report links resolve to existing repository files. No production, test,
dependency, generated API or other change artifacts were modified. No commit,
push, archive or specification synchronization was performed.
