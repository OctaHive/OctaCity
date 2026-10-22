# 6.8 — Console execution journal

- API seam Red: `pnpm exec vitest run src/features/issues/activityApi.test.ts` failed because `journal` was absent. Green: all three generated bounded no-store readers passed.
- Journey Red: `pnpm exec vitest run src/features/issues/IssueRunDetails.test.tsx` failed on missing stage/Job/event UI and exact journal continuation. Green: retained profiles/log targets, continuation recovery and historical stage → Job → explicit event inspection passed.
- Live Red: `pnpm exec vitest run src/features/issues/IssueJournalEvents.test.tsx` failed on absent opaque reconnect/bounded retention. Green: deduplicated cursor continuation, 512-row retention, explicit gaps, preserved scroll, terminal stop, abort on leaving, missing versus measured-zero usage, pause/resume and hidden-tab backoff passed.
- Selection Red/Green: `issueExecutionSelection.test.ts` failed on old Job/event selection surviving a changed execution; passes after clearing only execution-bound details.
- Transport backoff Red/Green: repeated 503 reads initially retried at the same delay; the behavioral test now passes with increasing delay.
- Existing ordinary Job follower tests passed before extracting common event retention/backoff bounds and remain green. Existing Factory Run command tests retain exact guarded retry/cancel/respond behavior.
- `pnpm test:unit`: 33 script tests and 613 component/unit tests passed before the final transport-backoff slice; its full 11-test event suite subsequently passed. Full final regressions are recorded below on completion.
- `pnpm exec playwright test`: 113 browser tests passed, including English/Russian keyboard journal inspection at 390px, no automatic content reads and no browser-persisted event bodies.
- `pnpm build`, `pnpm lint`, `pnpm format`: passed. The measured journal build initially exceeded the prior aggregate asset budget (53 assets; about 1.113 MB JS). A failing bundle-contract test preceded the bounded adjustment: +1 asset, +16 KiB aggregate JS and +8 KiB aggregate static output. Entry JS and CSS ceilings are unchanged; unplanned additional growth remains rejected.
- Builds share one owned temporary target; free disk was checked repeatedly (about 13 GiB remaining during this slice).

Additional event-detail Red/Green: a deep-linked event outside the visible window initially did not expand, and live reads continued during inspection. Dedicated exact content now expands independently and pauses the selected metadata stream. Bounded retention is exercised through eleven actual 50-row pages, including an explicit gap, rather than an oversized fake page. Final UI unit regression reached 616 passing tests; the affected English/Russian browser journeys passed again after this slice.

Final cross-slice regression and cleanup results: [6.10 verification](verification-6.10.md#final-regression-verification).

The final empty-stage test failed because a stage/budget collection used a Job-specific empty message. Green reuses the existing generic result-collection notice; it makes no assertion about Jobs outside that observation. `node node_modules/vitest/vitest.mjs run src/features/issues/IssueRunDetails.test.tsx -t 'empty stage observations'` established the behavioral Red before this copy-selection change.
