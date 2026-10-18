# Work-card task 2.3 verification

## Scope and public boundaries

Task 2.3 adds Project business-workflow REST discovery, validation, creation,
immutable replacement, exact retained reads and default selection, with OpenAPI
and generated Console client types. It uses the existing authorized application
and workflow store contracts. The Console settings UI remains task 2.4, and Issue
creation/movement remains task 2.5. No provider installation, credential, Factory
configuration, repository, model, execution or source synchronization is required.

The task also completes the bounded discovery port missing from 2.1/2.2, using
the shared current-definition page input and typed visibility registry. PostgreSQL
filters visibility/cursor before LIMIT; memory and PostgreSQL include the implicit
initial workflow. Point/default reads use the same workflow identity visibility.
Unsupported policy output fails closed rather than widening the grant.

The eight operations are described in
[Issue workflow REST contracts](../../../docs/reference/issue-workflows.md#management-rest-v1).
The server composition installs them independently of optional Factory/integration
services. Mutation grammar delegates to shared policy; transport schemas do not
introduce a second validator, provider-specific vocabulary or scripting language.

## Observed Red / Green

Each behavior was introduced at a public boundary before its implementation.
Incidental edit/compile errors, an incorrect initial assertion about internal
policy errors, and a later unavailable temporary PostgreSQL fixture are excluded
from Red evidence.

| Boundary / test | Observed Red | Observed Green |
| --- | --- | --- |
| REST `issue_workflow_default_http_needs_no_factory_or_provider_and_is_read_only` | Required workflow composition API was absent | 200, stable IDs, workflow settings and strong selection ETag |
| Application `issue_workflow_discovery_filters_before_paging_without_initializing_state` | Required bounded discovery query was absent | Visible IDs selected before two one-row pages; no mutation evidence; invalid page sizes rejected |
| PostgreSQL `discovery_applies_visibility_before_cursor_and_includes_the_unconfigured_default` | Discovery was unavailable; memory then returned Project-not-found for an empty scope | Both adapters agree on initial/default, restricted pages and empty scope |
| REST `issue_workflow_openapi_exposes_closed_bounded_provider_neutral_policy` | Definition schema absent; later the workflow path parameter was absent | Closed bounded policy, status ID/name only and generated UUID path input |
| REST `issue_workflow_http_validation_rejects_legacy_flags_scripts_and_execution_setters` | 404 instead of 200 | Valid policy accepted; legacy flags, guards/scripts and execution setters return 400 |
| REST `issue_workflow_publication_http_requires_strong_precondition_and_preserves_policy` | 404 instead of 200 | Original stable policy retained, ETag advances, missing/weak/wildcard preconditions rejected |
| Actual server `runtime_issue_workflows_are_available_without_factory_or_integrations` | 503 for default read; later 404 for creation and current/exact reads | Actual HTTP + migrated PostgreSQL lifecycle without Factory or integrations |
| Application `issue_workflow_reads_do_not_reveal_a_hidden_current_or_default_policy` | A hidden default definition was returned | Hidden default/current/exact reads return not-found without mutation evidence |
| REST `issue_workflow_default_http_selects_an_exact_pin_under_its_own_precondition` | 405 instead of 200 | Exact pin and independent selection version/ETag; missing/weak preconditions rejected |
| REST `issue_workflow_discovery_http_is_bounded_and_returns_summaries` | 405 instead of 200 | Bounded safe summaries; invalid size/cursor and unknown query fields rejected |
| REST `issue_workflow_http_invalid_version_has_the_standard_safe_error` | Raw text rejection instead of the standard JSON error | 400 `invalid_request` JSON through the common error boundary |
| Generated client `issueWorkflowContracts.test.ts` with `pnpm typecheck` | Workflow schema names and typed routes were absent | Generated types compile; behavioral client tests preserve separate mutation headers, body and credential omission |

Refactoring of version conversion uses the application-exported positive version
types instead of encoding integers through JSON. Existing behavior tests were
passing before it and remained passing afterward. Runtime test fixtures were
also moved to one shared test-only module declaration after Clippy identified
duplicate loading. The six installed-check tests, the PostgreSQL credential
publication test and the local protected-ingress test all passed before that
move; the same boundaries are rechecked afterward. No lint suppression was added
for duplicate modules, and no production integration behavior was changed. Documentation/formatting have
no executable behavior and were validated separately.

## Commands and final qualification

Rust commands reused the existing target with:

```sh
export CARGO_INCREMENTAL=0
export CARGO_PROFILE_DEV_DEBUG=0
export CARGO_PROFILE_TEST_DEBUG=0
export CARGO_BUILD_JOBS=2
```

Focused Red/Green commands (each named filter was run separately):

```sh
cargo test --locked -p octacity-server-api-rest --test v1_handlers issue_workflow_default_http
cargo test --locked -p octacity-server-api-rest --test v1_handlers issue_workflow_openapi
cargo test --locked -p octacity-server-api-rest --test v1_handlers issue_workflow_http_validation
cargo test --locked -p octacity-server-api-rest --test v1_handlers issue_workflow_publication_http
cargo test --locked -p octacity-server-api-rest --test v1_handlers issue_workflow_default_http_selects
cargo test --locked -p octacity-server-api-rest --test v1_handlers issue_workflow_discovery_http
cargo test --locked -p octacity-server-api-rest --test v1_handlers issue_workflow_http_invalid_version
cargo test --locked -p octacity-server-application --lib issue_workflow_discovery
cargo test --locked -p octacity-server-application --lib issue_workflow_reads_do_not_reveal
cargo test --locked -p octacity-server-store-postgres --test issue_workflows discovery_applies_visibility -- --ignored
cargo test --locked -p octacity-server --lib runtime_issue_workflows_are_available -- --ignored
```

PostgreSQL commands set `OCTACITY_POSTGRES_URL` to the loopback port of an owned
temporary PostgreSQL fixture. Tests create independent disposable databases and
run under the normal parallel harness. No timeouts, serialization or production
execution changes were introduced.

The runtime journey creates a Project/custom policy, reads current and retained
versions, renames/reorders a status without changing its ID, publishes an empty
terminal set, observes the unchanged default, explicitly selects a retained
version, changes the default, replays exact original receipts, rejects stale
selection and discovers both workflows through bounded HTTP pages.

| Final boundary | Result |
| --- | --- |
| REST crate (unit, handler/OpenAPI drift, wire, integrations) | 142 passed |
| Application workflow / memory behavior | 11 passed |
| Application management security / visibility | 21 passed |
| PostgreSQL workflow / discovery / replay / concurrency | 8 passed |
| Runtime workflow HTTP journey, installed integrations, credential publication and protected ingress | 9 passed |
| Generated client / ordinary API / integration contracts | 35 passed |
| Console generation, typecheck, lint, production build and bundle architecture | Passed |

Final behavioral regression commands:

```sh
cargo test --locked -p octacity-server-api-rest
cargo test --locked -p octacity-server-application --lib issue_workflow_
cargo test --locked -p octacity-server-application --lib management_security
cargo test --locked -p octacity-server-store-postgres --test issue_workflows -- --ignored
cargo test --locked -p octacity-server --lib -- --include-ignored runtime::integration runtime_issue_workflows_are_available local_integration_ingress_verifies
```

After the shared-fixture refactor, all nine runtime tests passed together under
the normal parallel harness. All eight workflow REST tests were rerun and passed
after the final borrow-only cleanup. These are 226 distinct passing behavioral
tests across the final qualified boundaries.

Console commands used pinned Node 24.21.0 and pnpm 12.8.1 from the existing local
installation, without modifying dependencies or lockfiles:

```sh
cd ui
pnpm api:generate
pnpm typecheck
pnpm exec vitest run src/api/issueWorkflowContracts.test.ts src/api/client.test.ts src/api/integrationContracts.test.ts
pnpm lint
pnpm build
pnpm exec prettier --check src/api/issueWorkflowContracts.test.ts
```

`pnpm build` also runs TypeScript and the production bundle architecture/budget
checks. Generated files remain gitignored and are produced from the REST schema;
there is no handwritten second client. No Console screen behavior changed, so
browser/visual suites were not rerun.

Required repository checks:

```sh
cargo clippy --locked -p octacity-server-api-rest -p octacity-server-application \
  -p octacity-server-store -p octacity-server-store-postgres -p octacity-server \
  --lib --tests -- -D warnings
cargo fmt --all -- --check
python3 tools/check_architecture.py
openspec validate add-factory-work-card --strict
git diff --check
```

Clippy passed for all five affected packages, including their tests. Architecture
checks still report 63 workspace packages and 255 dependency edges. The other
listed checks passed as well.

## Resource and scope handling

Free space was checked before/during compilation and stayed approximately 11 GiB.
No dependency was added, image rebuilt, cache deleted or existing stand container
reconfigured. Tests used the already installed `postgres:18.6-bookworm` image
with temporary data; a disappeared fixture was recreated and its environment-only
failure is not counted as Red. Existing `octacity-local-*` containers were left
running.

The prior uncommitted 2.2 migration, mutation plumbing and verification report
were preserved byte-for-byte. Only the workflow store implementation/test were
extended for discovery, along with the shared reference/task progress. The
integration setup change and English/Russian proposal/design copies are unchanged.
No commit, push, archive or main-spec synchronization was performed.


Task 2.3 is complete; change progress is **5 of 56 tasks**. The next workflow
settings screen is task 2.4. The owned PostgreSQL fixture was stopped by its exact
container ID after verification; `--rm` and tmpfs remove its disposable data.
