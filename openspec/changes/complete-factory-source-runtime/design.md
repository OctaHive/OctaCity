## Context

See [proposal.md](proposal.md) for the problem and scope. Inspection of the current implementation and running stand established these specific gaps:

- `IntegrationManagementApplication::with_source_configuration` exists in `server/api/octacity-server-api-rest/src/v1/adapter/integrations/sources.rs`, but the released `runtime/integration_management.rs` installs metadata, credentials, checks and grants without it. Valid input listing and source-choice requests return `503` through the real gateway.
- The same API family has separate preview, diagnostics, selector and reporting configuration seams. Wiring list/choices alone cannot qualify authoring or execution; each advertised operation must be covered by the released assembly.
- `FactorySourceConfigurationHandlers` already validates immutable inputs/groups and derives choices from exact installed capabilities, Project grants and configuration parents. Its history reads do not need execution profiles; the recent dependency-route fix deliberately supplies none and is not an execution installation.
- `FactorySourceWorker`, `FactoryRuntimeResources::source_worker`, `HostedFactoryWorkStatusProjector` and `work_reporter_dispatcher` already use authoritative journals, objects, current managed grants and the existing supervisor/outbox. `FactoryProfileRuntimeInstaller::with_work_sources` is exercised in tests but not selected by normal profile-file loading.
- `tools/factory_integrations.py` currently installs native read/write scopes, check profiles and credential publishers. These are not the additional preparation/mapping/target permissions required to admit Work or project an accepted action.
- `AutomaticInputs.tsx` requests real connections and current inputs, but collapses classified failures into a generic message. Component/browser fixtures can succeed despite missing released services.
- Recent working-copy fixes cover activation prerequisites, retained dependencies, provider egress and initialization of retained private directories. They remain prerequisites; their presence in a developer's live patched image is not proof that an immutable packaged build contains them.

## Goals / Non-Goals

**Goals:** One reusable source-runtime assembly shared by production startup and qualification; exact compatible profiles shared by authoring, preview and dispatch; observable explicit intake/reporting over existing lifecycle owners; reproducible qualification of the packaged stand with preserved state.

**Non-Goals:** Reimplementing connection enrollment, credentials, grant administration or archive/delete UX; creating Issues, synchronizing Issue fields or changing Project workflows; introducing another scheduler/outbox/secret store; redesigning the Factory canvas; making LLM decisions or remote permission checks authoritative. Source enablement is never inferred from a connection check or a successful grant.

## Decisions

### 1. Complete one assembly instead of repairing routes independently

Introduce or extend a narrow deployment-owned Source/Reporter assembly beside the existing integration and Factory assembly. It supplies the existing input configuration handlers, protected preview/diagnostic handlers, verified registry and Source/Reporter runtime dependencies. Instantiate handlers once per appropriate policy/store boundary and pass those instances to existing API builders. Keep the local verified service policy distinct from unverified TCP management; internal dependency checks use the same authorized caller rather than a broad fallback principal.

Inventory the public operation family against actual router registration: list/current/exact versions, choices, validate/publish inputs and exclusive groups, selector and reporting profile publication, preview, retained dependencies and input/group diagnostics. Extend REST/OpenAPI only for a genuinely missing safe operational fact, not to create a parallel API. A missing installed capability is classified; a missing handler for an advertised installation is an assembly/readiness failure. Empty placeholder handlers are not a valid repair.

Alternative rejected: supply an empty capability list just to turn `503` into `200`, or manually construct a different handler graph in an end-to-end test. Both conceal the next missing step and permit test-only readiness.

### 2. Share exact trusted profiles across authoring and execution

Extend existing typed deployment profile loading only where necessary to declare Source read bounds, explicit `SourcePreparationSettings`, permitted predicate/mapping customization and optional Reporter profiles. Native adapter access settings remain in the installed integration catalog; Factory preparation remains separately authorized. Produce existing `WorkSourceCapability`, profile and configuration-parent references so authoring and the worker resolve the same immutable selections, including persisted derived predicate/reporting profiles.

Project grants select a subset of installed authority. Resolve Project-bound profiles against the current grant and exact configuration, including a newly granted Project and newly published compatible input while the server is running. Do not require per-input image rebuilds or a restart to notice publication. Use the existing profile/store resolution mechanisms with the smallest missing seam; do not introduce a second mutable integration registry. A browser cannot supply arbitrary plugin paths, hosts, credentials or preparation authority. Missing/incompatible profiles fail explicitly before dispatch.

Task and acceptance Artifact mappings must satisfy the selected Factory schema with bounded real source content. If required content is absent, record the existing classified candidate failure; do not invent acceptance text to make a demonstration pass. Native task containers and code Repositories remain separate selections. No production repository name, Project UUID, business status or label is hardcoded; examples use fixture configuration. Bindings retain independent read/write purposes even when an operator has provisioned the same remote token for both.

Alternative rejected: derive all execution authority from a token or from native repository metadata, or silently reuse a legacy profile when a managed binding fails. Neither proves preparation/target permission or current Project access.

### 3. Install existing supervised Source and Reporter lifecycles

Construct Source workers through `FactoryRuntimeResources::source_worker` with managed connections and the same installed profiles exposed by management. Attach them through the existing Factory installer/supervisor and cancellation tree. Resolve enabled current input/group versions during bounded discovery; reuse claims, fences, progress, overlap/reconciliation, preparation journals, object publishing and ordinary admission deduplication. Input changes affect future discovery; accepted snapshots remain frozen. Disabled inputs and invalidated grants prevent later dispatch. Use fake time/barriers for failure tests, not longer deadlines or serialized execution.

Install Reporter dispatchers through the existing work-status outbox, exact accepted-action provenance and managed connection resolution. Reporting is optional: a read-only input needs no writer, mapping or remote mutation. An explicit accepted source-update action is the only Work-backed write producer. Source and Reporter outages remain input/effect-local, and retries keep original native targets, mappings, operation IDs and deadlines. Shutdown/restart resumes durable intent without a second poll loop, effect journal or lifecycle state machine.

Alternative rejected: call the adapter directly from UI handlers, poll from the launcher or browser, or reuse a pilot executable as the server runtime. These bypass the supervised authoritative boundaries already implemented.

### 4. Report operational facts without promising external availability

Distinguish installed support, profile compatibility, configured credentials, current grant, enabled input, worker lifecycle, scan disposition, actual admission and reporting delivery. Reuse existing safe diagnostics/operational metadata; add only the smallest typed field/reason if one is absent. UI messages use stable classifications and localized next steps, preserve drafts on version conflict and keep stale data marked. Recovering a read clears its old failure; it does not require another setup or grant publication.

For optional uninstalled Source support, show an honest unavailable-capability state and preserve manual intake. For an advertised installation whose API/worker graph is missing, fail scoped readiness/qualification. Startup must not contact a real repository, test writes, or block global CI/manual health on provider availability. A ready read check still does not prove admission prerequisites or remote write rights.

Alternative rejected: display a generic safe-failure sentence for every error, mark a worker ready from metadata alone, or interpret provider failure as an empty successful scan.

### 5. Make packaged composition the acceptance boundary

Qualification launches the actual packaged server and Console through the normal gateway using disposable PostgreSQL/object-store state, the installed native adapter process and a hermetic provider service whose API/TLS hosts are explicitly approved by the test installation. Do not intercept browser REST responses or replace the runtime worker/use case with a fake. A deterministic qualified execution fixture may replace external model work, but must produce real ordinary execution and retained Factory results; record that boundary and never claim live model qualification from it.

Drive the UI from connection setup and a separate Project grant to predicate/mapping selection, bounded preview, explicit enabled input publication, native-correlated Work/Run and accepted reporting effects. Use fixture-only task text with valid acceptance content and arbitrary label/status vocabulary; include a non-demo resource and a different code Repository. Verify the same browser diagnostics and Run links, then restart during a known admission/projection boundary and assert stable identity, bounded recovery and current revocation. Prove read-only/no-card and independent manual paths. Record executable/image revisions and profile pins so a patched container cannot stand in for the shipped source snapshot.

Default qualification uses no real GitHub secret or mutation. An optional real GitHub read smoke test is separate evidence, not a prerequisite for deterministic CI. Live mutation qualification requires an explicit disposable target and human authorization; an ordinary user repository is never selected implicitly.

Alternative rejected: treat UI mocks, direct handler calls, process health or connection checks as the end-to-end gate. They remain useful narrower tests but do not exercise the failed user journey.

### 6. Keep the three changes independently implementable

| Change | Owns | Shared boundary |
| --- | --- | --- |
| `complete-factory-source-runtime` | Installed automatic-input runtime, existing authoring API composition, execution/reporting and packaged qualification | Exact connection/resource/grant/profile references |
| `simplify-integration-setup` | One-form setup, resource enrollment, credentials and archive/delete UX | Publishes verified resources consumed by this runtime |
| `add-factory-work-card` | Persistent internal Issues, workflow/actions, field ownership and synchronization | Optional association/observation and separately authorized effect producers |

Implement this change against current installed connections first, without waiting for the simplified form or task cards. Share runtime/profile resolution with later managed enrollment instead of restricting consumers to a startup-only resource snapshot. This change does not add new enrollment or card semantics. Enforce currently available disable/revocation rules; later archive/delete semantics use the same dispatch and dependency fences. Preserve all completed tasks and artifacts in the other changes; reconcile shared implementation changes once rather than copy their task lists here.

Use one reusable packaged qualification harness for this change and the end-to-end setup checks in `simplify-integration-setup`. The latter adds its enrollment/single-form lifecycle cases to that harness; it does not need another copy of Source polling, admission or Reporter fixtures. Current multi-step connection administration is a supported starting point until that change lands.

## Risks / Trade-offs

- [A newly wired form still cannot dispatch its profiles] → Use one exact profile resolution path and test publication, derived selector/reporting replacement and actual dispatch in released composition.
- [Optional components accidentally become global prerequisites] → Qualify no-provider manual intake and provider-outage isolation separately from Source operational state.
- [Later grants/resources outlive a startup snapshot] → Resolve current published inputs/grants under installed ceilings and prove a second Project/input works without restart; preserve historical versions.
- [Restart alters permissions, credentials or authority] → Retain the recent private-volume/network regressions and compare safe state before/after a packaged restart, including pending effects and unchanged deadlines.
- [Qualification writes to an unintended repository] → Default to an isolated allowlisted provider fixture; separate optional real-provider read evidence and explicitly authorized disposable writes.
- [Builds exhaust local disk] → Measure free space before/during scoped builds, reuse verified source/cache inputs, limit build parallelism and remove only task-owned disposable resources. No broad cache or `target` deletion.

## Migration Plan

1. Establish the released API-family regression and baseline existing Source/Reporter/manual tests. Reuse or land the known dependency-route, egress and retained-volume permission corrections without duplicating fixes.
2. Extend versioned deployment profile loading compatibly. An old installation lacking Source/preparation support remains explicitly unavailable for automatic intake; it is not silently enabled or reinterpreted. Preserve existing immutable bundles and adopt a separately versioned additive profile configuration where needed.
3. Wire protected handlers and the supervised workers over the shared profile selections. New enabled inputs take effect without reboot; existing enabled inputs resume only under their current authority and original declared configuration. Never generate a new enabled input from a connection/grant.
4. Update Factory stand generation and readiness, then qualify clean and retained-state packaged journeys. Existing credentials, grants, Work/Run, source progress and reporting are retained; no reset, manual secret rewrite or live user-repository write is part of migration.
5. Roll back by disabling new intake/dispatch through existing configuration while preserving journals, profiles, pending effect history and credentials. Retain exact compatible binaries/schema policy; never delete authoritative progress to force an old binary to start. Preserve manual intake throughout.
