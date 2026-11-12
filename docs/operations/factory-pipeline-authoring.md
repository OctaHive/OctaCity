# Build a Factory pipeline from an empty Project

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
   rather than relying on a sibling's selection.

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

## Apply triage classification to the Issue

Classification is an explicit local trusted action. An accepted analyst result
alone does not edit the Issue or start delivery. The installed capability and
Project field administration remain separate from Flow publication.

For API-authored Flows, read the authorized Project's authoring capabilities and
reuse `issue_classification.field_contract.schema` as the analyst outcome and
classification action input schema. It contains optional built-in priority,
component, type (`kind`) and size fields, plus active semantic properties such as
severity, risk, reproducibility and recommended route. Choice IDs come from the
published Project definitions; size uses `small`, `medium`, `large`. Reference the
same immutable schema in each step rather than maintaining another enum list.
The all-fields preview is unavailable until a Project catalog is published, or
when the complete preview exceeds the existing Flow schema limits. A bounded
selection from a published catalog can still be authored and published without
that preview; validation compiles only its selected fields.

Configure `apply_issue_classification` with the offered `plugin`, `policy` and
`field_contract.selection` under `fields`, the exact accepted `source_node` and
`source_outcome`, and a successful receipt `outcome`. Bind the accepted record's
payload explicitly; constants cannot replace its values. Use the offered receipt
schema and byte budget. A narrower explicit field selection may reuse the
corresponding subset of the offered schema; it must retain the catalog version
and semantic bindings. Publication rejects incompatible choices, stale catalog
pins and substituted mappings. At execution the Issue owner also checks the
current catalog, Issue baseline, concurrent edits and accepted-result provenance.

If the accepted assessment declares `assessment_inputs`, retain that object in
the action input and project it unchanged from the same accepted record. It
records the exact subject and material dependencies used by the assessment;
publication and execution reject omission or substitution. The Issue owner
validates the references and detects changes to the selected materials.

Omit `size_labels` for typed size without label effects. Legacy numeric-priority
and explicit size-label actions remain supported with their reviewed mappings.
Unavailable legacy labels disable those label effects, while typed classification
without label effects remains available. Existing immutable examples keep their
original contracts. The local action
writes no provider effect, preserves admitted Work scheduling and cannot activate
another pipeline. Both reviewed manual execution and automatic Source intake
must retain an exact Issue requirements baseline. A reproduction choice alone
supplies no independently verified evidence.

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

From **Add pipeline**, start with an empty graph and bind **Incoming pipeline
completion** to the published triage Flow, terminal and accepted outcome.
Connect the triage fields used by the graph, including kind, size, component,
priority and applicable reproduction evidence. Put the expected implementation
behavior in the Issue's acceptance criteria before starting Delivery.

Build the routes in this order:

1. Route unresolved defects through the Tester reproduction step. A conclusive
   negative check ends in `cannot_reproduce`. Missing evidence or an unavailable
   environment needs its own stop or wait route.
2. Route large tickets through OpenSpec authoring. For small and medium tickets,
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
Project policies. Qualify one Issue manually with automatic intake and external
Reporter effects disabled. Use a test publication target, then inspect the
original triage handoff, operator decisions, Builds and publication receipt in
the Run. Check the corresponding cards in the Issue's **Overview**. For a
reproducible test environment, see [ticket delivery qualification](../testing/manual-factory-authoring.md#ticket-delivery-qualification).

The optional `ticket-delivery/v2` example resolves through the preset
API using an exact Project field-definition version and an explicit
`issue_snapshot.admission` policy. It produces an editable Issue-snapshot Flow
and, when requested, a disabled launch Configuration. It needs no triage Flow or
Run selector. The Issue input currently requires API authoring; visual controls
for selecting Project fields are not yet available. The catalog offers both
`v1` and `v2`; Console can open `v1` and explains the unsupported required input
when `v2` is selected. See the
[example's prerequisites and request shape](../../config/factory/presets/ticket-delivery/README.md#current-v2-issue-snapshot-example).

For retained `v1` examples or intentionally authored producer-result consumers,
select the exact published triage completion and compatible executors, command
reports and review target. The existing **Incoming pipeline completion** controls
remain supported. **Earlier Flow version** selects an exact older publication;
it does not reinterpret it as an Issue snapshot. Upgrading uses new reviewed Flow
and Configuration versions. Connect code to the steps that need it and review
each access request before publishing.

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
