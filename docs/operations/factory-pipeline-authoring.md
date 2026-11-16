# Build a Factory pipeline from an empty Project

For a complete Russian walkthrough of two independent pipelines, see
[Triage and Delivery through the UI](triage-and-delivery-ui.ru.md).

Open the Project's **Factory** page and choose **Add pipeline**. This opens an
empty visual step graph. No triage preset, earlier Flow, Build or demonstration
Run is required. The graph is a Flow Definition; a separate Factory Configuration
selects its exact published version and controls admission. Neither is an ordinary
CI/CD Pipeline. Definition publication and Work admission are separate commands.

## Prepare the installation

The editor uses installed capabilities; it does not install them or copy a
workstation's credentials. An operator needs:

| Prerequisite | What to review |
| --- | --- |
| Server and storage | Factory enabled, authorized Project access, migrated PostgreSQL and retained-input/object storage. |
| Worker | An enrolled Agent/pool, ordinary Build Configuration and matching execution/result capability. Reasoning and command steps run as ordinary Builds. |
| AI execution | A compatible installed runtime/plugin/model and reusable Role/Profile with the intended instructions, skills and execution settings. |
| Isolation | A qualified backend and Agent execution contract supporting the requested source mode, mounts, paths, tools and private inputs. An unavailable mode is a blocker. |
| Credentials | Model authentication, agent MCP access and server Source/Reporter credentials installed independently, only where needed. |
| Policy | Applicable Project policy, budgets, concurrency and access ceilings; supported gate/action capabilities for the chosen graph. |

See [Factory installation and operation](factory.md) and
[backend qualification](../testing/backend-contracts.md) for installation and
enforcement. A saved profile or a readiness result is not proof of a successful
model/MCP call. Missing choices require installation/access remediation; they do
not require creating a triage pipeline first.

## Create shared documents

In Project settings, use **Project documents** to create, view or edit shared
goals, component guidance, priority meanings and other reference material.
Choose **New document** to create one, or **View** beside an existing publication
to read its formatted Markdown. **Edit document** opens the full-width editor;
switch between **Write** and **Preview**, then **Publish changes** or **Cancel**.
Publishing an edit creates a new immutable Artifact, even when its name matches
an older publication. Preview the intended publication and explicitly select it
in each recipient step. Opening, editing or publishing a document attaches
nothing and changes no existing Run.

Each step has one **Documents** list. Choose each document's role explicitly:
agent instructions retain controlling-policy semantics; reference material is
data. Neither grants permissions. An empty list is supported unless the selected
Profile explicitly needs additional Context. There are no mandatory goals/rules
slots. Follow **Input details** to reconcile a Profile's required logical input.
Project document navigation preserves the mounted pipeline draft; reloading or
leaving that editor discards unpublished changes. Private drafts are not saved
in browser storage.

## Prepare canonical Issue inputs

In Project settings, open **Issue field settings**. Administer the Project's
**Priorities**, **Components**, **Issue types** and **Sizes** independently of **Custom
Issue fields**. Initial priorities are `blocker`, `high`, `medium`, `low`; types
are `defect`, `feature_request`. Sizes initially offer `small`, `medium`, `large`.
Add, rename or retire size choices in **Sizes** and use **Up / Down** to order
them from smallest to largest. Publish the reviewed catalog before selecting
values in Issue properties or creating Delivery. Changing the catalog does not
classify an Issue or alter an already published Flow. Add the
Project's components and any ordinary text/choice/boolean/integer/date fields.
Review stable IDs, bounds and the catalog version before publishing. Rename or
retire existing choices instead of changing their identities. Project
administration and Issue editing require separate rights; publication supplies
choices without filling or changing existing Issues.

During catalog conflict reconciliation, size names and size ordering have
independent decisions, including a choice whose stable ID is `order`. An operator
may keep a local name with the current published order, or the opposite.

Create or edit an Issue with explicit requirements and acceptance, then select
its priority, component, type, size and any custom values in **Properties**.
A human can prepare it without a triage Run. Alternatively configure the
[selected-field filling](#fill-selected-issue-fields), run
triage and verify that the Issue shows applied values and their origin. An
accepted assessment card alone is not an applied classification. Resolve stale
or conflicting required fields before starting delivery.

For task-specific text, use the Issue's **Documents** view to create or publish
an immutable version. These documents remain owned by this Issue and are absent
from Project document catalogs and step document selectors. Use **Link existing
Project document** separately to choose an authorized exact shared publication;
it keeps its Project owner. In **Activity**, post or reply through the ordinary
comment form, then explicitly include the retained comment as Issue material.
Document publication and comment append neither select pipeline input nor start
execution. Review document/comment material selection separately and preserve
the exact version, owner and digest. See the
[document and comment controls](operator-console.md#issue-documents-and-exact-material-selection).

For a simple consumer, open **Issue snapshot → Configure Issue snapshot** in
the ordinary Flow editor. Select Project-backed fields with required/optional
handling. If supplementary text is needed, enable **Include explicitly selected
Issue materials**, set count/UTF-8 bounds and choose the fields/materials each
step receives under **Issue input for each step**. Confirm **Use Issue snapshot**.
This selects no producer and does not give sibling steps access to undeclared
text. The shared ceiling is 32 references / 1 MiB; narrower consumer and source
limits still apply. Publishing newer documents or comments does not replace
admitted inputs. To derive scheduling from classification, enable **Map Issue
fields to admission**. Select the required priority field and enter scheduling
scores for the intended Project choices; select the risk field and map supported
choices to admission risk. Select allowed type values explicitly. Unmapped
choices block admission. These mappings grant no launch authority or evidence.
The editor retains these settings and per-step projections when reopened or an
input alias is renamed. Under **Evidence policy**, add an alias, select the
classified input field, set required/optional and the maximum reference count,
and include that alias only in the intended step projections. If a trusted
observation is required, enter the exact environment and criterion references
(identity, version, digest) and expected finding. An ordinary field value or an
agent's reproduction claim is not trusted evidence. Reopening the form retains
these policies and projections; renaming either alias preserves their links.
Mixed bindings and exact-reference material policies remain preserved advanced
contracts.

Selecting the optional **Triage** example immediately opens the ordinary graph
editor with configured steps and routes. It has no preliminary setup form or
**Open example draft** action. Select executors on the steps and edit
**Fill Issue fields** on `classification.observed` using the same controls as any
ordinary agent. The server generates the result schema and matching save scope
from that selection. Review changed catalog definitions through
**Use current field definitions** in the result form. Opening an example never
publishes a Flow, enables intake or executes a model.

## Add and configure steps

Add reasoning steps, gates, commands or trusted actions from installed choices.
Use the graph controls or their keyboard equivalents to select, connect and
remove steps. Drawing position and step names do not determine execution order.

For each reasoning step:

1. In **Expected result**, choose **Add result** and **Save result**.
   A new result defaults to text (up to 4096 UTF-8 bytes); describe its content
   in **Step task**. Use **Result structure and limits** only when you need
   fields, alternatives or different bounds. Names/versions of the underlying
   formats are generated automatically. A unique result key is also supplied;
   edit it under technical details only when needed. Existing formats and advanced reuse
   remain available under technical details. Then choose the Role and exact
   Profile in **Executor** and explicitly use the
   profile. Compatibility and the execution binding come from Server. Inspect
   **View profile** for that selected version; browsing does not assign it.
   A fresh executor also gets a server-normalized empty data input. This is
   independent of Context: it does not copy the ticket or connect Work on your
   behalf. Existing manual input schemas and mappings are retained. Advanced
   schema and Context controls remain available for custom contracts.
   The Flow input follows the entry step while their contracts agree; an explicit
   manual divergence is preserved and must be reconciled before publication.
2. In **Step task**, write this invocation's assignment. Keep capabilities,
   reusable methods and shared skills in the agent. The same exact Profile can
   serve another step with different instructions and independent inputs.
3. Explicitly select **Task from admitted Work** for a native task-backed step.
   This connects the original task Artifact as Context. Naming a reference
   document `task` or mentioning the ticket in instructions does not connect it.
   Other Work documents/results must be connected separately.
4. Select exact **Documents** and their roles for this step. In **Tools and
   access**, inspect the selected Profile's MCP bindings and readable scopes.
   Technical references are under **Details**; secret values are not displayed.
   Every candidate Profile's declared requirements must fit the step and
   enclosing ceilings; editing task text cannot add a tool, credential or
   network destination. **Review required access** previews the resulting step,
   Flow and admission requests before **Apply to draft**. It retains existing
   requests; cancelling changes nothing. Server/agent policy still determines
   effective authority. Manual access controls remain in expandable sections.
   Overlapping mount requests that would widen a read-only parent require manual
   reconciliation; the preview does not silently broaden that parent.
5. Review **Step code**, its budget and retry settings. New steps calculate
   budgets from their candidate Profiles and selection overhead when available.
   Choose **Set manually** to override; changing Profiles or closing settings
   does not discard the override. Retry, required-branch and phase-pool controls
   remain under **Advanced step settings**. Reconcile unsupported resources
   against the selected executor's requirements.

For a command step, **Use command** copies the installed compiler's permission
request into the draft and ensures its elapsed-time and combined artifact/report
limits fit the node budget. Existing grants and larger node limits are retained.
The Delivery example applies the same budget rules during resolution. Review the
resulting Flow and admission access before publication; selecting a compiler does
not grant effective execution authority.

For example, the following assignments need no specialized agent or stage name:

| Step | Explicit assignment and selected guidance | Result contract |
| --- | --- | --- |
| Assess request | Assess fit against the selected Project goals. Search duplicates only through the configured authorized MCP tool, retain scope/coverage and distinguish missing evidence from a negative finding. Route uncertainty to review. | Bounded fit/duplicate assessments, reasons and a concise summary; verified evidence requires the configured independent provenance checks. |
| Classify request | Use the original task, accepted assessment summary and selected component/priority documents. Preserve missing or ambiguous information and recommend a route without authorizing an external write. | Bounded component/priority strings, reasons and uncertainty fields. No duplicate component/priority enum is required. |

A schema validates structure and types, not semantic compliance with prose.
Configure stronger checks or human review explicitly. Model-written claims of
verification do not establish reproduction or duplicate evidence.

In **Accepted results**, select the producing step, its result and the field
to pass. **New input** creates a bounded input format from that selected field;
you only name the input. Additional selected fields extend an object input
without copying the producer's other fields or Context. Choose an existing
compatible input field to reuse it. An occupied name cannot be overwritten.
Result edits stay in the graph draft until saved or cancelled; incomplete edits
and pending input validation block Flow publication. Publication still requires
the existing authoritative validation and confirmation.

## Connect control and accepted data separately

Set **Pipeline name** at the top of the graph editor. The name is saved with each
immutable Flow version and appears in discovery, root Flow and subflow choices.
Renaming publishes a new version; existing Runs retain their original definition.
Historical unnamed Flows display their entry step until a name is assigned.

Choose immutable versions from the **Version** dropdown. Histories larger than
100 versions expose a search by number inside the menu, keeping rendered choices
bounded while retaining access to every version. Configuration summaries separate
execution state, human-readable resource limits and intake actions; **Technical
details** retains exact policy references and original budget units.

Dependent selectors appear after their prerequisite is selected: producing step,
outcome (when needed), result field, then destination input. Each selector is
labelled. Clearing a prerequisite hides its dependent choices without removing
saved connections. The same sequence applies to executor role/profile/runtime
selection, Reporter connection/source/mapping, and automatic intake
connection/container/input mapping. Independent settings, such as the code
repository, remain available.

Format selectors display current `step.result` aliases, or the owning step's input
name. Unassigned formats retain a numbered label. **Exact format identifiers**
opens the complete identity/version/digest catalog; selecting and renaming aliases
never rewrites an immutable reference.

The **Completion result** list offers step result formats. **Technical completion
formats** retains every declared data contract for manual/custom completions;
an existing custom selection is preserved. The incoming control transition
determines the producing step; choosing a format does not create or change a route.

Connect each declared outcome to its next step or typed terminal. Gates require
an explicit predicate, result-data mapping and routes. To pass an unchanged
bounded result, enable **Choose result data** and select the root path; for a
summary, select only that field. Provide declared rejection, review or no-effect
handling for uncertainty and unmatched values.

For a downstream pipeline, open **Flow settings** and choose **Incoming pipeline
completion**. Select a published Flow completion and the successful outcomes
that can supply its input. In each consuming step, select the **Incoming** input
source and the exact format supplied by that completion. Use **Retained step
input** in advanced mappings when a downstream trusted step needs the input that
an earlier step actually reviewed. A deterministic check can mark a successful
outcome to freeze the accepted Work contract. Its advanced options control the
precondition and the data retained for specific outcomes.

A control edge makes a successor eligible; it transfers no private Context.
Configure an explicit accepted-result input/projection for the successor, with
the predecessor, outcome and selected fields. In the example, classification
receives the accepted assessment summary, not its private investigation, tools,
credentials or transcript. Even when two steps share a Role/Profile/model, their
invocations and workspaces remain separate. Server retains authoritative states,
accepted results, input identities and recovery history.

Reporter settings are separate from assessment instructions and documents. Use
an installed authorized action, explicit accepted-result conditions, target and
allowed literal effects. Label-only leaves state and unrelated labels intact;
closing requires its own authorized state change. An unmatched model value uses
the configured fallback, never an inferred label or close command. Delivery
pending/failure remains distinct from the accepted action result.

## Fill selected Issue fields

In an ordinary **AI agent** step, edit a successful **Expected result** and enable
**Fill Issue fields** (off by default). Choose core or supported custom Project
fields once. The picker shows authoritative types, choices, bounds and the pinned
catalog version. Each selected field is required by default; use **Required: …**
to make an individual field optional. Omission preserves its previous Issue value.
Save the result: the server generates its typed contract and the same guarded
write scope. No second schema, result-path mapping or manual apply step is needed.
Free-form result editing remains non-writing. **Show on Issue** is an independent
optional card; an incompatible existing card must be reviewed or turned off.

Review the existing **Required access** proposal, which requests the local writer
for the controller without adding it to the Agent Profile. Publication reviews
fields, types and requiredness under the exact step/outcome. Accepted analysis and
confirmed application are separate facts: a save can remain pending or fail with
conflict/denial without undoing the analysis. The trusted controller owns the
Issue association, invocation baseline and result provenance. It applies only the
selected accepted values; it does not enable external effects or another pipeline.

Reopening and step/outcome/display-name edits preserve field identities and the
existing generated schema. Renaming an agent result also updates downstream
**Accepted candidate from another step** bindings to that exact producer/outcome;
other producers and other outcomes retain their references. Catalog changes retain
the draft and block saving
until explicit **Use current field definitions** review. Missing fields are not
substituted: remove them explicitly before saving. Turning off filling removes
the automatic write and its unused node-local writer permission while retaining
unrelated permissions and the ordinary result.

Headless authors use the same `issue_fields` form on the existing schema query and
outcome write declaration; see [the REST field contract](../reference/issues.md#selected-issue-fields-in-factory-rest-authoring).
Project setup requires ordinary field definitions, not fixed semantic roles.
CI reports, documents, accepted-result inputs and Decision Signal routing remain
separate existing input/control settings. A reproduction choice is data, not an
independently verified observation.

## Analyze CI reports with ordinary steps

A CI-analysis pipeline uses the same primitives as any other pipeline. Select an
installed **Command** and its typed retained report. Connect only the needed
report fields through **Accepted results** to an **AI agent**. Select its primary
**Task from admitted Work**, independently of those typed report inputs. Enable **Fill Issue
fields** on its successful result and choose the Project fields to update. The
report's test verdict and successful completion of its analysis are separate facts.
A missing required report fails the producing command; it is not a passing report
and cannot supply an accepted input to the analyst.

For advisory routing, add a **Decision Signal** step, select **Decision Signal
policy**, then **Use Decision Signal policy**. This copies the installed finite
input/output contract and requests its declared access. Connect the selected input
and wire each output through ordinary routes. A published Flow is not required as
a template. Provider configuration, credentials, calibration and control mode stay
with the installation; the selector grants no additional authority. Only visible
installed adapter/policy dependencies are offered. The selector shows the exact
policy version, independently of the adapter version. A policy with multiple
retained bindings for the same Project/reference is not offered for new
selection; the installation must supply an unambiguous policy reference.
Existing admitted Runs keep their exact executors. Save or cancel an open result
form before applying a policy: the editor retains the draft and disables
**Use Decision Signal policy** until the form is resolved. Applying a policy
also waits for an in-flight **Accepted results → Use selected field**
request to finish. The editor explains the wait and then re-enables the button;
applying the policy afterwards uses its own input contract, without a late
schema-normalization response overwriting it. Reapplying an unchanged
result contract preserves its Issue card and public-result disclosure settings.
A different result contract requires fresh display/disclosure review.
A shadow policy retains its configured baseline route regardless of the
provider's advisory choice.

An independent consumer selects the saved fields in **Configure Issue snapshot**
and starts from the Issue. It retains values, catalog pins and their original
assessment provenance without selecting a producer Run. No output-to-Issue mapping,
duplicate result schema or separate apply node is authored.

For real PR publication, select an exact source revision matching the current
target-branch tip. If the branch advances, publication returns `conflict`. Start a
new execution on the new exact base and repeat the frozen candidate checks; do
not reuse old review or validation as authority for another candidate.

## Select code only where needed

Declare authorized repositories in **Available code sources**, with revision
selection, access ceiling and scope. This inventory grants no step a checkout.
In each **Step code** section select **No code** or one declared source, its
revision/candidate selection and narrowed access/scope. Unused declarations grant
no input. Disconnect referencing steps before removing a source.

Select **No code** for document/MCP-only reasoning. Its worker must support
explicit source-free ordinary execution. An empty private workspace still exists;
there is no inherited Work checkout or bootstrap repository. Gates support no-code
execution. Source-free Jobs require negotiated execution contract v4 and a
qualified Agent/backend; legacy v1–v3 consumers do not gain support implicitly.

For code-backed steps, mutable selections freeze before dependent I/O and dispatch;
retries retain those exact pins. Different steps can select repositories A and B.
An accepted candidate must match the selected repository and retained provenance.
Work's repository subject is correlation/admission data, not a mount grant.

On the qualified Microsandbox path, read-only files and inclusive line ranges are
supported. Narrow writable selections can name directories or an existing regular
file. File writes inside a read-only tree require the
[qualified guest file-mount repair](../../config/runtime/microsandbox-agentd-v1/README.md);
the parent directory and unselected siblings stay read-only. Missing runtime
qualification blocks selected-source execution. Whole-repository access also
requires its declared policy; broadening a scope is not a runtime repair.
Backend/Build/Profile compatibility remains mandatory in every mode.

For a Build that captures a ChangeSet, open **Candidate capture** on the step
and list the writable directories. The same capture settings apply to every
candidate executor of that step. If the installed compiler uses a public
deliverable policy, enable **Require deliverables**, map the accepted Work
contract from the step input, and specify permitted paths, exclusions and the
minimum files per required category. These settings narrow the installed
capability and do not grant repository access.

Under **Advanced input and Context mappings**, select the exact candidate
revision and add named **Artifact collections** from typed retained inputs.
For optional branches, set **Only when input is present** to the pointer of the
required input. Connect protected evidence only to the trusted step that needs
it. Implementation can receive a separate public summary.

## Review purpose, limits and publication

In **Pipeline purpose**, edit optional **Pipeline goals** and **Completion
criteria** and select recipient steps for each. **Save goals input** / **Save
criteria input** confirms a new immutable input and selects it in this draft;
Flow publication is separate. Abandoning the Flow does not undo an already
published input. Editing process text does not edit a shared Project document.
Project goals, step result schemas and ticket-authored implementation acceptance
retain their separate meanings.

Review the policy sections before publication:

- **Start and queue**: entry step and concurrency within one Run; Configuration
  limits separately bound active Runs and nodes across that Configuration.
- **Execution limits**: cumulative Run spending, including attempts and retries;
  step/Profile budgets narrow it. A retry does not receive a fresh Run allowance.
  New Flows calculate this budget conservatively from every declared branch,
  execution retry and bounded repeat. Admission budget initially follows it.
  Either budget can be switched independently to manual values. Graph admission
  bounds are also calculated for a new flat graph and can be set manually.
  Subflows require manual budget/bound review; incomplete calculation is not
  presented as a resolved limit. Existing published Flow edits start in manual
  mode and retain their saved values.
- **Pipeline access**: requested capability ceiling, with independently narrowed
  step access. It attaches no documents, code or credentials by itself.

Local validation identifies missing executor inputs, Profile access/budget/Context
requirements, unrouted outcomes and absent completions before the server request.
Budget failures identify the resource, requested value and enclosing ceiling;
technical field paths remain under Details. A server rejection belongs only to
the exact draft validated; after an edit, validate again before publication.

Validate and review **Effective constraints**, their scopes and limiting sources.
Each candidate Profile has a separate access/resource ceiling, including its
Profile budget. Review these alongside the Flow/step ceiling; no candidate is
shown as selected by this preview. Exact Profile references are under **Details**.
Correct over-ceiling requests explicitly; unresolved support is not confirmed
enforcement. Validation creates no Flow, Build, Work or external effect. Confirm
the Flow publication, then create a Factory Configuration selecting that exact
published **Root Flow**. Review the **Launch policy** under **Start and queue**
and the **Access profile** under **Pipeline access**, alongside the queue and
execution limits. A new draft starts with the Project's exact defaults. Use
**View / Edit** to inspect them or select a different published version.
See [Project policy setup](factory.md#project-launch-policies-and-access-profiles)
if a default is missing or access is insufficient.

Keep the Configuration disabled until ready, then validate and publish an
enabled version. For a manual trial, use a policy that allows manual admission
and leave automatic inputs disabled. Automatic admission requires both an
enabled, authorized Source and a launch policy that allows it.

## Admit one task and inspect its result

For a manual trial, choose **Admit Work**, select the exact enabled Configuration,
and choose the Project/input subject for repository-free Work. Select **Project
documents** as the input origin and choose the exact task, completion/acceptance
and optional reference inputs; a previous Build is unnecessary. Review priority,
risk and the frozen inputs, then confirm Work admission. Use the process's own
completion input for a triage-only trial; never substitute it for missing
implementation acceptance in a later development workflow.

Optional automated input uses the separate Source editor: select the authorized
connection/container, preparation, filters and exact Configuration target;
validate, review and publish. Source preview only reads candidates. Publication
and activation are separate from manual Work admission; enable automatic intake
explicitly and inspect its saved current version. A matching preview is not an
admission receipt. Configure Reporter independently if external effects are wanted.

To start from a retained result, open the Issue and choose **Start pipeline**.
Select the downstream configuration and repository. Enable **Use a result from
an earlier run**, select the earlier Run of this Issue, then review and confirm.
Factory checks the selected Run against the published predecessor, terminal,
outcome, repository and current Issue requirements.

### Author ticket delivery

Prepare the [canonical Issue inputs](#prepare-canonical-issue-inputs), then
publish a consumer with an explicit Issue snapshot contract selecting its
required kind, size, component, priority and applicable evidence. It consumes
the applied Issue values, not a chosen assessment Run. Put the expected
implementation behavior in the Issue's acceptance before starting Delivery.
Use ordinary Flow authoring for a custom graph or open the released `v2`
example through **Start from an example** as described below. Keep **Incoming pipeline completion**
for an intentionally authored exact producer-result consumer or retained `v1`.

Build the routes in this order:

1. Route unresolved defects through the Tester reproduction step. A conclusive
   negative check ends in `cannot_reproduce`. Missing evidence or an unavailable
   environment needs its own stop or wait route.
2. Route sizes at or above the selected specification threshold through OpenSpec authoring. For smaller sizes,
   freeze the Issue's explicit acceptance criteria in the Accepted Work Contract.
   If requirements approval is needed, configure the component, size and priority
   rules and connect the exact prepared requirements to the operator gate.
3. Run protected test authoring before implementation. Give the implementer the
   accepted requirements and public test scope. Connect the captured candidate to
   an independent review and the original tests to trusted validation. Configure
   the rework limit and its exhaustion route.
   On a freshly provisioned stand, select **Tester · Delivery** for both AI steps.
   Choose **Prepare protected tests** as the author step's execution mode, and
   **Analyze CI results** for the optional analysis step. Review the required
   access for each step separately; the shared profile is a ceiling, not a grant
   of another step's tools or inputs. Connect the analysis to the mandatory
   verification Build. Factory starts that Build even if Tester makes no tool
   call; its failure cannot be replaced by an AI conclusion. Earlier published
   profiles and Flow bindings remain available for historical executions.
   After changing a mode, reconcile any incompatible permissions under **Manual
   step access**. Publication checks the selected mode's access limit; changing
   the mode does not silently remove an existing request.
4. Connect the passing review and validation of the same candidate to DevOps PR
   metadata preparation and trusted publication. Route the confirmed receipt to
   `delivered_for_review` and configure a separate blocked outcome for publication
   failures.

Publish the Flow and a Configuration pinned to that version and the reviewed
Project policies. A resolved Configuration draft starts disabled: review its
readiness and explicitly enable the manual target after validation. This does
not start a Run or enable Source intake. Qualify one Issue manually with automatic
intake and external Reporter effects disabled. Use a test publication target, then inspect the
reviewed Issue snapshot and its attribution, operator decisions, Builds and
publication receipt in the Run. Check the corresponding cards in the Issue's **Overview**. For a
reproducible test environment, see [ticket delivery qualification](../testing/manual-factory-authoring.md#ticket-delivery-qualification).

To use the optional example, choose **Add pipeline → Start from an example →
Ticket delivery · v2**. This immediately opens the ordinary graph editor.
Choose **Configure Issue snapshot** to inspect the Project definition version
and required Issue fields. Under **Snapshot admission**, map priority choices to
scheduling scores, map risk choices, and allow the supported `work_kind` values
(`defect` and/or `feature_request`). These are selections from the Project catalog;
do not maintain another set of classification enums in step outputs.

For each **Issue input field**, select the direct built-in or Project property
that supplies that template input. The current Project descriptor supplies its
type, labels and required/optional support; no semantic role binding or writer
plugin is needed to read it. A required Choice property can supply admission risk
or allowed values directly. Scheduling priority uses the required Priority input.
The existing admission mappings and per-step projections survive reopening.
Replacing an input's source clears that alias's scheduling/risk conversions and
allowed values, even if the new field uses the same choice IDs. Select the
intended conversions again; unrelated input mappings and thresholds are retained.
Changing required/optional handling alone does not replace the source.

After saving the snapshot, select the `requirements_needed` gate. Choose
**Choice field: size** and **Starting choice: size · Requirements needed**, then
**Validate and use condition**. The control uses the gate input schema's enum
order, supplied by the selected Project catalog. For example, with
`xs → s → m → l → xl`, choosing `l` routes `l` and `xl` through specification
authoring. It works for any enum input, without a Size-specific Console branch.
The released default expands against the current catalog when the draft opens;
if that choice is absent, the gate remains unconfigured until an explicit
selection. After replacing a snapshot source, review and validate the gate again.
Published gates contain concrete choice IDs: later catalog edits do not change
published routes or admitted Runs. Human requirements approval is a separate
policy; review its component/priority/size conditions as well.

Select each reasoning step's agent/Profile and installed execution mode. Use
**Tester · Delivery → Prepare protected tests** for protected test authoring.
Give that step an explicit compact test assignment within the installed test
bundle size limit. For repository-wide preservation checks, use an aggregate
checksum rather than embedding thousands of baseline file entries in the script.
Keep implementation assignments bounded as well. The native guest supplies
POSIX shell and Node; workstation tools such as Python or ripgrep are not
implicitly installed. Point the assignment at `/workspace/source`, batch small
checks, and review the Profile token budget: usage includes input tokens over
successive tool calls, including cached input, as well as output tokens.
The ordinary stand's Delivery stage allowance is configured separately in
`deployment/local-stand/factory-delivery.json` (1,000,000 tokens by default).
Triage retains its own smaller allowance. Reinstallation publishes additional
immutable Delivery bindings when the budget changes; retained Flows keep their
original pins. A Profile edited in **AI agents** may request a lower budget
within its installed capability ceiling.
Select the compatible installed baseline/candidate verification commands and the
reviewed publication target. Reproduction is optional only under the example's
explicit unresolved-defect stop route. Reproduction starts disabled in the
example draft. Configure all bindings in this editor before normal publication;
unconfigured executor bindings and report slots cannot be published. Choosing
an installed command connects its report contract to downstream typed consumers
and retains the configured capture policy. Examples start with no code attached. In **Available code sources**,
add the Project repository and reviewed revision, then set its access ceiling.
Use **Code scope → Selected paths** to limit the available tree to the task's
directories rather than giving every agent the entire repository.
Attach the admitted base to protected test preparation and the baseline command
with read-only access, and to implementation with read/write access within the
installed capture policy. The installed Coder instructions explain that Agent
captures source edits and generates the candidate artifacts after execution.
The isolated source may omit `.git`; the agent works on the approved files and
leaves capture artifacts and revision identity to the trusted controller.
Independent review receives a verified candidate change summary with paths,
statuses and file metadata restricted to its selected code scope. Its `complete`
flag identifies whether all captured changes are visible. Line-only selections
omit whole-file hashes and modes. The raw capture manifest and binary bundle
remain private controller inputs. Retain public regression scripts in the
candidate so review can rerun them using the installed guest tools.
For review, protected validation, failure classification
and candidate publication, select read-only **Accepted candidate from another
step → implementation → observed**. Keep deterministic gates and PR metadata
preparation without code. Review the requirements branch's source/capture scope
as well, especially when admitting large work that needs specification changes.
Under **Candidate capture → Require deliverables**, align permitted paths and
required kinds/paths with the task on both implementation and protected
validation. The example's documentation/test/script layout is a starting point;
for a change confined to `tools`, select the corresponding required deliverable
there and remove unrelated example requirements. Keep producer and validator
policies identical.
Review the requirements policy, instructions and publication settings, validate
and set the publisher's **published → Edit result → Show on Issue → Pull request**
to expose the confirmed PR in the Issue's Overview. Save the result, then validate
and publish. Create a Configuration for this
exact Flow with **Standard** launch policy and **Delivery** access profile, review
and explicitly enable it. No triage Flow or Run is selected. `v1` remains available
for intentional exact-producer use. The [headless resolver contract](../../config/factory/presets/ticket-delivery/README.md#current-v2-issue-snapshot-example)
remains an alternative for API clients.

Review/start the Issue against its current revision and returned snapshot
digest. The normal snapshot path asks for no earlier Run and can serve a
manually classified Issue and one prepared by a replacement triage Flow without
republishing delivery. Inspect **Runs** to compare the admitted snapshot with
current Properties; later edits do not reroute that execution. Re-review after
a conflict instead of resubmitting changed input under the old request key.
Snapshot review and detail resolve size display names from the retained catalog,
not the current Project list. A later rename preserves the original name and ID
in that snapshot; records without a retained label continue to show the safe ID.
The [headless sequence](../reference/issues.md#headless-issue-snapshot-sequence)
documents exact request/precondition handling. Enable Source intake and external
Reporter effects only through their separate reviewed configuration; completing
triage or selecting material enables neither.

An empty dependency list is valid without a dependency assessment. Nonempty
dependency claims still require applicable provenance. Admission freezes the
selected size, component, priority and reproduction decision; later Issue edits
do not change that Run's requirements branch or human-acceptance policy.
Recovery also preserves an older request's omitted empty optional dependency
list under its original snapshot digest; it does not add newly available input
to that sealed request.

For retained `v1` examples or intentionally authored producer-result consumers,
select the exact published triage completion and compatible executors, command
reports and review target. The existing **Incoming pipeline completion** controls
remain supported. **Earlier Flow version** selects an exact older publication;
it does not reinterpret it as an Issue snapshot. Upgrading uses new reviewed Flow
and Configuration versions. Connect code to the steps that need it and review
each access request before publishing.

New Flows can also select producer-result mode explicitly: pin the producing
Flow, terminal and eligible outcomes, then select an accepted Run. Its result is
frozen as Incoming data without copying it into Issue classification. Manual and
automatic recovery retain that original result and producer identity.

For trusted Delivery, choose **Server action** and **Publish candidate**.
Select the installed Project target and review its receipt result. Connect the
candidate revision, bundle and manifest explicitly. In **Step code**, select the
read-only source and **Accepted candidate from another step**, naming the original
implementation step and its accepted outcome. This binds the publication resource
to the candidate without allowing source changes. Under **Publication inputs**,
select the expiry paths for the accepted validation and review and the path to
the PR title and summary. The expiry paths must come from the accepted Build
proofs for review and validation. Both proofs must belong to the candidate being
published. A new PR write must happen before both expiries. The publisher can
still look up an existing PR within the original attempt.
If the Flow should branch on a publication conflict or rejection, add a failure
result and select it for that event under **Publication inputs**. Add a graph
route for each selected result. The example supplies both routes.

Use **Review required access** to see the selected target's adapter, network
hosts and logical credential profile before applying them to the draft. Review
the step request against the Project access policy before publishing. Keep
Delivery credentials scoped to the trusted action.

To show accepted results on the Issue, edit the step's **Expected result**
and use **Show on Issue**. The Ticket delivery example includes a card with the
retained specification manifest and bundle, and a card with the accepted
review's Build report. To edit either card, choose **Custom**, select the
conclusion field and choose the exact result fields under **Artifact links**.
Use **Specifications** for Project documents and **Code review** for an
attached Issue review. Each link follows its resource's own access rules.
For the successful **Publish candidate** outcome, choose
**Pull request** and use a Delivery receipt that requires the review URL. The
card links to the original Run and appears only after trusted Delivery has
confirmed a PR and the viewer can read its repository.
The link opens the PR in the forge. Review the retained head and target branch,
then merge there under the repository's branch policy, including for production
branches. `delivered_for_review` records PR publication. The forge records merge,
and any Issue status or label updates follow their separately configured rules.

In the Run, **Node attempts** shows which step stopped and whether it can be
retried. Open an attempt to see which accepted step results, admitted Work
inputs and kinds of Context it received. Before responding to a pending human
decision, **Run commands** lists the accepted results that reached that request.
The result digest distinguishes repeats of the same producing step. These views
omit private bodies, attachment handles and credentials.
**View stop details** opens the retained cause and usage for that
attempt. When a stop was observed after the deadline, the Console shows that
timing separately from the recorded cause. If a Build stops before its
verified usage is available, the affected attempt shows that wait reason until
the stop is settled. Pending approvals are available
through the Run's operator actions; ordinary Builds and validation evidence
remain under their own Run records.

Open the returned Run and inspect admission, node input/result records, linked
ordinary Builds and any separate delivery receipts. A blocked/unknown prerequisite
or waiting state is not successful execution. Restart uses original inputs and
accepted results, not the latest documents or Profile defaults. To stop new work,
disable the Configuration/Source; cancelling an admitted Run is a separate command.

See [migration and rollback](../reference/factory-pipeline-authoring-migration.md)
before upgrading. The [manual qualification guide](../testing/manual-factory-authoring.md)
records actual clean-state execution, isolation and recovery commands and their
scope. The optional [development stand example](local-stand.md#default-two-stage-triage)
uses the same contracts; it is not a prerequisite for this procedure.
