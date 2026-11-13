# Tasks 6.5–6.6: Review corrections

## Corrected behavior

Snapshot confirmation now sends the reviewed digest as the optional
`expected_snapshot_digest` precondition. The application checks it against its
own resolved snapshot after normal authorization/readiness and before reserving
an execution. A changed optional material body availability can therefore cause
a 409 even when the Issue revision is unchanged. Failed comparison creates no
request, audit or replay evidence. A successful exact retry returns its original
receipt before selecting current sources; changing the precondition under that
key conflicts. The shared Console confirmed-command owner freezes this field
together with the body, version and key across retries and continuations.

The precondition does not supply snapshot values or permissions. It is included
in command identity, not a new persistence model. Legacy clients may omit it;
serde omits an absent field to preserve retained replay encoding. Existing Rust
callers explicitly retain that mode. No database migration or permission change
is needed. Read-only review continues to resolve authorized input.

Snapshot presentation now retains the compact definition's classification slot
and normalizes legacy full-catalog bindings to the same lookup. Semantic aliases
resolve frozen property and choice names/IDs without reading current catalogs or
copying a pipeline vocabulary. Ordinary property lookup remains by property ID.

## Observed Red/Green

All commands below ran from the repository root unless stated otherwise. New
behavior was tested before implementation through the routed Issue UI and real
REST/application boundary; fixtures replace external dependencies, not the
snapshot selection or command reservation logic.

1. Semantic labels: from `ui`,
   `pnpm exec vitest run src/features/issues/IssueExecutionSnapshots.test.tsx`.
   Two new cases (compact `selected_v1` and legacy full definitions) failed
   because the frozen property name was absent. After slot-aware lookup, all
   17 tests passed, including the frozen choice name and no execution side effect.
2. Server review binding:
   `CARGO_INCREMENTAL=0 cargo test --offline --locked -p octacity-server-api-rest --test v1_handlers issue_snapshot_http_review_digest_fences_optional_availability_without_an_issue_edit`.
   Red returned 400 for the unknown precondition instead of the required 409.
   Green passed through the real endpoint. The regression covers optional text
   becoming inaccessible and accessible without an Issue edit; mismatch creates
   no mutation evidence, a fresh review succeeds using the previously rejected
   key, exact replay keeps its original omission, and a changed digest under an
   already successful key conflicts. Responses are checked against OpenAPI.
3. Console binding: the same Issue snapshot UI suite failed because start omitted
   the preview digest. After adding it to the confirmed request, all 17 tests
   passed. The browser scenario also asserts the current digest after conflict
   and renewed review in English and Russian at 390px width.

## Final checks

- `CARGO_INCREMENTAL=0 CARGO_NET_OFFLINE=true pnpm --dir ui api:generate`: passed.
- From `ui`, `pnpm test:unit`: 45 script tests and 1185 UI tests passed.
- From `ui`, `pnpm build`, `pnpm lint`, `pnpm format`: passed. Existing architecture
  and bundle budgets passed without an additional budget increase.
- From `ui`, `pnpm exec playwright test tests/browser/issue-execution-snapshots.spec.ts tests/browser/issue-executions.spec.ts tests/browser/factory-ticket-delivery.spec.ts tests/browser/project-issue-fields.spec.ts`:
  8 passed, including the retained producer-result flow and lost-response/pending
  execution continuation.
- `CARGO_INCREMENTAL=0 cargo test --offline --locked -p octacity-server-api-rest --test v1_contract --test v1_handlers`:
  33 contract and 274 handler tests passed.
- `CARGO_INCREMENTAL=0 cargo test --offline --locked -p octacity-server-application --lib issue_execution`:
  25 passed.
- `CARGO_INCREMENTAL=0 cargo test --offline --locked -p octacity-server-application --lib issue_snapshot`:
  24 passed.
- `CARGO_INCREMENTAL=0 cargo check --offline --locked -p octacity-server -p octacity-server-store-postgres --tests`:
  passed, including updated existing intent constructors. This is compilation,
  not runtime PostgreSQL qualification; no disposable `OCTACITY_POSTGRES_URL`
  was configured and PostgreSQL integration tests were not executed.
- `CARGO_INCREMENTAL=0 cargo clippy --offline --locked -p octacity-server-application -p octacity-server-api-rest --lib --tests -- -D warnings`:
  passed.
- `cargo fmt --all --check`, `python3 tools/check_architecture.py`,
  `openspec validate add-issue-classification-snapshots --strict`, and
  `git diff --check`: passed.

Documentation describes the precondition and frozen semantic label lookup.
Those prose-only updates have no executable behavior and need no TDD cycle.
Task scope remains 6.5–6.6; subsequent tasks are unchanged. No commit was made.

## Frozen classification attribution follow-up

The shared snapshot presentation now renders the retained per-alias assessment,
including manual/pipeline/source origin, actor, timestamp, assessed Issue and
requirements revisions, original definition version where applicable, producing
Run/result or exact source mapping/observation, and evidence references. Consumer
definition version and snapshot requirements applicability remain separate from
assessment pins. Missing attribution and empty evidence references are explicit;
retained references do not claim currently verified evidence. Current Issue
classification and mutable catalogs are not read to fill historical metadata.

From `ui`, the public-boundary cycles were:

- `pnpm exec vitest run src/features/issues/IssueExecutionSnapshots.test.tsx -t 'explains frozen classification attribution'`:
  both preview and historical-detail cases failed because pipeline provenance was
  absent. The fixture retains assessments under catalog v2, consumed under v7,
  while the current Issue carries different attribution. After implementation,
  `pnpm exec vitest run src/features/issues/IssueExecutionSnapshots.test.tsx`
  passed all 19 cases, checking original revision/Run links, timestamp, definition
  and requirements pins, manual origin, explicit absent metadata and no fabricated
  verification or execution side effects.
- `pnpm exec vitest run src/features/issues/IssueExecutionSnapshots.test.tsx -t 'retains the exact external mapping'`:
  failed because only the external-origin category was shown, without its exact
  mapping/observation. The source branch now retains both in the review; the case
  passed as part of the final full UI suite.

The existing English/Russian browser journey now also checks assessment v2 and
its original revision link in preview and in frozen detail after Issue edits and
reload, with keyboard navigation and a 390px viewport. No new endpoint, fetch,
persistence, authority or dependency was added. Documentation-only updates require
no separate TDD cycle. Task progress remains 35/44; no commit was made.

Final checks for this follow-up (from `ui` unless noted otherwise):

- `pnpm test:unit`: 45 script tests and 1188 UI tests passed, including all 20
  snapshot tests and the three new attribution cases.
- `pnpm exec playwright test tests/browser/issue-execution-snapshots.spec.ts`:
  2 passed (English/Russian at 390px), including frozen attribution after reload.
- `pnpm build`, `pnpm lint`, `pnpm format`: passed. TypeScript, frontend architecture
  and the existing bundle budgets passed without increasing the budgets.
- From the repository root,
  `openspec validate add-issue-classification-snapshots --strict` and
  `git diff --check`: passed.

This follow-up changes Console presentation, localized text, regression fixtures
and documentation only; no Rust or persistence behavior changed, so the Rust
suites recorded above were not repeated for it.

## Assessment revision navigation follow-up

The assessment-revision link now copies the existing Issue navigation parameters
and changes only the section and revision. The selected execution remains in the
URL across Activity and back to Runs, using the same parameter handling pattern
as ordinary requirements history. No navigation store, new shared abstraction or
execution mutation is introduced.

Observed Red/Green from `ui`:

- `pnpm exec vitest run src/features/issues/IssueExecutionSnapshots.test.tsx -t 'keeps the selected execution after opening'`:
  Red opened the assessment requirements, but the frozen-input region disappeared
  after returning to Runs. This was a navigation failure, not a setup failure.
- `pnpm exec vitest run src/features/issues/IssueExecutionSnapshots.test.tsx src/features/issues/IssueExecutions.test.tsx`:
  Green passed all 40 cases after preserving the query parameters. The new routed
  regression opens an older request outside the empty visible history page,
  inspects its original assessment, returns to Runs and checks both the retained
  exact request ID and original snapshot values. No review/start request occurs.

Existing attribution link assertions now check meaningful path/query values
without depending on parameter ordering. The English/Russian browser journey
also follows the actual revision link and returns to Runs, checking the original
execution and frozen text at 390px rather than only asserting a truncated href.
Documentation states the preserved selection. This remains a task 6.5 correction;
subsequent tasks and progress (35/44) are unchanged. No commit was made.

Final follow-up checks from `ui`: `pnpm test:unit` passed 45 script tests and
1189 UI tests; the focused snapshot/execution suite passed 40 cases.
`pnpm exec playwright test tests/browser/issue-execution-snapshots.spec.ts` passed
both language cases, including the actual navigation round trip.
`pnpm build`, `pnpm lint` and `pnpm format` passed; build includes TypeScript,
frontend architecture and unchanged bundle budgets. The new test's unsupported
Testing Library `exact` option was removed after type checking identified it;
the final build and focused behavioral run passed. Repository-root
`openspec validate add-issue-classification-snapshots --strict` and
`git diff --check` passed. Only Console presentation/tests and prose changed, so
the earlier Rust qualification was not repeated. Prose-only documentation updates
need no executable TDD cycle.
