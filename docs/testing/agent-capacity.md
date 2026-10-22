# Agent capacity and admission qualification

The public boundaries are the shared registration DTO and inventory producer,
Agent store ports, guarded management REST, and rendered Console/browser journeys.
CPU rank is intentionally omitted. Agents keep their outbound-only connection model.

## Test-first evidence

| Behavior | Observed Red | Green |
| --- | --- | --- |
| Full tree-row selection | Edge clicks left the URL at `/agents` for both members | Native links cover the row; expansion remains separate |
| Compact Agent explorer | Browser found the redundant Browse heading above the tree | Browse heading and capacity notice absent; Favorites and Pool links remain visible |
| Pool names and current routes | Pool-name link and published v2 runtime list absent | Named link, no Pool-version tile, current routes shown without double counting |
| Visibility-safe Pool counts | Missing public fields; PostgreSQL returned `(0,0)`; HTTP omitted counts | Counts cover all visible matching members before pagination and exclude unavailable/disabled admission |
| Admission toggle | Missing store command, HTTP 404 and absent Console action | Reviewed replay/version guards, retained registration and active Lease, resumed ordinary placement |
| Host diagnostics | Unknown `host_details`, producer omitted it, REST dropped it | Bounded optional diagnostics, compatible legacy input, reported host/IP/OS shown |
| Current free space | Missing observation port; heartbeat retained none; REST omitted it; UI showed total capacity | Fresh bounded resources survive restart and are displayed separately from totals |
| Revocation during a blocked sample | A waiting sample committed after registration revocation | Same Agent-row fence acquired before checking current registration; sample ignored |
| Bundle allowance | Required bounded Agent assets exceeded the previous limits | Limited aggregate allowance, unchanged entry/file caps, excess growth still rejected |

Selector-scope mistakes and an expired fixture Lease are harness failures, not
product Red. Documentation and formatting changes have no executable TDD boundary.

## Commands

Rust commands use `--locked`, an isolated `CARGO_TARGET_DIR`, two build jobs,
disabled incremental compilation and disabled dev/test debug information.
`OCTACITY_POSTGRES_URL` must select a disposable service; PostgreSQL cases explicitly
select ignored tests.

```sh
cargo test --locked -p octacity-protocol -p octacity-inventory -p octacity-server-store --lib
cargo test --locked -p octacity-server-application --lib local_admission_refusal_still
cargo test --locked -p octacity-server-application --lib agent_
cargo test --locked -p octacity-server-store-postgres --test lease_placement --test migrations -- --ignored --nocapture
cargo test --locked -p octacity-server-api-rest --test v1_handlers
cargo clippy --locked -p octacity-protocol -p octacity-inventory -p octacity-agent \
  -p octacity-server-store -p octacity-server-application -p octacity-server-store-postgres \
  -p octacity-server-api-rest -p octacity-server --all-targets --all-features -- -D warnings
pnpm --dir ui api:generate
pnpm --dir ui test:unit
pnpm --dir ui exec vitest run src/features/capacity src/app/presentation
pnpm --dir ui exec playwright test tests/browser/capacity-explorer.spec.ts tests/browser/build-explorer.spec.ts tests/browser/accessibility.spec.ts
pnpm --dir ui exec playwright test tests/browser/capacity-explorer.spec.ts tests/browser/shell.spec.ts
pnpm --dir ui exec node --test scripts/verify-architecture.test.mjs
pnpm --dir ui lint
pnpm --dir ui format
pnpm --dir ui build
```

Completed regressions include 73 protocol, 5 inventory, 82 store, 167 REST and
680 Console unit tests. All 16 selected Agent application regressions passed.
The final capacity/presentation subset passed 47 tests;
browser capacity, shared tree and accessibility/narrow-layout checks passed 23.
The bundle gate's 24 tests include bounded acceptance and over-budget rejection.
After obsolete controls/labels/styles were removed, production output measured
1,171,491 JavaScript bytes, 87,361 CSS bytes and 1,259,474 total bytes in 57 files.
The aggregate allowance increased by 6 KiB JavaScript and 0.5 KiB CSS; entry and
asset-count limits stayed unchanged. No browser runtime dependency was added.
PostgreSQL 18 passed all 10 placement/availability/observation cases and all
7 migration/upgrade/rollback cases. The broader eight-package Clippy check above
passed with warnings denied, including the Agent and server runtime composition.

This is controlled qualification, not a deployment of new native binaries into
the local stand. Updated server/Agent binaries must be installed and the Agent
restarted to publish new host diagnostics. Current v2 runtime visibility itself
is a Console correction and does not require inventing another runtime inventory.
