# Task 2.7 verification

## Scope and boundaries

The released server composes ten Project-scoped Issue REST operations independently
of Factory, repositories and integrations: current list/create/read/edit, revision
headers/exact revision, manual move review/command, status history and reviewed
workflow adoption. The route owner stays explicit as
`/api/v1/projects/{project_id}/issues`; the proposed design table and its Russian
copy now name the same paths. This preserves the existing Project/Issue store
boundary without another identity lookup or execution authority.

The REST adapter translates closed documents and strong Issue ETags into the
existing authorized commands. Current visibility precedes body/replay/history
access; current summary visibility precedes ordering, limits and cursors. A list
cursor must be visible in the current scope and belong to the selected Project.
Revision and organizational history use bounded exclusive revision cursors. These
lists omit requirements documents; dedicated content reads apply no-store.

Direct manual choices include finite ownership/origin/evidence/permission reasons.
The mutation and read projection share pure manual-edge eligibility, and reading
choices separately checks the manual transition right without reserving a move.
Adoption echoes the reviewed field policy as a precondition, never as an authority
setter. Edit, move, workflow administration/adoption and execution remain distinct.
No new Work/Run, provider call, executor, migration or reporting owner is added.

API transport tests use real application handlers and the application's opt-in
`test-support` fixture rather than a forbidden API-to-store dependency. Production
builds do not enable that fixture. The existing mutation audit/outbox/replay paths
remain authoritative. Public read failures identify their actual discovery
operation rather than incorrectly naming task creation or editing.

## Observed Red/Green

Environment failures were repaired before implementation and were not counted as
Red. New behavior was specified through public HTTP or store boundaries first.
The focused REST invocation is:

```sh
cargo test --locked -p octacity-server-api-rest --test v1_handlers <filter>
```

| Filter / boundary | Observed Red | Green |
| --- | --- | --- |
| `issue_http_manual_create_read_and_replay_need_no_factory_or_provider` | Issue HTTP composition API absent | Manual draft, exact body read and original scoped replay |
| `issue_http_edit_preserves_exact_revision_replay_and_rejects_implicit_status_setters` | PATCH returned 405 | Strong preconditions, old revision, original replay after later edit, reserved setters rejected |
| `issue_http_list_filters_visibility_before_limit_cursor_and_content` | GET collection returned 405 | Hidden-first pagination, visible cursor, scope-change/hidden cursor rejection, summaries only |
| `issue_http_revision_history_is_bounded_and_bodies_have_dedicated_no_store_reads` | Revision collection returned 404 | Header pagination, limits and dedicated no-store body reads |
| `issue_http_manual_moves_use_exact_policy_status_and_history_without_execution` | Move review returned 404 | Shared direct-edge/ownership/origin/evidence policy, explicit move, exact replay and history |
| `issue_http_adoption_requires_review_current_policy_and_separate_admin_rights` | Adoption route returned 404 instead of authorized denial | Separate administration, closed nested review, correspondence/ownership checks and original adoption replay |
| `issue_openapi_describes_closed_bounded_no_store_and_replay_contracts` | Replay schema/headers absent | Closed schemas, limits, UUIDs, If-Match/Idempotency-Key, no-store and replay responses |
| PostgreSQL Issue discovery/revision/field-policy public ports | Required read APIs absent | Memory parity, hidden/foreign cursors, headers/ownership reads and no task mutation |
| PostgreSQL invalid discovery requests | Correct read-operation classifications absent | Read failures no longer identify mutation operations |
| Released runtime Issue journey | Uncomposed route lacked its required no-store response | Real PostgreSQL-backed manual journey and exact original replay after server restart |
| Console generated-client typecheck | Issue schemas, paths and PATCH operation absent | Generated closed task/move/adoption types and exact strong-version requests |

Existing Issue/workflow application behavior was run before extracting the shared
manual-edge eligibility: 28 focused application tests passed. Additional HTTP
permission regression verifies that text edit rights offer/authorize neither
manual movement nor workflow administration. The all-route authorization fixture
covers the new operations and PATCH through the existing canonical request context.

## Commands and qualification

Rust builds reuse the existing target with debug information and incremental
compilation disabled; no dependency versions or lockfile entries are changed.

```sh
export CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0
cargo test --locked -p octacity-server-application -p octacity-server-store \
  -p octacity-server-api-rest --lib --test v1_handlers
cargo test --locked -p octacity-server-api-rest --test v1_handlers
# Set OCTACITY_POSTGRES_URL to a disposable PostgreSQL service before these:
cargo test --locked -p octacity-server-store-postgres \
  --test issues --test issue_workflows --test migrations -- --ignored
cargo test --locked -p octacity-server --lib runtime_issue_ -- --ignored
cargo clippy --locked -p octacity-server-store -p octacity-server-application \
  -p octacity-server-store-postgres -p octacity-server-api-rest \
  -p octacity-server --all-targets -- -D warnings
cargo fmt --all -- --check
python3 tools/check_architecture.py
openspec validate add-factory-work-card --strict
```

UI checks use the existing pinned pnpm 12.8.1 and supplied Node 24.19 runtime,
without installing packages or updating dependency versions. From `ui`:

```sh
pnpm api:generate
pnpm typecheck
pnpm exec vitest run src/api/issueContracts.test.ts src/api/issueWorkflowContracts.test.ts
pnpm lint
pnpm test:unit
pnpm build
pnpm exec prettier --check src/api/issueContracts.test.ts
```

The local pnpm executable used is
`/Users/roman/.cache/node/corepack/v1/pnpm/12.8.1/pnpm-native`, with
`/Users/roman/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/bin`
prepended to PATH. The generated API files remain reproducible ignored artifacts.

- Rust regression: **693 pass** — 504 application, 81 store, 12 REST library and
  96 REST HTTP tests, including the unchanged legacy Factory/Run contracts.
- PostgreSQL: **29 pass** — 15 Issue, 8 workflow and 6 migration/rollback tests.
- UI: generation/typecheck, both focused client tests, lint, all **477 unit
  tests**, formatting and production build/bundle architecture pass.
- Clippy passes all targets of all five affected packages with `-D warnings`.
- Formatting, whitespace, architecture (63 packages, 256 edges) and strict
  OpenSpec validation pass.

The released runtime qualification passes **2 PostgreSQL-backed tests**: the
existing workflow HTTP contract and the new complete manual Issue journey with
original replay after restart. The final REST rerun passes all 96 HTTP tests.
The separately owned disposable PostgreSQL container has been stopped and removed.
Task 2.7 is complete; implementation progress is **9/56**. No commit or archive
was performed. Documentation-only updates were validated rather than assigned
an executable Red/Green cycle.
No Console card/browser journey is claimed by this backend slice; task 2.8 remains
pending. No native/live forge or delivery qualification is added. The personal
Factory stand, its data and credentials are unchanged by task 2.7; the temporary
PostgreSQL test service is separately owned and uses bounded temporary storage.
Free disk space is monitored throughout (approximately 10–12 GiB), with no target
cleanup, full container rebuild, timeout increase or test serialization.
