# Tasks 6.4–6.6 — review fixes

## Scope

Runs keeps First page available after an unreadable continuation for both reported
execution profiles and original Jobs. Recovery requests a fresh observation while
retaining the selected execution.

Selecting a different execution clears only its predecessor's Results continuation
and selected result. History links, Activity execution links and successful pipeline
admission share `selectIssueExecution`; collection and unrelated navigation metadata
remain intact. Reselecting the same execution preserves its continuation and detail.
The helper documents this execution-scoped lifetime. No provider, manual task,
publication, server or HTTP-contract behavior changes.

## TDD evidence

Tests exercise the rendered Issue card through its router and existing injected
API ports. The initial four-file regression baseline passed 22 tests. Each row
below was implemented only after its behavioral failure was observed. Commands
use pinned Node 24.21.0 and pnpm 12.8.1; `pnpm --dir ui` abbreviates execution in
the UI directory with those pinned tools.

| Command | Observed Red | Observed Green |
| --- | --- | --- |
| `pnpm --dir ui exec vitest run src/features/issues/IssueRunDetails.test.tsx -t 'recovers'` | Two cases could not find First page after the continuation error | Both collections recover with a null cursor and retain the selected request; full file: 3 passed |
| `pnpm --dir ui exec vitest run src/features/issues/IssueResults.test.tsx -t 'starts fresh Results'` | History and Activity retained the previous request's cursor | Both clear the old cursor/detail and read the new Work with a null cursor; full file: 4 passed |
| `pnpm --dir ui exec vitest run src/features/issues/IssueExecutions.test.tsx -t 'clears previous result'` | Pipeline admission selected another request with the old result cursor | Receipt selection clears the old result cursor/detail; full file: 14 passed |

The existing historical Results test also characterizes preserving continuation
and detail when reselecting the same request and navigating sections. This is
unchanged behavior, not another claimed Red/Green cycle. Formatter repairs and
removing an unsupported Testing Library option are not behavioral Red evidence.

## Final verification

| Command | Result |
| --- | --- |
| `pnpm --dir ui exec vitest run src/features/issues/IssueRunDetails.test.tsx src/features/issues/IssueResults.test.tsx src/features/issues/IssueActivity.test.tsx src/features/issues/IssueExecutions.test.tsx` | 27 tests passed after all test/type repairs |
| `pnpm --dir ui run test:unit` | 33 script tests and 600 unit/component tests across 88 files passed |
| `pnpm --dir ui exec playwright test tests/browser/issue-activity.spec.ts tests/browser/issue-executions.spec.ts tests/browser/issues.spec.ts` | 9 browser tests passed, including EN/RU, keyboard/narrow layouts, historical selection and manual task creation |
| `pnpm --dir ui run format`, `lint`, `typecheck`, `build` | Passed; existing architecture and bundle limits unchanged |
| `openspec validate add-factory-work-card --strict` | Passed |
| `git diff --check` | Passed |

Production output is 1,100,336 JavaScript bytes, 86,462 CSS bytes and 1,187,420
total static bytes across 52 files (51 assets and the HTML entry). The JavaScript
increase over the recorded 6.5 output is 44 bytes. No budget increase was needed.

SHA-256 comparison with the pre-fix baseline confirms `Cargo.toml`, `Cargo.lock`,
`ui/package.json`, `ui/pnpm-lock.yaml` and `.github/octa-source-revision` are
unchanged. API generation and Rust builds were unnecessary because contracts did
not change; generated UI types already present in the checkout were reused.

Free disk remained at least 13 GiB during UI verification. The temporary pinned
Node distribution was checked against the official SHA-256 manifest and removed
after verification. No dependency upgrade, commit or push was performed. Existing
6.4–6.6 working changes were preserved.
