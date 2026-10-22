# Section 8 — migration, lifetime and complete journey qualification

## Current qualification after review

Tasks 8.3 and 8.4 are qualified through the real PostgreSQL/HTTP/browser pilots
and the shared owner contracts listed below. The new qualification adds no product
logic, provider adapter or execution engine. Manual creation still runs without
Factory or provider installation.

The PostgreSQL imported-card pilot now runs both controlled vocabularies against
one durable store. The production association worker links both Source admissions;
no manual association is written in that pilot. The older isolated in-memory
consumer remains a regression fixture, not evidence of production linkage.
Native GitLab/Jira and live provider/model release qualification remain separate.

## Boundaries

This section uses the existing store/application, released HTTP and generated
Console client boundaries. New behavior is driven by failing tests before its
implementation. Existing journeys are characterized and composed before reuse;
test-harness mistakes and unavailable services are not counted as Red.

The manual browser talks to a real runtime/PostgreSQL through a byte-preserving
HTTP proxy, with no fixture task responses. The execution pilot additionally
uses disposable MinIO and the common Project-text intake. The imported pilot
uses the already installed controlled Source/Reporter process peers with an
actual Issue consumer and independently scoped code-review reader. It does not
pretend that provider-named peers are live GitLab/Jira adapters.

## Red → Green evidence

Commands below use `--locked`, an isolated target directory, two Cargo build
jobs, disabled incremental compilation/debug symbols and explicitly selected
disposable services. PostgreSQL commands select their ignored tests explicitly.

| Behavior | Command (Cargo package/test filters) | Observed Red → Green |
| --- | --- | --- |
| Legacy association/restart | `cargo test --locked -p octacity-server-store-postgres --test factory_discovery legacy_backfill -- --ignored --nocapture` | Missing public backfill API → original Work/Run and unavailable Issue retained; repeat/restart converges |
| Verified native grouping | same command | Distinct Issue IDs for the same verified native identity → one card for two independent inputs; installation/security partition isolation retained |
| Upgrade cutoff | same command | New admission incorrectly captured → only migration-frozen Work is eligible |
| Queue retirement | same command | A completed pending record remained → atomic checkpoint deletion with association |
| One-connection transaction | same command | Backfill failed acquiring its own occupied pool → association reads reuse the held transaction |
| Runtime backfill and legacy HTTP | `cargo test --locked -p octacity-server --test project_text_intake runtime_backfills_only -- --ignored --nocapture` | Worker absent, then HTTP 404 → independent worker, no-store metadata and exact original Work reads |
| Run-to-Issue navigation | same command; `pnpm --dir ui exec vitest run src/shared/FactoryRunSummary.test.tsx` | Missing no-store/association and links → independently authorized exact execution/Work navigation with unavailable fallback |
| Legacy Console/API | `pnpm --dir ui exec vitest run src/features/issues/executionApi.test.ts src/features/issues/IssueExecutions.test.tsx` | Missing API/component → bounded original history, no fabricated request, preserved URL selection and truthful empty state |
| Explicit content lifetime | `cargo test --locked -p octacity-server-store-postgres --test issues explicit_content_expiry -- --ignored --nocapture` | Missing policy/store/availability contracts → held and bounded cleanup, redacted original replay, preserved quota, workflow/status and pending-request restore |
| Independent lifetimes | `cargo test --locked -p octacity-server-store-postgres --test issues discussion_expiry_has_independent -- --ignored --nocapture` | Missing independent scope API → discussion expires while requirements remain under their own hold/version/deadline |
| Later explicit edit | `cargo test --locked -p octacity-server-store-postgres --test issues later_requirements_are_available -- --ignored --nocapture` | New revision incorrectly stayed unavailable → fresh requirements available; original expired revision remains unavailable |
| Durable action body cleanup | `cargo test --locked -p octacity-server-store-postgres --test factory_retention completed_retention_removes -- --ignored --nocapture` | Two derived rows remained after completed retention → child bodies/checkpoint removed through bounded common cleanup |
| Runtime cleanup/admin HTTP | `cargo test --locked -p octacity-server --test issue_discussion runtime_content_expiry -- --ignored --nocapture` | Worker timeout and missing administrative route → cleanup without Factory/provider, no-store administration, HTTP/replayed body unavailable |
| Hold privacy | `cargo test --locked -p octacity-server-store content_hold_diagnostics` | Debug exposed private hold explanation → redacted diagnostics |
| Administrative schema bounds | `cargo test --locked -p octacity-server-api-rest --test v1_handlers issue_content_retention_contract` | Missing UTF-8 ceiling → common 128-byte bound and independent administration/contracts |
| Unavailable Console text | `pnpm --dir ui exec vitest run src/features/issues/IssuesView.test.tsx -t 'expired|unavailable legacy title'` | Empty-draft copy and English retained title → explicit current/historical unavailable text and English/Russian headings |

The migration schema rehearsal's expected table inventory was extended for 0093
and 0094; forward migration/reentry and pre-upgrade snapshot rollback pass. This
inventory update is contract maintenance, not a separate invented product Red.

## Journey coverage without another execution engine

The composed pilots reuse the exact owners qualified by earlier slices instead
of duplicating their state machines inside one large browser harness:

| Scenario | Public qualification boundary |
| --- | --- |
| Manual/default workflow, edit, keyboard/narrow Properties/Unassigned | `issue_console_pilot::manual_console_pilot_uses_postgres_http_without_factory_or_provider` and `postgres-issue-pilot.spec.ts` |
| Custom workflow/adoption/direct move, visible parent and cycle rejection | The same real PostgreSQL/browser pilot plus `runtime::issue_tests::runtime_issue_journey_and_original_replay_survive_restart_without_factory_or_providers` and PostgreSQL planning contracts |
| Frozen original input after edit, pending publication, restart/lost acknowledgement, later reviewed launch, original Run navigation | `project_text_intake::issue_execution_http_resumes_original_inputs_after_runtime_restart`, with `OCTACITY_PILOT_BROWSER=1` for the real Console branch |
| Explicit configured actions, sequential applicability, stale edits/authority, retained rejected commands, lost acknowledgement | `project_text_intake::status_actions` and application `issue_status_action_tests` |
| No-action completion, exact nested/parallel actions, declared clarification, eligible/exhausted technical retry | Existing configured Flow/control/Issue-action public contracts, rerun in application and PostgreSQL configured-owner suites |
| Exact journal, qualified operation events, reconnect/gaps, original Job logs and controls | `factory_configured_flow::issue_journal::original_issue_job_events_resume_after_restart_without_private_payloads`: ordinary Build service, fenced PostgreSQL Job append, runtime restart, retained HTTP cursor and real browser inspection; application contracts additionally cover gaps/rights |
| Original input/output provenance, text/raster preview and expiry | The real reviewed-execution pilot previews original MinIO text, deletes it through `FactoryExecutionInputRetentionWorker`, rejects HTTP content and reopens retained provenance in the browser; raster/output variants retain the existing result-panel contracts |
| Assessment/implementation, one Issue/two Work, explicit Ready/Declined, operator comment, Reporter lost acknowledgement/restart, outage and revoked TaskRead/TaskUpdate | `factory_work_source_pilot::postgres::imported_card_keeps_independent_work_across_reporter_restart_and_second_vocabulary`: one PostgreSQL card, automatic association, actual process peers and browser; the older in-memory pilot remains regression coverage |
| Independent review attachment, changed head, retained old/new CI, failed/missing/indeterminate checks and independent reader revocation | The same imported-task pilot and `issue_code_review` PostgreSQL restart/visibility contract; external CI stays partial, never acceptance |
| Field ownership, ambiguous/unmapped/forbidden pull, overlap rejection, push conflicts/explicit resolution and no echo | The PostgreSQL imported-card pilot refreshes real observations to classify ambiguous/forbidden pull, explicitly resolves a retained Reporter conflict, and scans an overlapping group without a new admission; existing conformance contracts cover the remaining bounded variants |
| Manual editing and ordinary CI during provider/Factory outages | Both pilots and ordinary/configured owner regressions; no provider client enters Issue contracts |

Run the browser pilots explicitly (they are intentionally not silently skipped
when selected through Rust):

```sh
cargo test --locked -p octacity-server --test issue_console_pilot -- --ignored --nocapture
OCTACITY_PILOT_BROWSER=1 cargo test --locked -p octacity-server \
  --test project_text_intake issue_execution_http_resumes -- --ignored --nocapture
```

Without the pilot environment, ordinary Playwright runs report these two real
service tests as skipped; they are executed by the commands above. Existing
portable browser scenarios continue using their declared protocol fixtures.

## Initial implementation regression and required checks

The run record below is updated only after commands finish. Documentation and
formatting changes have no executable behavior; TDD does not apply to those edits.

- PostgreSQL `issues`, `factory_discovery`, `factory_retention`: **64 + 35 + 8 passed**;
  migration suite: **7 passed**, including forward failure/snapshot rollback.
- Real manual browser pilot: **passed**. Real execution PostgreSQL/MinIO/browser
  pilot: **passed**, including a third separately reviewed launch.
- Post-expiry snapshot restore: **passed**, including unchanged pending execution
  request/digests and comment quota/replay metadata.
- Focused Console regression: **48 passed**. Complete unit suite: **669 passed**
  plus the repository's Node contract scripts.
- Locked API generation, Console Prettier/lint/typecheck: **passed**.
- Architecture: **passed**, 63 packages/256 dependency edges. Strict OpenSpec: **passed**.
- Final focused Rust: **648 application + 82 store + 165 REST handler tests passed**;
  the REST library's **12 tests** also passed in the broader command.
- Runtime library: **211 passed**. Issue/intake/imported-review integration suites:
  **20 passed**. Configured-owner regression: **77 passed**, then the one historical
  schema fixture was corrected and its targeted rerun **passed**; eight lengthy
  requirements scenarios remain excluded as recorded below.
- Final `--all-targets --all-features` Clippy with `-D warnings`: **passed** for
  store, PostgreSQL, application, REST and server packages. Their Rustdoc with
  `RUSTDOCFLAGS='-D missing_docs' --all-features --no-deps`: **passed**.
- Console full rerun after internal extraction: **669 unit + 124 browser passed**;
  the two service-pilot tests were run separately and **passed again**. Locked API
  generation/typecheck and production build/bundle budgets **passed again**.
- `cargo fmt --all -- --check`, `git diff --check`, Console format/lint,
  architecture and strict OpenSpec validation: **passed**.

## Capability and release limits

Controlled process peers establish protocol/authorization/recovery behavior; they
do not establish live GitHub permissions, rate-limit behavior in an operator
account, native GitLab/Jira support or live model quality. Existing read-only
GitHub controlled-peer qualification remains separate from live release evidence.
No new provider adapter, SDK, credential path, Agent protocol, execution engine,
comment command interpreter or automatic completion/status inference is added.

The packaged Codex event path's existing process/release/backend qualification
remains its own release evidence. Missing native/privileged/live provisioning
cannot be relabeled as a passing local browser test. These changes do not publish
a release, archive/sync OpenSpec, or complete neighboring quickstart/role-profile
work. UI redesign remains deferred as requested.

### Final-run limits and internal read-state reuse

The full unfiltered application run did not finish two pre-existing
`factory_release_journey_tests` cases within five minutes (both continued using
CPU without completing). It was stopped; this is **not** a passing full release
suite and no claim of a diagnosed baseline cause is made. A second expanded run also exceeded the local time window in the existing
requirements journey group, including a long defect/rework scenario. Those
expanded runs were stopped. The focused application/store command excludes
`factory_release_journey_tests` and `factory_requirements_tests`; the PostgreSQL
configured-owner command excludes `requirements_contract` and
`specification_publication_and_correction_recover_exact_contracts_in_postgres`.
These groups are unqualified by this run, not diagnosed as baseline failures.
Intake, exact status actions, control, Source/Reporter and ordinary CI contracts
are selected independently in the final commands below.
No release-flow implementation or test expectation is changed to hide this limit.

The first Console build exceeded the existing total JavaScript budget by 2,023
bytes. After the existing behavioral baseline passed, repeated initial/error/
background query envelopes in task sections were replaced by `QueryContent` over
the existing notices. Query keys, independent authorization, bodies, pagination,
selection and UI flows are unchanged. The full 669-unit/124-browser regression
passes after extraction and the production build fits the **unchanged** budgets.
This is internal duplication removal; the requested UI redesign stays deferred.

### Final commands and fixture maintenance

```sh
cargo test --locked -p octacity-server-store -p octacity-server-application \
  --all-features --lib -- --skip factory_release_journey_tests \
  --skip factory_requirements_tests --test-threads=2
cargo test --locked -p octacity-server-api-rest --all-features --test v1_handlers
cargo test --locked -p octacity-server --all-features --lib \
  -- --include-ignored --test-threads=2
cargo test --locked -p octacity-server --all-features \
  --test project_text_intake --test factory_work_source_pilot \
  --test issue_execution_journal --test issue_discussion --test issue_code_review \
  -- --include-ignored --test-threads=2
cargo test --locked -p octacity-server --all-features --test factory_configured_flow \
  -- --include-ignored --skip requirements_contract \
  --skip specification_publication_and_correction_recover_exact_contracts_in_postgres \
  --test-threads=2
cargo test --locked -p octacity-server --all-features --test factory_configured_flow \
  the_unreleased_factory_schema -- --ignored --nocapture
cargo clippy --locked -p octacity-server-store -p octacity-server-store-postgres \
  -p octacity-server-application -p octacity-server-api-rest -p octacity-server \
  --all-targets --all-features -- -D warnings
RUSTDOCFLAGS='-D missing_docs' cargo doc --locked -p octacity-server-store \
  -p octacity-server-store-postgres -p octacity-server-application \
  -p octacity-server-api-rest -p octacity-server --all-features --no-deps
pnpm --dir ui api:generate
pnpm --dir ui format
pnpm --dir ui lint
pnpm --dir ui typecheck
pnpm --dir ui test:unit
pnpm --dir ui test:browser
pnpm --dir ui build
```

The old technical-state test seeded schema 51 and immediately used today's full
snapshot reader, failing because later outbox provenance columns were absent.
It now seeds the original Run at 51, applies the real forward migrations, then
verifies the same technical-only constraint and unchanged replayable snapshot.
No production behavior or constraint expectation was weakened. The disabled-
Factory input cleanup case initially lacked the disposable MinIO credential
environment; it passed after supplying that fixture configuration, without a
product change. Neither setup failure establishes product Red.

The final HTTP contract inspection also caught a missing `work_id` path-parameter
declaration. `issue_work_http_contract_declares_the_original_work_path_identity`
failed before the fix, then passed with the common required UUID path parameter;
all 165 REST handler tests and regenerated Console types passed afterward.

Canonical native lookup remains owned by the existing PostgreSQL Issue Source
module and is reused by backfill, rather than making normal Source import depend
on a migration module. The existing 35 Factory discovery/native association
contracts passed before and after that behavior-preserving move. Test-owned
PostgreSQL/MinIO and the isolated compilation target are removed after validation;
pre-existing services and the repository's existing target are preserved.

## Review fixes: behavioral Red → Green

Each new regression ran before its implementation. Tests use existing store,
HTTP and rendered Console boundaries rather than a second execution mechanism.
All Rust commands below use an isolated target, two build jobs, no incremental
compilation/debug data, and an explicitly disposable PostgreSQL service.

| Correction | Command | Observed Red → Green |
| --- | --- | --- |
| Indexed receipt retirement | `cargo test --locked -p octacity-server-store-postgres --test issues requirements_expiry_does_not_scan -- --ignored --nocapture` | Public expiry performed one sequential scan of shared replay history → zero scans with migration 0095; replay body remains unavailable |
| Production Source linkage | `cargo test --locked -p octacity-server-store-postgres --test factory_discovery new_source_admissions -- --ignored --nocapture` | New settled Source Work produced zero recovery associations → exact original Work/Run linked after restart, later input linked to the same native Issue, no invented request/revision |
| Stale legacy cursor | `pnpm --dir ui exec vitest run src/features/issues/IssueExecutions.test.tsx -t 'restart legacy history'` | First-page recovery disappeared after Conflict → reset remains available and fetches a fresh first page |
| Requestless card detail | `pnpm --dir ui exec vitest run src/features/issues/IssueExecutions.test.tsx -t 'opens original legacy stages'` | No stage hierarchy inside the card → shared stages, Results, Checks and Artifacts without a request lookup or launch |
| Original Work event HTTP | `cargo test --locked -p octacity-server-api-rest --test v1_handlers requestless_work_events -- --nocapture` | Work event URL returned 404 → bounded no-store metadata/detail through the existing authorized reader; revoked content rights still return 403 |
| Generated Work event transport | `pnpm --dir ui exec vitest run src/features/issues/activityApi.test.ts -t 'requestless Work'` | Request URL contained an object in place of a fabricated request → exact original Work metadata/detail paths and retained cursor |
| Mutually exclusive selection | `pnpm --dir ui exec vitest run src/features/issues/issueExecutionSelection.test.ts` | Reviewed execution retained old Work selection → original Work/event detail cleared, reviewed request selected |

The current Source producer writes pending association membership in its own
admission transaction. Native grouping/isolation tests therefore seed the current
producer/schema; the pre-upgrade schema/membership rehearsal remains in
`factory_discovery::legacy_issues`. Legacy and current Source recovery share the
same association transaction, canonical native lookup and bounded worker.

Final regression results for this correction:

- PostgreSQL: **108 passed** — `cargo test --locked -p octacity-server-store-postgres --test issues -- --ignored --test-threads=2` (65), `--test factory_discovery` (36) and `--test migrations` (7).
- REST: **166 passed** — `cargo test --locked -p octacity-server-api-rest --test v1_handlers`, including the complete allow/deny operation inventory and Work event metadata/detail rights.
- Console: **674 unit passed** — `pnpm --dir ui test:unit`; the final selection-key simplification also passed the 35 selected execution/journal/results cases. Original Work association rejection was characterized before replacing its bespoke error with the common invalid-response guard. Final `pnpm --dir ui exec playwright test tests/browser/issue-executions.spec.ts tests/browser/issue-journal.spec.ts`: **4 passed** after the shared-detail simplifications.
- Browser: **124 passed, 2 real-service pilots skipped** — `pnpm --dir ui test:browser`. These two PostgreSQL/MinIO-backed browser pilots were not rerun for this correction; their earlier results above are historical evidence.
- Locked generated API, TypeScript and production build passed: `pnpm --dir ui api:generate`, `pnpm --dir ui typecheck`, `pnpm --dir ui build`. Existing bundle budgets are unchanged. Shared `QueryContent` replaces repeated read-state wrappers; selected request/Work identity resets the shared detail once instead of repeating keys on each collection.
- `cargo clippy --locked -p octacity-server-api-rest -p octacity-server-store-postgres --all-targets --all-features -- -D warnings` and `RUSTDOCFLAGS='-D missing_docs' cargo doc --locked -p octacity-server-api-rest -p octacity-server-store-postgres --all-features --no-deps` passed.
- `cargo fmt --all -- --check`, `pnpm --dir ui format`, `pnpm --dir ui lint`, `python3 tools/check_architecture.py` (63 packages/256 edges), strict OpenSpec and `git diff --check` passed.
- Documentation/comments and formatting introduce no executable behavior and need no Red/Green cycle. No full workspace/release qualification is claimed. This was the earlier review-fix checkpoint; the subsequent completed qualification is recorded below.
- Disk remained at approximately **28–30 GiB free** with an isolated ~1.7 GiB Rust cache and two build jobs. The owned disposable PostgreSQL container was removed; existing local-stand services were untouched. No commit, push, archive or spec sync was performed.

## Completed PostgreSQL/browser qualification of 8.3–8.4

The manual pilot runs in three deliberately small test boundaries, sharing the
same runtime and browser launcher rather than another test execution engine:

- `issue_console_pilot`: real HTTP create/edit/reload, default workflow without
  Factory/provider, custom workflow adoption and a direct allowed move, optional
  declared label/number/date values with Unassigned and no directory, visible
  parent/cycle rejection, keyboard/narrow layout and a retained operator comment.
- `project_text_intake::issue_execution_http_resumes_original_inputs_after_runtime_restart`:
  pending publication/restart/lost acknowledgement, frozen original inputs, an
  actual running HumanGate during a later HTTP requirements edit, a later reviewed
  browser launch, declared clarification and one eligible technical retry. A
  completed Run with no Issue status action leaves status unchanged. A separately
  published explicit Issue action moves status once and recovers its lost response.
  The browser previews original MinIO bytes, then the common retention owner
  deletes them; HTTP and a second browser pass reject reopening while retaining
  provenance. The existing PostgreSQL status-action cases additionally cover
  authority changes and stale publication; application contracts cover competing
  and sequential applicability.
- `factory_configured_flow::issue_journal::original_issue_job_events_resume_after_restart_without_private_payloads`:
  a genuine ordinary Build/Job accepted through the existing service, fenced Job
  events persisted through the public append port, continuation after runtime
  restart and the exact original raw-event reader. Browser reload/resume retains
  two ordered typed observations without duplicating or exposing private transcript
  text until explicit payload inspection. The retained original Work is associated
  through the existing trusted association port in this journal fixture; it does
  not substitute for the automatic Source association exercised by the import pilot.

The PostgreSQL imported-card pilot uses `github` and `custom-tracker` as two
controlled vocabularies. On the same card it exercises assessment/implementation
admissions, Ready/Declined moves, title ownership, an ordinary comment, a declared
HumanGate, technical failure/one confirmed retry, and unchanged business status
on completion/merged PR. It classifies ambiguous/forbidden native pulls, records a
native conflict without rolling back local state, explicitly resolves it through
Reporter/outbox and preserves the original failed receipt. Native field scopes
are separately declared for assessment and the later Issue effect; the test does
not erase or bypass an earlier uncertain-write fence. Source outage, revoked
TaskRead/TaskUpdate and overlapping selectors preserve both original Work records.
Manual HTTP creation still succeeds during the outage. Independent review reads
retain old/new heads and passed/failed/missing/indeterminate external CI after
reader revocation; the real browser shows both original Runs and retained review
history/checks after server restart. Original ordinary CI/Job upload contracts are
also rerun.

### Test-first evidence and scope

These additions characterize already implemented behavior. Product implementation
was not changed, so no new product Red/Green cycle is claimed. Tests were extended
before qualification was accepted. Initial failures in fixture setup (lost default
Flow, incomplete asynchronous association wait, old versus current Run summary,
HTTP status expectations and browser selectors) were corrected in the harness;
those failures are **not** reported as missing product behavior or TDD Red.
Earlier product Red/Green evidence remains in the tables above.

The shared Source PostgreSQL installation was extracted from the existing runtime
fixture, the original in-memory pilot and PostgreSQL ordering regression pass after
extraction, and manual/imported clarification/retry use one shared controlled
executor over `FactoryFlowRunner`/`FactoryControlHandlers`. No production contracts,
dependencies, bundle limits or provider-specific behavior were changed.

```sh
cargo test --locked -p octacity-server --test issue_console_pilot -- --ignored --nocapture
OCTACITY_PILOT_BROWSER=1 cargo test --locked -p octacity-server \
  --test project_text_intake issue_execution_http_resumes -- --ignored --nocapture
cargo test --locked -p octacity-server --test project_text_intake status_actions -- --ignored --nocapture
cargo test --locked -p octacity-server --test factory_work_source_pilot -- --include-ignored --nocapture
cargo test --locked -p octacity-server --test factory_configured_flow original_issue_job_events -- --ignored --nocapture
cargo test --locked -p octacity-server --test factory_configured_flow deleting_a_managed_upload_does_not_replenish -- --ignored --nocapture
cargo test --locked -p octacity-server --lib ordered_projection_claims_and_hazards_survive_postgres_restart -- --ignored --nocapture
cargo clippy --locked -p octacity-server --lib --test issue_console_pilot \
  --test project_text_intake --test factory_work_source_pilot --test factory_configured_flow -- -D warnings
```

The real-service/browser tests remain opt-in and require explicitly disposable
PostgreSQL/MinIO and installed locked Playwright dependencies. Selecting their
Rust launchers runs the browser rather than silently skipping it. The earlier
full-release/requirements-suite limits above are unchanged and are not relabeled
as passed by this focused qualification.

Final outcomes: manual browser **1 passed**; reviewed execution/retention browser
launcher **1 passed** (two browser phases); shared status-action PostgreSQL
regression **2 passed**; Source/Reporter suite **3 passed** (the PostgreSQL pilot
runs its browser once per vocabulary); original Job-journal/browser **1 passed**;
ordinary upload **1 passed**; shared PostgreSQL ordering **1 passed**. Clippy
`-D warnings`, API generation, Console lint/typecheck/build with unchanged bundle
budgets, formatting and strict OpenSpec validation passed. The opt-in browser
assertions were executed through Rust launchers, not counted as skipped tests.
The macOS linker emitted its existing large-unwind-table warning when linking the
large configured-owner test binary; the test passed and Clippy passed separately.
The full release-suite limits recorded above remain unchanged.

Free space stayed above **22 GiB**, using the existing isolated Rust target with
two build jobs and incremental/debug data disabled. Only the two disposable
qualification containers were stopped/removed after verification; existing local
stand services were untouched. No commit, push, release, archive or spec sync was
performed.
