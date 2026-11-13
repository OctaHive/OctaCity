# Tasks 6.1–6.2: review corrections

## Scope

Snapshot detail now verifies the request's original admission security partition
through the existing scoped request read before looking up private preparation.
A preparation hidden by another partition cannot be treated as a request that
never prepared input. Other authorized Issue viewers can still read execution
metadata and its original digest; private snapshot detail is unavailable. No
store port, migration, retention setting or new authorization capability is added.

`IssueSnapshotValue` uses an inclusive `anyOf` because empty material text and
dependency arrays are the same JSON value, `[]`. The pinned alias determines
which collection it represents. Nonempty dependency references and material text
remain valid; a mixed array is rejected. The shared test validator now implements
exclusive `oneOf` separately from inclusive `anyOf`, so overlapping branches can
no longer hide schema drift.

The native incompatible-code-subject readiness fixture explicitly selects the
same classification as its Source preparation profile. Its Flow has no pinned
admission policy, so omission must remain invalid. This restores the intended
readiness test without weakening the production scheduling guard.

## Observed Red/Green

Tests exercise authorized application queries, the exported OpenAPI contract and
the existing native Source worker boundary. All four cycles observed a behavioral
failure before changing the relevant implementation or fixture. Compilation and
environment failures were not used as Red.

| Test | Red | Green |
| --- | --- | --- |
| `pending_snapshot_cannot_reconstruct_expired_private_input_from_another_security_scope` | A lost publication acknowledgement left a real pending preparation. Its owner saw expiry, but another authorized reader reconstructed an available snapshot. | Both expiry reads return unavailable with the original digest. Same-scope unprepared review and cross-scope metadata reads remain usable; no Issue/Factory mutation or Run appears. |
| `openapi_validator_requires_exactly_one_one_of_variant` | Two matching branches incorrectly satisfied `oneOf`. | Exactly one match is required; `anyOf` permits overlapping matches, and both reject zero matches. |
| `issue_snapshot_openapi_accepts_canonical_empty_dependencies` | The exported `IssueSnapshotValue` rejected `[]` because two `oneOf` array branches matched. | Empty and nonempty dependencies and material text pass; mixed arrays fail. |
| `issue_snapshot_incompatible_code_subject_waits_with_existing_run_without_fallback` | Against disposable PostgreSQL, request creation failed with `Application(InvalidInput)` before testing readiness. | Explicit profile classification restores the intended incompatible-subject wait assertions; one native test passed. |

Targeted commands (each run before and after its fix):

```sh
export CARGO_INCREMENTAL=0
cargo test --offline --locked -p octacity-server-application --lib --all-features \
  pending_snapshot_cannot_reconstruct_expired_private_input_from_another_security_scope -- --nocapture
cargo test --offline --locked -p octacity-server-api-rest --test v1_handlers \
  openapi_validator_requires_exactly_one_one_of_variant -- --nocapture
cargo test --offline --locked -p octacity-server-api-rest --test v1_handlers \
  issue_snapshot_openapi_accepts_canonical_empty_dependencies -- --nocapture
# OCTACITY_POSTGRES_URL selects only the disposable local review service.
cargo test --offline --locked -p octacity-server --lib --all-features \
  issue_snapshot_incompatible_code_subject_waits_with_existing_run_without_fallback -- --ignored --nocapture
```

Documentation clarifies the security-scope requirement and its relation to
retention. Documentation and formatting changes have no separate executable
behavior requiring TDD.

## Regression qualification

- Application Issue owner suite: **202 passed**, no failures or ignored tests.
- REST, all targets/features: **347 passed** (13 library, 28 administration,
  32 wire contracts, 273 handlers and one deterministic export test), no failures
  or ignored tests. The shared validator test runs in both administration and
  handler test binaries.
- Native Source snapshot suite: **19 passed**, no failures or ignored tests,
  using an isolated PostgreSQL 17 container and test-owned databases. The
  disposable container was stopped and removed afterwards. macOS linking emitted
  a large unwind-table warning; the native tests passed.
- Application/REST Clippy, all targets/features with warnings denied: passed.
- OpenAPI export and Console API generation, TypeScript, ESLint and Prettier:
  passed. These corrections add no Console interaction or form behavior.
- Rust formatting, architecture (64 packages, 263 edges), strict OpenSpec and
  diff whitespace: passed.

```sh
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server-application --lib --all-features issues::tests::issue_tests -- --quiet
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server-api-rest --all-targets --all-features
# Uses the same disposable PostgreSQL service as the targeted native cycle.
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server --lib --all-features runtime::sources::tests::worker::issue_snapshots -- --include-ignored
CARGO_INCREMENTAL=0 cargo clippy --offline --locked \
  -p octacity-server-api-rest -p octacity-server-application --all-targets --all-features -- -D warnings
CARGO_INCREMENTAL=0 cargo run --offline --locked --quiet \
  -p octacity-server-api-rest --example export_openapi > /tmp/octacity-6162-openapi.json
cd ui
OCTACITY_OPENAPI_SCHEMA_FILE=/tmp/octacity-6162-openapi.json pnpm api:generate
pnpm typecheck
pnpm lint
pnpm format
cd ..
CARGO_INCREMENTAL=0 cargo fmt --all -- --check
python3 tools/check_architecture.py
openspec validate add-issue-classification-snapshots --strict
git diff --check
```
