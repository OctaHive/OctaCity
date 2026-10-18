# Integration management contracts

The live `/api/v1/openapi.json` document is the source for the console's generated
`components`, `paths` and `operations` types. Connection, Project access, credentials,
connection checks, automatic input selection and Source diagnostics have separate
contracts. Provider names and native task containers are open identities; a task
container does not select the code Repository or an OctaCity Project.

Canonical synthetic examples live in
[`integration-contracts.json`](../../server/api/octacity-server-api-rest/fixtures/v1/integration-contracts.json).
They are published as OpenAPI component examples, validated against that document,
checked against typed application intents and reused by console transport tests.
Negative fixture checks reject invalid nullable credential types, oversized UTF-8
status values and over-limit preview arrays, while accepting their valid boundaries.
The credential example contains only a documentation placeholder. Ordinary responses
contain configured state and safe references, never the submitted value. The secret
submission's `writeOnly` field has a UTF-8 byte ceiling in addition to `maxLength`.

Initial connection/access/administrative publication uses `If-None-Match: *`;
replacement uses `If-Match` with the expected metadata version or administrative
revision. Mutations also require an `Idempotency-Key`. The individual precondition
headers are optional in OpenAPI because the required choice depends on the intent.
Checks, validation and preview need neither mutation header. Integration operations
require verified protected ingress; setting a browser header does not supply that proof.

## Console setup and discovery

Open **Integrations** in the primary navigation or command center. Create a named
connection by choosing an operator-installed provider, an approved endpoint and
authentication method, and resource/purpose pairs. Validate the draft, review it,
then publish its immutable metadata version. Editing starts from the current
version; a publication conflict requires refreshing the baseline and validating
the retained draft again. No arbitrary provider configuration or private path is
entered in this form.

The console uses three bounded, protected contracts in addition to the existing
publication and administration endpoints:

| Operation | Contract and authority |
| --- | --- |
| `GET /api/v1/integrations` | Current metadata, `View` collection permission and connection visibility applied before pagination. |
| `GET /api/v1/integration-installations` | Safe trusted declarations: open provider identity, immutable pins, approved endpoints/authentication, native resources, capabilities and logical credential references. Requires collection `Administer` permission and unrestricted connection visibility; restricted scopes fail closed. |
| `POST /api/v1/integrations/{integration_id}/validate` | Resolves the ordinary `IntegrationConnectionPublication` intent against its installed declaration under instance `Administer` permission. Returns a safe definition without publication, grants, credential reads or provider calls. |

Both lists accept optional exclusive `after` cursors and `limit` (default 50,
maximum 100). Empty results and unavailable services are different outcomes.
Discovery is composed explicitly with
`IntegrationManagementApplication::with_discovery`; it does not install adapters
or configure verified ingress. Deployments must compose the existing protected
management services and trusted declarations before using this UI. Missing
services or ingress proof fail closed; the console does not supply authentication.

On a connection page, credential setup offers provision, rotation, adoption and
revocation of its installed logical references. Submitted values are write-only:
the uncontrolled password input is cleared before sending and on cancellation,
scope/action changes or navigation. Values never enter confirmed-intent replay
snapshots, React Query query/mutation state, browser storage or returned metadata.
Recovery retries reuse the safe command identity without retaining the value.
Remote error text and details are discarded because a peer could echo a value.
Activation is a separate explicit choice, and revocation cannot request activation.

**Check access (read-only)** checks only the selected resource and purpose.
Configuration, authentication, resource access and capability readiness remain
distinct; unverified write capability is shown explicitly. Project access is
configured separately. Checks do not activate a connection or grant Project use.
A failed background refresh keeps previously loaded metadata
visible with a stale-data notice. English/Russian messages and keyboard review
dialogs use the existing console components.

Guided pipeline input selection is available in the Factory configuration workbench;
guided status mapping and progress presentation remain task 5.12.
A connection does not select an OctaCity Project, code Repository or pipeline and
does not create a task card. Existing manual Factory intake remains available;
manual internal-Issue creation belongs to the independent card capability.

## Guided pipeline inputs

Inspect a published Factory Configuration and open **Pipeline inputs**. **Admit
manual Work** opens the existing Artifact-based form without connection discovery.
Manual internal-Issue creation remains owned by the independent task-card change.

**Configure automatic intake** loads current visible inputs/groups and reusable
connections. **Add automatic input** selects an existing connection, an authorized
task container and installed selector/input mapping. The code Repository is a
separate choice and can use another host/provider; the server returns its exact
`definition_digest`. No credential is entered or copied into an input. Missing
connection setup or usable read access links to Integrations. Source input editing
does not import or refresh cards and does not request execution for a manual card.

New inputs start disabled. A user can enable intake explicitly, preview a bounded
read without admission, validate against current server authority and review the
pinned target, Repository, source and predicates before confirming publication.
Empty results, filtered tasks, exclusive-group ambiguity and unavailable reads
are distinct. Independent inputs can each admit a task; an exclusive group admits
only when exactly one retained member matches. Group publication references exact
member versions without replacing them or silently changing their pipeline targets.
The editor retains every selected member's immutable snapshot, including newly
selected historical versions, so refreshed lists cannot hide an active choice.
Publication still sends only the member identity/version references.

An installer can advertise editable normalized predicates using
`FactorySourceConfigurationHandlers::with_predicate_states`. This requires an
explicitly declared unrestricted base selection and a bounded list of opaque state
keys. Unadvertised profiles keep their installed filters; the UI cannot fabricate
support or widen the provider's resource, network, credential or read scope.
`WorkSourceInputDraft.predicate` is optional and omitted for legacy definitions.
When present, it is immutable with the input version: nullable state, literal
`all_of`, `any_of` and `none_of` label arrays. Each list is bounded to 64 unique
labels of at most 256 UTF-8 bytes. Contradictory filters are rejected. UI textareas
use one label per line and preserve spaces/commas; empty lists include unlabeled
tasks. Preview and automatic selection use the same normalized evaluator before
admission. Existing native provider protocols remain unchanged.

Drafts, validation proofs and preview results exist only in the mounted editor.
A proof belongs to the exact draft. Conflicts preserve the operator's selection,
require successful current-version/access refresh and then fresh validation and
confirmation. Publication uses existing idempotent confirmed commands and version
preconditions; creating another input does not change automatic replay identity
for an existing input. Metadata queries remain separate from transient preview
state. Closing the editor discards its transient data. An unpublished input/group
can refresh current dependencies without an existing publication and keeps its
initial-creation precondition; an existing version still requires its current
published record. A refresh never silently repins selected group members.

A change to the source connection, installation pin or native task resource omits
the previous Reporter and its parent configuration together from the new input
version. The form explains the resulting absence of external status reporting,
and the publication review shows it explicitly. Changing only the input mapping
within the same scope preserves the retained Reporter. Existing accepted Work,
Run history and frozen reporting operations retain their original references.

These controls require the deployment's existing opt-in protected Source
management, installed profile declarations and runtime/preview composition. They
do not install adapters, grant access or configure authentication for a stand.

## Project access and retained dependencies

On the connection page, choose **Manage Project access**, then an existing visible
OctaCity Project. The selector supports paged root Projects and child Projects;
native external resource names do not identify an OctaCity Project. The usable
resource list comes from the server's filtered `/resources` contract, independently
of the connection's administrative metadata.

**Edit Project access** replaces the Project's explicit resource/purpose grant.
Task reading, task updates, repository reading and Delivery remain separate
checkboxes; remote token permissions cannot add a checked purpose. Selecting no
purposes revokes all access. Review shows the Project, connection version, exact
resources/purposes and the effects of revocation before sending. Publication still
requires both connection and Project administration on the server. Reading choices
or opening the UI grants neither authority. No credential is entered here.

Access revisions are independent of connection metadata and administration
revisions. A conflict preserves the in-memory selection but requires a successful
baseline refresh and a new confirmation with a new command identity. Switching to
a newer connection version resets the old grant selection and explicitly requires
selecting resources again. Activation and read-only access checks remain separate.

`GET /api/v1/integrations/{integration_id}/projects/{project_id}/dependencies`
returns `WorkSourceConfigurationPage` containing visible retained input versions
using that connection, including disabled/historical versions. It requires visible
connection and Project access under the existing management policy, without current
execution authority. Target Configuration, Repository and connection visibility
are applied before pagination. The opaque exclusive cursor is scoped to connection,
Project and exact input identity/version; `limit` defaults to 50 and is bounded at
64. Group copies are excluded. The read reuses the existing input journal and
requires no migration, provider call or new dependency database. Install its
authorized service explicitly with `.with_dependencies(...)`.

Expand **Show Run and reporting history** on an input to read bounded existing
Source diagnostics. Only visible Runs for that exact input version are displayed;
reporting pending, in progress, applied and exhausted remain distinct from Run
execution. Additional pages and truncated reporting summaries are indicated.
This is Project-scoped visible history, not a claim that all global dependents have
been enumerated or that an old enabled version is still active. Hidden/unavailable
history is never presented as a complete empty result.

Revocation blocks subsequent protected provider operations, including retries.
Accepted Work, Run results, queued reporting history and already sent remote
requests remain retained; no external effect is undone. The card capability is
not consulted or required, and manual Factory intake remains independent.

`FactoryWorkStatusActionParameters` describes the existing installed `set_status`
action: an arbitrary `status` plus an optional finite `outbox` retry policy. The
original input's immutable Reporter configuration resolves the status to permitted
provider effects. An ordinary `trusted_action` node can use these parameters on a
stage outcome route, including an intermediate or nested stage. The generic
`FactoryFlowAction.parameters` envelope remains open for other installed plugins.
This contract does not add label-effect fields or a new source-update endpoint;
guided per-outcome effect editing belongs to task 5.13.

Source diagnostics retain bounded native correlation separately from input-scoped
Work/Run identities. An `issue` link is optional and supplied only by the authorized
association capability. The fixtures include absent and visible associations; no
card, Issue CRUD, explicit card execution or field-ownership API is required.
[Manual Artifact admission](management-rest-v1.md) continues independently of a
connection, token, automatic input or internal Issue.

Regenerate the ignored local client files using the repository's locked exporter:

```sh
CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_BUILD_JOBS=2 pnpm --dir ui api:generate
pnpm --dir ui typecheck
pnpm --dir ui exec vitest run src/api/integrationContracts.test.ts src/api/client.test.ts src/features/factory/api.test.ts
```

CI regenerates from the live Rust document before console type checking. Do not
hand-edit `.generated/api/`, maintain a second schema snapshot, or introduce
handwritten provider DTOs alongside these generated contracts.

## Guided status mappings and retained progress

At an automatic Factory input, select a Reporter mapping from the same installed
Source resource and separately granted `task_update` choices. Read intake does not
require this selection. Missing reporting access links to Integrations; selecting a
profile does not verify native write permission. Read-only connection checks may
still report write readiness as unverified.

`WorkSourceInputChoices.items[].reporting` optionally describes an exact installed
mapping and an `editable` ceiling. Missing descriptions are unknown, not editable.
A null editing ceiling permits selection/review of installed effects only. Trusted
installation composition attaches these descriptions with
`FactorySourceConfigurationHandlers::with_reporting_mapping`; descriptions do not
install an adapter, credential or Project grant. The declared ceiling must match
the provider-validated installed configuration and the hosted Reporter profile.

When editing is advertised, the operator can add/remove logical status rows,
select only permitted labels to add/remove, and preserve or select an advertised
native state. Logical statuses have no special approval/classification vocabulary.
`WorkSourceInputDraft.reporting` holds the resulting typed immutable mapping in the
existing versioned input publication. Each row requires `status`, `add`, `remove`
and nullable `state`. At most 128 unique statuses, 64 labels per effect list,
256 UTF-8 bytes per literal and 64 KiB for the mapping are accepted. Duplicate,
contradictory or out-of-ceiling effects fail validation. Validation and confirmation
are tied to the exact complete draft; changes require fresh validation and review.
An unavailable retained Reporter remains explicitly identified until replaced or
removed; it does not acquire fresh use or write authority.

The original admitted input freezes this mapping. Changing a current input cannot
retarget pending updates. The hosted Reporter resolves only a configured value
selected by an accepted action, validates its installed editing ceiling and uses
the existing independently fenced outbox/current-grant checks. No matching value
means a safe failure, not a guessed label. Ordinary Flow action authoring is still
the stage-effect surface; selecting an input mapping alone queues no update.

Use **Source and reporting progress** on an exact input/group to inspect retained
scan acquisition, selection and scoped retries. A match is not admission; ambiguity
in an exclusive group starts neither target. Visible native correlation and original
Work/Run links remain usable with no task card. A card link appears only for an
optional visible authoritative association. Pending/claimed projections await
confirmation; only applied projections confirm external effects. Exhausted updates
do not undo accepted Work or imply a failed Run. Attempt and eligible dispatch time
are reporting facts, separate from task business status and execution. Diagnostics
and previews do not poll production cursors, retry effects, import tasks or create
cards. Diagnostic error bodies/identities are redacted before query caching while
safe retry metadata is retained. Draft and preview payloads never enter browser
persistence. Manual Work admission requires no provider configuration; manual Issue
creation, field ownership and synchronization conflict resolution stay with the task
card capability.

## Stage-local source effects

An ordinary deferred `set_status` Flow action can include an optional typed
`source_update` (`FactoryStageSourceUpdate`) alongside its logical `status` and
existing finite `outbox` policy. The update contains the exact original Source and
Reporter bindings, a reviewed connection/access reference and one bounded
`WorkSourceStatusEffects` row. Its status must match the action's status. Add/remove
sets cannot overlap. Native item IDs, credentials and provider wire requests are
not authoring fields. Unrelated labels remain preserved.

Before activating an automatic input, validation also compares its original
Source, Reporter and complete connection revision against every explicit stage
source update in the target's pinned closure, including nested definitions.
Mismatch returns safe `/inputs/{index}/definition/target` diagnostics before
publication. Disabled inputs remain retainable for repair; enabling them performs
this check. Flow/action authorization and current runtime grants remain separate
checks. Reporter dispatch reuses the same input-pin comparison against original
accepted Work, so later configuration edits cannot redirect an existing effect.

Install the existing `FactorySourceConfigurationHandlers` as a
`FactoryStageSourceAuthorizer` with `with_source_actions` on both
`FactoryFlowDefinitionHandlers` and `FactoryConfigurationHandlers`. This is explicit
protected-service composition, not a grant created by the editor. The authorizer
reuses current connection choices and Project task-read/task-update grants, validates
parent profile pins and requires the installed Reporter's editable effect ceiling.
Both validation and publication inspect the complete resolved Flow closure. Absent
support fails closed for these parameters while ordinary/manual flows remain usable.

In the Flow editor, use a published deferred status action profile and connect a
declared outcome to that trusted-action node using the existing transition editor.
Select the original task source and compatible Reporter, choose/customize the literal
effects, apply them to the draft, then validate and review publication. Different
outcomes can route through different action nodes; the action need not be terminal.
For stage-level diagnostic provenance, use the existing explicit predecessor/record
input binding. Each nested definition is edited/published as an immutable Flow and
selected by exact reference. Stages without a selected action enqueue no update.

Confirmation includes exact effects and immutable Reporter mapping identity/digest
from the normalized closure, including nested definitions. Changed drafts need a
new validation proof. Choice lists use full immutable binding keys rather than
positions, so a background refresh cannot redirect the selected profile. Drafts and
unpublished effects stay outside browser storage and are discarded on leaving.

At dispatch, `accepted_factory_status_source_update` returns only parameters proved
by accepted action history and a live outbox claim. The hosted projector checks them
against the original admitted input and installed policy; stage effects override the
input-wide default mapping for that action. Current protected authorization and all
existing ordering, retry and deadline bounds still apply. Later input/mapping
replacement or card association cannot select another native item or authorize a new
write. Existing validation APIs and actions without `source_update` remain compatible.
