# Task 6.15 verification — Console Checks and Artifacts

Status: implementation and final checks recorded below. Task 6.14 supplies the
unchanged REST contracts; no commit, release or dependency change is included.

## Scope and reuse

- Results adds URL-addressed Checks and Artifacts alongside existing PR/MR and
  evidence/Decision views. Properties links open those same panes. Manual tasks
  remain editable without an execution/provider; external CI needs only its
  independently retained review association.
- Factory selection uses the common bounded `candidates` result read and the
  original publication record. Exact Work/candidate and executed requirements
  survive navigation/reload; selecting another execution clears only its own
  candidate and Artifact continuations. No latest-execution inference is added.
- External CI reuses the existing review membership query/key and retained
  original-head HTTP observation, including historical heads, unknown required
  policy, skipped checks, partial lists and blocked authority. GETs never refresh
  a provider. No check roll-up or merged review becomes Factory acceptance.
- A small common page frame reuses query-state notices, per-page coverage,
  continuation recovery and explicit refresh across Checks/Artifacts/CI. Shared
  check-state and producer rendering keep exact safe identities without traversing
  private object members. Existing Factory diagnostic rendering and Build links
  remain the metadata authority.
- Artifact grouping, names/types/sizes/origin/retention and progressive provenance
  come from the retained projection. Expired/unavailable references remain visible
  without byte controls. Content expansions reuse the generated same-origin REST
  client, independently reauthorizing explicit preview/download. Text stays plain;
  only server-normalized PNG becomes an image. No HTML/SVG renderer, provider
  hotlink, uploader, second artifact reader or provider-specific branch is added.
- Content queries use no-store, zero cache retention, no automatic retry or
  focus/reconnect refresh and cancellation on leave. Mounted Blob elements own
  and revoke their local object URLs. Failed metadata refresh retains visibly
  stale names but closes content and disables onward paging. No body or URL
  enters browser persistence, router state or generic notifications.
- Runtime packages and asset count are unchanged. The measured aggregate increase
  is admitted by a test-first budget change; entry/CSS limits remain unchanged.

## Behavioral Red → Green

Unless noted otherwise, each command was:

```sh
pnpm --dir ui exec vitest run src/features/issues/IssueResultPanels.test.tsx
```

The tests exercise the composed Console route, rendered operator actions and
public typed HTTP client boundaries. Rejected fixture setup, incorrect DOM
selectors and environment failures are not counted as behavioral Red.

| Test/filter | Observed Red → Green |
| --- | --- |
| `opens exact historical Factory checks without turning a partial pass into acceptance` | Factory checks region missing → selected original Work/record, missing required check, pass, historical requirements and partial coverage rendered without executable markup or acceptance claim. |
| `selects a retained candidate and pages only its checks, preserving selection through navigation` | Candidate selection missing → explicit selection and its own cursor persist across sections. |
| `groups authorized artifact metadata and leaves expired or unavailable content as references` | Artifact region missing → grouped original inputs/reports, truthful origin, unavailable name, expired references without byte controls and independent continuation. |
| `reads a plaintext preview only on demand and requires an explicit download request` | Preview action missing → no eager content read, inert text, separate download request and Save link; URL revoked on leave. |
| `shows historical external CI on a manual task independently of Factory and pages the original review` | External CI region missing → original head history, skipped/unknown required policy, partial retained detail and no Factory/provider command. |
| `clears candidate and artifact continuations when another execution is deliberately selected` | Old candidate/cursors remained → all execution-bound selection clears; selected pane survives. |
| Browser `inspects exact checks and safe artifacts with keyboard at 390px in en` | Properties Artifact link missing → same Results pane opens with keyboard and performs explicit safe content reads. |
| `exposes exact Build, Attempt, Job and candidate provenance without rendering private extra members` | Producer identity absent from Details → shared allowlisted producer details and existing Build navigation; private URL omitted. |
| Measured bundle policy | Existing build/policy test rejects the new aggregate → bounded 20 KiB allowance admits the measured feature and continues to reject an extra unplanned 16 KiB chunk. |
| `selects a retained candidate…` with actual `changeset` publication grammar | Existing candidate-only selector omitted the publication returned by `changesets` → exact ChangeSet publication is selectable through the same reader, without a new HTTP contract. |
| `announces a pending download independently from preview and aborts it on leave` | Download announced as Preview → explicit Download loading label; request aborted when leaving. |
| `reads a plaintext preview…` with the real `text/plain; charset=utf-8` response type | Text was rejected because MIME parameters were compared as the media type → base media type is parsed; real UTF-8 text renders inertly while HTML/SVG still fail. |

The bundle-policy Red/Green command was:

```sh
pnpm --dir ui exec node --test --test-name-pattern='admits measured check' scripts/verify-architecture.test.mjs
```

Browser Red and Green commands were:

```sh
pnpm --dir ui exec playwright test tests/browser/issue-result-panels.spec.ts -g '390px in en'
pnpm --dir ui exec playwright test tests/browser/issue-result-panels.spec.ts
```

Additional regressions reject HTML/SVG preview representations, preserve stale
metadata while removing revoked byte expansions, and distinguish stale/unavailable
coverage from empty successful reads. Text/raster/download browser cases use the
real generated client and browser Blob/download behavior, with controlled HTTP
responses; they do not claim a new live provider qualification.

## Final checks

- Targeted component suite: 13 passed; existing Results/execution-selection
  regression run: 11 passed before the later added safety cases.
- `pnpm --dir ui test:unit` — 33 Node tests and 644 Vitest tests passed.
- `pnpm --dir ui exec playwright test tests/browser/issue-result-panels.spec.ts tests/browser/issue-code-reviews.spec.ts`
  — 7 passed, including English/Russian at 390/1280 px, reload, actual text/raster
  rendering and explicit download. After the behavior-preserving Blob lifecycle
  refactor, the 4 result-panel browser cases were repeated.
- `OCTACITY_OPENAPI_SCHEMA_FILE=/tmp/octacity-614-openapi.json pnpm --dir ui api:generate`
  — passed using the exported current task 6.14 schema, without a Rust rebuild.
- `pnpm --dir ui lint`, `pnpm --dir ui format`, `pnpm --dir ui build`
  (including typecheck and architecture/bundle checks) — passed.
- `openspec validate add-factory-work-card --strict` and `git diff --check`
  — passed. Documentation changes have no executable behavior and are covered by
  these checks; TDD is not applicable to prose-only edits.

The initial task 6.15 bundle contained 53 files, 1,151,982 bytes of JavaScript and 1,239,135
bytes of static output. Entry JavaScript remains below 450 KiB and CSS below 85 KiB; aggregate limits are 1,130/1,216 KiB.

Disk space was checked before and during verification (52 GiB initially). No
Cargo target, Docker container, dependency installation or new worktree was
created. Shared caches and pre-existing changes were preserved. Final free space:
approximately 47 GiB. OpenSpec progress: 47/56 tasks complete; section 7 remains
pending.


## Review corrections

The follow-up [review correction evidence](verification-review-6.11-6.15.md)
qualifies real Candidate/ChangeSet selection through memory/PostgreSQL and the
same generated REST client, exact producing Attempt/Job navigation and known
indefinite retention labels. It supersedes the initial fixture-only candidate
choice qualification above and records current regression/bundle measurements.
