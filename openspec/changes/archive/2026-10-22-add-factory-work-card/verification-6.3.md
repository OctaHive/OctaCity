# Task 6.3: durable discussion and dedicated HTTP body access

## Boundaries

The existing `IssueDiscussionStore` now has a PostgreSQL adapter. Additive
migration 0089 retains separate comment metadata/body columns and an Issue-local
lifetime counter independent of the card revision. The adapter reuses the common
Issue lock, scoped management mutation identity, transaction, audit and outbox
publisher. No second replay journal, execution publisher, provider dispatcher or
outbox is added. Roots and replies share the validated deployment quota; exact
replay precedes newly narrowed limits and returns original metadata.

REST exposes `POST /api/v1/projects/{project_id}/issues/{issue_id}/comments` and
`GET /api/v1/projects/{project_id}/issues/{issue_id}/comments/{comment_id}` over
the already authorized application handlers. Append requires `Idempotency-Key`,
returns 201/200 for applied/replayed metadata-only receipts and identifies the
dedicated body resource in Location. Both successful responses use no-store.
The closed input accepts body and optional root parent only; actor, time,
sequence, target, attachment and control setters reject. OpenAPI and the generated
client retain byte/lifetime hard caps and separate discussion permission mappings.

Current discussion rights and owning-Issue visibility precede retained
parent/replay/body lookup. Metadata editing, transition or execution rights do
not grant discussion body authority. Trusted-network attribution stays honestly
unauthenticated; verified actor provenance and replay scope remain distinct.
Bodies exist only in the dedicated comment record/read. Generic receipts, audit,
outbox, replay records, Debug and errors exclude them. The existing conservative
sensitive-material check remains the single marker policy, not a complete secret
detector. SQL values are bound parameters and no new dependency is introduced.

The production composition installs the optional handler without execution,
provider or object-store reads. A real PostgreSQL-backed HTTP test creates a
manual draft, appends discussion, fully restarts ServerRuntime, replays the same
command and reads the original content with object storage deliberately
unreachable. The Issue resource remains unchanged.

Merged timeline/HTTP metadata pagination remains task 6.4, Console discussion
forms remain 6.5, and explicit retention/holds/backup content qualification remains
8.2. This task does not claim those behaviors or live provider qualification.

## TDD evidence

Rust commands use `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0
CARGO_PROFILE_TEST_DEBUG=0 CARGO_BUILD_JOBS=2` and `--offline`. Database tests also
set `OCTACITY_POSTGRES_URL` to the explicitly disposable PostgreSQL service and
use `-- --ignored`.

| Command/filter | Observed Red | Observed Green |
| --- | --- | --- |
| `cargo test -p octacity-server-store-postgres --test issues discussion_matches -- --ignored` | PostgreSQL discussion trait methods absent | Memory parity, exact identity/actor/sequence/time/digest/body, original replay after restart and lowered limits; no Issue revision change |
| `cargo test -p octacity-server-store-postgres --test issues discussion_concurrent -- --ignored` | All three concurrent appends succeeded with a quota of two | Exactly two winners with unique sequences 1/2; classified quota rejection, stricter UTF-8 byte bound and bounded metadata pages |
| `cargo test -p octacity-server-store-postgres --test issues discussion_replies -- --ignored` | Nested reply was accepted | Only same-Issue retained root replies pass; nested/foreign/unknown parents reject without consuming sequence or replay identity |
| `cargo test -p octacity-server-api-rest --test v1_handlers issue_discussion_http_append` | REST discussion composition API absent | Closed append and independently authorized no-store body read; applied 201, replay 200 and unchanged manual card |
| `cargo test -p octacity-server-api-rest --test v1_handlers issue_discussion_openapi` | Discussion schemas absent | Closed request/response definitions, 16,384-byte/1,000-lifetime caps, UUID paths, scoped replay and separate permission inventory |
| `cargo test -p octacity-server --test issue_discussion -- --ignored` | Real runtime returned 503 for uninstalled discussion | Production composition returns 201, survives full server restart, returns original replay/body, needs no provider or reachable object storage |
| `cargo test -p octacity-server-store-postgres --test issues discussion_metadata_continuations -- --ignored` | A continuation beyond the lifetime range returned an error in PostgreSQL | Empty terminal page matches the existing memory contract, including `u64::MAX` without SQL overflow |

Additional characterization tests cover concurrent exact-key replay, different
verified actor/security scopes, changed body/parent/target conflicts, one safe
evidence set, and publication interruptions at audit insertion, outbox insertion
and replay completion. Each failure leaves no body, sequence, quota, replay,
audit or outbox mutation; retry allocates sequence 1. Generic persisted evidence
contains no body. Forward migration from schema 88 and repeated current migration
preserve an existing manual Issue. Seven PostgreSQL discussion tests pass.

HTTP characterization additionally checks current rights before original replay,
separate append/body permissions, owning-Issue denial before content lookup,
unchanged usability without metadata-update/transition/execution grants, exact
UTF-8 boundary, sensitive-pattern/forged-field rejection, nested reply rejection
and safe errors without body echoes. Four discussion HTTP/schema tests pass.

Fixture/compiler repairs are not counted as behavioral Red: correcting a test
trait import/action name, using the common SQL error mapper and making the
injected replay-completion CHECK explicitly reject SQL NULL. The first full REST
regression identified two missing requests in the exhaustive authorization test
fixture (153 requests versus 155 registered operations). Adding both typed cases,
allow/deny calls and composition keeps the existing exhaustive gate intact;
production authorization was not relaxed.

## Regression verification

| Command | Result |
| --- | --- |
| `cargo test --offline -p octacity-server-application --lib issue_` | 124 passed |
| `cargo test --offline -p octacity-server-application --lib management_security` | 21 passed |
| `cargo test --offline -p octacity-server-store --lib` | 81 passed |
| `cargo test --offline -p octacity-server-store-postgres --test issues --test migrations --test issue_workflows --test issue_execution -- --ignored` | Issues 50, migrations 7, workflows 8 and execution 19 passed |
| `cargo test --offline -p octacity-server-api-rest --lib --test v1_handlers` | 12 library and 141 HTTP tests passed after the inventory fixture update |
| `cargo test --offline -p octacity-server --test issue_discussion -- --ignored` | PostgreSQL-backed runtime restart test passed |
| `cargo clippy --offline -p octacity-server-api-rest -p octacity-server-store-postgres -p octacity-server-application -p octacity-server-store -p octacity-server --all-features --all-targets -- -D warnings` | Passed; REST is checked again after the inventory fixture update |
| `pnpm --dir ui run api:generate`, `format`, `lint`, `typecheck`, `test:unit`, `build` | Passed with pinned Node 24.21.0/pnpm 12.8.1; 33 script checks and 580 Vitest tests, architecture/bundle budgets passed |
| `cargo clippy --offline -p octacity-server-api-rest --all-features --all-targets -- -D warnings` | Passed after the authorization inventory fixture update |
| `cargo fmt --all -- --check` | Passed |
| `python3 tools/check_architecture.py` | Passed: 63 packages, 256 dependency edges |
| `openspec validate add-factory-work-card --strict` | Passed |
| `git diff --check` | Passed |

The generated API directory remains ignored as designed; no generated build output
or dependency lockfile changed. Existing 6.1/6.2 work and the prior MutexGuard CI
fix are preserved. Documentation-only edits use specification/diff validation,
not executable TDD.

Builds kept incremental compilation and debug symbols disabled with two Rust
jobs. Checks retained approximately 9.6–11 GiB free. PostgreSQL used an existing
image with a bounded disposable tmpfs data directory, then was stopped/removed;
no database data or image download was left behind. The temporary pinned Node
toolchain was SHA-256 checked against the official release manifest and removed
after Console verification. No global toolchain, dependency, lockfile or checkout
was changed. Task 6.3 is complete: 35 of 56 tasks checked. No commit was created.
