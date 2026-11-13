# Task 6.5: Reviewed Issue execution input and frozen execution detail

Subsequent review corrections and their Red/Green evidence are recorded in
[the 6.5–6.6 review verification](verification-6.5-6.6-review.md).

## Behavior and boundaries

The existing Issue start form now reads the exact selected Configuration before
choosing its input controls. Issue-snapshot targets need no producer Run; only a
published accepted-producer contract displays the earlier Issue Run selector.
Ordinary targets retain their generated Work scheduling controls. A snapshot with
an admission policy omits client-supplied scheduling and displays the policy's
server-derived result. An invalid legacy scheduling draft does not block switching
to a policy-driven target.

Review uses the existing no-store execution review endpoint with `If-Match`, no
idempotency key and no publication. The confirmation displays server-resolved
values, pinned Project definitions and choice labels/IDs, required/optional flags,
material limits, exact comment/document identities and owners, selection
attribution/digests, independently authorized plain text and optional omissions.
Declared exact references stay visible even when an optional body is omitted.
The preview must match the Issue, revision and exact target. Snapshot text stays
in memory and React renders it as data. It is neither permission nor approval.

The modal opens after preview completes so keyboard focus can reach confirmation.
Pending review can be abandoned without losing the source/repository draft. A
material-selection conflict uses the existing refresh-and-review flow, preserves
the source draft and obtains another server preview before a new command key.
Transport retries and pending admission continuations retain the original body,
headers and key through the existing confirmed-intent owner.

Execution detail reads only the selected request's retained snapshot. It checks
its digest, Issue, revision and exact target; current values or newer text never
replace historical data. Current Issue revision is displayed separately. Original
input can be refreshed independently; bodies are hidden while rechecking and
after current access/retention fails. Cached review bodies are likewise hidden
if their post-command refresh fails. Legacy requests without a snapshot digest
keep their existing detail reader without a new body read.

A public HTTP test exposed that the Console previously dropped the application's
`issue_snapshot_*` diagnostics. The existing closed OpenAPI validation-code enum,
error normalizer and shared EN/RU problem labels now include those nine existing
server reasons. This is a contract correction, not a second readiness evaluator.
Existing Factory publication diagnostics reuse the same labels.

This task does not add material mutation, document administration or Activity
selection controls. Those remain tasks 6.7–6.10; this task consumes the already
implemented server review/detail contracts. It grants no additional permissions,
starts no automatic intake and adds no body store.

## Observed Red/Green cycles

The public boundaries were the existing Issue card start/detail UI and generated
HTTP transport. HTTP fixtures replace the server boundary; private component
helpers, current catalogs and database internals are not test oracles.

Before extracting the shared execution test fixture, existing execution/detail
behavior passed (25 tests); the unchanged execution suite then passed again
(19 tests).

Each cycle ran `pnpm exec vitest run` from `ui` on
`src/features/issues/IssueExecutionSnapshots.test.tsx`, except the two contract
cycles listed separately below:

1. Manual Issue field review: Red lacked the server's `high` value; Green showed
   required fields/optional omissions and submitted a policy-driven request
   without classification or predecessor data.
2. Material preview: Red lacked the selected comment region; Green displayed
   comments and both document owners with exact identities, bounds and escaped
   text, without starting execution.
3. Review blockers: Red lacked the material-specific diagnostic pointer; Green
   displayed the blocker and kept confirmation disabled. The same run qualified
   conflict refresh/re-review with the source revision preserved.
4. Historical input: Red lacked frozen execution input; Green showed original
   values/text under the exact request while current Issue data differed.
5. Producer mode: Red offered a checkbox for unrelated targets and did not show
   the declared producer's selector automatically; Green used the selected
   Configuration contract and retained exact predecessor submission.
6. Retained-input authority: Red had no refresh action and displayed a mismatched
   Issue snapshot; Green hid denied cached text and rejected mismatched identity.
7. Preview identity: Red allowed a response for another target; Green rejected it
   without exposing its selected values or enabling confirmation.
8. Pending cancellation: Red had no cancel-input-review action; Green preserved
   the source draft and ignored the late response after cancellation.
9. Frozen definitions: Red lacked the selected property's display name and pinned
   catalog version; Green used the retained declaration and choice label/ID.
10. Optional exact source identity: Red hid the expired comment's reference; Green
    retained its identity/digest alongside the explicit omission.
11. Review refresh authority: Red kept old text after a forbidden refresh; Green
    hid the body while keeping the command receipt independently readable.
12. Mode switching: Red let an invalid hidden legacy priority block the new
    policy-driven target; Green validated only the active scheduling mode.
13. Concurrent Issue refresh during preview: Red disabled confirmation without
    explaining the changed revision; Green explicitly required closing the stale
    preview and reviewing the current inputs.

`pnpm exec vitest run src/features/issues/executionApi.test.ts` also established
Red/Green for a real HTTP snapshot diagnostic: Red normalized its errors to an
empty list; Green retained the bounded material pointer/code. A transport
regression verifies exact no-store review/detail paths, `If-Match`, no review
idempotency key and no GET body.

`CARGO_INCREMENTAL=0 cargo test --offline --locked -p octacity-server-api-rest
--test v1_contract issue_snapshot_blockers_are_part_of_the_public_validation_contract`
failed with `missing public blocker issue_snapshot_missing_value`, then passed
after the enum included the nine existing application codes. No server admission
behavior changed.

The first browser execution could not bind sandbox port 4173 (`EPERM`); this was
an environment failure, not Red. Retrying with permission to run the local test
server restored the environment. A completed browser run then exposed disabled
initial modal focus; delaying the modal until the read finished restored the
existing keyboard launch/resume journey. EN/RU 390px scenarios also verify
selection-conflict re-review, reload of original text and absence of text/keys
from local/session storage. Existing direct producer-result delivery remains
covered in its original browser journey.

## Final validation

From `ui`:

```sh
CARGO_INCREMENTAL=0 CARGO_NET_OFFLINE=true pnpm api:generate
pnpm test:unit
pnpm exec vitest run src/features/issues
pnpm build
pnpm lint
pnpm format
pnpm exec playwright test tests/browser/issue-executions.spec.ts tests/browser/issue-execution-snapshots.spec.ts tests/browser/issue-classification.spec.ts tests/browser/issue-properties.spec.ts tests/browser/factory-ticket-delivery.spec.ts
```

- API generation passed; generated schema/authorization inventory remain coherent.
- Full unit suite: **44 script tests and 1169 Vitest tests in 136 files passed**.
- Focused Issue regression: **235 tests in 20 files passed**, including all 15
  snapshot start/detail tests. The extracted fixture supports an explicitly
  manually prepared Issue with Project-choice priority and built-in size; no
  producer association is invented.
- Production build passed TypeScript, frontend architecture and bundle budgets.
- ESLint and Prettier passed.
- Browser regression: **11 tests passed**, including EN/RU 390px snapshot review,
  selection conflict and frozen reload, prior lost-response/pending-resume
  behavior, Properties/classification editing and direct producer-result delivery.

From the repository root:

```sh
CARGO_INCREMENTAL=0 cargo test --offline --locked -p octacity-server-api-rest --test v1_contract --test v1_handlers
CARGO_INCREMENTAL=0 cargo clippy --offline --locked -p octacity-server-api-rest --lib --tests -- -D warnings
cargo fmt --all --check
python3 tools/check_architecture.py
openspec validate add-issue-classification-snapshots --strict
git diff --check
```

REST contract/handler suites passed **33 + 273 = 306 tests**. Clippy, Rust
formatting, workspace architecture (64 packages, 263 edges), strict OpenSpec
validation and whitespace checks passed. The only server implementation change
is the additive OpenAPI error-code enum; admission/storage readers were reused.
Documentation-only edits require no executable Red/Green cycle; their local
verification links, OpenSpec validation and whitespace were checked.

Browser runs use the existing HTTP fixture boundary and local Vite server. No
native executor/packaged release, external provider write or real PR publication
was performed or claimed; those remain the later end-to-end qualification tasks.
The root `github.token` remains untouched and untracked. No commit was requested
for this implementation task.
