# 6.13 — Console review cards and explicit attachment

Date: 2026-10-21. Scope: task 6.13. Existing uncommitted 6.11–6.12 changes are
preserved. No commit or GitHub publication was requested.

## Public boundaries and Red → Green

Tests exercise the composed Console Issue route with replaceable API boundaries,
plus the real generated management client through browser HTTP fixtures. Provider
and protected-reader behavior remains owned by the existing server contracts;
this task adds no server behavior, credential path or provider SDK.

Rust was not rebuilt: API generation reused the actual OpenAPI export qualified
in 6.12, whose server contracts are unchanged in this task. Generated files remain
normal ignored build inputs rather than a handwritten client.

Commands below run from `ui/`. Each behavioral slice observed Red before its
implementation; syntax/fixture corrections are not counted as behavioral Red.

| Command | Observed Red | Observed Green |
| --- | --- | --- |
| `pnpm exec vitest run src/features/issues/IssueCodeReviews.test.tsx` — compact summary | No PR/MR summary on the Issue route. | Summary links open the same retained Results data while preserving execution selection; facts render as text, including literal markup. |
| Same command — blocked/unknown facts | Revoked read authority had no blocked label. | Unsupported facts remain Not reported, blocked refresh is disabled, another ticket source and local editing remain independent. |
| Same command — attachment | No Attach PR/MR control. | Authorized repository selection and reviewed native ID use the shared frozen confirmation; lost acknowledgement retries the original body/version/key. |
| Same command — changed head/history | No historical original-execution-head label. | The original candidate remains historical, prior-head checks stay in on-demand bounded association history and continuation stays exact. |
| Same command — explicit refresh | No explicit Refresh PR/MR control. | Viewing makes no provider call; confirmed refresh failure keeps last known facts visibly stale without blocking local edits. |
| Same command — collection continuation | No Next page control. | Bounded collection paging preserves selected execution, offers a first-page recovery and retains stale facts after a failed GET refresh. |
| `pnpm exec playwright test tests/browser/issue-code-reviews.spec.ts --workers=1` | English/Russian narrow summary navigation left the review card unfocused. | Both 390 px journeys focus the selected review, restore confirmation focus after Escape, use actual generated attachment headers/body and keep body/key out of browser storage. |
| `pnpm exec vitest run src/features/issues/IssueCodeReviews.test.tsx src/features/integrations/api.test.ts` — retained grant pins | The form substituted the latest installation for the grant's older connection version; the common exact-version client read was absent. | Existing grant/version REST reads supply the original pins. The connection transport uses the exact version URL and omitted credentials. |
| `pnpm exec vitest run src/features/issues/IssueCodeReviews.test.tsx -t 'Unicode control'` | A native ID containing C1 U+0085 enabled review. | Native IDs reject Unicode control characters and use the existing generated native-identity byte bound. Server validation remains authoritative. |
| `node --test --test-name-pattern='measured review cards' scripts/verify-architecture.test.mjs` | The measured additive review-card bundle exceeded the old aggregate budget. | The narrow measured allowance passes, while an additional unplanned 16 KiB chunk still fails aggregate JavaScript/static budgets. Entry/CSS/file ceilings remain unchanged. |

Additional regression cases verify manual creation while review reads are
unavailable and reject ticket-read grants as repository-read choices. A browser
journey exercises real generated history GET and refresh POST, original-head
labeling and safe dependency-error feedback. Its first fixture used an invalid
null `request_id`; the fixture was corrected to the existing error contract,
without weakening client validation or changing production error logic.

## Shared mechanisms and measured size

- Summary and Results reuse one bounded query vocabulary and the same generated
  review API. History expansion reuses the existing association-observation GET.
  GETs are retained reads; no client provider poller or forge operation is added.
- Explicit attachment uses existing integration connection/Project-access/resource
  reads, exact retained connection metadata, immutable source pins, generated
  native-ID bounds and `ConfirmedCommand`. It creates an organizational link;
  it does not fabricate optional execution lineage, Delivery provenance or task
  status authority. Existing verified lineage remains visible on retained cards.
- Explicit refresh uses the same confirmed-intent mechanism with an empty body
  and original association ID. Nullable counts and summaries are not converted
  into zero, complete coverage or Factory acceptance. Read availability is
  separate from connectivity/freshness and original observation time is retained.
- No dependency, migration, provider-specific UI branch, token form, browser
  persistence or execution/merge control was added. Runtime feature isolation and
  central query/network/persistence boundaries remain enforced.
- The build measured 1,133,649 JavaScript bytes and 1,220,733 static bytes, including
  bilingual labels. Aggregate ceilings increased by 16 KiB JavaScript and 24 KiB
  static output to 1,110/1,196 KiB. The existing 450 KiB entry, 85 KiB CSS and
  53-file limits are unchanged; the budget test still rejects unplanned growth.

## Verification

- Initial `pnpm exec vitest run src/features/issues/IssueResults.test.tsx
  src/features/issues/IssuesView.test.tsx`: 29 existing tests passed.
- `pnpm test:unit`: 33 Node contract/architecture tests and all 630 Vitest tests
  passed, including 10 review-route cases. After the budget fixture update,
  its focused Red/Green and the final Node suite were run again.
- Browser regressions for Issue journal, Source, execution and Properties: nine
  existing journeys passed. Final `issue-code-reviews.spec.ts`: three passed.
- `OCTACITY_OPENAPI_SCHEMA_FILE=/tmp/octacity-612-openapi.json node
  scripts/generate-api.mjs`: passed using the unchanged qualified REST document.
- `pnpm build`: full TypeScript checking, Vite build and bundle architecture passed.
- `pnpm lint`, `pnpm format` and source architecture checks: passed.
- Final `node --test scripts/verify-architecture.test.mjs
  scripts/factory-authoring-schemas.test.mjs scripts/issue-workflow-constraints.test.mjs
  scripts/issue-constraints.test.mjs scripts/issue-source-constraints.test.mjs`:
  all 33 tests passed under the updated measured policy.
- `git diff --check`, 25 local documentation links and
  `openspec validate add-factory-work-card --strict`: passed.
- Task 6.13 is complete; overall change progress is 45/56. Tasks 6.14–6.15
  remain open for typed Checks/Artifacts projections and their Console panels.

Documentation-only changes need no executable Red/Green cycle. Reference docs
now describe current review controls, exact grant pins, retained freshness,
independent errors and actual measured Console budgets.

## Resource use

Free space was checked throughout: about 8.0 GiB initially and above 8 GiB during
checks. No Cargo target, Docker instance or dependency installation was created.
The Console build is about 1.2 MiB; existing user caches/checkouts are preserved.
Final free space is about 9.2 GiB. No task-owned heavy runtime remains running.
