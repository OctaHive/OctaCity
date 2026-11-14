# Tasks 6.9–6.10 — Documents lifecycle review fixes

This follow-up fixes the three confirmed P2 findings from review of the
uncommitted Documents/Activity work. Regression tests use the same public
application-router and generated-client HTTP boundaries as tasks 6.9–6.10.
No backend, authorization, storage or execution contract changed.

## Red/Green evidence

### Clear an invalidated shared preview

```sh
pnpm --dir ui exec vitest run src/features/issues/IssueDocuments.test.tsx -t 'explicit reread'
```

Red: both 403 and 404 cases retained the previously authorized body after an
explicit reread failed. Green: starting that read clears the local preview/link
receipt; failure leaves safe metadata/error feedback but no old text or linking
button. A later successful explicit read can offer that exact publication again.
No publication or material mutation is triggered by these reads. The existing
Project reader continues to enforce exact source identity and body access.

### Preserve current intent while viewing history

```sh
pnpm --dir ui exec vitest run src/features/issues/IssueDocuments.test.tsx -t 'historical round trip'
```

Red: entering historical presentation unmounted the current document editor,
losing its edited name/text, open publication review or retained transport-retry
identity. Green: the editor remains mounted and hidden while historical metadata
is displayed. Its readiness flag disables a fresh publication confirmation in
history; returning to current data restores the original draft/review.

The same mount-lifetime rule applies to an already open material confirmation.
Both unsubmitted review and failed-command replay were tested before changing
that branch: they also disappeared on historical navigation. They now remain
mounted, with new submission disabled in history and original safe replay kept
intact. The tests independently verify frozen request, idempotency key and
original `If-Match` where applicable; navigation never invents another command.
The shared confirmed-intent implementation remains unchanged. Leaving the Issue
or explicitly closing/canceling the form retains its existing discard behavior.

### Reconcile a removed replacement target

```sh
pnpm --dir ui exec vitest run src/features/issues/IssueDocuments.test.tsx -t 'replacement target disappears'
```

Red: after an external edit removed the chosen target while Documents was hidden,
there was no reconciliation warning; the selected replacement could be absent
from an otherwise successful full-set command. Green: replacement eligibility is
derived from the current exact source identity. A missing target disables both
Issue-publication replacement and shared-publication linking, displays localized
feedback and blocks creation of a new command. The operator cancels replacement
and explicitly reviews a new choice using the current Issue version.

Both Issue-owned and Project-owned target variants are qualified. The new choice
preserves an unrelated selected comment, carries the chosen exact publication
and uses the independently specified current version (`If-Match: "2"`). It does
not publish text or silently append a substitute under the old replacement
intent. No duplicate state, generic state machine or new dependency was added.

## Regression qualification

The **nine new component cases** cover two denied/unavailable rereads, three
publication editing/review/retry history states, two material review/retry history
states and two removed-target owner variants. The complete Documents file now
contains 32 cases.

The existing four EN/RU Documents browser journeys at 390/1280 px were extended
with actual Activity revision navigation and return, failed shared preview
reread/recovery, and externally removed replacement target reconciliation. They
retain normal replacement, publication replay, source ownership, historical
selection, Project catalog/step-selector exclusion and no automatic execution
coverage. Browser navigation waits for the historical view to commit before
following the next link; there are no arbitrary sleeps or retries. Synthetic
private text capture remains disabled for traces, screenshots and video.

```sh
pnpm --dir ui test:unit
pnpm --dir ui exec playwright test tests/browser/issue-documents.spec.ts tests/browser/issue-activity.spec.ts tests/browser/issue-comment-materials.spec.ts tests/browser/project-documents.spec.ts tests/browser/issue-execution-snapshots.spec.ts --workers=1
pnpm --dir ui build
pnpm --dir ui lint
pnpm --dir ui format
python3 tools/check_architecture.py
openspec validate add-issue-classification-snapshots --strict
git diff --check
```

The full unit run passed **46 script tests and 1,236 Vitest tests in 140 files**.
All **18 browser regressions** passed, including the four extended Documents
journeys. Production build/type checking, lint, Prettier, repository architecture
(64 packages, 263 dependency edges), strict OpenSpec and whitespace checks passed.
The final build contains **1,613,565 JavaScript bytes, 88,030 CSS bytes, 1,702,182
static bytes and 58 files**. The existing bundle ceilings, dependency inventory
and API contracts are unchanged.
All implementation slices observed behavioral Red before Green; test-selector
or environment failures were corrected and do not count as Red evidence.
Documentation-only edits have no executable behavior requiring TDD.

The qualification uses fixture HTTP and public UI service ports. Native database,
object-store cleanup, Agent/executor and provider effects were not run or claimed.
No commit was requested. The earlier 6.9–6.10 work remains uncommitted, and the
untracked `github.token` was not read or changed.
