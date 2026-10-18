# Review corrections for tasks 5.1–5.3

## Scope

These corrections address the three review findings while preserving the
provider-independent connection/application boundaries. Task 5.4, Project
grants, Factory manual admission, Source polling and Reporter behavior remain unchanged. No
dependency, migration, service, commit or branch is added.

Connection names reuse the domain's 128-byte limit through the application
boundary. OpenAPI documents UTF-8 byte validation as well as its character bound.
Retained credential status spans immutable metadata versions; its response schema
does not claim the current version's 256-capability ceiling limits that history.

Private temporary names are deterministic for the exact installed destination
and administrative operation. The existing native directory lock protects every
writer, including legacy provisioning. Recovery validates private ownership,
permissions, regular-file type, link count, ancestry and inode identity before
unlinking an exact temporary path. Discard retains its cancellation marker and
erases partial preparation/activation writes plus interrupted legacy installation
bytes for the same handle. No directory scan or removal of active material is
needed, and incomplete JSON cannot prevent owned temporary cleanup.

The first expiry transition records its accepted actor, request identity, safe
operation identity and trusted cleanup request time. PostgreSQL reuses the
existing atomic mutation/audit/outbox commit helper; Memory records the same
transition. Terminal cleanup retries and acknowledgements add no duplicate
transition evidence. Cleanup time and expiry cutoff are separate trusted fields;
HTTP computes both from one server-clock observation.

## TDD evidence

| Public boundary / behavior | Observed Red | Observed Green |
| --- | --- | --- |
| OpenAPI and typed metadata publication | An ASCII name of 129 characters passed the documented ceiling but failed typed publication | Draft and representation share the 128-byte domain-derived bound |
| REST lifecycle inspection | A response with 257 retained credential references exceeded documented `maxItems: 256` | The schema accepts history spanning metadata versions |
| Private publisher cleanup after interruption | Cleanup left partial token bytes in a temporary file | Repeated discard erases the private crash snapshot without provider calls |
| Interrupted legacy installation | The deterministic crash test still found token bytes after cleanup of the same installed handle | Bounded cleanup covers preparation, activation and legacy temporary destinations |
| Memory expiry audit | No fact retained the cleanup operator | Exactly one attributed fact remains despite repeated cleanup |
| PostgreSQL expiry audit | The audit query returned no cleanup actor | Actor, request ID and cleanup time match; each expiry has exactly one transactional outbox event |

The final process-death test writes an incomplete private-storage snapshot in a
disposable child and aborts without unwinding. The parent reloads the public
publisher, checks the partial secret exists, then checks repeated discard removes
it. The snapshot covers preparation, activation and legacy installation. An
earlier kernel file-size-limit probe established the original failure, but was
replaced because EFBIG and SIGXFSZ delivery ordering varies. The retained test uses
no sleeps, timeout increases, global serialization or production fault hooks.

## Commands

```sh
CARGO_INCREMENTAL=0 cargo test --locked -p octacity-server-api-rest --test integration_administration integration_openapi_name_limit_matches_publication_validation
CARGO_INCREMENTAL=0 cargo test --locked -p octacity-server-api-rest --test integration_administration
CARGO_INCREMENTAL=0 cargo test --locked -p octacity-github --test work_credentials cleanup_after_process_death_erases_partial_private_writes
CARGO_INCREMENTAL=0 cargo test --locked -p octacity-server-store credential_preparation_cleanup_is_bounded_and_fences_expired_operations
OCTACITY_POSTGRES_URL=<disposable-service> CARGO_INCREMENTAL=0 cargo test --locked --all-features -p octacity-server-store-postgres --test integration_administration administration_cleanup_is_bounded_and_recoverable -- --ignored
CARGO_INCREMENTAL=0 cargo test --locked -p octacity-server-secrets -p octacity-server-store -p octacity-github -p octacity-server-vcs -p octacity-delivery-protocol -p octacity-server-api-rest
CARGO_INCREMENTAL=0 cargo test --locked -p octacity-server-application
OCTACITY_POSTGRES_URL=<disposable-service> CARGO_INCREMENTAL=0 cargo test --locked --all-features -p octacity-server-store-postgres --test integration_administration --test integration_connections --test migrations --test factory_discovery -- --ignored
CARGO_INCREMENTAL=0 cargo clippy --locked -p octacity-server-secrets -p octacity-server-store -p octacity-server-application -p octacity-github -p octacity-server-store-postgres -p octacity-server-api-rest --all-targets --all-features -- -D warnings
python3 -m unittest discover -s tools/tests
python3 tools/check_architecture.py
cargo fmt --all -- --check
openspec validate add-github-work-source --strict
git diff --check
# From ui/, with the existing locked dependency installation:
CARGO_INCREMENTAL=0 pnpm api:generate
pnpm typecheck
```

## Results

- Six-package provider/core/REST regression: 311 tests passed, including doctests.
- Full application suite: 440 unit tests, 89 integration tests and 3 compile-fail
  doctests passed (532 total).
- PostgreSQL: 38 tests passed with the default parallel harness: 5 administrative,
  4 connection, 6 migration and 23 Factory/discovery contracts.
- GitHub credential contracts were rechecked after the final legacy temporary
  cleanup correction: all 29 passed, covering all three crash snapshots.
- Tools: 432 tests succeeded with 8 existing platform/environment skips.
- Strict Clippy passed for the six changed Rust packages, all targets/features.
- Architecture (63 packages, 253 edges), formatting, strict OpenSpec validation,
  diff whitespace, OpenAPI client generation and TypeScript checking passed.

The Rust groups above cover 881 distinct tests including doctests; focused
Red/Green reruns are not added to that total. The disposable PostgreSQL container
and its anonymous volume were stopped and removed. The four pre-existing local
stand containers were left running. Validation ran on `main`; planning
progress remains 22/36 with task 5.4 pending.
