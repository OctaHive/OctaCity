# Task 5.8 verification

## Scope

Updated the live management OpenAPI contract, canonical wire fixtures and generated
console types for existing connection/access administration, write-only credentials,
read-only checks, automatic input selection, previews and diagnostics. The shared
fixture contains 20 synthetic examples across 15 components, including initial and
rotated credentials, grouped inputs with optional Reporter, native Work/Run
correlation and absent/visible internal-card associations. Public examples are
compiled into OpenAPI rather than maintained in another schema snapshot.

Added `FactoryWorkStatusActionParameters` for the existing installed `set_status`
action. Arbitrary status values and optional finite outbox policy remain ordinary
Flow plugin configuration. Retry ceilings are shared with application validation;
existing behavior is unchanged. The generic plugin parameter envelope stays open.
Guided label additions/removals and new per-outcome authoring remain task 5.13.

Credentials retain a separately authorized write-only request. The documented
UTF-8 byte ceiling now comes from the actual credential limit. No secret read-back,
provider-specific Factory branches, new Issue CRUD/execution/field-ownership API,
card creation or extra mutation endpoint was added. Existing manual Artifact
admission remains independent of integrations. The optional card link retains the
visibility authority established in task 5.7.

## Test boundaries and Red/Green evidence

The task explicitly selects exported public schema/fixtures and generated client
contracts as its boundaries. REST adapter responses and the installed action's
public validation boundary provide independent checks against those examples.

All Cargo commands and API generation used:

```sh
export CARGO_INCREMENTAL=0
export CARGO_PROFILE_DEV_DEBUG=0
export CARGO_PROFILE_TEST_DEBUG=0
export CARGO_BUILD_JOBS=2
```

Baseline before implementation/refactoring:

```sh
cargo test --locked -p octacity-server-api-rest --test integration_administration
# 12 passed.
cargo test --locked -p octacity-server-application --lib factory_flow_action_tests
# 18 passed.
```

1. Public action contract:

```sh
cargo test --locked -p octacity-server-api-rest --test v1_handlers work_status_action_has_a_typed_provider_neutral_parameter_contract
```

Red: `missing component schema FactoryWorkStatusActionParameters`. Green: the same
test passed after adding the named bounded schema. The fixture also passes the real
installed action validator, which rejects invalid retry bounds, oversized UTF-8
status and attempted Issue retargeting before any projection.

2. Published shared examples:

```sh
cargo test --locked -p octacity-server-api-rest --test v1_handlers integration_openapi_publishes_schema_checked_shared_wire_examples
```

Red: missing public component `examples` (null instead of the documented fixture).
Green: schema/fixture tests passed after publishing the canonical examples. Typed
application publication intents round-trip those fixtures; actual successful REST
responses are also checked against the same component schemas. Response-reference
traversal rejects secret-input reachability and private response fields.

3. Credential byte ceiling:

```sh
cargo test --locked -p octacity-server-api-rest --test v1_handlers credential_schema_documents_the_transient_utf8_byte_ceiling
```

Red: `x-max-utf8-bytes` was null instead of 16384. Green: the test passed after
publishing the authoritative byte ceiling. Existing protected credential REST tests
also reject an oversized multibyte value, preserve no-store responses and return no
submitted token on success/rejection.

4. Generated client:

```sh
pnpm --dir ui typecheck
```

Red: TypeScript could not find `FactoryWorkStatusActionParameters` in the stale
local generated client. After `pnpm --dir ui api:generate`, type checking passed.
Transport tests use the shared fixtures and generated types for connection/access,
checks, rotation, preview and native/card diagnostics; ordinary generated response
types exclude secret submission fields. Existing Factory transport tests preserve
manual admission without connection or card parameters.

## Final checks

```sh
cargo test --locked -p octacity-server-api-rest --lib --tests
# 121 passed: 12 library, 12 administration, 25 v1 contracts, 72 handlers.
cargo test --locked -p octacity-server-application --lib factory_flow_action_tests
# 19 passed, including existing stage/nested-stage reporting and retry coverage.
cargo clippy --locked -p octacity-server-application -p octacity-server-api-rest --all-targets --all-features -- -D warnings
pnpm --dir ui api:generate
pnpm --dir ui typecheck
pnpm --dir ui exec vitest run src/api/integrationContracts.test.ts src/api/client.test.ts src/features/factory/api.test.ts
# 40 passed.
pnpm --dir ui exec eslint src/api/integrationContracts.test.ts --max-warnings=0 --report-unused-disable-directives
pnpm --dir ui exec prettier --check src/api/integrationContracts.test.ts
cargo fmt --all -- --check
git diff --check
python3 tools/check_architecture.py
# 63 packages, 253 dependency edges.
python3 tools/security_matrix.py validate
node ui/scripts/verify-architecture.mjs
openspec validate add-github-work-source --strict
openspec validate add-factory-work-card --strict
```

Documentation-only reference/progress edits have no executable behavior; OpenSpec
and whitespace checks apply instead of Red/Green.

Builds ran sequentially with two Cargo jobs, incremental compilation disabled and
without debug information. Free space stayed approximately 17 GiB; `target` stayed
approximately 6.5 GiB. No cleanup, database migration, live provider/model request,
stand change, branch change or commit was performed. Earlier uncommitted 5.6/5.7
work was preserved.


## Post-review corrections

The subsequent [review-fix verification](verification-review-fixes.md) records
Red/Green fixes for reusable group preview intents, incomplete fixture validation,
and bounded reporting reads that do not lock Run execution. It supplements the
original task evidence without changing completed task identifiers.
