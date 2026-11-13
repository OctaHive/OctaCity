# Task 6.3: Issue classification in Properties

## Behavior and ownership

Properties displays the built-in component/type/size alongside existing Project
properties without promoting legacy custom fields. Component/type choices come
from the Project catalog. Fixed sizes and omitted-list priority/type defaults
are generated from OpenAPI; explicit empty lists stay empty. Project field-list
administration remains task 6.6.

The existing reviewed planning command edits component/type/size and custom
classification fields. Source-owned controls remain disabled. Change priority
opens the existing content editor and focuses its priority selector; it uses
Project choice IDs instead of creating numeric priorities. Retained numeric
priorities survive unrelated edits. Observed catalog changes block a new priority
confirmation, and confirmation retains the original readable label as well as
the existing frozen body, Issue version and retry identity.

The new Classification section uses the existing exact-revision applicability
endpoint with no-store and a revision-specific in-memory query. Issue values and
retained attribution remain authoritative; outcome recommendations and application
conflicts stay in their existing separate cards. The section exposes origin,
assessment time, links to the assessment revision/producing Run and localized
field/evidence problems. Links grant no producer-content authority. Missing or
denied applicability never becomes a current or verified assessment. Reproduction
evidence is shown as verified only when the server's required evidence and field
checks report no problems; target-dependent unresolved checks remain explicit.
Refreshing the task rechecks evidence even without a new Issue version.

Draft/review/command state stays in the existing mounted editors. Conflict refresh
preserves the draft and requires explicit reload/discard followed by new review.
No new writer, producer picker, provider effect, browser persistence or automatic
execution is introduced. Console labels and diagnostics support EN/RU; Project
field/choice display names remain authoritative catalog data.

## Observed Red/Green

Tests use the existing public component/route boundary and Playwright's existing
HTTP fixture boundary. No private helper, source-text assertion or compilation
failure is used as behavioral Red.

1. The constraint-generator test expected fixture-defined priority/type defaults
   and fixed size choices. Red omitted those outputs; generating them from the
   published schema made all five constraint tests pass.
2. `edits built-in classification from Project choices without promoting legacy
   custom properties` failed because the built-in component selector was absent.
   Green reviewed and sent the exact planning body with built-in values and empty
   custom values, retaining Issue If-Match and a separate confirmation.
3. Priority tests failed on four default string IDs and the Project-specific
   priority menu: the old selector offered numeric levels. After switching to
   catalog choices, all nine priority tests passed, including retained numeric
   and string values on unrelated edits.
4. `shows committed values and server freshness separately from a conflicting
   recommendation` failed because Classification was absent. Green displayed
   committed values, manual/pipeline origins, a stale-requirements reason and
   revision/Run links while the conflicting large-size recommendation remained
   separate.
5. `refreshes time-dependent evidence without an Issue edit or fabricated
   verification` initially retained the prior review and missed expired evidence.
   Invalidating applicability on task refresh made it pass without an Issue
   mutation or verification claim.
6. The English browser journey reached the current/stale/conflicting data but
   failed because Change priority was absent. After adding the existing-editor
   handoff and focus behavior, both EN/RU journeys passed at 390 px. They cover
   keyboard selectors, confirmation/Escape focus restoration, a real HTTP 409
   response, retained draft, explicit reload and a second reviewed If-Match.
   Fixture error envelopes were corrected to use a valid request ID; malformed
   fixture responses were not treated as evidence of a production defect.
7. `keeps the reviewed priority label frozen when a new Project catalog arrives`
   observed the disabled confirmation change its label to the newly renamed
   choice. Capturing the label during review made it retain the original label
   and block confirmation until renewed review. The focused filter passed one
   test and skipped nine unrelated priority cases by selection.
8. `blocks an open priority confirmation when the field becomes source-owned`
   initially disabled the selector but left confirmation enabled. Passing the
   existing draft-staleness guard to confirmation made it block the unsubmitted
   command too. Its focused Green passed one test and skipped ten unrelated
   priority cases by selection. An already submitted indeterminate command still
   follows the existing frozen retry behavior.

Additional focused regressions confirm that source-owned built-ins stay locked
while an independent Project risk field can be reviewed, and denied applicability
does not hide committed values or report them as current.

## Qualification

- Final full UI unit run: **44 script tests and 1,116 Vitest tests passed**, 130
  files, including the source-ownership and unavailable-applicability regressions.
  The earlier focused Properties/task/i18n regression passed **92 tests**.
- Browser regression: **16 passed**, covering new EN/RU classification journeys,
  existing Properties/label workflows, outcome recommendations/conflicts and
  manual task editing. No failures or ignored tests remain in that run.
- EN/RU 390 px screenshots were captured; the Russian screenshot was inspected.
  Browser checks assert no horizontal overflow and no classification values in
  local/session storage. Project-provided English display names are intentional.
- API generation, TypeScript, ESLint, Prettier and production build including
  frontend architecture/bundle budgets passed.
- Repository architecture (64 packages, 263 edges), strict OpenSpec validation
  and diff whitespace checks passed.

```sh
cd ui
node --test scripts/issue-constraints.test.mjs
OCTACITY_OPENAPI_SCHEMA_FILE=/tmp/octacity-6162-openapi.json pnpm api:generate
pnpm exec vitest run src/features/issues/IssueClassification.test.tsx
pnpm exec vitest run src/features/issues/IssuePriority.test.tsx
pnpm exec vitest run src/features/issues/IssueProperties.test.tsx \
  src/features/issues/IssuePriority.test.tsx src/features/issues/IssuesView.test.tsx \
  src/features/issues/IssueClassification.test.tsx src/app/presentation
pnpm test:unit
pnpm exec playwright test tests/browser/issues.spec.ts tests/browser/issue-outcomes.spec.ts \
  tests/browser/issue-properties.spec.ts tests/browser/issue-classification.spec.ts
pnpm typecheck
pnpm lint
pnpm format
pnpm build
cd ..
python3 tools/check_architecture.py
openspec validate add-issue-classification-snapshots --strict
git diff --check
```

This slice qualifies the Console with typed mocked HTTP responses, not a native
end-to-end browser/server/provider journey. Native server contracts retain the
qualification from tasks 6.1–6.2; this slice changes no Rust implementation.
Snapshot start/detail controls remain task 6.5. Documentation changes have no
executable behavior requiring a separate Red/Green cycle.
