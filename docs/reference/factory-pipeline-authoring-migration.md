# Factory authoring migration

Factory uses one ordinary Flow/Configuration editor. **Add pipeline** opens an
empty step graph. The dedicated **Set up triage** entry and its document/profile,
policy, Source and reporting wizard are removed. This is an authoring UI change;
there is no new pipeline, execution, credential or document owner.

## Existing definitions and execution

Open a published Flow using **Inspect Flow Definition**, then **Edit Flow draft**.
Open its Factory Configuration separately to edit **Start and queue**,
**Execution limits**, **Pipeline access** and its inputs. This includes saved
triage definitions: their node kinds, names, schemas, Role/Profile versions,
Context bindings and accepted-result projections are ordinary retained data.
No preset resolution is needed to inspect, edit or execute them.

Editing creates an in-memory draft. Validation performs no publication or
execution; confirmed publication creates a new immutable version. Existing
Configurations and Runs retain their original exact Flow, document, task and
execution references. Specialized historical agent instructions are preserved;
opening a saved definition does not replace them with new reusable instructions.
Source activation, Issue import, manual Work admission and Reporter authority
remain separate explicit operations. Removing the wizard changes none of them.

Project settings own shared document creation, exact viewing and immutable edits.
Each step selects its own documents, instructions, Work-task input, executor,
code and permitted tools. Pipeline goals and process criteria are edited in the
common Flow and sent only to selected steps. See the
[operator procedure](../operations/factory-pipeline-authoring.md) for blank
construction and the [qualification guide](../testing/manual-factory-authoring.md)
for the real clean-state execution journey.

## Headless and generated clients

The read-only legacy operations `listFactoryPresets` and `resolveFactoryPreset`
are **deprecated**, not removed. OpenAPI marks them `deprecated: true` and supplies
migration guidance. They retain their original bounded request/response schemas,
authorization and `no-store` behavior for existing clients. The stand launcher
already uses the common contracts and does not require these operations. They
still only assemble drafts; they never publish or activate them. This
compatibility window is not a requirement or an entry point for common authoring.
No removal date is implied.

New clients use these existing contracts:

| Purpose | Contract |
| --- | --- |
| Installed execution, gate and action choices | `GET /api/v1/projects/{project_id}/factory-authoring-capabilities` |
| Authorized exact policy and definition choices | `GET /api/v1/projects/{project_id}/factory-configuration-choices` |
| Exact Profile-to-execution/result binding | `POST /api/v1/projects/{project_id}/factory-execution-bindings/resolve` |
| Declarative gate normalization | `POST /api/v1/projects/{project_id}/factory-gate-programs/validate` |
| Project document metadata, reads and publication | `/api/v1/projects/{project_id}/input-documents` |
| Schema normalization | `POST /api/v1/flow-data-schemas/validate` |
| Flow draft normalization | `POST /api/v1/flow-definitions/validate` |
| Flow publication and replacement | `POST /api/v1/flow-definitions`, `POST /api/v1/flow-definitions/{flow_definition_id}/versions` |
| Configuration validation/publication/replacement | `/api/v1/factory-configurations/validate`, `/api/v1/factory-configurations`, `/api/v1/factory-configurations/{factory_configuration_id}/versions` |
| Exact Configuration readiness (read-only query with an explicit input body) | `POST /api/v1/factory-configurations/{factory_configuration_id}/versions/{version}/readiness` |
| Manual Artifact/Project-input Work admission | `POST /api/v1/projects/{project_id}/factory-work` |
| Optional Source choices and preparation review | `GET /api/v1/projects/{project_id}/factory-source-choices`, `POST /api/v1/projects/{project_id}/factory-sources/validate` |
| Optional Source input publication/replacement | `POST /api/v1/projects/{project_id}/factory-source-inputs/{input_id}/versions` |

Read the served OpenAPI contract and regenerate clients with `pnpm --dir ui
api:generate`. Common validation takes an explicit ordinary draft, not a
`FactoryPresetResolutionRequest`. Migrate deliberately: author each step task,
schema, executor, document/resource selection and route, validate, review, then
publish with the ordinary idempotency key and replacement precondition. Do not
rename a URL and reuse a preset body; the common endpoints reject that shape.
Use `/api/v1/openapi.json` for exact methods, headers and bounded schemas. The
execution resolver takes `profile`, installed `capability` and `result_outcome`;
retain its exact returned Build binding rather than deriving a digest or copying
one from another step. Schema/gate normalization likewise returns authoritative
references. Flow-validation diagnostics explain the preview; they are not fields
to copy into a retained Configuration's `flow`. Use the returned ordinary
definition fields and, after publication, the actual published Flow reference.

Mutations use `Idempotency-Key`; Flow/Configuration replacement additionally uses
strong numeric `If-Match`. Reuse the original body/key for transport recovery;
after a stale-head conflict, reconcile and explicitly review a new intent.
Role/Profile publication carries each item's own `expected_current_version`.
Source-input creation/replacement has its own expected-version/conditional-header
contract. These preconditions are not interchangeable. Sequential publication
can retain a Flow or input even if a later step fails; keep successful receipts
and recover through the original owners without enabling partial setup.

The Console Factory adapter no longer exposes `listPresets` or `resolvePreset`.
Generated management clients retain the deprecated operations for compatibility.

## Upgrade and rollback

There is no saved-definition rewrite for this authoring cutover. Additive storage
migrations still apply to the new optional inputs and source forms; see
[schema migration rules](../operations/schema-migrations.md).

1. Record exact component, schema, plugin/backend and private installation
   versions. Stop new admission/automatic inputs as needed and capture a verified
   database/object-store consistency unit using
   [backup/restore](../operations/backup-restore.md). Keep old publication receipts.
2. Upgrade compatible Server readers, signing/compilation and Agent/runtime/backend
   support before admitting new forms. Source-free Jobs negotiate execution
   contract v4; legacy protocol fixtures and repository-backed intent keep their
   original meaning. No broad Host, repository or permission fallback is allowed.
3. Allow forward migrations, inspect saved exact definitions, and validate new
   drafts through common controls. Keep historical specialized Role/Profile
   versions intact. To adopt reusable agents or change task/documents/resources,
   publish explicit new versions and select the new Flow in a new Configuration.
4. Check authoritative readiness and exact Source targets before enabling new
   intake. Existing Runs and retries retain original content and input pins;
   capability/authorization loss can block dispatch without repinning them.

For a **definition rollback**, keep the current compatible installation and
validate the previous immutable Flow against current policy/capabilities, then
publish a new Configuration selecting it.
Reconcile Source inputs to its exact target explicitly. Existing admissions stay
on their original versions; disabling intake does not cancel them or undo a
provider effect. Inspect independent delivery receipts and cancel exact Runs when
needed.

For a **binary/schema rollback**, stop mutations and follow the snapshot-based
rollback window. An old Server cannot open a database with unknown newer
migrations. Restore the verified pre-upgrade database/object-store consistency
unit and compatible private configuration before starting the previous binary.
Reconcile admissions and external effects since that snapshot; restoring storage
does not undo provider writes or preserve records outside the backup.
Do not delete migration rows, edit checksums, rewrite Work subjects/digests or
reset a stand. An authoring UI rollback alone does not make old workers/readers
capable of processing new admitted subject or execution forms.

## Shared-contract handoff to planned workflows

This handoff records reusable boundaries for
[`add-factory-bugfix-quickstart`](../../openspec/changes/add-factory-bugfix-quickstart/proposal.md)
and [`add-factory-ticket-delivery-pipeline`](../../openspec/changes/add-factory-ticket-delivery-pipeline/proposal.md).
Their business workflows, preset/setup decisions and qualification tasks remain
owned by those changes; none is implemented or marked complete here.

| Boundary to reuse | Coordination required in the sibling change |
| --- | --- |
| Common capability/schema/gate/binding validation and Flow/Configuration publication | Optional business setup must produce ordinary definitions. Do not require a preset, saved Flow or specialized agent for common Add pipeline; do not add another compiler/binding owner. |
| Reusable Role/Profile and Flow-node `task` | Put reproduction, test-authoring, coding, review and delivery assignments in their own steps. Shared roles/models still get separate invocation packages, Context and workspaces. Preserve historical specialized profiles. |
| Project input publication and `ProjectTextIntake` | Reuse the [existing shared-input handoff](../../openspec/changes/add-factory-bugfix-quickstart/shared-project-inputs.md), original bytes, fenced preparation, deadlines and replay receipts. Support selected Project/input subjects explicitly rather than fabricate a Repository. The quickstart's stricter ingress bounds remain its own contract. |
| `context_binding.work_task`, Project documents and Flow process inputs | Explicitly select original task, reference/instruction documents and process recipients per step. Implementation acceptance remains ticket-authored data; triage completion is not evidence that requirements or a fix are accepted. |
| Flow `code_sources`, node `code` and ordinary Build/JobSpec | Select no code or a declared exact resource for each step. Check candidate/evidence repository identity, qualified path enforcement and v4 source-free support; preserve legacy byte/signature fixtures. Do not infer authority from Work correlation or a role name. |
| Accepted results and projections | Downstream delivery admission must explicitly consume the accepted triage result through its owned intake. Completing triage alone starts no development. Project only selected fields; protected tests, private investigations and transcripts remain inaccessible unless explicitly authorized. |
| Trusted actions, Reporter and Delivery | Configure effects separately, with accepted-result predicates, allowed literals and explicit fallback. Retain one logical operation, current write authority, original deadlines and truthful pending/failed/delivered receipts. PR publication is not merge or deployment authority. |
| Readiness and recovery | Reuse common bounded prerequisite facts, dispatch revalidation and retained owners. Do not add repeated readiness gates as a replacement for enforcement, a second input journal or a second workflow engine. Finite-peer qualification does not qualify live model/provider credentials. |

The sibling proposals predate parts of these shared contracts, including optional
Work subjects and protocol v4. Reconcile those assumptions when implementing each
workflow; retaining a proposal's old protocol label is not permission to execute
an unsupported mode. Use the
[compatibility matrix](../testing/manual-factory-authoring.md#compatibility-matrix)
and the qualification required by that workflow before making new release claims.
