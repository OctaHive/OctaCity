# Task 2.2 — PostgreSQL workflow persistence and migration parity

Implemented on 2026-10-18. Scope: the existing `IssueWorkflowStore` port backed by
PostgreSQL, an additive migration, and retained immutable/default/replay behavior.
REST discovery, generated clients and Console settings remain tasks 2.3–2.4;
manual Issue creation and actual guarded movement remain 2.5. This task does not
create Issues, migrate their workflow pins, admit Work or call a provider.

## Implementation and reuse

- Migration `0067_issue_workflows.sql` adds only `issue_workflows`,
  `issue_workflow_versions` and `issue_workflow_defaults`. Composite foreign keys
  enforce Project ownership, retained versions and exact selected defaults.
  Existing migrations, Work/Run identities and provider configuration are unchanged.
- The adapter reuses the shared workflow definition validation and declarative
  initial template from 2.1, rather than duplicating vocabulary or provider rules.
  The initial and terminal selections remain workflow-level fields.
- Default reads do not initialize records or require administration, Factory or a
  provider. An authorized mutation materializes the original template when an
  exact retained foreign-key reference is needed. A read-only default does not
  block ordinary Project deletion; published history does.
- Publication and explicit default selection use the existing security-scoped
  mutation digest, replay, audit and outbox transaction. A Project row lock closes
  first-publication/default races across connections without a global lock.
  Reads remain ordinary nonlocking queries. No second journal or lifecycle owner
  is introduced, and no provider effect is dispatched.
- Publication retains old versions and does not advance the selected default.
  Default selection uses its separate optimistic version and exact workflow pin.
  Replays return the original receipt after later publications/selections, with
  no duplicate evidence or restoration of an older current selection.
- Selection audit targets its owning Project, matching memory behavior;
  publication audit targets the workflow. Safe metadata contains IDs/versions,
  not the workflow's display text or credentials.

See [workflow contracts](../../../../docs/reference/issue-workflows.md).

## Test-first cycles

The public seam was the existing `IssueWorkflowStore` used with the established
Project store and disposable PostgreSQL migration fixture. Audit assertions use
`AuditFactStore`; SQL in fault-injection cases only configures an isolated failure,
while policy, evidence and retry outcomes are read through public ports.

All Rust commands used the existing target and this environment:

```sh
export CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
export CARGO_BUILD_JOBS=2
export OCTACITY_POSTGRES_URL=postgres://postgres@127.0.0.1:32801/postgres
```

The port belongs to this run's disposable fixture and must be rediscovered for a
future run. Each Red command used this prefix and the test name below:

```sh
cargo test --locked -p octacity-server-store-postgres --test issue_workflows \
  <test-name> -- --ignored
```

| Test / cycle | Observed Red before implementation | Observed Green |
| --- | --- | --- |
| `default_workflow_matches_memory_without_factory_provider_or_administration` | Missing PostgreSQL `default_issue_workflow` / `issue_workflow` methods (required store API) | 1 passed after implementing read-only defaults through the existing Project authority |
| `publication_retains_exact_versions_and_replay_after_schema_upgrade_and_restart` | Runtime publication returned `Unavailable` | 2 focused tests passed after adding retained PostgreSQL publication and migration |
| `default_selection_pins_an_exact_version_and_replays_without_restoring_it` | Runtime selection returned `Unavailable` | 3 focused tests passed after adding exact default persistence |
| The same selection test, extended with the public audit contract | Runtime assertion: target kind was `issue_workflow`, expected `project` | Selection test passed after correcting the mutation's target kind |

The first Red is a missing-required-API compiler failure allowed by repository
instructions. An incidental private helper visibility error during implementation
was corrected and is not counted as behavioral Red.

Additional regressions characterize the same implemented transaction and
precondition guarantees; they passed without a further production correction.
They cover concurrent first replacement/default selection (one winner), identical
concurrent publication (one applied and one replay), foreign/missing Projects and
workflows, invalid definitions without receipts, independent security/operation
scopes and actual audit attribution. Injected outbox failure proves that policy,
audit and replay roll back together and the corrected retry applies once.
These passing characterization runs are not claimed as new Red failures.

The restart tests close every connection in the original pool and open a new
pool before retained reads and replay. Migration coverage creates a Project on
schema 66, upgrades it, publishes subsequent versions and reruns migration without
changing the Project or retained definitions. Existing migration rehearsal also
covers schema 46, failed forward migration, snapshot restoration and repeated
upgrade; the previous released-binary schema remains 46.

Final focused run after simplifying duplicated safe-metadata construction:

```sh
cargo test --locked -p octacity-server-store-postgres --test issue_workflows -- --ignored
# 7 passed, ordinary test parallelism, no ignored cases left in this selection.
```

## Regression qualification

```sh
cargo test --locked -p octacity-server-store-postgres \
  --test issue_workflows --test migrations --test projects \
  --test factory_flow_management --test factory_discovery --test store_contract \
  --test audit -- --ignored
```

The first combined run passed audit, Factory discovery, Flow management and all
new workflow tests, then stopped at the migration inventory: its old expected set
omitted the three new tables. The inventory was extended explicitly, including
absence checks for the previous binary's schema, without weakening existing
checks. The failed and not-yet-run selections were then rerun:

```sh
cargo test --locked -p octacity-server-store-postgres \
  --test migrations --test projects --test store_contract -- --ignored
cargo test --locked -p octacity-server-application --lib issue_workflow_
```

| Boundary | Final result |
| --- | --- |
| PostgreSQL workflow | 7 passed |
| PostgreSQL migrations | 6 passed |
| PostgreSQL Projects | 4 passed |
| PostgreSQL general store contracts | 17 passed |
| PostgreSQL audit | 1 passed |
| PostgreSQL Factory discovery | 24 passed |
| PostgreSQL Flow management | 2 passed |
| Application workflow / authorization / memory contracts | 8 passed |

This is 61 PostgreSQL and 8 application tests, all passing in their final relevant
runs. Existing general store/Factory regressions retain manual admission and
immutable execution behavior. No UI or HTTP behavior changed, so their suites
were not rebuilt. No timeout increase, test serialization, paid model or live
provider credential/mutation was used.

## Checks and resource handling

The run used the already installed `postgres:18.6-bookworm` image in an owned
fixture with temporary database storage:

```sh
docker run --detach --rm --name octacity-work-card-22-pg \
  --env POSTGRES_HOST_AUTH_METHOD=trust --publish 127.0.0.1::5432 \
  --tmpfs /var/lib/postgresql postgres:18.6-bookworm
docker port octacity-work-card-22-pg
docker exec octacity-work-card-22-pg pg_isready -U postgres
```

The existing `octacity-local-*` stand was not altered. Free space was checked
before and during compilation and remained approximately 11 GiB. Only the
existing Rust target was reused, with two build jobs and no incremental/debug
output; no image build, dependency addition or cache deletion was needed.

Required final checks:

```sh
cargo clippy --locked -p octacity-server-store-postgres \
  --lib --test issue_workflows --test migrations -- -D warnings
cargo fmt --all -- --check
python3 tools/check_architecture.py
openspec validate add-factory-work-card --strict
git diff --check
```

All listed final checks passed. Architecture remains 63 workspace packages and
255 dependency edges. The owned fixture was stopped with
`docker stop octacity-work-card-22-pg`; `--rm` removed its temporary data. No
existing stand container was stopped or reconfigured.

Task 2.2 is complete; change progress is **4 of 56 tasks**. No commit, push,
archive or main-spec synchronization was performed. The neighboring integration
setup change and the English/Russian design/proposal copies were not modified.
