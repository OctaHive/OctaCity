# Dependency baseline and shared-contract handoff

Task 1.1, inspected on 2026-10-27 at `69f0502f`. This is an implementation
baseline, not an executable-triage readiness certificate. Only this task is
covered; task 1.2 still owns the broader baseline suites and dependency checks.

## Observed installation

The generated personal stand catalog has one Project, one Role/Profile
compatibility declaration and **zero `codex` executor bindings**. Read-only
management observations returned `ready` for both `/health/ready` and
`/health/factory`; the visible Agent was online with one execution target and
**zero `factory_executions`**. These observations contain no provider request or
model invocation and do not qualify reasoning execution.

The distinction is explicit in the code:

- [Runtime installation](../../../tools/factory_local_runtime.py) publishes the
  Codex compatibility/model-discovery declaration and copies policy permissions.
  Its advertised HTTP/stdio settings support has no qualified MCP executables.
- [Default Runtime policy](../../../deployment/local-stand/factory-runtime.json)
  has empty network, tool, executable and credential-profile permissions and
  zero resource grants. It remains a metadata authoring ceiling.
- [Factory launcher](../../../tools/factory_local_stand.py) publishes an empty
  `codex` list and does not accept/forward an operator execution catalog and
  independent Factory Agent policy through its own CLI.
- The [ordinary launcher](../../../tools/local_stand_launcher.py) already accepts
  `--factory-profiles`, `--factory-runtime-bundle` and `--factory-agent-profile`.
  [Agent preflight](../../../tools/configure_local_stand_agent.py) validates that
  policy independently; its current changed-profile path requires a disposable
  reset. The supported non-destructive upgrade is still upstream work.
- [Runtime registration](../../../server/app/src/runtime/factory_profiles.rs)
  registers supplied executor bindings and keeps model discovery on explicit
  refresh. The startup regression below must remain green during later changes.

## Dependency and owner matrix

| Boundary | Current owner and implementation | Status and required handoff |
| --- | --- | --- |
| Source observation, import and automatic Work admission | Archived [complete-factory-source-runtime](../archive/2026-10-27-complete-factory-source-runtime/tasks.md); [composition](../../../server/app/src/runtime/composition.rs), [shared Source management](../../../server/app/src/runtime/source_management.rs), [installed profiles](../../../server/app/src/runtime/source_profiles_file.rs), [Issue worker](../../../server/app/src/runtime/issue_source_worker.rs), [managed Factory worker](../../../server/app/src/runtime/managed_factory_source.rs) | Delivered common owner. Preserve separate Project import, link pull and execution-input controls, current grants and original admission identities. Reuse its existing packaged environment; archive completion does not prove this triage's execution. |
| Issue and Work-backed Reporter delivery | [Shared Reporter composition](../../../server/app/src/runtime/source_reporting.rs), [ordered Work reporting](../../../server/app/src/runtime/work_reporter_ordering.rs), [Issue delivery contracts](../../../server/app/src/runtime/source_issue_delivery_tests.rs), [Work reporting contracts](../../../server/app/src/runtime/source_work_reporting_tests.rs) | Delivered common outbox/ordering owner. Triage 6.1–6.3 must configure its own exact effects/local Issue transitions and retain producer-conflict checks; no new Reporter or journal is needed. |
| Roles, executable reasoning and MCP | [Runtime/Agent setup change](../enable-local-stand-runtime-mcp/tasks.md), existing file installer and [executor registration](../../../server/app/src/runtime/factory_profiles.rs) | Upstream change is 0/5 tasks. Its 1.1–1.3 own forwarding execution inputs, enforcing actual guest/MCP/Agent capabilities and safe installation updates; 3.1 owns normal setup qualification. Triage consumes these boundaries in 4.3 and 8.1–8.2. Visible settings/model suggestions alone cannot satisfy this prerequisite. |
| Managed GitHub connection and resource enrollment | [Current native installation builder](../../../tools/factory_integrations.py), existing protected integration management; [simplify-integration-setup](../simplify-integration-setup/tasks.md) | Existing installed resources work through managed authority. The pending 0/32 setup change owns authenticated public/private resource enrollment, immediate/restarted use and connection lifecycle. A compatible preinstalled public resource is sufficient for an initial pilot; arbitrary private-repository setup remains conditional on that owner. |
| Existing-label vocabulary and updates | [Installation builder](../../../tools/factory_integrations.py), [native mapping validation](../../../server/infrastructure/octacity-github/src/status_mapping.rs), common installed reporting policy | Fresh public installation snapshots existing labels; retained startup reuses the same snapshot offline and refuses changed immutable bundles. There is no delivered in-place vocabulary refresh. A state-preserving refresh handoff is required before claiming arbitrary existing-label updates on an already configured stand. See the explicit acceptance boundary below. |
| Shared task/acceptance bytes and recovery | [Source mapping](../../../server/application/src/factory_source_mapping.rs), [Source preparation](../../../server/application/src/factory_source_preparation.rs), [ProjectTextIntake](../../../server/application/src/project_text_intake.rs), [Project input store ports](../../../server/core/octacity-server-store/src/project_text_input.rs) and [PostgreSQL implementation](../../../server/infrastructure/octacity-server-store-postgres/src/project_text_input.rs) | Publication, exact byte reads, retention and recovery owners exist. Task 3.1 adds explicit reviewed process-completion draft/reference contracts, bundled criteria and the shared private Project input reader; see [verification](completion-verification.md). Task 3.2 connects managed runtime publication/admission, fenced settings retention and ordinary signed transfer; see [managed verification](managed-completion-verification.md). Task 3.3 qualifies recovery and late revocation; see [recovery and section review](completion-recovery-verification.md). Do not duplicate the Project-input/submission journal or infer ticket acceptance from process criteria. |
| Strictly unlabeled selection | [Common predicate](../../../server/core/octacity-server-factory/src/source_predicate.rs), native selector and current managed Source API | Existing empty all/any/excluded sets allow labeled and unlabeled tickets. `label_presence` support is absent. Triage 2.1–2.3 own an advertised compatible extension and tests preserving old canonical bytes/digests/pins. This work does not require a live model backend. |
| Preset resolution and configuration readiness | [Template manifest resolution](../../../server/core/octacity-server-factory/src/flow/template_manifest.rs), [template instantiation](../../../server/core/octacity-server-factory/src/flow/template.rs), [Flow authoring](../../../server/application/src/factory_flow_definition.rs), [configuration choices](../../../server/application/src/factory_configuration_choices.rs), current scoped health | Generic graph/choice/validation boundaries exist. There is no implemented `factory-presets` management endpoint or exact Configuration/Repository readiness endpoint in the REST operation inventory. Triage 7.1–7.3 and bugfix quickstart share the planned contracts below; scoped health is not their substitute. |
| Concrete context, assessments and proof | Existing [eligibility](../../../config/factory/flows/base-flow/nodes/eligibility.json) and [classification](../../../config/factory/flows/base-flow/nodes/classification.json) parts, generic instructions, Context/Data projections and verified command outputs | Task 4.1 supplies reviewed required Project context, original-ticket projection and named failure diagnostics; see [context verification](project-context-verification.md). A supported executable two-agent preset, bounded duplicate lookup/tool attachment, isolated handoffs, classification vocabulary and applicable reproduction evidence remain triage 4.2–5.4 work. Full Repository Knowledge, JEV and downstream bugfix execution are not prerequisites. |

## Explicit prerequisite acceptance boundaries

### Executable installation

Before triage 4.3/8.x can claim success, consume an installed normal reasoning
executor with exact approved plugin/executable/model and Build bindings, a
qualified Agent inventory and separately approved backend/tool/resource policy.
Required HTTP/stdio tools must be available in the actual guest, not merely
listed by a browser form. Preflight/update failure must preserve the usable
installation, and successful restart must preserve settings, credentials,
Agent identity, accepted Work/Run and pending effects.

This is the remaining contract of `enable-local-stand-runtime-mcp`, not an
invitation to widen `factory-runtime.json`, add a second executor or grant
private fixture authority. Do not mark that change complete from this matrix.

### Label-scope update

The current `simplify-integration-setup` plan covers resource enrollment and
runtime use of newly enrolled resources; it does not by itself provide evidence
of refreshing a retained label snapshot. Keep this as an explicit undelivered
handoff to the integration installation owner, coordinated with triage 6.1 and
8.2. Its required behavior is:

1. Resolve existing labels using the resource's approved authority and finite
   bounds; create no remote label.
2. Present and explicitly review the replacement scope before activation.
3. Preserve old native/profile/mapping identities for retained tasks and effects;
   failure leaves the prior usable installation intact.
4. Retain credentials, Project data and accepted executions across restart,
   without resetting the stand or editing a retained bundle in place.

Selecting labels already in a compatible installed snapshot remains valid and
does not require implementing the wider enrollment workflow first. Missing
labels are a visible setup blocker, not an automatic scope expansion.

## Shared handoff to bugfix quickstart

`add-factory-bugfix-quickstart` remains 0/26 tasks and is not a hard prerequisite
for triage. Its [design](../add-factory-bugfix-quickstart/design.md) already reserves
the same generic APIs. The current implementation has template instantiation and
ordinary publication, but neither planned management API below is delivered.

| Planned common contract | First triage consumer and future quickstart reuse |
| --- | --- |
| `GET /api/v1/projects/{project_id}/factory-presets` and `POST /api/v1/projects/{project_id}/factory-presets/resolve` | Triage 7.1 owns the minimal shared discovery/resolution slice unless an upstream implementation arrives first. Resolve ordinary immutable drafts and exact visible dependencies, with no publication, provider/model request or object write. Quickstart reuses the same owner for its later `bugfix` preset; no second registry/interpreter or triage-specific lifecycle API. |
| `POST /api/v1/factory-configurations/{id}/versions/{version}/readiness` | Triage 7.2 owns the minimal shared exact-subject query unless already delivered. Reuse authoritative required-closure, installation/Agent, context and scoped dependency facts with freshness. Distinguish missing execution capability from configured/unverified remote reporting; collect no new provider/model/secret-value fact during the query. Quickstart extends/reuses this boundary instead of duplicating placement or validation policy. |
| Existing Project-input publication/read/recovery ports | Triage 3.x reuses Source preparation and shared input publication for purpose-bound process completion. Quickstart keeps its separate manual text-ingress/submission contract and stricter ingress bounds described in its [existing handoff](../add-factory-bugfix-quickstart/shared-project-inputs.md). Neither feature creates another input journal, upload service or synthetic Build/evidence owner. |

Triage 7.3 must record the delivered DTO/route/schema owner and actual validation
evidence here. This is a pending ownership agreement, not a claim that a shared
API or bugfix task has been implemented. Existing Work/Run inspection and
confirmed versioned publication remain the execution and mutation boundaries.

## Evidence and limits for this task

No executable source, script, configuration or migration is changed by 1.1.
Repository AGENTS.md therefore exempts this documentation slice from a new
Red/Green behavior cycle. Existing behavioral regressions are run unchanged;
their success is not reported as a new TDD Red/Green pair.

| Check | Result and scope |
| --- | --- |
| `python3 -m unittest tools.tests.test_factory_local_runtime tools.tests.test_factory_local_stand tools.tests.test_factory_source_installation` | 30 tests collected: 26 passed, 4 skipped. Covers generated compatibility/deny-all installation, retained Runtime state, immutable Source/label snapshots, scoped native mapping and normal launcher fixtures. The opt-in real Runtime REST pilot is among skipped tests and is not execution evidence here. |
| `CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 cargo test -p octacity-server --lib runtime::factory_model_catalog_tests -j2` | 7 passed, 0 failed/ignored. Includes `runtime_registration_never_waits_for_optional_model_discovery`, explicit-refresh cache preservation, independent/overlapping refresh behavior, plugin parameter bounds and installed settings support. |
| Current stand read-only observation | Both health scopes ready; one generated compatibility profile, no generated Codex executors; online Agent advertises no Factory execution. No remote lookup, model discovery, admission or native write was performed. |
| Existing Source packaged evidence | [Source runtime qualification](../../../docs/testing/source-runtime.md) documents the archived change's normal-stack provider-fixture journeys and reusable `SourceEnvironment`. It was inspected, not rerun in 1.1. Its deterministic execution/model fixtures do not qualify this two-agent triage or semantic model accuracy. |

The broader Factory/application/REST/store/UI baseline, live dependency readiness,
backend release evidence and packaged triage remain 1.2/8.x work. No paid model,
live GitHub write or installation update is part of this task. Disk space was
checked before the scoped build (about 26 GiB free) and after it (about 23 GiB
free); cleanup was not required. The cleanup threshold remains 10 GB.

## Implementation sequencing

Proceed with task 1.2's scoped baseline, then independent predicate and preparation
work in groups 2–3. Context/schema/preset contracts can use the existing generic
boundaries while upstream installation work is pending. Do not claim successful
real reasoning/tool execution or packaged operational readiness until the exact
execution and label-scope prerequisites above are qualified. The absence of that
final evidence does not make this inventory task or independent pure/Source work
blocked. Keep all neighboring change checkboxes unchanged.
