# Task 5.10 verification

## Scope

The connection detail now opens a separate Project-access surface. It selects
visible root/child Projects with bounded pagination, displays server-filtered
usable resources, and edits independent task-read/task-update/repository-read/
Delivery grants through the existing confirmed-intent/versioned REST contracts.
An empty replacement revokes every purpose. Review includes exact Project,
connection version, resources and consequences: subsequent protected operations
are blocked, while accepted Work, Run results and already sent remote requests
remain retained. Checks and activation keep their separate existing controls.

The missing dependency read was added within this task's specified dependency
review scope. Its protected, typed endpoint is
`GET /api/v1/integrations/{integration_id}/projects/{project_id}/dependencies`.
It reuses the existing input journal/page and policy-derived dependency scopes,
returning retained input versions for one connection/Project. Configuration,
Repository and connection visibility precede limits/cursors in both stores. Exact
identity/version cursors preserve history after replacement and revocation; group
copies are excluded. Ordinary current-definition discovery keeps its existing
semantics. No migration, new persistence authority or provider I/O was introduced.

Expanding a retained input reads existing bounded Source diagnostics and shows
only that exact input version's visible Run associations and independent reporting
health. Pagination and truncated reporting are explicit. This is visible history
within the selected Project, not a complete global dependency graph or evidence
that a historical enabled definition is currently active.

Provider identities remain open. No credential form or provider-specific branch
was added to Project access. Internal cards remain optional; manual Factory intake
continues through its existing independent path. Guided pipeline intake and status
authoring remain subsequent tasks. No commit, push or live integration activation
was performed.

## TDD evidence

All Cargo commands and API generation used one Cargo operation at a time with:

```sh
export CARGO_INCREMENTAL=0
export CARGO_PROFILE_DEV_DEBUG=0
export CARGO_PROFILE_TEST_DEBUG=0
export CARGO_BUILD_JOBS=2
```

1. Public application dependency history:

```sh
cargo test --locked -p octacity-server-application --lib connection_dependencies_keep --quiet
```

Red: the requested `ListIntegrationDependenciesQuery` public API did not exist.
Green: two retained input versions remain separately paginated after input
replacement and grant revocation. The existing discovery test also checks that
hidden target definitions do not influence dependency results or cursors.

2. Shared memory/PostgreSQL read contract:

```sh
cargo test --locked -p octacity-server-store-postgres --test factory_discovery managed_source_configuration_publication -- --ignored --quiet
```

Red: PostgreSQL returned a group copy where the dependency request required an
input. Green: connection filtering returns retained inputs, excludes other
connections and groups, and applies visibility before pagination. Extended checks
page three records including two versions of one input in both stores and reject
hidden Configuration/Repository/connection results without hidden cursors.
The contract also retains its pre-existing atomic replay, current discovery and
revocation-before-publication assertions across github/gitlab/jira/custom peers.
An initial extended fixture reused a stable ID across those peers; that test-data
collision was corrected with a fresh ID per fixture, without changing production
behavior or timeouts.

3. REST and generated transport:

```sh
cargo test --locked -p octacity-server-api-rest --test integration_administration dependency_review --quiet
pnpm --dir ui exec vitest run src/features/integrations/api.test.ts
```

Red: the protected dependency operation was absent from OpenAPI; the console had
no Project-access methods. Green: typed read contracts, protected ingress and
`no-store` responses pass, substituted connection cursors and invalid page limits
are rejected before dispatch. Project grant preconditions use the access version,
not the connection's administrative revision. Initial publication uses
`If-None-Match: *`, replacement/revocation uses `If-Match`, and reads carry no
mutation key. The generated client was regenerated from Rust, without new DTOs.

4. User-visible access and dependency journeys:

```sh
pnpm --dir ui exec vitest run src/features/integrations/ProjectAccess.test.tsx
pnpm --dir ui exec playwright test tests/browser/integrations.spec.ts
```

Red: the Project-access component and navigation action did not exist; dependency
and reporting history were absent. Green: independent purposes, reviewed read-only
grants, explicit full revocation, safe failures, filtered usable resources, visible
input/Run/pending-reporting history and keyboard confirmation/focus pass.
Additional coverage verifies that version conflicts preserve the draft, failed
baseline reads prevent confirmation, and a successful refresh uses the current
version and a new confirmed command identity. Error text cannot echo private peer
details. Existing shared immutable-reference equality was extracted for both
connection and grant editing after passing their behavioral baseline; all 18
integration feature tests remained green.

5. Bounded UI growth:

```sh
node --test ui/scripts/verify-architecture.test.mjs
pnpm --dir ui build
```

Red: the measured administration/history/translation scenario exceeded the 5.9
total budgets. Green: total JS and total static ceilings each increased by 16 KiB
to 884736 and 966656 bytes respectively; entry (460800), CSS (81920) and file-count
(42) ceilings remain unchanged. Tests also reject further unplanned growth.
Both integration routes still share one lazy module with no new package.
Final output: 881550 JS bytes, 81069 CSS bytes, 963165 total static bytes, 42 files.
The optional integration module is approximately 29.9 KB / 6.8 KB gzip.

## Regression and repository checks

```sh
cargo test --locked -p octacity-server-application --lib source_configuration --quiet
# 17 passed, including visibility, retained dependencies, publication and preview contracts.
cargo test --locked -p octacity-server-application --lib integration_project_access_tests --quiet
# 2 passed: independent purpose/current revocation and management/use authorization.
cargo test --locked -p octacity-server-store -p octacity-server-api-rest --lib --tests --quiet
# 210 passed: 81 store; REST 12 library, 17 administration, 25 public errors, 75 handlers.
cargo test --locked -p octacity-server-store-postgres --test factory_discovery managed_source_configuration_publication -- --ignored --quiet
# 1 shared contract passed against both memory and disposable PostgreSQL 18.
cargo clippy --locked -p octacity-server-store -p octacity-server-application -p octacity-server-store-postgres -p octacity-server-api-rest -p octacity-server --all-targets --all-features -- -D warnings
pnpm --dir ui api:generate
pnpm --dir ui test:unit
# 27 script checks and 422 Vitest tests across 66 files passed.
pnpm --dir ui exec vitest run src/features/integrations
# 18 passed after sharing immutable-reference equality.
pnpm --dir ui exec playwright test
# 76 passed, including accessibility, manual Factory intake and provider-outage independence.
pnpm --dir ui build
pnpm --dir ui lint
pnpm --dir ui format
cargo fmt --all -- --check
git diff --check
python3 tools/check_architecture.py
python3 tools/security_matrix.py validate
python3 tools/verify_local_stand_context.py
openspec validate add-github-work-source --strict
openspec validate add-factory-work-card --strict
```

No zero-test filtered run is counted as validation. Documentation changes have no
executable behavior; OpenSpec, formatting and whitespace checks validate them.

## Runtime and resource limits

The existing opt-in protected management composition must supply Project-access,
Source diagnostic and `.with_dependencies(...)` handlers. The browser does not
provide ingress verification or install adapters. No user stand/authentication,
external task or paid model was used or changed.

Builds retained approximately 16 GiB free throughout, with incremental/debug
artifacts disabled and bounded Cargo parallelism. Tests used only the owned
`octacity-510-pg` disposable container labeled `octacity.test=integration-ui-510`
on loopback; it was stopped and removed after qualification. No other container,
cache or user data was cleaned up.
