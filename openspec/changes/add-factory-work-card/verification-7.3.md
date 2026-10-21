# Task 7.3 verification — shared action REST and Console

## Scope and shared authority

Global, Project and Issue collections read the 7.2 PostgreSQL projection through
one collection port. Current Run, Project and linked Issue scopes precede counts,
coverage, ordering and continuation; hidden linked Work never becomes unlinked.
Counts and pages share a read-only repeatable-read observation. Canonical cursors
bind selection, security partition and all current visibility scopes. All reads
are no-store, use trusted time, and perform no backfill or provider/execution work.

The exact-attempt observation reuses existing Run discovery/Project authorization,
original association reads and the common eligibility descriptor. It exposes safe
recorded/owner-consumed provenance without response bodies or reasons. Optional
Project/Issue selection cannot substitute another card or a newer execution.
Candidate acceptance comes from the existing exact-current Decision summary.
Existing response, typed operation and retry commands remain the sole mutation
boundary, including their Run preconditions, replay scope and owner processing.

Frozen permitted response shapes are added to the common descriptor. Migration
0092 discards only the older derived projection format; the existing bounded
worker rebuilds it. No new index owner/version protocol or source invalidation
writes are introduced.

Console uses one workspace for `/factory-actions`, Project selection and the
card's Actions section. The Run page and workspace share response/operation forms,
schema inputs, generated node-control clients and confirmed-command handling.
There are no new dependencies, provider branches or approve/close setters.
Private drafts and transport replay identities remain in memory. Exact selection
survives reload; recorded intent remains distinct from owner completion and Issue
acceptance. Unlinked Work and manual task creation remain independent.

## Observed test-first cycles

Commands below ran with an isolated target, two build jobs, incremental compilation
and debug information disabled. PostgreSQL used an explicitly configured disposable
PostgreSQL 18.6 container, a random local port and tmpfs storage.

1. `cargo test --locked -p octacity-server-store-postgres --test factory_discovery shared_action_queue_filters -- --ignored`
   — **Red:** required collection API absent. **Green:** one test passed. It
   covers visibility before count/page/cursor, unlinked Work, Project/Run scopes,
   incomplete coverage and trusted expiry.
2. `cargo test --locked -p octacity-server-application --lib action_response_projection_publishes`
   — **Red:** required response shapes missing from the descriptor. **Green:**
   one test passed; only admitted schemas referenced by allowed outcomes appear,
   with no private input schema.
3. `cargo test --locked -p octacity-server-api-rest --test v1_handlers factory_action_http_scopes`
   — **Red:** shared HTTP application/query API absent. **Green:** one test
   passed for all three scopes, no-store, trusted observation and cross-scope
   cursor rejection before persistence dispatch.
4. `cargo test --locked -p octacity-server-api-rest --test v1_handlers factory_action_openapi`
   — **Red:** closed shared schemas absent. **Green:** one test passed for
   bounded generated contracts and unchanged original guarded mutation endpoints.
5. `cargo test --locked -p octacity-server-application --lib exact_action_observation`
   and `cargo test --locked -p octacity-server-api-rest --test v1_handlers factory_action_http_observes`
   — **Red:** exact authoritative observation API/composition absent. **Green:**
   both passed. The application test records a real response, verifies no premature
   completion, reconciles through the owner, observes consumption and replays the
   winning original version after progress. HTTP omits private values/reasons.
6. `pnpm exec vitest run src/api/factoryActions.test.ts`
   — **Red:** generated shared client absent. **Green:** client tests passed
   for collection paths and exact original control paths/headers. A later
   scope-preservation assertion failed because exact observation omitted its
   selected Project/Issue; after sending the typed query it passed.
7. `pnpm exec vitest run src/shared/FactoryActions.test.tsx`
   — **Red:** shared workspace absent. **Green:** the original interaction test
   passed for two same-named gates in different Runs, exact Run version, explicit
   confirmation and recorded feedback, with no values in navigation/storage.
8. `pnpm exec vitest run src/features/issues/IssueActions.test.tsx`
   — **Red:** all three route/card entries absent. **Green:** three tests passed
   using the same reader and complete empty-state contract without commands.
9. `cargo test --locked -p octacity-server-application --lib issue_action_observation_checks`
   — **Red:** scoped original-association observation API absent. **Green:** one
   test passed with two real manual Issues: only the original associated card can
   observe that target; source Run and Issue state do not change.
10. `cargo test --locked -p octacity-server-store-postgres --test factory_discovery action_response_shape_upgrade -- --ignored`
    — **Red:** schema-91 derived JSON without response shapes remained unreadable
    after upgrade. **Green:** one test passed after migration 0092 invalidated only
    derived rows. Existing backfill restores the same attempt/shapes, and source
    execution is unchanged.
11. `node --test --test-name-pattern='admits measured shared action workspace' scripts/verify-architecture.test.mjs`
    — **Red:** measured new aggregate output exceeded the old budget. **Green:**
    one test passed after a 10 KiB increase in aggregate JavaScript/static budgets
    and four assets for the new lazy workspace/shared form chunks. The test still
    rejects a further unplanned 16 KiB; entry and CSS limits remain unchanged.

Existing Run command/form and Issue tests ran before extracting shared forms and
transport. Those behavior-preserving refactors kept their relevant tests green.
Fixture corrections, query-registry setup errors and environment failures were
not counted as behavioral Red. Documentation/formatting changes have no executable
behavior and use formatting/spec checks instead.

## Regression and integration qualification

The real PostgreSQL linked-action regression creates two admitted Runs associated
with one manual Issue, edits that Issue without retargeting either action, hides
its rows under revoked Issue visibility, submits two concurrent real responses
(one winner), recovers the owner claim and replays the winner after progress.
HTTP qualification rejects malformed bounds/cursors, changed visibility scopes
and incomplete exact selections before dispatch. UI tests additionally cover
412 refresh/new confirmation, incomplete/stale observations, retry, typed delivery
and escalation without generic response bypass, and private-draft disposal when
selecting another execution.

English and Russian browser journeys use the generated HTTP client on a 390 px
viewport: keyboard/focus confirmation, lost acknowledgement with identical replay,
recorded feedback and reload of the exact target. Dedicated primary-route checks
cover labeled semantics, narrow layout and retained data after failed refresh.

| Command | Observed result |
| --- | --- |
| `cargo test --locked -p octacity-server-application --lib` | 658 passed, including common controls, replay, security and manual-Issue regressions |
| `cargo test --locked -p octacity-server-store --lib` | 81 passed |
| `cargo test --locked -p octacity-server-store-postgres --test factory_discovery -- --ignored` | 31 passed |
| `cargo test --locked -p octacity-server-store-postgres --test migrations -- --ignored` | 7 passed |
| `cargo test --locked -p octacity-server-api-rest --test v1_handlers` | 163 passed, including all-operation allow/deny inventory and OpenAPI/route drift checks |
| `cargo clippy --locked -p octacity-server-application -p octacity-server-store -p octacity-server-store-postgres -p octacity-server-api-rest -p octacity-server --all-targets --all-features -- -D warnings` | passed; server composition and all targets checked |
| `cargo clippy --locked -p octacity-server-api-rest --all-targets --all-features -- -D warnings` | passed again after expanding the HTTP authorization fixture |
| `pnpm test:unit` (in `ui`) | 33 script tests and 659 Vitest tests passed |
| `pnpm exec playwright test` (in `ui`) | all 124 passed, including visual snapshots and manual/provider-independent journeys |
| `pnpm typecheck`, `pnpm lint`, `pnpm format`, `pnpm build` (in `ui`) | passed, including architecture/bundle budgets |
| `python3 tools/check_architecture.py` | passed: 63 packages, 256 edges |
| `cargo fmt --all -- --check`, `git diff --check` | passed |
| `openspec validate add-factory-work-card --strict` | passed |

The HTTP authorization fixture now enumerates all four new reads and installs
recording authorized peers, so the existing deny-before-side-effects suite covers
them too. An Issue test fixture initially omitted the new read peer and produced
an unrelated extra error alert; after supplying the same deterministic empty
reader, the full Console suite passed. No production fallback or suppression was
added for either fixture correction.

Measured output is 1,164,603 bytes of JavaScript and 1,252,013 static bytes in 57
assets. Budgets are 1,167,360 and 1,255,424 respectively; the entry and CSS budgets
remain 460,800 and 87,040. Shared chunks include the extracted existing schema
inputs/forms rather than duplicated response implementations.

Free disk space was checked repeatedly: approximately 39 GiB initially and no less
than 36 GiB during this task's builds. The task-owned temporary target (about
2.4 GiB), wrapper/marker and disposable PostgreSQL container were removed after
qualification; shared caches and the pre-existing workspace target were preserved.
No commit or push was requested or performed.


## Review correction — scope-bound observation cache

The exact observation key now includes normalized Project/Issue selection, matching
its existing scoped HTTP request. Omitting selection still produces the original
Run/attempt prefix for command invalidation; no separate cache, transport or
provider-specific mechanism is added.

The regression exercises the real `FactoryActions` component, QueryClient and
router with an unchanged selected Run/attempt. After an authorized observation,
it immediately changes the Project or Issue and receives a scoped `404`. Neither
the previous response form nor the waiting state may appear under that selection,
and no command is sent. This tests rendered behavior rather than cache internals.

- **Red:** `pnpm exec vitest run src/shared/FactoryActions.test.tsx -t 'does not retain another'`
  failed in both Project and Issue cases: the previous Human response checkbox
  remained after the new scoped read failed.
- **Green:** the same command passed both cases after adding selection to the
  existing common key and its observation call.
- The existing successful-response journey additionally observes the subsequent
  `recorded` read state, qualifying that the unscoped target invalidation prefix
  still refreshes the now-scoped observation.

Qualification after the correction:

| Command (in `ui` unless stated) | Observed result |
| --- | --- |
| `pnpm exec vitest run src/shared/FactoryActions.test.tsx src/api/factoryActions.test.ts src/features/issues/IssueActions.test.tsx src/features/factory/FactoryRunCommands.test.tsx` | 22 passed |
| `pnpm test:unit` | 33 script tests and all 661 Vitest tests passed |
| `pnpm exec playwright test tests/browser/factory-actions.spec.ts tests/browser/issues.spec.ts` | 5 passed, including English/Russian exact-action journeys and manual creation without provider/Factory calls |
| `pnpm typecheck`, `pnpm lint`, `pnpm format`, `pnpm build` | passed; existing architecture/bundle budgets unchanged |
| `python3 tools/check_architecture.py` (repository root) | passed |
| `openspec validate add-factory-work-card --strict`, `git diff --check` (repository root) | passed |

No backend behavior, generated HTTP contract, dependency or manual-creation path
changes. Backend builds and PostgreSQL are unnecessary for this frontend cache
correction and were not started. Documentation-only updates were validated by the
specification checks. Free disk space remained approximately 38–39 GiB and was
39 GiB at completion. No commit or push was requested or performed.
