# Task 2.11 — Planning HTTP and generated contracts

## Scope

The existing Issue service now translates property choices, current/exact catalog
reads, reviewed immutable publication, planning replacement and bounded child
reads into the already authorized application commands from 2.9–2.10. There is
no second validation engine, directory, hierarchy, provider SDK or mutation
transaction. Manual creation accepts optional planning while keeping the original
flat content body and original empty-planning fingerprint. Issue/revision detail
includes nonempty planning; summary/header lists omit its bodies.

Catalog publication requires `Idempotency-Key` and closed
`{expected_version, definition}` JSON. The required reviewed version supports
zero without changing the existing positive Issue `If-Match` contract. Planning
edits retain positive Issue `If-Match` plus the current catalog version in the
closed snapshot. Definition administration is independent of Issue editing.
Queries and responses use `no-store`; body values cannot set status, actor,
permissions or execution. Parent selection reuses visible Project Issue discovery,
and child reads declare the typed Issue visibility contract before paging.

Closed OpenAPI types publish supported scalar/choice kinds, count/value/UTF-8
bounds and directory availability. People come only from the server-authorized
optional directory; its absence/outage leaves Unassigned usable and preserves
unchanged assignments. No service principal is manufactured as a person.

Existing requirements UI compatibility is limited to filtering ownership through
its original four content fields. New property controls remain task 2.12. No
provider, Factory, Work, Run, Source or Reporter execution path was added or changed.
The dependency graph remains 63 packages and 256 edges. Existing uncommitted
2.9/2.10 and Docker-context work was preserved; no commit was created.

## Red/Green evidence

The main public seam is the actual Axum router, composed with real authorized
application handlers and memory persistence. Responses are checked against the
published OpenAPI; a generated-client test qualifies typed paths, bodies and
headers through the existing transport. Existing browser journeys qualify the
small requirements-editor compatibility correction.

Rust commands use `CARGO_PROFILE_DEV_DEBUG=0`, `CARGO_PROFILE_TEST_DEBUG=0`,
`CARGO_INCREMENTAL=0` and `CARGO_BUILD_JOBS=2`.

```sh
cargo test --locked -p octacity-server-api-rest --test v1_handlers FILTER
```

| FILTER / other command | Observed Red before implementation | Green |
| --- | --- | --- |
| `issue_http_planning_choices` | Choices route returned 404 | Unavailable directory is explicit, empty catalog and manual creation need no provider |
| `issue_http_planning_catalog` | Catalog route returned 404 | Current/exact versions, version-zero review, bounded closed publication, retirement, stale conflict and original replay |
| `issue_http_planning_create` | Legacy-only content parser rejected optional planning | Scalar/choice/date creation and explicit edit, positive ETag, exact revision/replay and unchanged content/status/applicability/history |
| `issue_http_planning_children` | Child route returned 404 | Visible parent and direct children checked before pagination; hidden/wrong-parent cursors, cycles and hidden creation replay reject |
| `issue_openapi_planning` | Planning schemas were absent | Closed bounded schemas match actual HTTP documents, all references resolve and routes retain exact authorization mappings |
| `issue_http_planning_rejects` | 33 repeated labels were silently collapsed and accepted with 201 | Shared deserialization rejects duplicate/oversized wire lists; unsupported/nested/fractional/invalid-date/reserved inputs fail before mutation |
| UI `pnpm exec tsc --noEmit` with new `issuePlanningContracts.test.ts` | Generated types and new routes were absent | Existing generator produces closed planning unions, exact catalog-zero URL, reviewed publication body and positive Issue mutation headers |
| UI `pnpm exec vitest run src/features/issues/IssuesView.test.tsx -t 'keeps planning ownership'` | Requirements ownership displayed planning fields rather than only Title | Existing editor narrows ownership to its original content-field list without implementing 2.12 controls |

The initial 97 HTTP scenarios passed before implementation. Additional regression
scenarios verify separate definition/edit rights, hidden Project reads, authorized
person choices, directory loss and retained responsibility. The existing complete
allow/deny inventory now exercises all 139 routes, including every new route;
it still asserts denial before application dispatch and protected side effects.
Fixture compilation/path mistakes and an expected 409 directory-conflict assertion
were corrected; they are not counted as behavior Red or production fixes. No
environment failure was used as Red evidence.

## Final verification

```sh
cargo test --locked -p octacity-server-store-postgres \
  --test issues --test issue_workflows --test migrations --test projects -- --ignored
# 42 passed against a disposable PostgreSQL 18.6 service.
cargo test --locked -p octacity-server-store --lib
# 81 passed.
cargo test --locked -p octacity-server-application --lib issue_
# 38 passed; unrelated application scenarios were not rerun.
cargo test --locked -p octacity-server-api-rest --test v1_handlers
# 105 passed, including all 139-route allow/deny checks.
cargo check --locked -p octacity-server
cargo clippy --locked -p octacity-server-store -p octacity-server-application \
  -p octacity-server-api-rest -p octacity-server-store-postgres --all-targets -- -D warnings
cargo fmt --all -- --check
python3 tools/check_architecture.py
openspec validate add-factory-work-card --strict
git diff --check
```

Using pinned pnpm 12.8.1 and the bundled Node 24 runtime from `ui/`:

```sh
pnpm api:generate
pnpm exec tsc --noEmit
pnpm test:unit
# 30 Node tests and 503 Vitest tests passed.
pnpm lint
pnpm format
pnpm build
# Type checking, frontend architecture and bundle budgets passed.
pnpm exec playwright test tests/browser/issues.spec.ts
# 3 passed: manual HTTP create/focus, Russian 390px keyboard/history,
# and desktop header/properties layout.
```

All checks passed. Complete workspace/browser suites and full container image
builds were not rerun. PostgreSQL used an existing cached image, loopback-only
random port, memory limit and tmpfs; the dedicated container was removed after
tests. User stand containers, volumes, credentials and target artifacts were not
removed. Free space was checked before builds (initially 5.1 GiB, approximately
6.6–7.0 GiB during compilation); no debug/incremental artifacts or image builds
were requested. Documentation-only edits were validated with OpenSpec and diff
checks; they have no executable TDD behavior.
