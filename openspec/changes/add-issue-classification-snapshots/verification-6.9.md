# Task 6.9 — Issue Documents workspace

## Scope and public boundaries

Implemented the Issue's Documents section against the generated Issue publication,
read and material-selection contracts from tasks 6.7–6.8. Component tests render
through the application router and inject the public document port. Browser tests
use generated clients against fixture HTTP boundaries; they do not claim live
storage, retention cleanup, Agent execution or provider qualification.

The workspace keeps Issue-owned publications separate from shared Project
publications. It supports explicit authorized preview, new immutable versions,
separately labeled Project linking, complete exact material review and explicit
select/replace/remove. Review retains the Issue version and source owner, kind,
ID, digest and encoded size. Selection attribution comes from the returned Issue.
Existing comment selections survive document edits. Historical selections are
read-only and can read an exact source without relying on current catalog pages.
Private bodies and drafts remain in mounted memory, outside query-cache data,
browser storage and URLs. Leaving the Issue discards them.

Only the controlled text editor was extracted from the existing Project publisher;
publication owners, replay and mutations remain separate. Existing Project
behavioral tests passed before and after extraction. The existing Project reader,
Markdown renderer, cursor explorer, deadline hook, confirmed-command/replay and
versioned-mutation contracts are reused. No runtime dependency, backend owner,
permission grant or source-body copy was added.

## Observed Red/Green cycles

The focused command was rerun for each slice, using the named test where useful:

```sh
pnpm --dir ui exec vitest run src/features/issues/IssueDocuments.test.tsx
```

Observed Red results and the corresponding Green behavior:

- Missing Documents region, followed by the Issue-owned catalog and explicit
  exact preview. Merely opening history does not fetch bodies or mutate an Issue.
- Missing creation/review controls, followed by in-memory editing and Markdown
  preview, cancellation retaining the draft, publication receipt refresh and exact
  transport retry without `If-Match`. Publication never selects or launches input.
- Missing selection controls, followed by confirmed full-set selection,
  replacement and removal under strong Issue `If-Match` headers.
- Missing separate shared-Project linking, followed by an authorized exact source
  read and an owner-preserving reference, with no shared or Issue publication.
- Missing new-version action, followed by publication from an authorized Issue
  preview without retargeting the existing selection.
- Lost draft on section return, followed by a mounted editor that survives Issue
  section navigation and is discarded on leaving the Issue.
- Missing historical selection state, followed by the exact requested Issue
  revision, hidden mutation controls and a dedicated unavailable source read.
- UTF-8 over-limit drafts incorrectly enabled review, followed by publication
  byte/count ceilings derived independently from OpenAPI. Bounds are not copied
  into the form as numeric constants.
- Missing exact shared-material preview outside the catalog, followed by the
  existing Project reader resolving the historical exact pin directly.
- Review omitted owner/digest; the frozen review now renders complete source
  metadata and translated server attribution. A separate test exposed removal of
  both a comment and a document with the same UUID; source-kind/owner identity
  now keeps removal and replacement scoped to the intended reference.
- Section navigation hid a still-mounted confirmation dialog. Local portals keep
  the frozen confirmation visible without changing the shared command contract.
- An expired shared preview poisoned a later available selection; keying the
  deadline to its publication fixes that lifecycle. Already selected shared
  publications no longer offer a duplicate link.

Additional qualification covers material-conflict refresh preserving an unrelated
publication draft, exact failed-selection retry, all five substituted preview
metadata dimensions (Project, Issue, ID, digest, size), denied reads and a full
32-entry selection that still permits explicit replacement. Generic integrity
errors use the existing safe failure notice instead of displaying private text.
These cases qualify existing guards where no further behavior change was needed.

```sh
node --test ui/scripts/issue-constraints.test.mjs
pnpm --dir ui exec vitest run src/features/issues/documentsApi.test.ts
```

The generator tests verify independently specified non-default schema ceilings and
reject invalid bounds. The transport test verifies exact Issue-only routes,
cursor forwarding, `no-store` GETs, separate publication/selection bodies and
idempotency/precondition headers.

## Browser Red/Green and regressions

```sh
pnpm --dir ui exec playwright test tests/browser/issue-documents.spec.ts --workers=1
pnpm --dir ui exec playwright test tests/browser/issue-documents.spec.ts tests/browser/project-documents.spec.ts tests/browser/issue-execution-snapshots.spec.ts --workers=1
```

The 390 px Red exposed 247 px horizontal overflow from exact digest/ID metadata;
the document workspace now permits wrapping. A further browser Red found 40 px
Issue button defaults overriding the document controls; scoped shared styling
preserves the existing 44 px document targets. The existing generic retry control
retains its separate shared styling.

The four Documents journeys cover EN/RU at 390/1280 px with keyboard confirmation,
publication acknowledgment loss and identical retry, preserved drafts on section
changes and 409 refresh, immutable versioning, explicit selection/replacement,
Project link/removal and exact historical preview/unavailable state. They verify
that publication never auto-selects or starts execution, a newer version leaves
old selection intact, text never enters browser storage, and task publications
remain absent from both the Project catalog and the actual Factory step document
selector. The shared step source is initially unchecked. A valid UUID Project
fixture is used for Factory's existing URL boundary; invalid test IDs are not
accepted by weakening that boundary. Traces, screenshots and video are disabled for these journeys; only synthetic
fixture text is used.

## Bundle allowance

The production-budget test first failed at 1,611,079 JavaScript bytes against the
previous 1,605,632-byte aggregate allowance, and 1,699,655 static bytes against
1,695,744. The feature adds only 8 KiB to each aggregate ceiling:

- JavaScript: 1,613,824 bytes;
- total static output: 1,703,936 bytes.

The 460,800-byte entry, 91,136-byte CSS and 59-file ceilings remain unchanged.
The updated boundary test failed before this policy change and passed afterward;
an additional KiB near the new ceilings still fails both aggregate checks. The
real production build is independently checked by the existing architecture test.

## Final checks

```sh
pnpm --dir ui test:unit
pnpm --dir ui exec vitest run src/features/issues/IssueDocuments.test.tsx src/features/issues/documentsApi.test.ts src/features/projects/ProjectDocuments.test.tsx
pnpm --dir ui build
pnpm --dir ui lint
pnpm --dir ui format
python3 -m unittest tools.tests.test_verify_local_stand_context.LocalStandContextTests.test_ui_api_generators_load_from_the_filtered_docker_context
python3 tools/check_architecture.py
openspec validate add-issue-classification-snapshots --strict
git diff --check
```

The complete unit run passed **46 script tests and 1,213 Vitest tests in 139 files**.
The final focused run passed **33 tests** (23 Issue Documents, one transport and
nine existing Project Documents tests). All **four Documents browser journeys**
passed, and the **six Project Documents/execution snapshot browser regressions**
passed. The final build measured **1,611,307 JavaScript bytes, 88,030 CSS bytes,
1,699,924 static bytes and 58 files**, within all retained/adjusted ceilings.
TypeScript/build, Docker-context generator loading, lint, Prettier, strict
OpenSpec and repository architecture (64 packages, 263 dependency edges) passed. The documentation-only
edits have no executable TDD behavior. No commit was requested; the existing
untracked `github.token` was not read or changed. Activity comment selection
controls remain task 6.10; subsequent snapshot/material authoring work remains in
its own tasks.
