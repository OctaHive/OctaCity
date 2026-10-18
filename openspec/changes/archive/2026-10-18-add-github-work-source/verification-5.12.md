# Task 5.12 verification

## Scope and public boundaries

The operator approved expanding 5.12 to include the backend/runtime prerequisites
for editable immutable status mappings. Implementation uses the existing input
validation/publication, Source choices/diagnostics, accepted action/outbox and
ordered Reporter boundaries. No independent mapping store, poller, task editor or
provider write endpoint is introduced.

- An optional input-owned mapping freezes arbitrary logical values to literal
  label additions/removals and optional opaque states. Absent mappings retain
  legacy serialization. Publication requires an explicit Reporter, an installed
  editing ceiling and independent current task-update authority.
- Trusted installation composition supplies installed mapping descriptions and
  optional editable choices; the hosted profile independently pins its ceiling.
  GitHub independently opts into editing within its existing managed groups and
  permitted native states. Provider names and native state vocabularies remain
  outside Factory execution.
- Optional Reporter v2 effects require explicit `custom_effects` support and exact
  receipt echo. Bulk v1 is rejected. The original accepted input, current grants,
  conditional steps, ordering, finite deadlines and existing outbox remain the
  only dispatch path.
- The input editor selects compatible granted Reporter profiles, reviews exact
  effects and invalidates validation after edits. Unknown/retained unavailable
  mappings cannot imply current write readiness. Native permission remains
  unverified unless the existing read-only check can prove it.
- On-demand input/group diagnostics show scan selection, ambiguity, retry and
  accepted Work separately. Shared Factory/Integrations/preview presentation
  keeps native identity, original Work/Run, optional visible card links and
  pending/claimed/applied/exhausted projections distinct. Empty visible history
  does not prove absence of earlier/hidden executions.
- Manual Artifact admission remains independent. Card links consume only an
  authoritative visible association and use the owning change's canonical route;
  no task CRUD, import, refresh, field ownership or conflict resolver is added.

## Red → Green evidence

Each implementation slice started with an existing public boundary or a newly
required typed public API. Environment/setup failures were corrected and rerun;
they were not counted as Red. Commands below were run before and after the
corresponding implementation.

All Cargo commands used:

```sh
CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_BUILD_JOBS=2
```

| Boundary / command | Observed Red | Observed Green |
| --- | --- | --- |
| `pnpm --dir ui exec vitest run src/features/factory/sourceApi.test.ts` | Missing exact-version diagnostic method | Input/group paths, bounded cursor, cancellation and mutation-free reads pass |
| `pnpm --dir ui exec vitest run src/features/factory/SourceProgress.test.tsx` | Missing progress control, scan ambiguity and card correlation | Original Work/Run links, optional encoded card path, pagination, selection versus admission and confirmed versus pending effects pass |
| Same progress tests with `-t 'honors a bounded'` and `-t 'shows retained Work'` | Retry delay discarded; projection attempt/eligible time missing | Safe error classification preserves the server retry delay; attempts and localized UTC times are shown without inventing execution outcomes |
| `cargo test --locked -p octacity-server-factory input_reporting_maps` | Required mapping/policy API absent | Arbitrary values, exact installed scope, duplicate/overlap/state/byte rejection pass |
| `cargo test --locked -p octacity-server-factory input_publication_freezes_reporting` | Input mapping field absent | Exact immutable retention/restoration, explicit Reporter requirement and legacy field absence pass |
| `cargo test --locked -p octacity-server-application --lib source_reporting_choices_and_publication` | Installed mapping description/editing API absent | GitHub and synthetic provider publication/replay, separate writer authority and unsupported/out-of-ceiling rejection pass |
| `cargo test --locked -p octacity-work-source-protocol --test contract ordered_reporting_carries` | Custom effects rejected as an unknown field; bulk v1 then incorrectly accepted them | Bounded typed v2 effects, unchanged legacy wire and v1 rejection pass |
| Same contract suite with `reporting_receipts_confirm` and `advertised_roles` | Exact custom effects could not be confirmed; v1-only manifest could claim editing | Receipt echo/substitution checks and explicit ordered-protocol advertisement pass |
| `cargo test --locked -p octacity-server-work-source --test host custom_reporting_requires` | Process dispatched without explicit support | Closed unsupported result before process dispatch passes |
| `cargo test --locked -p octacity-github --test status_mappings editable_mappings_narrow` | Native editable mapping API absent | Explicit native opt-in, case-insensitive managed-label checks, native state ceiling and original installed-map preservation pass |
| Same native suite with `custom_effects_keep` | Reencoding installed catalogue plus valid effects exceeded an unrelated configuration-size bound | Shared direct effect validation accepts declared maximum label counts/lengths without JSON roundtrips |
| `cargo test --locked -p octacity-github --test issue_reporting input_owned_mapping_converges` | Existing installed-only resolution rejected the custom logical value | Stateful HTTPS/process inspection and three individually conditional effects converge while preserving human-owned labels |
| `cargo test --locked -p octacity-server --lib input_owned_reporting_keeps` | Hosted profile/intent support absent | GitHub and synthetic process peers receive only the original frozen effects after current-input replacement/disablement |
| `pnpm --dir ui exec vitest run src/features/factory/SourceInputEditor.test.tsx` | Reporter selection/custom mapping controls absent; retained unavailable pin displayed as no Reporter | Scoped profile selection, advertised label/state editing, exact review/publication, proof invalidation and honest retained-pin presentation pass |
| Same editor suite with `-t 'selects an authorized task'` | Preview omitted existing admission/reporting correlations | Preview shares the safe original Work/Run/reporting presentation without publication |
| Same editor suite with `-t 'selects only a same-resource'` | Confirmation omitted preservation/readiness explanations | The immutable review itself states unrelated-label preservation and unverified remote write readiness |
| `pnpm --dir ui exec vitest run src/features/integrations/ProjectAccess.test.tsx -t 'reviews visible retained inputs'` | Integration history lacked the shared confirmation distinction and Work link | The common presenter supplies both without another history renderer |
| `pnpm --dir ui exec vitest run src/features/factory/sourceApi.test.ts -t 'drops remote error text'` | Remote message/request identity could reach diagnostic query cache | Shared API redaction drops text/identity before caching and retains finite retry metadata |
| `cargo test --locked -p octacity-server-api-rest --test v1_handlers input_reporting_contract` | Mapping/policy schemas absent | Closed bounded optional input/choice contracts and arbitrary status schema pass |
| `pnpm --dir ui exec node --test scripts/verify-architecture.test.mjs` | Previous optional-UI/asset ceiling rejected measured mapping and shared history | Measured allowance passes; entry/CSS bounds and rejection of unplanned growth remain enforced |

The UI component tests run through rendered controls and the public typed API.
Provider peers observe requests and native HTTPS state rather than private runtime
methods. Extra browser tests qualify keyboard confirmation/cancel/focus, Russian
narrow-layout guidance, on-demand history, and absence of mapping/preview values
from browser storage. No credentials are collected by these forms.

## Regression qualification

```sh
cargo test --locked -p octacity-server-factory -p octacity-server-store \
  -p octacity-work-source-protocol -p octacity-server-work-source -p octacity-github
# 519 tests including doc-tests passed; the subsequent native boundary test adds one.
cargo test --locked -p octacity-github --test status_mappings --test issue_reporting
# Final native changes: 7 + 18 tests passed.
cargo test --locked -p octacity-server-application --lib factory_admission_tests
# 81 passed, including manual admission, source publication/preview and provider-neutral replay.
cargo test --locked -p octacity-server-api-rest
# 132 passed (12 library + 17 administration + 25 public errors + 78 handlers).
# Final full REST and protocol suites were rerun together after correcting a synthetic example key;
# 132 REST + 16 protocol tests passed. Fixture/setup failures were not counted as Red.
cargo test --locked -p octacity-server --lib work_source
# 63 passed; 9 pre-existing optional PostgreSQL tests ignored.
pnpm --dir ui test:unit
# 27 script checks and 449 tests across 70 Vitest files passed.
pnpm --dir ui test:browser
# 81 passed, including ordinary/manual Factory and existing visual/accessibility checks.
pnpm --dir ui api:generate
pnpm --dir ui build
pnpm --dir ui lint
pnpm --dir ui format
# All passed. After removing unused old reporting messages, 5 focused localization tests passed.
```

Repository gates passed:

```sh
cargo clippy --locked -p octacity-server-factory -p octacity-server-store \
  -p octacity-server-application -p octacity-server-api-rest -p octacity-server \
  -p octacity-work-source-protocol -p octacity-server-work-source -p octacity-github \
  --all-targets --all-features -- -D warnings
# After the final native-boundary change, the GitHub package was checked again with the same flags.
cargo fmt --all -- --check
python3 tools/check_architecture.py
python3 tools/security_matrix.py validate
python3 tools/verify_local_stand_context.py
openspec validate add-github-work-source --strict
openspec validate add-factory-work-card --strict
git diff --check
```

## Resource and bundle qualification

The existing target was reused with incremental/debug artifacts disabled and two
Cargo jobs. Free space was checked throughout: approximately 16 GiB initially and
15 GiB at completion. No release build, target cleanup, unrelated cache deletion
or running-stand mutation was necessary.

The final UI has 926,879 JavaScript bytes, 81,069 CSS bytes, 1,008,494 total static
bytes and 43 files. Entry JavaScript remains approximately 424 KB. The
1.37 KiB shared history chunk avoids importing the full Factory workbench into
Integrations. Measured ceilings are 927,744 JS bytes and 1,009,664 static bytes;
entry (460,800) and CSS (81,920) limits remain unchanged. One additional shared
asset is admitted, and the architecture tests still reject extra assets/growth.
No runtime dependency or lockfile changed.

## Remaining owning work

Per-outcome stage action authoring stays in 5.13. Task observation handoff and the
composed pilot remain 5.14/6.1. Native GitLab/Jira implementations and live GitHub
account qualification are not claimed by synthetic provider tests. Installed
Reporter declarations, protected ingress and explicit grants are still required;
no provider service is enabled implicitly. Persistent cards/manual Issue CRUD and
synchronization ownership remain in `add-factory-work-card`.

Documentation/planning changes have no executable behavior; OpenSpec and
whitespace checks validate them. No commit was created during this task.

## Follow-up: Linux browser overflow assertion

The narrow Russian input scenario incorrectly required the raw root width
difference to equal zero. With `scrollbar-gutter: stable`, Linux Chromium reserves
15 pixels and returns -15 without horizontal overflow. The assertion now clamps
negative differences to zero, following the existing Factory/accessibility browser
checks. Positive overflow still fails; no UI styles, retries or timeouts changed.

The existing browser test provided the regression boundary. Using the cached
`mcr.microsoft.com/playwright:v1.63.0-noble` Linux image, a read-only checkout and
a temporary localhost proxy to the host Vite server:

```sh
# Red: original assertion failed with Expected: 0 / Received: -15.
node node_modules/@playwright/test/cli.js test \
  --config .generated/scrollbar.playwright.config.ts \
  tests/browser/factory-inputs.spec.ts --grep 'shows Russian input'
# Green: the complete input browser file passed, 4 tests with 2 workers.
node node_modules/@playwright/test/cli.js test \
  --config .generated/scrollbar.playwright.config.ts \
  tests/browser/factory-inputs.spec.ts
# macOS regression: 4 passed.
pnpm --dir ui exec playwright test tests/browser/factory-inputs.spec.ts
pnpm --dir ui lint
pnpm --dir ui format
pnpm --dir ui typecheck
```

The temporary config only disabled automatic server startup and selected test
and output directories; it did not alter browser defaults or test deadlines.
The container removed itself after each run; the temporary config and owned Vite
server were removed/stopped after verification. No Rust build or image download
was needed; approximately 15 GiB remained free.
