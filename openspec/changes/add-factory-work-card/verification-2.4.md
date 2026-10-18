# Work-card task 2.4 verification

## Scope

Project workflow settings are available at
`/projects/:projectId/issue-workflows`, linked from the existing Project view.
They use the eight Project-owned workflow HTTP operations delivered in 2.3.
No Factory, integration, repository, external token, executor or new dependency
is required. Existing manual Factory intake remains available. Issue creation
and guarded movement are still task 2.5; this task does not implement that API
or the later Issue card.

The form edits ordered ID/name-only statuses, a separate workflow initial
selection and a terminal-status checkbox group. Named edges use generated
origin/evidence choices. Removing referenced statuses requires an explicit
initial/edge edit; deleting a terminal status removes its terminal selection.
Zero terminal statuses is supported. The existing shared SelectMenu and
ConfirmedCommand components provide keyboard, focus, confirmation and replay.
There is no second graph canvas or arbitrary policy/JSON/script editor.

Publication and exact default selection are separate confirmed operations with
separate version preconditions. Validation belongs to the exact in-memory draft.
Stale publication refresh preserves that draft and requires validation and fresh
review. A successful publication receipt becomes the next editing baseline.
Default-selection review retains its immutable target, summary and request key
through a background head refresh and an indeterminate transport failure.
No browser storage receives drafts, workflow payloads or command identities.

## Observed Red / Green

Each implementation slice followed a failing public-boundary test. Mistyped local
paths, fixture envelope/label corrections and temporary Linux test-environment
setup failures are excluded from Red evidence.

| Public behavior | Observed Red | Observed Green |
| --- | --- | --- |
| Project navigation | `Task workflow` link absent | Project-owned deep link without Factory/provider reads |
| Generated form constraints | Required constraint projection API absent | Alternate authoritative bounds/enums retained; missing schema rejected |
| Workflow editing and publication | Settings heading/route absent | Ordered custom names, stable IDs, independent initial/terminal selections, reviewed publication under workflow version 3 |
| Transition policy | Add/remove status and transition controls absent | Generated origins/evidence, named endpoints and complete human-readable policy review |
| UTF-8 bound | 65 Cyrillic letters sent to validation instead of rejected | 128-byte bound enforced; late validation receipt cannot enable changed draft |
| Stale publication | Refreshed baseline remained version 3 instead of 5 | Draft retained, baseline refreshed, validation and a new intent required, then `If-Match: "5"` |
| Exact default selection | Workflow catalog absent | Exact historical version 2 selected under independent selection revision 7; no publication/task move |
| Independent creation | Create-workflow control absent | Caller-generated workflow ID, validated definition and idempotent creation without default selection |
| Successive edits | Second publication still sent `If-Match: "3"` | Uses successful version-4 receipt for the next edit |
| Concrete publication review | Workflow name/base version absent from confirmation | Confirmation includes exact draft name, reviewed base version and complete policy |
| Background refresh during indeterminate selection | Version change unmounted the confirmation and lost retry | Old target/policy/key survive; same exact selection request is replayed |
| Measured optional UI budget | 45 assets and measured aggregate bytes rejected | Tight measured ceilings admit the new lazy route; unplanned growth and entry oversize remain rejected |

Additional passing regression coverage verifies empty terminal selections,
removal of terminal references, stale default-selection review, frozen publication
transport replay, memory-only drafts, exact generated HTTP paths/headers and
credentials omission. The browser uses the production generated client and
canonical server fixtures: keyboard initial selection/confirmation/Escape/focus,
English/Russian narrow layout, forbidden publication without retry/fallback,
and no provider/Factory request or canvas. Both Project-page visual snapshots
were inspected and updated for the intentional navigation addition on macOS
and Linux.

## Commands and qualification

Commands ran from the repository root unless `cd ui` is shown. The installed
Node 24.21.0 / pnpm 12.8.1 cache was used. The existing 2.3 OpenAPI exporter was
reused; no Rust compilation was needed for this UI-only task.

```sh
# Authoritative generated types/constraints, from the already-built 2.3 exporter.
target/debug/examples/export_openapi > /tmp/octacity-workflow-24-openapi.json
OCTACITY_OPENAPI_SCHEMA_FILE=/tmp/octacity-workflow-24-openapi.json pnpm --dir ui api:generate

# Initial Red and matching Green runs used these public test boundaries.
pnpm --dir ui exec vitest run src/features/projects/ProjectViews.test.tsx -t 'offers Project workflow'
node --test ui/scripts/issue-workflow-constraints.test.mjs
pnpm --dir ui exec vitest run src/features/issueWorkflows/WorkflowView.test.tsx
node --test --test-name-pattern='tight asset|measured lazy workflow' ui/scripts/verify-architecture.test.mjs

# Full regressions and required UI gates.
cd ui
pnpm test
pnpm typecheck
pnpm lint
pnpm format
pnpm build
cd ..
openspec validate add-factory-work-card --strict
git diff --check
df -h .
```

Final qualification:

- Standard `pnpm test`: 28 architecture/generator tests, 471 unit/component tests
  in 74 files, and all 89 browser/accessibility/visual scenarios pass.
- `typecheck`, lint, formatting, production build and architecture/bundle gate pass.
- Linux Chromium in cached `mcr.microsoft.com/playwright:v1.63.0-noble`: five
  targeted browser/visual checks pass, including verification of the two Linux
  Project snapshots without snapshot updates. A temporary localhost HTTP relay
  connected the container browser to the host Vite fixture server, retaining the
  normal secure localhost context. No browser security checks were disabled.
- Final measured build: 958,958 raw JavaScript bytes, 82,650 CSS bytes,
  1,042,154 total bytes, 45 assets. Reviewed ceilings are 944 KiB
  JavaScript / 82 KiB CSS / 1,026 KiB total / 45 assets; the entry ceiling remains
  450 KiB. The optional settings chunk is about 13.4 kB raw; no dependency was added.
- Disk checks before/during/after builds report about 10 GiB available. Existing
  Rust target data and the user's running local stand were not rebuilt or removed.

The browser HTTP fixtures establish Console/client behavior; they do not claim
that the user's running stand was redeployed. Native HTTP/PostgreSQL workflow
qualification remains the existing 2.3 verification. Backend, integration fixes,
`simplify-integration-setup` and `complete-factory-source-runtime` changes present
before this task were preserved. Documentation-only edits have no executable
behavior and were validated with formatting/spec/diff checks. No commit was made.

## Follow-up review corrections

The review identified three Console defects, fixed in separate Red/Green slices
at the same public Console/API boundaries. No backend or provider behavior changed.

| Correction | Observed Red before implementation | Observed Green |
| --- | --- | --- |
| Refresh current policy after publication | Opening the updated v4 catalog row still showed cached v3 instead of `Published policy` | The selected current read refreshes and explicit default selection sends version 4 under selection revision 7; publication itself selects nothing |
| Keep catalog independent of the default read | Both `not_found` and `unavailable` default reads hid the independently visible policy | Visible policy can be inspected, edited and published; default selection stays disabled without its revision |
| Restore focus after stale review | Both successful and failed baseline refreshes left focus on `body`; Chromium also reported `Validate draft` inactive | Focus returns to the enabled validation control; draft and required fresh review are preserved |

The publication invalidation targets only the workflow list and the affected
current-definition query, using an exact key for the latter. Historical query keys
and frozen confirmed requests remain separate. The catalog accepts an unavailable
default selection instead of duplicating readiness flags or inventing a revision.
Focus uses the existing confirmation component with a local validation-button ref;
no shared focus framework, fallback API, dependency or timeout was added.

Exact focused commands (each slice was run in Red before its implementation and
then Green; the full workflow file was also rerun after each implementation):

```sh
pnpm --dir ui exec vitest run src/features/issueWorkflows/WorkflowView.test.tsx -t 'opens the newly published'
pnpm --dir ui exec vitest run src/features/issueWorkflows/WorkflowView.test.tsx -t 'keeps visible workflows'
pnpm --dir ui exec vitest run src/features/issueWorkflows/WorkflowView.test.tsx -t 'restores editing focus'
pnpm --dir ui exec playwright test tests/browser/issue-workflows.spec.ts --grep 'returns keyboard focus'
pnpm --dir ui exec vitest run src/features/issueWorkflows/WorkflowView.test.tsx
pnpm --dir ui exec playwright test tests/browser/issue-workflows.spec.ts
```

The focused Green runs pass 16 workflow component scenarios and five browser
scenarios. The existing historical-version, stale-selection, indeterminate replay,
background-refresh, Russian/narrow-layout and provider/Factory independence checks
remain passing. Final regression commands are `pnpm --dir ui test`, `typecheck`,
`lint`, `format`, and `build`, followed by OpenSpec validation and `git diff --check`.
Disk checks before and during the UI build report about 10 GiB free. No Rust or
Docker build, running-stand deployment, budget increase or commit is part of these
corrections. Documentation-only changes are validated without executable TDD.

Final follow-up qualification passed: 28 architecture/generator checks, 476
unit/component tests in 74 files and all 91 browser/accessibility/visual scenarios,
including existing manual Factory intake. TypeScript, lint, formatting, production
build, architecture/bundle verification and OpenSpec validation pass. One formatting
warning in the test file was corrected and the formatting gate rerun successfully.
The final bundle contains 959,017 JavaScript bytes, 82,650 CSS bytes and 45 assets,
within the unchanged budgets. The last disk check still reports about 10 GiB free.
