# Task 6.5 — Console Activity, Runs, Results and discussion

## Delivered contracts

The existing Issue card composes the generated timeline, result and dedicated
comment contracts without a provider or Factory prerequisite for manual tasks.
Activity shows bounded ordered references, independent observation coverage and
on-demand plain-text bodies. Root replies are identified; nested replies are not
offered. The form derives its UTF-8 ceiling from OpenAPI and reuses the shared
confirmed-command lifecycle: one reviewed body/parent/key survives transport
retry, while definitive errors require another review. Success notifications
contain only the static operation title. No body, draft, response value or key is
written to browser storage or URL parameters. Expanded body reads explicitly use
`cache: no-store` and unmount with zero query retention.

Runs and Results share the exact selected request rather than choosing the latest
execution. Runs reuse the authorized summary, reported technical model/tool
profile references and original Job associations. Missing profile/Job data stays
Not reported. Declared human responses open the exact existing Run controls;
discussion has no control/Context authority. Results uses common allowlisted
metadata rendering, preserves original subjects and metadata-only deep links, and
shows Partial/Stale/Unavailable without inventing global acceptance. Failed reads
retain stale data; unreadable continuations can return to a fresh first page.

Original log links retain Build/Attempt/Job identity and Job log scope. The existing
Build view now accepts the selected Attempt, verifies its identity/owning Build,
and reports missing exact Jobs instead of silently selecting a different Job or
the latest retry. Legacy Build URLs still select their current Attempt. Historical
labels distinguish the selected Attempt from current Build controls.

Read-only diagnostic rendering, Run summaries and their styles live in `shared`.
Factory and Issues reuse these components without feature-import exceptions,
new dependencies, another executor/control implementation or a second journal.
The typed Run-summary port is supplied at app composition. No new polling was
added. Detailed event production/journals, editable role profiles, PR/MR/CI,
artifact expansion and the shared action queue remain their separate work.

## TDD evidence

Commands use pinned Node 24.21.0 and pnpm 12.8.1. The table uses `pnpm --dir ui`
as shorthand; logs record the same commands run through the cached pinned pnpm
entry point with the temporary Node directory first in `PATH`.

| Command/filter | Observed Red | Observed Green |
| --- | --- | --- |
| `pnpm --dir ui exec vitest run src/features/issues/activityApi.test.ts` | Generated activity client API absent | Bounded GETs, exact comment POST/body target, frozen header, no mutation headers on reads |
| `node --test ui/scripts/issue-constraints.test.mjs` | Generated comment byte bound missing | UTF-8 byte limit derives from the published schema, independently of character bounds |
| `pnpm --dir ui exec vitest run src/features/issues/IssueActivity.test.tsx -t 'reads only'` | Task activity region absent | Bodies expand independently and HTML-like/mention text remains inert plain text |
| `pnpm --dir ui exec vitest run src/features/issues/IssueActivity.test.tsx -t 'retries one'` | Reply/form command absent | Lost acknowledgement retries the same body/root/key; success clears the draft without execution/control or browser persistence |
| `pnpm --dir ui exec vitest run src/features/issues/IssueResults.test.tsx` | Exact execution Results region absent | Original candidate/Decision, partial coverage, private-field exclusion and preserved deep selection |
| `pnpm --dir ui exec vitest run src/features/issues/IssueRunDetails.test.tsx` | Reported profile/Job navigation absent | Optional real profile versus Not reported, original log targets and separate declared Run responses |
| `pnpm --dir ui exec vitest run src/features/builds/BuildView.test.tsx -t 'exact historical'` | Receiver read the current Attempt instead of the requested original | Exact retained Attempt/Job read and historical label |
| `pnpm --dir ui exec vitest run src/features/builds/BuildView.test.tsx -t 'does not render'` | Foreign/mismatched Attempt metadata was rendered | Identity/owning Build mismatch rejects before Job expansion |
| `pnpm --dir ui exec vitest run src/features/issues/activityApi.test.ts` (cache case) | Comment request used default browser caching | Dedicated body request uses no-store |
| `pnpm --dir ui exec vitest run src/features/builds/BuildView.test.tsx -t 'unavailable exact Job'` | Missing requested Job silently fell back to another Job and rewrote selection | Unavailable target remains selected with explicit feedback and no other Job/log read |
| `pnpm --dir ui exec vitest run src/features/issues/IssueActivity.test.tsx -t 'failed continuation'` | No way to leave failed continuation | Fresh first page remains available without losing the unsubmitted draft |
| `pnpm --dir ui exec vitest run src/features/issues/IssueResults.test.tsx -t 'fresh result'` | Failed result continuation hid first-page recovery | Fresh observation available while retaining exact request selection |
| `pnpm --dir ui exec vitest run src/features/issues/IssueActivity.test.tsx -t 'identifies a root'` | Reply relationship unlabelled | Root-reply provenance is explicit, nested Reply remains unavailable |
| `node --test ui/scripts/verify-architecture.test.mjs` | Existing policy rejected measured Activity/Runs/Results footprint | Tight measured ceilings admit the feature while another unplanned 8,000 bytes and extra assets still reject |

Characterization additionally covers exact multi-byte boundary versus blank text,
independent stale Source coverage, failed refresh retaining metadata, declared gate
separation and no publication during reads. The initial baseline passed 39 existing
Issue/confirmation component tests. Common renderer extraction started with 8
passing Factory diagnostic tests and kept them green. Compiler/fixture repairs
(async read waits, JSX delimiters/import placement, generated reference fields,
message-key reuse and ref naming) are not counted as behavioral Red.

## Verification

| Check | Result |
| --- | --- |
| `pnpm --dir ui run api:generate` | Passed; generated API and UTF-8 constraints, Rust offline with two jobs/incremental and debug symbols disabled |
| `pnpm --dir ui run format`, `lint`, `typecheck`, `build` | Passed with no warnings; source architecture and bundle limits enforced |
| `pnpm --dir ui run test:unit` | 33 script tests and 595 unit/component tests in 88 files passed |
| `pnpm --dir ui exec playwright test tests/browser/issue-activity.spec.ts tests/browser/issues.spec.ts tests/browser/issue-properties.spec.ts tests/browser/issue-executions.spec.ts tests/browser/issue-sources.spec.ts tests/browser/factory.spec.ts` | 20 browser tests passed, including four new EN/RU journeys, keyboard/focus, narrow layouts, lost response, discarded drafts, exact subjects and stale results |
| Focused tests after shared extraction | 11 Factory/result/profile tests passed; full component/browser regressions repeated successfully |
| `python3 tools/check_architecture.py` | Passed: 63 workspace packages, 256 edges |
| `openspec validate add-factory-work-card --strict` | Passed |
| `git diff --check` | Passed |

Production output measures 1,100,292 JavaScript bytes, 86,462 CSS bytes and
1,187,376 total static bytes across 52 assets. The measured JavaScript total is 16,900 bytes above the previous aggregate
ceiling; common rendering introduces one shared asset.
Policy caps are 1,078 KiB JavaScript, 85 KiB CSS, 1,164 KiB static and 52 assets;
the 450 KiB entry cap is unchanged. Source import, dependency, network and
persistence rules were preserved. The budget change has its own observed
Red/Green test; it does not disable validation or add a dependency.

Browser fixtures use synthetic content and disable traces/screenshots/video for
these sensitive-form journeys. No production data or live provider qualification
is claimed. Task 6.4 work is preserved. No server behavior, migration, dependency
lockfile or global toolchain changed in 6.5. Documentation-only changes use
specification/diff validation rather than executable TDD.

Disk remained above 9 GiB. The temporary pinned Node distribution was verified
against its official SHA-256 manifest and removed after checks. Generated build
output remains ignored. No database container or new Rust build profile was
created for this task. Task 6.5 is complete: 37 of 56 tasks checked. No commit or
push was created.
