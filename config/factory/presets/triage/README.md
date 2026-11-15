# Configured triage example

## Ordinary editable Flow

The advertised `triage · v4` release loads `v4.json` through the common bounded
`FlowDefinitionTemplate::from_manifest` compiler. It contains two ordinary
reasoning steps, `eligibility` and `classification`, and the eligibility gate.
Names identify configuration objects; the runtime assigns no triage semantics
to a pipeline, agent name or classification role.

`assignments-v4.json` supplies the eligibility instructions, including native MCP
observation examples. `inputs-v3.json` supplies the existing task/context input
bindings. Native observations still pass the shared independent observation
validator before the eligibility gate consumes them. Missing checks and
indeterminate coverage remain explicit; a model conclusion does not create proof.

`parameters-v1.json` selects the classification step's Issue fields: built-in
Priority, Component, Type and Size, plus properties `impact`, `risk`, `route` and
`repro`. These are example IDs, not reserved system names. Publish the Project
catalog first. The resolver derives one `IssueFieldForm` from the current catalog;
that same form determines the result schema and permitted write set. No copied
field enums, manual output-to-Issue mapping or separate apply step is needed.
A request may replace a node's selection through `issue_fields`, using the ordinary
Project/form contract. Required and optional fields keep their existing meaning.

The `selected_fields` schema in `selected-fields-v1.json` is a template placeholder,
not an executable field contract. Resolution replaces it with the Project-derived
schema and pins the form's definition version. Publication rechecks the writer,
form, Project authority and root/node permission ceilings. Resolution writes
nothing and creates no Work, Run, Configuration or Source.

## Access and accepted results

Choose exact independent Role/Profile/Build bindings for each reasoning step.
Project Context is independently selected per step; the handoff exposes only its
declared data projections. Accepted Context provenance remains retained through
reasoning steps and nested Flow returns for later applicability checks.

The common installed local writer is selected from ordinary capability metadata.
Its grant belongs to the field-filling node and its enclosing Flow ceiling. It is
removed from the signed Agent execution permissions: saving accepted fields is a
Server operation, not an Agent tool. Agent aliases and display labels never select
credential or tool access. Selected executors, budgets, source attachments and
permission ceilings remain subject to ordinary publication/admission checks.

For an Issue-associated execution, the trusted save owner applies only the
accepted selected fields and retains the ordinary application receipt and source
provenance. Without the reviewed Issue association it cannot guess a target.
Conflicts, changed requirements, deleted selected materials and unavailable
writers keep their existing fail-closed behavior. A successful reasoning result
alone is not a successful field-application receipt.

The example records no implicit label/status changes, Reporter effects or next
pipeline launch. Add explicitly reviewed ordinary actions when needed. Filling
fields grants neither implementation acceptance nor proof of reproduction or
successful delivery. Delivery independently reads the Issue; explicit generic
producer-result handoffs remain available for pipelines that require them.

## Local stand

`deployment/local-stand/factory-triage.json` chooses the example release,
`completion-v3.json`, initial Project property definitions and the common writer.
Only an unpublished catalog is initialized; published operator definitions and
Issue values are preserved on restart. For a published custom catalog, select its
actual fields in the example form or author an ordinary Flow; the installer does
not invent missing properties. Built-in choice defaults come from the Project
owner, not another installer schema.

The launcher calls the same read-only preset resolver, reviews its draft and
publishes through ordinary Flow/Configuration APIs. Source intake and the
Configuration start disabled. `factory-agents.json` supplies reusable executor
seeds and their explicit connection scopes; changing an agent alias does not
change its access. Installation never invokes a model or performs provider writes.

`completion-v3.json` describes current selected-field completion and receipt checks.
Older versioned files are explicit historical/compiler fixtures, not advertised
fallbacks or a compatibility path in the installer. The Console/API selectors are
being updated in change tasks 10.6–10.8; these assets alone do not establish native
UI qualification.
