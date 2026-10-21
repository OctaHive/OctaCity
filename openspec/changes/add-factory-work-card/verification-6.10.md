# 6.10 — PostgreSQL review associations and recovery

## Test boundaries and observed Red → Green

- `cargo test --locked -p octacity-server-store-postgres --test issues code_review -- --ignored`: Red on the absent PostgreSQL port implementation; Green for memory parity, concurrent canonical uniqueness, exact original attachment/refresh receipts across a reopened store, retained old/new head observations and immutable original task state.
- Native metadata visibility Red returned a hidden PR before the visible MR. Green filters native repository visibility before watermarks/paging and rejects hidden observation-history lookup in both stores.
- Continuation Red accepted a fabricated future watermark. Green uses shared backend-neutral bounds for membership, position and history, while application cursors additionally bind Issue/security/native visibility.
- Equal-time replay Red rewound memory's latest head while PostgreSQL retained the newer sequence. Green removes memory's redundant latest-observation projection and selects from the same append-only history by original time/sequence.
- Failure injection rejects publication independently at audit, outbox and idempotency completion for both attach and observe. Public reads confirm complete rollback, original sequence allocation and retry recovery without candidate/producing-record retargeting. Generic publication facts contain no provider title/link/author content.
- `cargo test --locked -p octacity-server --test issue_code_review -- --ignored`: passed application + real PostgreSQL composition after recreating the store, including read revocation, exact replay, retained old/new heads, changed metadata visibility, cursor rejection and unchanged manual Issue.
- Additive upgrade from schema 89 retains an existing manual task, is reentrant and performs no association/publication until an explicit command.
- Migration inventory check first detected the two new tables; its expected authoritative inventory now includes only those additions.

## Persistence

Migration 90 adds scoped canonical associations and append-only safe observations, reusing the existing Issue lock and common mutation transaction. There is no new outbox worker, read-time provider request, status setter, accepted-candidate pointer or execution owner. Unknown facts remain absent. The original association/binding is retained while observed external heads change.

Console and application TDD details are recorded in [6.8](verification-6.8.md) and [6.9](verification-6.9.md).

## Final regression verification

- `cargo test --locked -p octacity-server-store --lib`: **81 passed**, covering existing authoritative/memory contracts.
- `cargo test --locked -p octacity-server-application --lib`: **628 passed**, including existing complete requirements/publication/release-owner journeys.
- `cargo test --locked -p octacity-server-api-rest --lib --test v1_handlers`: **12 library + 147 HTTP/OpenAPI tests passed**. Existing manual Issue and exact journal REST contracts remain compatible.
- `cargo test --locked -p octacity-server-store-postgres --test issues --test migrations -- --ignored --test-threads=2`: **61 Issue + 7 migration tests passed** against a disposable PostgreSQL 18.6 instance with bounded tmpfs.
- `cargo test --locked -p octacity-server --test issue_code_review -- --ignored`: **1 application/real-PostgreSQL restart/revocation/cursor integration test passed**.
- `cargo clippy --locked -p octacity-server-application -p octacity-server-store -p octacity-server-store-postgres -p octacity-server-api-rest --all-targets -- -D warnings`: passed.
- `pnpm test:unit`: **33 script + 616 unit/component tests passed**. The final empty-collection regression brought `node node_modules/vitest/vitest.mjs run` to **617 passing unit/component tests**. After centralizing the backoff-counter ceiling from existing shared delays, the affected **18 event/follower tests** passed again.
- `pnpm exec playwright test`: **113 passed**; the affected **8 EN/RU activity/execution/journal journeys** passed again after independent detail expansion and fixture-scope cleanup.
- `pnpm build`, `pnpm lint`, `pnpm typecheck`, `pnpm format`, `cargo fmt --all -- --check`, `git diff --check`: passed.
- `python3 tools/check_architecture.py`: **63 packages / 256 dependency edges passed**, unchanged. Its **32 policy tests** passed.
- `openspec validate add-factory-work-card --strict`: passed.

The only native execution data source remains the existing Journal/event readers; review adapter and HTTP composition remain the subsequent tasks. No commit or remote publication is part of this implementation slice.

## Resource cleanup

Only the task-owned Cargo target and downloaded Node runtime were removed after checks completed. The disposable PostgreSQL container (512 MiB tmpfs, no persistent volume) was removed. Existing caches/checkouts were preserved. Final host free space: approximately **14 GiB**, with about **3.8 GiB** of task-owned Rust build artifacts reclaimed. OpenSpec progress: **42/56** tasks complete; 6.8–6.10 are checked and 6.11 onward remains pending.

## Follow-up review fixes — 2026-10-21

Two behavior corrections were tested through the existing Console API/navigation and application command boundaries:

- From `ui/`, `node node_modules/vitest/vitest.mjs run src/features/issues/IssueRunDetails.test.tsx -t 'browser history'`: **Red** sent the Flow continuation to Node Attempts after Back. **Green** keeps the continuation paired with its collection, so Back starts an unrelated collection without that cursor and Forward can reuse the matching Flow continuation. A further focus assertion caught a remount-based fix losing keyboard focus; the final state-pair solution preserves the existing selector DOM and focus. All **6** tests in `IssueRunDetails.test.tsx` pass.
- `cargo test --locked -p octacity-server-application --lib issue_code_review_delivery_uses_the_retained_publication_receipt`: **Red** accepted the original receipt for another installed namespace with the same review ID/head. **Green**, via `cargo test --locked -p octacity-server-application --lib issue_code_review`, passes **6** tests, including rejection of both a different installation and a different native repository, original Delivery/replay, explicit attachment and unchanged manual Issue/Factory state. The original action's typed frozen publication policy is passed with its receipt to an independent local destination check. Readers without that policy mapping cannot establish Delivery provenance. No provider protocol, credentials, database schema or publication behavior changed.
- Removed the three outdated future-journal statements in `docs/reference/issues.md`, linked the implemented Console section and documented the exact Delivery destination check. These documentation-only edits have no executable behavior requiring a separate Red/Green cycle.

Targeted regression commands after the final behavior changes:

- `cargo test --locked -p octacity-server-application --lib issue_tests`: **126 passed**, including manual tasks, execution observations and source/synchronization contracts.
- `cargo test --locked -p octacity-server-application --lib factory_flow_action_tests`: **22 passed**, including existing publication/status actions and Delivery association.
- From `ui/`, `node node_modules/vitest/vitest.mjs run src/features/issues/IssueRunDetails.test.tsx src/features/issues/IssueJournalEvents.test.tsx src/features/issues/activityApi.test.ts src/features/issues/issueExecutionSelection.test.ts src/features/issues/IssueResults.test.tsx src/features/builds/jobEventFollower.test.ts`: **32 passed**.
- From `ui/`, `node node_modules/@playwright/test/cli.js test tests/browser/issue-journal.spec.ts tests/browser/issue-executions.spec.ts`: **4 EN/RU browser journeys passed**, including keyboard/narrow journal inspection and manual launch/resume/history.
- `cargo clippy --locked -p octacity-server-application --all-targets -- -D warnings`: passed.
- From `ui/`, `node node_modules/typescript/bin/tsc --noEmit`, `node node_modules/vite/bin/vite.js build`, `node scripts/verify-architecture.mjs --dist dist`, targeted ESLint for the two changed UI files and `node node_modules/prettier/bin/prettier.cjs --check .`: passed, with existing bundle limits unchanged.
- `python3 tools/check_architecture.py`: passed (**63 packages / 256 edges**, unchanged).
- `cargo fmt --all --check`, `git diff --check` and `openspec validate add-factory-work-card --strict`: passed.

Rust checks used one owned temporary target, two build jobs, no incremental cache and no debug information. Disk availability was checked before and during builds (approximately 11–12 GiB free). Only that temporary target was removed after verification; final free space was **11.9 GiB**. No commit or push is part of this correction.
