# Tasks 6.9–6.10 — Preview lifetime and read ordering

This follow-up addresses the three confirmed P2 findings from the second review.
It preserves the existing owner-aware reads, material command and confirmed
mutation contracts. All new tests run through the application router and public
UI service ports or generated-client fixture HTTP boundaries.

## Observed Red/Green

### Removing a source while its preview read is pending

```sh
pnpm --dir ui exec vitest run src/features/issues/IssueDocuments.test.tsx -t 'reference is removed before its read'
```

Red: after removing a selected Project reference and resolving/rejecting its
pending read, the other preview controls stayed disabled. The removed row had
owned the mutation completion callback that released a parent loading flag.

Green: Documents owns one exact read operation for both document owner kinds.
The closed generated material-reference union chooses the existing Issue or
Project exact-source reader. Common preview clearing, error feedback and
`finally` cleanup remain in the mounted workspace even if its selected row is
removed. This replaces the duplicate Project-row callback/loading glue rather
than introducing another lifecycle helper or content store.

Two regression cases cover successful completion and a 404 after row removal.
Both verify that another explicit Issue preview works, source selection is
removed exactly once, and no publication occurs.

### Historical preview with a preserved current draft

```sh
pnpm --dir ui exec vitest run src/features/issues/IssueDocuments.test.tsx -t 'independently of the preserved current editor'
```

Red: both historical owner variants fetched text but rendered no preview because
`editing` remained true for the preserved hidden current editor. Green: preview
visibility uses presentation state (`readOnly` or no current editor), rather than
only the existence of an editing session. The historical view still offers no
version-publication action. Returning to current data restores the exact draft
name/text and hides historical preview while the current editor is shown.

The two cases qualify independently authorized Issue-owned and Project-owned
previews and verify neither publication nor material mutation occurs.

### Serialize shared publication reads

```sh
pnpm --dir ui exec vitest run src/features/issues/IssueDocuments.test.tsx -t 'serializes shared publication reads'
```

Red: another Project publication remained selectable during an active read, so
independent row responses could overwrite a later explicit source choice.

Green: the shared picker passes its busy state through the existing reader and
disables collapse while reading. A second body request requires a separate
explicit action after the active read settles. Closing the picker cannot unmount
its in-flight row and strand that busy state.

The shared reader's busy-state release is a mutation-lifetime `onSettled`, while
body installation remains an observer-scoped success callback. Thus metadata-row
removal still releases the parent when the operation finishes, without installing
an orphan row's body. Existing callers keep their default interaction behavior.

Three deferred-promise cases cover success, denial and a catalog invalidation
that removes the active row before completion. They verify serialization, reader
recovery and exact source B in the separately opened material review, with no
implicit write. No timers or internal component state are used as test controls.

## Browser qualification

The existing EN/RU Documents journeys at 390/1280 px now also qualify:

- Historical Project preview alongside a preserved hidden current document draft,
  followed by return to the exact draft.
- A deliberately delayed shared publication read, disabled alternative selection,
  subsequent explicit selection/review of a different exact source and cancellation
  without writing.
- Removal of a selected Project reference while its dedicated body read is delayed,
  then release of another document's preview control after the response arrives.

The fixtures use HTTP response barriers rather than sleeps. Existing publication
retry, source access failures, missing replacement reconciliation, immutable
history and exclusion from Project catalogs/step selectors remain covered. All
text is synthetic; traces, screenshots and video remain disabled for these flows.

```sh
pnpm --dir ui test:unit
pnpm --dir ui exec playwright test tests/browser/issue-documents.spec.ts --workers=1
pnpm --dir ui exec playwright test tests/browser/issue-activity.spec.ts tests/browser/issue-comment-materials.spec.ts tests/browser/project-documents.spec.ts tests/browser/issue-execution-snapshots.spec.ts --workers=1
pnpm --dir ui build
pnpm --dir ui lint
pnpm --dir ui format
python3 tools/check_architecture.py
openspec validate add-issue-classification-snapshots --strict
git diff --check
```

All checks passed:

- **1,243 Vitest cases** across 140 files and **46 script tests**.
- **18 browser journeys**: four extended Documents journeys plus 14 Activity,
  comment-material, execution-snapshot and Project Documents regressions.
- Production build, ESLint, Prettier, repository/frontend architecture checks,
  strict OpenSpec validation and whitespace checks.

The **seven added component cases** bring Documents to 39 tests; the focused
Documents/comment-material/Project Documents/Flow Documents run passed **74 tests**.
Each focused TDD command above failed for its specified missing behavior before
implementation and passed afterward (2, 2 and 3 cases respectively). The only
format-check failure was corrected by formatting the test file; its rerun passed.

Production output: **1,613,473 JavaScript bytes**, **88,030 CSS bytes**,
**1,702,090 total bytes**, **58 files**. All existing bundle limits passed without
an allowance increase.

No runtime dependency, bundle allowance, permission, persistence or backend
contract was widened. Native database/object-store, Agent/executor and provider
effects were not run or claimed. Documentation-only changes have no executable
behavior requiring TDD. No commit was requested; earlier uncommitted work remains
present, and `github.token` was not read or changed.
