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
   Narrow requests within the Profile and enclosing policy; editing task text
   cannot add a tool, credential or network destination.
5. Review **Step code**, its budget and retry settings. Reconcile any missing or
   unsupported resource rather than relying on a sibling's selection.

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

Connect each declared outcome to its next step or typed terminal. Gates require
an explicit predicate, result-data mapping and routes. To pass an unchanged
bounded result, enable **Choose result data** and select the root path; for a
summary, select only that field. Provide declared rejection, review or no-effect
handling for uncertainty and unmatched values.

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
- **Pipeline access**: requested capability ceiling, with independently narrowed
  step access. It attaches no documents, code or credentials by itself.

Validate and review **Effective constraints**, their scopes and limiting sources.
Each candidate Profile has a separate access/resource ceiling, including its
Profile budget. Review these alongside the Flow/step ceiling; no candidate is
shown as selected by this preview. Exact Profile references are under **Details**.
Correct over-ceiling requests explicitly; unresolved support is not confirmed
enforcement. Validation creates no Flow, Build, Work or external effect. Confirm
the Flow publication, then create/edit a Factory Configuration selecting that
exact published Flow. Review its admission policy, pool/capability prerequisites,
limits and access ceiling. Keep it disabled until ready, then explicitly publish
an enabled version. Keep automated inputs disabled for a manual trial: enabling
the Configuration permits admission by an already activated Source as well.

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
