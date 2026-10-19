# Task 3.8: explicit local Issue-status action authoring

## Scope and shared contracts

The existing generic Flow/action envelope and publication commands now expose
`FactoryIssueStatusActionParameters`, backed by the same closed
`IssuePipelineStatusSelection` used by execution. One shared selector checks the
declared outcome for Flow publication, Factory Configuration publication,
known execution-target compatibility and the installed runtime plugin. Workflow
reads retain current authorization and exact Project/workflow/version identity;
only an edge allowing the pipeline origin is selectable. The workflow owns its
finite evidence requirements. Selection does not grant move authority.

The Console extends the existing Flow editor and installed execution profiles.
It lists visible bounded workflow choices, allows explicit retained-version
inspection, filters pipeline edges, reviews their evidence and preserves a
configured outcome. The existing typed input/source mapper binds candidate and
retained evidence references. Existing confirmation owns frozen publication,
replay and stale Flow version review. The router supplies its existing workflow
API dependency; no second client, graph canvas or browser persistence is added.
Local Issue effects and native Source/Reporter effects have separate controls
and confirmation summaries.

Issue execution rejects mismatched workflow pins before reservation. Ordinary
automatic Source input cannot yet supply reviewed Issue association, so an
incompatible enabled input is rejected while disabled repair drafts survive.
Legacy missing association retains the existing explicit unavailable runtime
result. Manual Issue creation/editing and local actions need no connection,
provider, token, model or Reporter. Completion/prose cannot select an edge.
No new transition algorithm, execution journal, intake or database migration is
introduced in this task; task 3.7's uncommitted implementation is preserved.

## Observed Red/Green

Public seams: existing authorized management handlers, REST Flow/schema
contracts, rendered Console controls, and the bundle-policy checker. Initial
fixture/import/command errors were corrected before treating a run as Red.
The pre-change source editor/publication baseline passed 8 tests.

Cargo commands used:

```sh
CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_BUILD_JOBS=2
```

Application filters below were run with
`cargo test -p octacity-server-application --lib FILTER` under that environment.
REST used `cargo test -p octacity-server-api-rest --test v1_handlers FILTER`.
UI commands run from `ui/`.

| Filter / command | Observed Red | Observed Green |
| --- | --- | --- |
| `issue_status_action_publication_requires_workflow_authorization` | Publication accepted an unchecked local action | Missing workflow authorizer yields unavailable with no publication |
| `pipeline_authoring_accepts_only_an_exact_visible_pipeline_workflow_edge` | Required authorized authoring API absent | Exact retained pipeline edge accepted; unknown/foreign/manual-only/revoked choices rejected |
| `issue_execution_rejects_a_local_action_for_another_workflow_before_reserving_work` | Incompatible workflow reserved execution | No reservation or mutation for mismatched pin |
| `issue_execution_rejects...` stale-review refinement | Combined stale Issue review / incompatible target returned invalid | Conflict takes precedence; no execution reserved, original replay remains unchanged |
| `source_input_cannot_enable_a_pipeline_requiring_an_issue_association_it_cannot_supply` | Incompatible automatic input was accepted | Enabled input rejected; disabled repair draft validates without writes |
| `local_issue_status_parameters_have_a_closed_provider_free_http_contract` | Typed schema absent / closure assertion failed | Closed four-field generated contract, no connection fields |
| `pnpm exec vitest run src/features/factory/FlowIssueStatus.test.tsx` | Local authoring controls absent | Visible exact workflow, pipeline edge, declared outcome and evidence review; source controls not invoked |
| `pnpm exec vitest run src/features/factory/FactoryPublication.test.tsx` | Local transition pins missing from confirmation | Local effects reviewed separately; stale publication uses existing refresh/review path |
| `FlowIssueStatus.test.tsx` retained-version case | Explicit version review control absent | Exact requested version read; no implicit latest retarget |
| `FlowIssueStatus.test.tsx` configured-outcome case | Review replaced `succeeded` with first outcome `cancelled` | Existing explicit outcome retained; new unselected actions require a choice |
| `FlowIssueStatus.test.tsx` empty-catalog case | Empty visible catalog had no explanation | Explicit empty state, disabled apply, no invented choice/read/mutation |
| `pnpm exec node --test scripts/verify-architecture.test.mjs` | Measured 1,038,000-byte JS fixture exceeded total budgets | Measured optional workbench admitted; 1,040,000-byte fixture still rejected |

Bundle measurement initially exposed an extra shared chunk. Passing the existing
API from the router removed that chunk and kept asset count within the existing
50-resource limit. The total JS/static budgets each increase by exactly 2 KiB
for the measured controls/translations. Entry, CSS and resource-count budgets
remain unchanged. This configuration change has its own observed Red/Green;
no minifier workaround or extra package was added.

## Regression and qualification

- `cargo test -p octacity-server-application --all-features --lib`: 550 passed before the final stale-review refinement.
- `cargo test -p octacity-server-application --lib issue_`: initial 67 passed; final 68 passed after adding publication coverage and the stale-review refinement.
- `cargo test -p octacity-server-application --lib factory_configuration_cqrs_tests`: 22 passed.
- `cargo test -p octacity-server-application --lib factory_admission_tests::source_configuration`: 28 passed, including provider-neutral sources and disabled repair drafts.
- `cargo test -p octacity-server-application --lib pipeline_authoring_accepts_only_an_exact_visible_pipeline_workflow_edge`: passed after adding retained-version/manual-only/current-revocation coverage.
- `cargo test -p octacity-server-api-rest --test v1_handlers --test v1_contract`: 117 + 25 passed. Includes exact local parameters forwarded through the existing validation route.
- `OCTACITY_POSTGRES_URL=postgres://postgres:octacity@127.0.0.1:32778/postgres cargo test -p octacity-server --test project_text_intake status_actions:: -- --ignored`: 2 passed. Actual Flow validation/publication retains local pins, rejects stale publication and malformed outcome without moving the Issue; native status execution still recovers lost acknowledgement with current rights.
- `pnpm api:generate`: passed with the resource environment above; generated contracts remain untracked outputs.
- `pnpm test:unit`: 548 Vitest + 32 Node tests passed; the subsequently added empty-catalog case and final refactors were qualified by the focused run below.
- `pnpm exec vitest run src/features/factory src/features/issueWorkflows/WorkflowView.test.tsx`: 146 passed after final UI behavior/refactors.
- `pnpm exec playwright test tests/browser/factory-issue-status.spec.ts tests/browser/factory.spec.ts tests/browser/issues.spec.ts tests/browser/issue-workflows.spec.ts`: 16 passed. Includes EN/RU, 390px, keyboard/focus, stale review and manual Issue creation without provider/Factory requests.
- `pnpm exec playwright test tests/browser/factory-issue-status.spec.ts`: 2 passed again after final UI changes.
- `pnpm build`, `pnpm lint`, and `pnpm typecheck`: passed; build also enforces architecture and measured bundle budgets.
- `cargo clippy -p octacity-server-application -p octacity-server-api-rest -p octacity-server --all-features --all-targets -- -D warnings`: passed.
- `cargo fmt --all -- --check`, `git diff --check`, `python3 tools/check_architecture.py`: passed (63 packages, 256 dependency edges).
- `openspec validate add-factory-work-card --strict`: passed.

The dedicated PostgreSQL 18.6 container used a cached image, 512 MiB memory and
384 MiB tmpfs; it was removed after qualification. Disk checks remained around
8.9–10 GiB available. No image pulls, additional service or uncontrolled cleanup
was needed. Native provider/live qualification and source-status mapping remain
with their later tasks. Documentation-only changes were checked by diff and
OpenSpec validation; they add no executable behavior. No commit was created.


## Review corrections

Three review findings were corrected through public behavioral seams, with no
new abstraction, provider dependency, migration or bundle-budget change. Existing
3.7–3.8 work remains uncommitted.

| Command / behavior | Observed Red | Observed Green |
| --- | --- | --- |
| `pnpm exec vitest run src/features/factory/FlowIssueStatus.test.tsx` — profile switch | Switching to profile B kept profile A's `succeeded` outcome instead of B's `failed` | A profile-selection event resets local binding drafts; applying modified review edits preserves them. The test reviews A, applies a changed outcome, switches to B, then reviews and applies B's own pins. |
| `cargo test -p octacity-server-store-postgres --test issue_execution pipeline_status_commit_failure -- --ignored` | Pending retry recorded audit time 45 while history kept original time 30 | The public audit read and status receipt both retain 30 after restart/replay, with one audit record. The restored time now also sets the identity used by the shared outbox publisher. |
| `pnpm exec vitest run src/features/factory/FlowIssueStatus.test.tsx` — retained version | Loading v2 from a catalog listing v3 left the workflow selector showing its empty placeholder | The exact read/validated retained version appears in the selector; applying the reviewed edge retains workflow version 2. |

The pre-change UI baseline passed 5 tests. Final focused profile/action tests
passed 6 tests. Test fixture/type corrections were not counted as Red.

All Cargo test/build/check commands used
`CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_BUILD_JOBS=2`.
PostgreSQL commands additionally used
`OCTACITY_POSTGRES_URL=postgres://postgres:octacity@127.0.0.1:32779/postgres`.
UI commands run from `ui/`.

Regression results:

- `cargo test -p octacity-server-store-postgres --test issue_execution --test issues -- --ignored --test-threads=2`: 19 + 24 passed. The non-ignored in-memory watermark test was not part of this PostgreSQL invocation.
- `cargo test -p octacity-server --test project_text_intake status_actions:: -- --ignored --test-threads=2`: 2 passed, including native lost-acknowledgement recovery and retained Flow publication pins.
- `pnpm test:unit`: 550 Vitest + 32 Node tests passed. After strengthening the profile-switch fixture and correcting a Testing Library type argument, `pnpm exec vitest run src/features/factory/FlowIssueStatus.test.tsx src/features/factory/ExecutionProfiles.test.tsx` passed all 6 tests again.
- `pnpm exec playwright test tests/browser/factory-issue-status.spec.ts tests/browser/factory.spec.ts tests/browser/issues.spec.ts tests/browser/issue-workflows.spec.ts`: 16 passed, including EN/RU, keyboard/focus, narrow layout, stale review and provider-free manual task creation.
- `pnpm build`: passed type checking, production build, architecture and the unchanged bundle limits. `pnpm lint` passed.
- `cargo clippy -p octacity-server-store-postgres -p octacity-server --all-features --all-targets -- -D warnings`: passed.
- `cargo fmt --all -- --check`, `git diff --check`, `python3 tools/check_architecture.py` and `openspec validate add-factory-work-card --strict`: passed.

The dedicated PostgreSQL 18.6 container used its cached image with 512 MiB memory
and 384 MiB tmpfs, and was removed after qualification. Disk checks stayed around
10 GiB free. No cleanup of unrelated files or image downloads was needed.
Documentation-only clarification of these contracts adds no executable behavior;
formatting/diff and OpenSpec validation apply instead of TDD.
