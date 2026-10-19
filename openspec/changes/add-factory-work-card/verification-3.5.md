# Task 3.5: Console execution review, continuation and history

## Scope and boundaries

The Issue card now opens an explicit pipeline review using the task 3.4 HTTP
contracts. Manual create/edit and Issue status remain independent. Opening the
card or saving requirements performs no execution/catalog/provider command.
Missing acceptance criteria lead to requirements editing; missing or disabled
visible pipelines and repositories retain explicit setup links.

`PipelineTargetFields` and its hook reuse the existing Factory configuration and
repository read ports. The manual Artifact-based Factory Work form uses the same
selector, eliminating a second implementation of version/source-policy selection.
The shared target does not depend on Issue mappings or provider types. The Issue
form adds the finite v1 requirements mapping and ordinary generated Work
classification. No Console byte publisher, provider adapter or alternate admission
path was introduced: the existing POST owns shared Project text publication.

`ConfirmedCommand` and `confirmedIntent` retain one frozen request, reviewed
`If-Match` and key through transport failures and accepted pending preparation.
An optional completion predicate preserves existing commands' default behavior.
A 202 receipt is resumable without a success notification or modal focus trap.
Requirements edits, including removal of acceptance, and section navigation cannot
rewrite the confirmed command. Stale conflicts require another review and key,
preserving local target edits. Later executions have distinct keys and select the
exact returned request/Run. Leaving the form discards its local continuation;
late responses cannot change another mounted page's selection.

Runs reads bounded history and exact selected metadata; GET never publishes.
The `execution` URL parameter survives card-section, requirements-history and
revision links, including reload. Earlier Issue revisions are labelled explicitly.
Preparation/admission are separate from Run completion and business status. Run
links use the retained association through existing Console routes, not arbitrary
response URLs. Refresh is explicit and read-only; stale metadata/choices remain
visible with existing safe query feedback and unsafe new confirmations disabled.

The form keeps drafts, body snapshots and keys in mounted memory, without browser
storage or key/body URL parameters. Reload restores historical metadata selection,
not mutation authority or an automatic retry. Existing Run pages retain progress,
diagnostic, retry and cancellation authority; detailed execution journals remain
later tasks.

## Observed TDD slices

Tests exercised the existing typed HTTP client, public confirmed-intent boundary
and operator-visible card/router actions before implementation. Missing API/module
behavior established Red; fixture naming, working-directory and TypeScript import
corrections were not counted as behavioral Red evidence.

| Behavior | Observed Red | Observed Green |
| --- | --- | --- |
| HTTP transport | Issue execution adapter absent | Exact POST body/review/key and bounded metadata GETs through the shared client |
| Accepted preparation | A resolved pending receipt cleared the intent as succeeded | Pending receipt retains the original immutable attempt until admission |
| Continuation presentation | Pending response announced completion and omitted continuation | Resume stays available without completion history or modal focus trapping |
| Missing acceptance | Start remained a disabled placeholder | Explicit remediation opens manual requirements editing without execution |
| Pipeline review | Pipeline/source selectors absent | Exact configuration/repository/source, requirements and classification reviewed before POST |
| Historical selection | Selected execution/Run link absent | Exact metadata GET and URL selection survive card sections and reload |
| Edits during preparation | Removing acceptance unmounted the retained continuation | Same body, Issue version and key survive the edit and resume |
| History/revision navigation | Full history dropped `execution` | History and revision links preserve the execution selection |
| Disabled pipeline | Nonempty disabled catalog produced an empty selector without remediation | Visible unconfigured guidance and blocked review |
| Long narrow review | Dialog heights were 944/1013px on an 844px viewport | Bounded scrollable confirmation in both languages |
| Feedback overlap | Global outcome notification intercepted Retry clicks | Confirmation controls stay above feedback |
| Pending keyboard lifecycle | Focus returned away from Resume; Start could reset an active form | Resume receives focus and active input review cannot be replaced through Start |
| Measured bundle policy | Measured new workspace exceeded previous total/asset limits | Measured optional growth admitted; entry/CSS and excessive-growth rejection stay bounded |
| Late acknowledgement | Response after leaving added `execution` to another page | Unmounted form cannot rewrite that page's URL |
| Resume after section navigation | Frozen callback reset Activity to Overview | Frozen publication uses current card selection callback without altering inputs |

Focused commands, run from `ui/` and repeated after the corresponding minimal
implementation:

```sh
pnpm exec vitest run src/features/issues/executionApi.test.ts
pnpm exec vitest run src/shared/confirmedIntent.test.ts
pnpm exec vitest run src/shared/ConfirmedCommand.notifications.test.tsx
pnpm exec vitest run src/features/issues/IssueExecutions.test.tsx
pnpm exec vitest run src/features/issues/IssueExecutions.test.tsx src/features/factory/FactoryWorkForm.test.tsx
pnpm exec playwright test tests/browser/issue-executions.spec.ts --workers=1
node --test scripts/verify-architecture.test.mjs
```

Before the shared selector/confirmation refactor, existing Issue, manual Factory
Work and confirmation tests passed: 4 files, 48 tests. After extraction, the
focused shared/card/Factory suite passed. The card suite also characterizes stale
input/status refresh, separate transition-choice authority, empty setup, opaque
history continuation, current versus earlier requirements and distinct later
request/Run identity.

Browser scenarios use the real Console client and deterministic HTTP boundaries.
Both English and Russian at 390px exercise keyboard review/Escape/focus, a lost
POST acknowledgement, a retained 202 receipt, manual editing while preparation
is pending, identical original resume, exact Run links, history reload and no body
or admission-key persistence. The first scoped browser qualification also passed
all 28 Issue/manual Factory/accessibility scenarios.

## Qualification

Commands from `ui/`:

```sh
CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_BUILD_JOBS=2 pnpm run api:generate
pnpm run format
pnpm run lint
pnpm run test:unit
pnpm run test:browser
pnpm run build
```

Repository commands:

```sh
python3 tools/check_architecture.py
openspec validate add-factory-work-card --strict
git diff --check
```

All qualification commands passed:

- Unit qualification: 31 Node generator/architecture tests and 545 Vitest tests
  across 80 files, with no failures or skipped tests.
- Full browser qualification: 102 tests, including visual regression and narrow
  English/Russian execution flows, passed in approximately 1.3 minutes.
- The final naming-only refactor retained all 19 focused card/confirmation tests.
- Formatting, zero-warning ESLint, generated API types and the production build
  passed; build includes TypeScript and frontend architecture/bundle enforcement.
- Workspace architecture passed for 63 packages and 256 dependency edges.
- Strict OpenSpec validation and `git diff --check` passed.

The production output measured 431,836 bytes for the entry JavaScript,
1,031,282 bytes of total JavaScript, 85,477 bytes of CSS and 1,117,305 bytes of
static output across 50 assets. Total JavaScript grew by approximately 11 KiB
against the previous bound. One shared optional target-selector chunk is reused
by both workflows. The measured-slice policy admits at most 1,036,288 bytes total
JavaScript, 1,122,304 bytes total static output and 50 assets. Entry JavaScript
(460,800) and CSS (86,016) ceilings are unchanged; tests still reject another
unplanned 8,000-byte payload and an extra asset.

## Resource and security review

Disk availability was checked before generation/build and throughout browser
qualification: approximately 14 GiB remained available. Existing dependencies and
browser binaries were reused; no dependency, image pull, disposable service or
new Rust integration build was needed beyond the bounded OpenAPI exporter.
Incremental compilation and debug data were disabled for that exporter.

The shared HTTP client retains same-origin relative routes, omitted browser
credentials, safe normalized errors and cancellation for metadata reads. All
untrusted text remains React-escaped. Run routes derive from an encoded retained
Run identity rather than trusting `run_url`. Query keys contain only scoped
identities/cursors; no provider credentials, requirements bodies or admission keys
are persisted or logged by the new code. Authentication, authorization, transfer
policy and publication remain the existing server boundaries.

No dependencies or lockfiles changed. This is not a release or a change to
credentials, auth/CORS, permissions, provider integration or retention policy;
those boundaries require no additional configuration here.
