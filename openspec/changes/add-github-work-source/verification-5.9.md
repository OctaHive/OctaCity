# Task 5.9 verification

## Scope and boundaries

Dedicated `/integrations` and `/integrations/:integrationId` console routes provide
named connection discovery, installed provider/endpoint/authentication/resource
choices, pure draft validation, reviewed immutable publication and editing,
credential provision/rotation/adoption/revocation, activation/disablement and explicit
read-only resource/purpose checks. Existing confirmed commands, version headers,
SelectMenu, query notices and typed English/Russian catalogs are reused.

The user explicitly approved the missing backend prerequisites for this task:
visible connection enumeration, safe installed declaration enumeration and draft
validation. Their typed OpenAPI/REST/application contracts reuse existing metadata
and authorization; no migration, provider-specific Factory branch or dependency was
added. Connection visibility precedes pagination in memory and PostgreSQL. The
installed catalog requires unrestricted administration visibility and fails closed
for restricted scopes. Validation resolves trusted declarations without publication,
credentials, Project grants or provider I/O.

Credential values stay in an uncontrolled password field until confirmed sending.
The field clears before the API call, on scope/action changes, cancellation, close
and navigation. Confirmed intents and retries contain safe metadata only. Tests
inspect query/mutation caches and browser storage and reject remote errors that
echo submitted values. Credential success retains only the closed safe receipt.
Revoke cannot activate a connection. Missing credentials default to provisioning;
configured credentials default to rotation. Recovery can resend the safe command
identity without a stored secret.

Task business data, code Repositories and Project access remain independent of the
connection. No task card, import, automatic intake, status write or paid model call
is needed. Existing manual Factory intake remains available. Project grants and
guided pipeline inputs stay in tasks 5.10–5.12; card CRUD/execution/synchronization
remain in `add-factory-work-card`.

## Red/Green evidence

All Cargo commands and API generation used the resource bounds below, one Cargo
operation at a time:

```sh
export CARGO_INCREMENTAL=0
export CARGO_PROFILE_DEV_DEBUG=0
export CARGO_PROFILE_TEST_DEBUG=0
export CARGO_BUILD_JOBS=2
```

1. Public application/store discovery and validation:

```sh
cargo test --locked -p octacity-server-application --lib integration_connection_tests
cargo test --locked -p octacity-server-store-postgres --test integration_connections -- --ignored
```

Red: tests requested the absent `ListIntegrationConnectionsQuery`, installed
catalog/validation queries and store list contract. Green: bounded visible paging,
independent github/gitlab/jira/custom-tracker declarations, denied catalog visibility
and invalid endpoints/authentication/resources/purposes pass through public
handlers. Validation creates no metadata. PostgreSQL passes five contracts against
a disposable loopback PostgreSQL 18 instance, including filtering before cursor
derivation. Missing test-wrapper trait delegation found by all-target Clippy was
updated to forward to the ordinary store, preserving its Reporter deadline behavior.

2. Protected typed REST discovery:

```sh
cargo test --locked -p octacity-server-api-rest --test integration_administration
cargo test --locked -p octacity-server-api-rest --test v1_handlers
```

Red: three operation IDs were absent from OpenAPI. Green: typed protected list,
catalog and validation operations, schemas and safe response reachability pass.
Red: a zero page limit reached the injected query peer. Green: local guards reject
zero/over-limit pages before query dispatch. New routes are included in existing
authorization/ingress route inventories; their assertions were extended with actual
requests. `no-store` and write-only credential contracts remain covered.

3. Console behavior and transport:

```sh
pnpm --dir ui exec vitest run src/app/shell/sections.test.ts src/app/shell/CommandCenter.test.tsx src/features/integrations
pnpm --dir ui exec playwright test tests/browser/integrations.spec.ts tests/browser/query-states.spec.ts
```

Red: missing section paths, navigation command, feature public modules and route
headings. Green: dedicated list/detail navigation, installed-only authoring and
reviewed publication/administration work with generated contract types. Further
individual Red/Green cycles cover revoke retaining activation, missing credentials
offering rotation, equivalent immutable pins with different JSON key order, and
failed conflict-baseline refresh. A failed refresh preserves the draft and requires
successful baseline loading and fresh validation before publication.

Browser tests verify keyboard review cancellation/focus restoration, Russian narrow
layout, safe credential rejection, cleared fields and absence from local/session
storage. The shared route inventory also found two Red stale-refresh cases: list
data lacked a stale notice and detail data disappeared. Reusing QueryBackgroundNotice
keeps both visible with a safe warning; all 13 route refresh cases then pass.

4. Bounded bundle growth:

```sh
node --test ui/scripts/verify-architecture.test.mjs
pnpm --dir ui build
```

Red: an independently specified scenario containing the optional integration views
and translations exceeded the old total budgets. Green: both integration routes
share one lazy module and API code stays out of the entry import. Total JS allowance
increased by 16 KiB (851968 → 868352), total static allowance by 32 KiB
(917504 → 950272). Tests keep entry (460800), CSS (81920) and asset-count (42)
ceilings unchanged and reject additional unplanned growth. Final measured output:
864464 JS bytes, 81069 CSS bytes, 946079 total static bytes and 42 output files.
The optional views module is approximately 19.6 KB, 5.1 KB gzip. Production checks
pass without native-control exceptions, added packages or regenerated visual baselines.

## Regression checks

```sh
cargo test --locked -p octacity-server-application --lib
# 464 passed.
cargo test --locked -p octacity-server-store -p octacity-server-api-rest --lib --tests --quiet
# 208 passed: 81 store, 12 REST library, 15 administration, 25 public errors, 75 handlers.
cargo test --locked -p octacity-server-store-postgres --test integration_connections -- --ignored --quiet
# 5 passed against the owned disposable PostgreSQL instance.
cargo test --locked -p octacity-server --lib managed_reporter_deadline --quiet
# 2 passed; the test wrapper delegates the new read contract to the ordinary store.
cargo clippy --locked -p octacity-server-store -p octacity-server-application -p octacity-server-store-postgres -p octacity-server-api-rest -p octacity-server --all-targets --all-features -- -D warnings
pnpm --dir ui api:generate
pnpm --dir ui typecheck
pnpm --dir ui test:unit
# 27 script tests and 417 Vitest tests across 65 files passed.
pnpm --dir ui exec playwright test
# 75 passed, including accessibility, stale refresh, manual Factory intake and visual regressions.
pnpm --dir ui lint
pnpm --dir ui format
pnpm --dir ui build
cargo fmt --all -- --check
git diff --check
python3 tools/check_architecture.py
python3 tools/security_matrix.py validate
python3 tools/verify_local_stand_context.py
openspec validate add-github-work-source --strict
openspec validate add-factory-work-card --strict
```

## Operational limits

The console calls opt-in management services. Runtime composition must provide the
existing verified protected ingress, authorized handlers and trusted installed
declarations, including `.with_discovery(...)`. This task does not configure login,
credentials or live integrations on the user's local stand. Missing composition
fails closed. A read-only check is neither activation nor proof that a provider can
write. Live GitHub qualification remains a separate explicit opt-in task.

Disk space was checked throughout compilation: approximately 17 GiB initially,
16 GiB at final builds. Incremental artifacts/debug symbols were disabled and Cargo
parallelism bounded. Only the owned labeled disposable database container was used;
existing user containers and caches were not removed. No commit or push was made.
