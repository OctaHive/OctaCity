# Review corrections after tasks 5.6–5.8

## Scope and boundaries

Corrected the three reviewed issues through public boundaries: authorized Source
preview/validation, exported OpenAPI fixture validation, and authorized Source
reporting diagnostics against a real disposable PostgreSQL service. No task beyond
5.8, new provider protocol, database migration, Issue command or UI feature is added.

All Cargo commands and client generation used these resource bounds:

```sh
export CARGO_INCREMENTAL=0
export CARGO_PROFILE_DEV_DEBUG=0
export CARGO_PROFILE_TEST_DEBUG=0
export CARGO_BUILD_JOBS=2
```

Only one Cargo operation ran at a time. No commit or branch change was performed.

## Preview publication intent: Red → Green

```sh
cargo test --locked -p octacity-server-application --lib source_preview_freezes_exclusive_members_and_explains_ambiguity_before_activation
```

Red: the returned configuration contained replacement intents for retained input
v1 members instead of the submitted group-only intent. Green: the same test passed
with the submitted intent preserved in the response and expanded members used only
for read preparation. It also repeats preview using that response and validates it
for group replacement without replacing the unchanged input versions.

This preserves exact input/group version semantics for draft and retained members;
provider selection, manual admission and optional card links are unchanged.

## Fixture contract validation: three Red → Green slices

```sh
cargo test --locked -p octacity-server-api-rest --test v1_handlers integration_fixture_validation_rejects_non_string_credential_values
cargo test --locked -p octacity-server-api-rest --test v1_handlers integration_fixture_validation_rejects_oversized_utf8_status
cargo test --locked -p octacity-server-api-rest --test v1_handlers integration_fixture_validation_rejects_oversized_preview_pages
```

Each test first failed because the existing validator accepted the invalid value.
Each then passed after the corresponding small correction:

- A credential value of `42` fails the declared `string|null` union; string/null
  values remain valid.
- A 2050-byte Cyrillic status fails the 2048-byte ceiling; exactly 2048 bytes pass.
- A preview with 101 candidates fails `maxItems: 100`; exactly 100 pass.

The shared helper now checks type arrays, UTF-8 byte ceilings and array minimum/
maximum cardinality. Existing positive wire fixtures, typed intents and REST
responses continue to pass. No dependency or second validation subsystem was added.

## Reporting diagnostics without Run locks: Red → Green

Using the explicitly created disposable PostgreSQL service:

```sh
OCTACITY_POSTGRES_URL=postgres://postgres@127.0.0.1:32789/postgres \
  cargo test --locked -p octacity-server --lib source_reporting_diagnostics_does_not_wait_for_a_run_writer -- --ignored
```

Red: an authorized diagnostics query failed with `Application(Store(Unavailable))`
while another connection held the Run row lock. Green: the same query returned the
original Run association and pending reporting status while that writer still held
its lock.

The test retains the writer transaction until the query completes. A separate
read connection has `lock_timeout = '1ms'` so an accidental locking read fails
without a wall-clock latency assertion. Production timeouts and test parallelism
are unchanged. Tokio's existing controlled-I/O fixture keeps this deterministic
under scheduler load.

The application now uses one bounded reporting read in
`FactorySourceDiagnosticsStore` instead of `FactoryRunStore::factory_run_snapshot`.
PostgreSQL selects latest outbox metadata for operations registered atomically with
accepted actions after checking the original Source Reporter. Existing immutable
association fields and Project scope restrict the read. No private preparation
payload, full Flow history or execution lock is loaded. The existing acceptance
registry also covers migrated history and excludes unrelated Reporter actions.

Safe reporting DTOs are reused from the read port, preserving the REST shape and
removing the application's snapshot processing. The 64-operation ceiling is shared
with OpenAPI. A fresh exporter document was compared with the snapshot captured
during review: the complete public OpenAPI JSON is unchanged. Memory and PostgreSQL retain the same operation ordering and safe
state vocabulary. Optional authorized Issue links and native correlations remain
independent of reporting and execution authority.

## Regression and repository checks

```sh
OCTACITY_POSTGRES_URL=postgres://postgres@127.0.0.1:32789/postgres \
  cargo test --locked -p octacity-server --lib reporting::ordering::postgres -- --ignored
# 9 passed in the normal parallel test run, including restart, migration,
# acceptance order, unrelated Reporter filtering and late-write recovery.
cargo test --locked -p octacity-server-store -p octacity-server-api-rest --lib --tests
# 81 Store and 124 REST tests passed.
cargo test --locked -p octacity-server-application --lib factory_admission_tests
# 76 passed, including provider peers, manual intake, visibility and optional cards.
cargo test --locked -p octacity-server --lib
# 156 passed; 9 PostgreSQL tests ignored here and run explicitly above.
cargo clippy --locked -p octacity-server-store -p octacity-server-application -p octacity-server-store-postgres -p octacity-server-api-rest -p octacity-server --all-targets --all-features -- -D warnings
pnpm --dir ui api:generate
pnpm --dir ui typecheck
pnpm --dir ui exec vitest run src/api/integrationContracts.test.ts src/api/client.test.ts src/features/factory/api.test.ts
# 40 passed.
cargo fmt --all -- --check
git diff --check
python3 tools/check_architecture.py
# 63 packages, 253 dependency edges.
python3 tools/security_matrix.py validate
node ui/scripts/verify-architecture.mjs
openspec validate add-github-work-source --strict
openspec validate add-factory-work-card --strict
```

Reference-document updates have no executable behavior and are validated through
whitespace/OpenSpec checks rather than Red/Green. Earlier uncommitted task work
and completion identifiers are preserved.

Free space remained approximately 16–17 GiB; `target` grew from about 6.5 GiB to
about 6.9 GiB. The disposable PostgreSQL container was stopped and removed after
testing. No user files/build cache were deleted, running stand changed, external
provider contacted or paid model invoked.
