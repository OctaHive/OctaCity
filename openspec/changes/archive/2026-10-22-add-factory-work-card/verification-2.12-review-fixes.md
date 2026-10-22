# Task 2.12 review fixes — verification

## Scope and implementation

This follow-up fixes the three confirmed review findings without advancing the
change beyond 2.12. Earlier uncommitted backend, Docker-context and UI work remains
intact. No provider, manual creation, authorization, REST or execution contract
changed. There is no new dependency, timeout, persistence mechanism or bundle
budget increase, and no commit was created.

1. Valid custom keys such as `__proto__` are written with computed own properties,
   rather than the legacy prototype setter. A small shared lookup in the existing
   presentation module reads only own properties for display, validation, retained
   comparison, review and clear controls. Absent `constructor`/`__proto__` values
   are honestly Not set. Both names remain supported by the existing server
   contract; no provider-specific reserved-key workaround was added.
2. Retained-catalog loading/error rendering now replaces only the read view. The
   mounted editor and confirmed-command state remain stable. New confirmations
   require fresh definitions and explicit reconciliation; a previously submitted
   indeterminate command can still retry its exact body, ETag and key while the
   saved catalog is loading or unavailable. Drafts stay in memory only.
3. The first-page action for direct children depends on the current cursor,
   independently of whether the next page succeeded. A rejected continuation can
   be discarded explicitly and the first page reread without remounting the card.

## TDD evidence

The existing public test boundary was reused: the real App/router/query-client
card, with typed HTTP-facing APIs injected through the shared card fixture.
Tests assert user-visible state and mutation intent, not private component
state. Expected dictionary bodies are independent JSON literals.

Before implementation, this baseline passed:

```sh
pnpm --dir ui exec vitest run src/features/issues/IssueProperties.test.tsx \
  src/features/issues/IssuesView.test.tsx src/api/issuePlanningContracts.test.ts
# 45 passed.
```

The commands below use pinned pnpm 12.8.1 and bundled Node 24. All targeted
commands use `pnpm --dir ui exec vitest run src/features/issues/IssueProperties.test.tsx`:

| Filter (`-t`) | Observed Red before the fix | Green |
| --- | --- | --- |
| `'round-trips the supported'` | `__proto__` serialized as `values: {}` instead of the literal own-key value; the constructor case already passed | Both keys survive editing, review and serialization without moving/creating a task |
| `'absent inherited'` | Both absent special keys displayed `undefined` instead of Not set | Read view and review ignore inherited properties; no spurious clear action |
| `'keeps a saved task draft mounted'` | Publishing catalog 8 for a task retaining catalog 7 removed the Component control and its draft during the deferred read | Draft survives loading/completion; fresh review stays blocked until explicit reload/discard, then uses catalog 8 |
| `'continuation becomes invalid'` | A rejected second page exposed no First sub-issues action | Explicit first-page recovery fetches with a null cursor and shows the updated children |

An additional regression test, `'indeterminate command retryable'`, passed after
fixing the shared lifetime cause. It verifies a real saved-version task whose
confirmed request lost its response: publication starts the retained read, that
read then fails, and retry still sends the identical original body, Issue version
1 ETag and catalog version 7. It does not silently submit catalog 8 or persist a
draft. Deferred promises control read completion/failure; there are no sleeps,
increased timeouts or serialized test execution.

The complete targeted card/API regression then passed 52 tests. A fixture error
code was corrected to the generated `invalid_request` union during TypeScript
checking; compilation/fixture corrections were not counted as behavioral Red.

## Final verification

```sh
pnpm --dir ui test:unit
# 31 Node script tests and 529 Vitest tests (78 files) passed.
pnpm --dir ui exec playwright test tests/browser/issues.spec.ts \
  tests/browser/issue-properties.spec.ts tests/browser/accessibility.spec.ts
# 22 passed: manual creation, Properties, keyboard/focus, EN/RU and narrow layouts.
pnpm --dir ui lint
pnpm --dir ui format
pnpm --dir ui build
# TypeScript, production build, frontend architecture and existing budgets passed.
python3 tools/check_architecture.py
# 63 packages and 256 dependency edges passed.
openspec validate add-factory-work-card --strict
git diff --check
```

No Rust or container/image builds were needed for these UI fixes. Full workspace
Rust/PostgreSQL and the entire browser suite were not rerun. Available space was
checked before/during verification: initially 5.5 GiB, down to 5.1 GiB during
checks and 5.4 GiB at the final check. User build artifacts, containers, volumes and credentials were
not removed. This verification document has no executable behavior; documentation
checks use OpenSpec and diff validation rather than an executable TDD cycle.

## Commit-time Docker-context regression

The final portable-context check exposed an outdated generator fixture: its mock
OpenAPI omitted the selected-label bound introduced in 2.12. Before correction,
`python3 -m unittest discover -s tools/tests -p test_verify_local_stand_context.py`
failed with `Missing Issue form bound` in the filtered-context generator scenario.
The fixture now supplies `IssuePlanning.labels.maxItems: 2` and independently
expects `labelsMaximumCount: 2`. The production generator and limits were not
relaxed. All 18 tests passed with the bundled Node runtime on PATH. The Docker
allowlist/build-input fix is committed separately from this UI-contract fixture
update.
