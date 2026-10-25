## Context

See [proposal.md](proposal.md) for intent and scope. This baseline was reviewed against `main` at `8e1dacaa` on 2026-10-26. It is repository evidence, not a new live-stand or packaged qualification result; tasks remain pending until their behavioral gates pass.

| Area | Existing evidence | Remaining released-composition gap |
| --- | --- | --- |
| Input administration | `server/api/octacity-server-api-rest/src/v1/adapter/integrations/sources.rs` exposes Source configuration/read builders; `FactorySourceConfigurationHandlers` owns bounded immutable input/group configuration | `server/app/src/runtime/integration_management.rs` installs metadata, checks, grants and retained dependencies, but not Source configuration/preview/diagnostics; the empty capability vector for dependency history is not an execution installation |
| Installed execution profiles | `factory_profile_file.rs`, `factory_profiles.rs`, Runtime settings and AI-agent Role Profiles provide normal typed installation and execution settings | Normal file installation does not attach the existing `with_work_sources` or managed Reporter paths; installed read/check credentials alone do not supply preparation, target or writer authority |
| Internal task import and refresh | `server/application/src/issue_source.rs` and `issue_source_read.rs` provide canonical import, explicit bounded read consumers and shared polling/progress; `docs/reference/issues.md` records their contracts | `ServerRuntime::start` passes empty `IssueRuntimeIntegrations`; explicit Source installation is available to alternate startup entry points. Project import controls are not exposed by the current Issue REST/Console path, which primarily attaches/refreshes an already existing Issue |
| Status synchronization | Issue mapping/transition handlers, `factory_issue_source_push.rs`, `work_reporter_projection.rs` and `FactoryWorkStatusOutboxWorker` already retain Issue and Work producers, exact effects and recovery | Normal deployment must load compatible mappings/writer profiles and supervise their delivery, including Issue projections when no Factory execution is configured |
| Workflow authoring | Archived `add-visual-issue-workflow-editor` provides canvas/inspector CRUD, connections, navigation, validated frozen publication and separate exact default selection | Consume these controls and immutable workflow pins; no second workflow editor or native mapping encoded as canvas layout |
| Console and stand | `AutomaticInputs.tsx`, Issue source/mapping/projection views, `tools/factory_integrations.py` and current stand installation helpers exist | Generic automatic-input errors hide classifications; a browser fixture or checked connection does not establish a working packaged import/admission/reporting journey |

The dependency-history, connection activation-prerequisite, provider-egress and retained-private-volume fixes are existing regression prerequisites. Recheck their current tests and packaged inclusion; do not repeat their implementations. The archived card pilot already spans native task consumers and reporters, but direct application calls and alternate installers do not prove normal startup.

## Goals / Non-Goals

**Goals:** One installed Source observation/profile boundary serving independently enabled Project Issue import/refresh and optional Factory admission; reachable protected UI/API operations; exact authorized outbound Issue/Work effects through existing ordering; reproducible packaged qualification and compatible retained state.

**Non-Goals:** Rebuilding Issues, canonical identity, field ownership, workflows, Runtime/Profile authoring, polling drivers, schedulers or effect journals; redesigning connection enrollment or the Factory canvas; mandatory execution/acceptance content for a task draft; automatic Issue creation from a connection/grant or automatic execution from import/refresh/status changes. No extra provider implementation or live user-repository write is part of default qualification.

## Decisions

### 1. Complete one normal installed assembly

Extend the current integration/Factory composition with a narrow deployment-owned Source installation shared by management, bounded observations and workers. Reuse the existing typed catalogs, verified registry, stores and protected authorization builders. Install Issue Source consumers through normal `ServerRuntime::start`, rather than qualifying only `start_with_issue_sources` or a manually constructed test graph. Source-only installation must work without optional Factory execution. Optional absence remains classified and manual tasks stay available; an advertised but omitted handler/worker fails scoped readiness.

Inventory list/current/exact versions, choices, validate/publish inputs/groups and derived profiles, preview, dependencies and diagnostics, together with Project Issue-import choices/control and existing per-Issue attachment/mapping/refresh/resolution. Reuse current public operations where they fit. Add the smallest generated Project-scoped contract only where an existing operation requires an Issue that has not been imported yet or cannot express explicit import lifecycle. Do not invent an Issue first, expose private configuration, return placeholder empty success or build another management service.

Alternative rejected: wiring only list/choices, or demonstrating a different installer in tests. Both can hide the next missing operation in the operator journey.

### 2. Share trusted profiles and keep activation separate

Resolve Source read bounds, supported predicates, observation consumers, optional Factory preparation/target and optional Reporter mappings from compatible trusted declarations and current Project grants. Reuse persisted derived profile/configuration parents and exact installed pins. Normal Runtime/Profile settings and Agent policy remain the owners of execution permissions; Source configuration cannot widen them. No hardcoded repository, Project UUID, business status or credential fallback is permitted.

Three independent intents remain explicit: enabling Project Issue import/refresh, enabling Factory input admission, and enabling field/status pull or push. A connection, credential, grant, access check, preview, workflow publication or default selection enables none of them implicitly. Initial import uses the existing ordinary workflow default pin and initial status. Native state does not select a local status until a separately reviewed supported mapping and guarded pull permits it. Later workflow/default publication does not migrate existing Issue pins or rewrite native mappings.

New grants, compatible inputs and import selections must be usable while the server is running. Resolve them under installed ceilings rather than a startup-only snapshot or per-input image rebuild. Read-only import needs neither a writer nor Factory task/acceptance Artifact mappings. Factory preparation separately requires its explicit valid bounded mappings and resolved Repository/target; absent acceptance data is classified, never fabricated.

Alternative rejected: deriving all authority from token scope, using a Factory input as the import opt-in, or making the workflow graph define remote effects. Each joins independently reviewed policy boundaries.

### 3. Reuse canonical Issue import before adding execution concerns

Use `IssueSourceHandlers`, `IssueSourceReadHandlers`, `WorkSourceObservations` and their existing Project/security/installation/resource/item correlation. Add only the missing installed lifecycle/control adapter around those consumers, with current grants, durable ownership and cancellation. Do not add a second polling algorithm or progress journal. Repeated/concurrent imports resolve one Issue; explicit attachment reports a visible canonical conflict without text-based merging.

Import creates an ordinary bounded draft with the published default's initial status, no fabricated acceptance and no Work/Run. Retained-link refresh remains independently opt-in and works after an execution selector no longer matches or its input is disabled. Link pull disablement, import discovery disablement, Factory input disablement and read-grant revocation retain their separate effects. Preserve reconciled freshness, source-owned field policy and in-memory/public-body retention bounds; unchanged, stale, uncertain or echoed observations cannot create uncontrolled revisions, push loops or admissions.

When independently enabled Factory inputs admit the same native task, use existing Source preparation and `associate_issue_work`/legacy reconciliation rather than creating an Issue per Run. Retain distinct input-scoped Work/Run identities, original snapshots and provenance. If admission precedes import, later canonical import/association recovery converges under existing policy; the admission poller itself is not new permission to import. Execution-only/no-card intake remains valid.

Alternative rejected: creating one Issue for each source admission, treating equal text as identity, or requiring a pipeline to import a task. These contradict the completed card contracts.

### 4. Install supervised consumers and both existing Reporter producers

Attach Factory Source workers through the existing installer/supervisor and current input/group resolution. Reuse claims, fences, overlap/reconciliation, retained preparation and ordinary admission replay. Run the independent bounded Issue observation consumer under the same established supervision/cancellation conventions without making Factory configuration a prerequisite. Current disablement/grants are checked before subsequent protected operations; failures stay consumer/input-local.

Install the existing Reporter projector/outbox for both accepted Issue metadata effects and Work-backed configured source-update actions. A permitted manual or pipeline Issue-status transition can retain a push intent only under its exact workflow/link/mapping, explicit OctaCity-owned push policy and current writer authority. A Work-backed action retains its originally admitted native target and configured effects. Neither prose, model output, Run completion, source echo nor a later card attachment creates a producer. Reuse overlapping-producer rejection and native-field ordering; do not enqueue both routes for the same desired effect or add a second outbox.

An Issue push must be deliverable without a Work/Run or Factory target when its installed writer permits it. Uncertain responses require observation before another write; retries retain operation/native/mapping identity and original deadlines under current grants. Mapping/credential replacement cannot retarget pending operations or renew their windows. Provider failure preserves accepted Issue status/revision and execution results, with pending/conflicting/exhausted delivery reported separately.

Alternative rejected: making Work-only Reporter installation stand in for Issue delivery, or writing remotely from UI handlers. Neither completes task synchronization through its existing authority boundary.

### 5. Complete the operator journey using existing task and workflow surfaces

Provide a Project task entry to authorized Source choices, bounded discovery preview and reviewed import enablement/import, without a preexisting Issue, code Repository or Factory target. Reuse current selectors, confirmation, generated contracts and Issue list/card/source views; expose only installed supported operations. Keep explicit per-link attachment, pull/mapping publication and conflict resolution in their current views. Use the finished workflow canvas for business statuses/transitions and separately reviewed default selection; configure native status translation in the source mapping view with the Issue's exact pinned workflow.

Distinguish installed support, configured/granted read/write, import enabled, input enabled, worker availability, observation/import disposition, canonical Issue, admitted Work/Run and external delivery. Preserve classified safe errors and localized next steps, exact version review, keyboard/narrow layout and transient drafts. Recovering queries replace old failures with actual data; a provider outage is not a successful empty scan. Explicit execution remains discoverable after import, while automatic input authoring keeps its existing separate configuration path.

Alternative rejected: a pipeline-only form for all task imports, opaque identity entry, generic safe-failure text or a new status editor. These hide missing prerequisites or duplicate finished controls.

### 6. Qualify the normal packaged path

Use one reusable harness through the actual Console/gateway/server, disposable PostgreSQL/object storage, verified adapter process and hermetic approved provider API. First prove a native task becomes exactly one visible Issue with no Factory configuration/Run. Then review a supported mapping and manual business transition and observe its exact fixture effect through the Issue projection path. Independently enable Factory inputs or request execution and verify original Work/Run association and an explicitly configured pipeline status action. Include two inputs/Projects, differing native and code repositories, arbitrary workflow/provider vocabulary and read-only/no-card variants at the appropriate narrower boundary; do not build a cross-product matrix.

Restart at controlled import/admission/effect barriers and verify stable Issue/Work identities, snapshots, mapping pins, ordering and original deadlines. Exercise connection/input/import/pull disablement and separate read/write revocation without credential fallback. Provider outage must leave manual tasks, manual Factory admission and retained results usable. Browser REST stubs, direct use-case imports and alternate startup helpers cannot substitute for this gate. A deterministic execution/model fixture may supply real ordinary results; record that boundary without claiming live model qualification.

Share this harness with `simplify-integration-setup`, which adds its own enrollment lifecycle cases rather than duplicating Source/Reporter fixtures. Current supported multi-step setup is sufficient here. Default qualification uses no live user credential or mutation; optional live reads are separate evidence and optional live writes require an explicitly authorized disposable target.

## Risks / Trade-offs

- [Installed API still has no normal consumer] → Test normal startup and protected HTTP, not only alternate installers or mocks; qualify both Issue and Work projections.
- [Import and execution become accidentally coupled] → Prove a Source-only Project with no Factory target and an independently enabled execution-only input; reuse canonical association recovery.
- [Duplicate or looping status producers] → Keep field authority, workflow pin and native mapping separate; reuse overlap validation, source-echo suppression and ordered outbox tests.
- [Later grants/resources outlive startup state] → Resolve current managed declarations under installed ceilings and prove a second Project/input/import becomes usable without restart.
- [New installation replaces operator Runtime/Profile or private state] → Apply additive compatible configuration, preserve immutable bundles and verify retained-state restart; unrelated runtime MCP setup remains independent.
- [Builds exhaust disk] → Check free space before/during scoped builds, reuse caches, bound parallelism and remove only task-owned disposable resources; no broad cache/target deletion.

## Migration Plan

1. Record the repository and packaged baseline and run existing Source/Reporter/Issue/manual tests. Preserve completed card, workflow and Runtime/Profile behavior; do not mark an inspection as packaged completion.
2. Add compatible explicit Source consumer/preparation/writer declarations through existing typed installation. Old absent capabilities stay unavailable; no connection/grant automatically imports, executes or pushes. Preserve existing immutable versions and credentials.
3. Compose protected handlers and independently enabled Issue/Factory consumers in normal startup. Existing accepted links, Issue workflow pins, mappings, Work/Run and pending effects keep original provenance and current authority checks.
4. Update stand generation/readiness and qualify clean plus retained-state journeys through the supported UI. No reset, private-file hand editing, real-repository write or migration to the latest workflow/default is required.
5. Roll back by explicitly disabling new discovery/admission/delivery through compatible configuration while preserving canonical links, journals, pending history and credentials. Manual task/Factory paths stay usable; never erase progress to force an old binary to start.

## Implementation evidence

### Section 1 baseline (2026-10-26)

All Rust commands below use `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0` and `-j 2`. PostgreSQL commands use `OCTACITY_POSTGRES_URL` for a task-owned disposable PostgreSQL 18.6 instance; the personal stand is not modified.

- `cargo test -p octacity-server-application --lib source -- --test-threads=4`: 184 passed.
- `cargo test -p octacity-server --lib runtime::work_source -- --test-threads=4`: 80 passed, 25 PostgreSQL cases excluded by their explicit ignore marker.
- Re-run those 25 with `--ignored --test-threads=2`: 25 passed against PostgreSQL. Normal-startup `runtime::integration_management_tests -- --ignored`: 1 passed; `runtime::issue_tests -- --ignored`: 3 passed.
- `pnpm exec vitest run src/features/factory/SourceInputEditor.test.tsx src/features/factory/SourceProgress.test.tsx src/features/issues/IssueSource.test.tsx`: 41 passed.
- `python3 -m unittest tools.tests.test_factory_local_stand tools.tests.test_factory_local_runtime`: 16 passed, 4 opt-in integration scenarios skipped; those skips are not packaged qualification.
- First normal-startup regression for uninstalled Source choices: Red observed HTTP 503 instead of specified `422 capability_unavailable`. This is a behavior failure, not an environment failure.

Packaged Source/import/delivery qualification remains pending. Normal startup still lacks the Source configuration/read builders, Issue installation and supervised Source/Reporter profiles described above; no live-provider or packaged success is inferred from the baseline.
