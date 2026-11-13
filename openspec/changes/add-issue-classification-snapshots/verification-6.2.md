# Task 6.2: REST snapshot execution review, detail and compatibility

Follow-up [review corrections](verification-6.1-6.2-review.md) tighten the original
security-scope boundary, correct empty-collection OpenAPI validation and qualify
the native Source readiness fixture against disposable PostgreSQL.

## Behavior and ownership

Policy-driven Issue-snapshot targets can start without caller `classification`
or `predecessor_run`. The HTTP publisher receives the request's original effective
classification, including the server-sealed policy result. Other targets still
require explicit scheduling; exact producer-result input keeps its typed exact
Run selection and existing validation. Completed replay remains ahead of current
Issue/target selection in the original request owner.

Two optional read services are installed in production composition:

- `POST .../issues/{issue_id}/executions/review` accepts the same closed target
  body and exact If-Match, without an idempotency key. It reuses the ordinary
  execution authorization and input selection, returns no-store and the reviewed
  ETag, and creates no execution request, preparation, Work/Run or replay receipt.
- `GET .../issues/{issue_id}/executions/{request_id}/snapshot` uses Issue View
  authority. It returns the original snapshot/digest and frozen repository subject
  when available, `not_selected` for legacy/producer input, or `unavailable` under
  the original digest when private input or independent source access has expired.

Review is input inspection, not an admission or human-acceptance verdict. Ordinary
repository resolution and final admission still enforce their prerequisites.
Pending requests without a preparation may resolve only their original revision
and complete sealed digest. Admitted or expired private inputs are never rebuilt
from current Issue data. Detail independently rechecks selected material bodies,
compares their original metadata/text and exposes no source failure internals.
Metadata-only request/history reads contain no selected text.

The application extracts existing target authorization/selection into two private
methods shared by review and reservation. The original selection, snapshot builder,
Project text preparation, source readers and replay/store owners remain authoritative.
There is no new snapshot store, byte publisher, migration or provider integration.
Closed generated schemas cover original definitions, values, attribution, material
owners/publications, omissions and policy results using the existing shared bounds.
The generated client is refreshed under the ignored `ui/.generated/api` directory.

## Observed Red/Green

1. `issue_execution_http_snapshot_start_uses_sealed_policy_without_caller_classification_or_predecessor`
   returned 400 for the missing caller classification. After making it optional
   and using the retained effective policy result, it passed along with all
   existing execution transport tests (15 passed, 0.24s).
2. The reviewed execution route initially returned 405 instead of requiring its
   exact revision (428). The real-owner HTTP journey also required the missing
   snapshot query composition API. After wiring the common owner, review/start/
   detail/replay passed (1 passed, 0.05s), including a later Issue size edit.
3. Extending that journey to validate real responses against OpenAPI failed on
   the missing `IssueExecutionSnapshot` component. Closed review/detail and sealed
   request schemas made it pass (1 passed, 0.11s).
4. `issue_snapshot_http_legacy_target_still_requires_classification_before_any_reservation`
   found that a rejected legacy request had nevertheless incremented idempotency,
   audit and outbox counts from 2 to 3. Moving the scheduling prerequisite into
   shared preflight before reservation made it pass (0.04s), while explicit
   legacy start, `not_selected` detail and exact receipt replay remained valid.
   New application fixtures now supply their existing explicit scheduling review;
   retained older requests still decode and replay before this fresh-input check.
5. Required acceptance review returned generic `invalid_request`, losing its
   selected-field reason. Running the shared snapshot selection before generic
   text mapping validation made both review and start return
   `/issue_snapshot/fields/acceptance` / `issue_snapshot_missing_value` (0.10s).
6. The retention test required a missing trusted `checked_at` query field. Adding
   that server-owned observation time removed the hidden system-clock dependency
   from application detail reads. The public query passed before its original
   preparation expiry and returned only `unavailable` with the sealed digest at
   expiry, without republishing or mutating receipts (1 passed, 0.04s).

Fixture syntax/import corrections and an initially invalid ExactRevision wire
shape were setup failures, not behavioral Red. Material/evidence consumer shapes
were corrected to the existing shared contract: material strings permit zero
minimum bytes, and evidence uses declared fixed-length digest arrays. Production
compatibility or evidence checks were not loosened to accommodate fixtures.

Commands for the targeted cycles:

```sh
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server-api-rest --test v1_handlers \
  issue_execution_http_snapshot_start -- --nocapture
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server-api-rest --test v1_handlers \
  issue_execution_http_review_requires_exact -- --nocapture
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server-api-rest --test v1_handlers \
  issue_snapshot_http_review_detail -- --nocapture
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server-api-rest --test v1_handlers \
  issue_snapshot_http_legacy_target -- --nocapture
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server-api-rest --test v1_handlers \
  issue_snapshot_http_blockers -- --nocapture
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server-application --lib --all-features \
  snapshot_detail_obeys_private_input_retention -- --nocapture
```

## Authorization and qualification boundaries

Real Issue/Factory owners behind HTTP qualify frozen values, optional component
omission, immutable definitions/provenance, policy scheduling, original subject,
later edits and exact receipts. Missing size/acceptance, stale assessment,
incompatible field contract and missing required evidence all identify their
exact input field without request or Factory mutation. Stale versions and changed
replay intent conflict; client-supplied snapshot values are rejected. Independent
discussion-body denial makes retained detail unavailable; Issue View alone grants
no Execute, while a read remains usable after Execute is denied. Both new routes
join the common allow/deny-before-dispatch inventory.

The exact-producer HTTP test verifies typed Run/classification forwarding rather
than simulating producer acceptance in the transport. The existing application
journey separately qualifies real accepted-producer publication, manual/automatic
admission, execution and exact recovery. HTTP fixtures use the existing memory
stores, byte port and pure admission fixtures. No native service, model, external
provider or packaged Issue-to-PR journey is claimed by this slice.

Console generation initially exposed a type error where the existing form indexed
the now-optional wire classification. Its local risk state uses NonNullable for
that same enum; defaults, validation and submission behavior are unchanged.
Existing unit behavior passed before this type-only correction, and the focused
19-test Issue execution component suite passed afterwards. Documentation and type
annotation changes introduce no separate executable behavior needing a Red/Green
cycle. The UI interactions remain scoped to the later Console tasks.

## Regression commands

Final observed qualification, with no failed or ignored tests:

- REST all targets/features: **344 passed** (13 library, 27 administration,
  32 wire contracts, 271 handlers, one deterministic export test).
- Application Issue owner suite: **201 passed**, 2.48s, including the original
  explicit producer-result/manual/automatic execution journey and retention test.
- Versioned Issue delivery resolver/routes: **10 passed**, 18.36s.
- Console: **44 script and 1,108 component tests passed**; the focused **19 Issue
  execution tests** passed after the type-only form adjustment.
- API generation, final TypeScript, ESLint and Prettier: passed.
- REST/application Clippy, all targets/features with warnings denied: passed.
- Production server composition check, all features: passed (22.53s).
- Rust formatting, architecture (64 packages), strict OpenSpec and diff whitespace:
  passed.

```sh
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server-api-rest --all-targets --all-features
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server-application --lib --all-features issues::tests::issue_tests -- --quiet
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server-application --lib --all-features issue_delivery_ -- --quiet
CARGO_INCREMENTAL=0 cargo clippy --offline --locked \
  -p octacity-server-api-rest -p octacity-server-application \
  --all-targets --all-features -- -D warnings
CARGO_INCREMENTAL=0 cargo check --offline --locked \
  -p octacity-server --lib --all-features
CARGO_INCREMENTAL=0 cargo run --offline --locked --quiet \
  -p octacity-server-api-rest --example export_openapi > /tmp/octacity-62-openapi.json
cd ui
OCTACITY_OPENAPI_SCHEMA_FILE=/tmp/octacity-62-openapi.json pnpm api:generate
pnpm typecheck
pnpm lint
pnpm format
pnpm test:unit
pnpm exec vitest run src/features/issues/IssueExecutions.test.tsx
cd ..
CARGO_INCREMENTAL=0 cargo fmt --all -- --check
python3 tools/check_architecture.py
openspec validate add-issue-classification-snapshots --strict
git diff --check
```
