# Task 6.10 — Explicit comment materials in Activity

## Scope and test boundaries

Activity reuses the existing append/reply form and dedicated comment body reader.
It now offers explicit **Include comment … as Issue material** and **Remove comment
… from Issue materials** actions for an exact same-Issue root or reply. The shared
material review from task 6.9 freezes the complete desired reference set, current
Issue version and owner/ID/digest metadata. Existing documents and other comments
remain selected. The review depends only on the existing material command port;
it does not require document publication or body-reading capabilities.

The confirmed API/schema boundaries were specified by task 6.10: component tests
render the application router with typed public service ports, and browser tests
exercise generated HTTP clients against fixture routes. No new comment system,
body store, source integration, permission grant, gate response or execution
command was introduced. Backend source authorization and retention remain the
existing owners' responsibility. This qualification does not claim live
PostgreSQL/object storage, Agent execution or external-provider effects.

## Observed Red/Green

Existing Activity and Documents behavior passed first: **29 tests** across their
two component files, before narrowing the shared review dependency or extending
Activity.

```sh
pnpm --dir ui exec vitest run src/features/issues/IssueActivity.test.tsx src/features/issues/IssueDocuments.test.tsx
pnpm --dir ui exec vitest run src/features/issues/IssueCommentMaterials.test.tsx
```

The new component file observed these behavioral Red/Green cycles:

1. **Red:** no include-comment control. **Green:** include/remove each require an
   explicit complete material review with exact comment owner/ID/digest and a
   strong Issue `If-Match`. Unrelated document references survive both commands;
   selecting/removing metadata does not fetch, append or publish source text.
2. **Red:** a 33rd inclusion remained enabled. **Green:** the existing generated
   32-entry ceiling disables inclusion at capacity and keeps removal available.
   No numeric limit is duplicated in the implementation.
3. **Red:** five substituted body responses rendered text despite mismatched
   Project, Issue, comment ID, digest or encoded size. **Green:** the dedicated
   read verifies the opened metadata before returning data to the view/cache.
   A failed identity check produces the existing generic safe failure notice.
4. **Red:** no operator explanation separated posting from input selection.
   **Green:** localized help explains the explicit selection and separately
   reviewed pipeline input. The same test covers a 409 refresh retaining the
   unposted draft, followed by identical body-free selection retry using the
   frozen reference set, original version and key. Re-review after conflict uses
   a new key/current Issue version.
5. **Red:** an already opened body remained visible after a material mutation
   invalidated its read and the server returned 403/404. **Green:** the local
   private-body view displays an unavailable notice instead of stale text. Safe
   Activity metadata and the selection/removal controls remain available; the
   generic metadata stale-data presenter is unchanged.

Additional component qualification verifies that posting a reply does not select
it, explicit inclusion selects only that reply (not its root/thread), later
comments do not enter the selection, unavailable source text does not prevent
removal, foreign metadata is not offered as material and the overview preview has
no mutation controls. Tests use independently specified expected references,
headers and visible outcomes through the public application UI.

## Browser journeys

```sh
pnpm --dir ui exec playwright test tests/browser/issue-comment-materials.spec.ts --workers=1
pnpm --dir ui exec playwright test tests/browser/issue-comment-materials.spec.ts tests/browser/issue-activity.spec.ts tests/browser/issue-documents.spec.ts --workers=1
```

The new journeys cover EN/RU at 390/1280 px. They use keyboard activation and
Escape/focus restoration; separate plain-text body expansion; root reply append
with lost acknowledgement and exact retry; cancellation of material review;
selection conflict with a concurrently added shared document; preserved draft
and a fresh complete reference review; lost material acknowledgement and exact
retry; later comments without implicit membership or body reads; rejection of
revoked body reads; and removal without deleting source text.

The fixture accepts only the append and material routes as writes. Assertions
reject any execution, gate/control or provider command. Posting and selection
have distinct requests/keys/feedback, source bodies never enter the selection
payload/review, markup stays inert, and synthetic text/drafts/keys stay outside
browser storage and URLs. Leaving Activity and reloading discard discussion
drafts under the existing form lifecycle. Tests await the visible navigation
commit before returning to Activity, and await modal closure before moving focus;
no arbitrary sleeps or retries mask a failure. Horizontal overflow is checked on
the narrow layout. Traces, screenshots and video are disabled for these synthetic
content journeys.

## Regression checks

```sh
CARGO_NET_OFFLINE=true CARGO_INCREMENTAL=0 pnpm --dir ui api:generate
pnpm --dir ui typecheck
pnpm --dir ui test:unit
pnpm --dir ui build
pnpm --dir ui lint
pnpm --dir ui format
python3 tools/check_architecture.py
openspec validate add-issue-classification-snapshots --strict
git diff --check
```

All **14 new component tests** passed; the focused Activity/comment-material/
Documents run passed **43 tests**. The complete suite passed **46 script tests
and 1,227 Vitest tests in 140 files**. All **12 browser tests** passed (four new
EN/RU × 390/1280 journeys and eight existing Activity/Documents regressions).
The final production build measured **1,613,173 JavaScript bytes, 88,030 CSS bytes,
1,701,790 static bytes and 58 files**, within the existing allowances. Build,
lint, Prettier, repository architecture (64 packages, 263 dependency edges),
strict OpenSpec validation and whitespace checks passed.
OpenAPI/client regeneration and TypeScript checking passed; the management API
contracts are reused unchanged. No runtime dependency, bundle allowance, schema,
permission or persistence policy was widened for task 6.10. Documentation-only
edits have no executable TDD behavior. No commit was requested; task 6.9's
uncommitted work remains present, and `github.token` was not read or changed.
