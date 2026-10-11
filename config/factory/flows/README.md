# Configurable Factory Flows

`base-intake.json` is a supplied `FlowDefinitionTemplate`, not a runtime-owned
business flow. Edit or clone it to create another flow. Node keys and outcome
keys are operator-defined. Their names never select Rust execution behavior.

The editable document contains local bounded schemas, nodes, explicit input
bindings, finite output contracts, deterministic gates, transitions, bounded
repeats and terminal schemas. `instantiate` resolves schema choices and exact
trusted Build profiles into an immutable `FlowDefinition` and schema catalogue.
Publish them with the complete definition closure, limits and named pool
settings in `FactoryFlowConfiguration`; admission freezes that version.

## Inputs and outputs

A node's `input_binding.sources` selects immutable Work fields, incoming data
under an exact schema, accepted records or a declared predecessor. Choose the
payload view for reported data or the metadata view for trusted identities and
verified Build output facts. The mapping copies only selected values into the
node's closed input schema. Context access is separately explicit. A nested node may select `caller_input` to project the exact frozen payload of its parent call; context is never inherited implicitly. The parent retains the exact child terminal result as provenance.

`node_configuration` explicitly selects a field of this node's published
definition, including its resolved Build binding. It reads the admitted version,
not incoming/provider values or a mutable current configuration. Missing fields
fail closed. This lets task instructions consume the same operator-owned setting
that governs execution without keeping another constant copy.

Build nodes bind one exact command or reasoning profile. Model observations
use the configured result schema; independently verified evidence has separate
logical output names, schemas and tool/plugin identities. Terminal research routes require a separate `terminal_resolution` verifier Report under the configured `terminal_facts` schema, including acceptance, exact outcome, input digest and freshness. Build profiles serving the supplied research gates must select that output as independent evidence. Reproduction facts alone cannot resolve Work.

Gates use bounded
predicates over those facts, finite outcomes and explicit fallback. A shared
`default_output` mapping serves outcomes without an explicit `outputs` override;
the selected outcome still checks its exact result schema. Gates can
compare freshness to `observation_time`; incoming data cannot replace it.

A `trusted_action` node binds a plugin action and bounded parameters. A status
action can set any allowed status string; its connector translates that value
to a label or native status. Notifications and other actions use the same port.
No research outcome automatically invokes an external action.

## Extending the example

1. Add a schema choice or reuse a compatible existing one.
2. Add a node with any new name, input selection, capability/profile, budget,
   narrowed permissions and declared typed outcomes.
3. Connect accepted predecessor outcomes to that node and its outcomes to
   successor nodes or terminals. Declare limits and exhaustion for repeats.
4. For a queued node, add a named pool binding and its immutable policy/settings
   to the journey configuration. Project severity and exact dependencies from
   the frozen node input; selection reserves the full node budget for each node generation. Repeats freeze a new input identity and acquire a new selection even when the payload is unchanged.
5. Validate and publish a new version. Previously admitted runs keep their old
   schemas, profiles, graph, policy parameters and input provenance.

The application contract inserts `accessibility_audit` by these configuration
operations and runs it through the same owner in memory and PostgreSQL.
Permissions in the supplied example default to deny-all: publication must
select qualified profile and authority choices for real execution.

`requirements_needed.accepted_work_outcomes` marks the configured bypass result
as an accepting gate. Accepted Work freezes the original Work and complete
accepted ancestry. The configured bypass still enters every declared downstream
protected-test, implementation, review and verification node. These example
nodes provide control-path bindings; their production capability, protected-test
isolation and independent review work is completed in later plan tasks.

## Supplied specification authoring

The `specification` route now enters `specification_author`, a configured
reasoning node, and `specification_gate`. These names select no execution code.
The author input explicitly retains the accepted predecessor decision, Work
identity and original base. An explicitly selected Context may point at the
original source or an accepted predecessor candidate; trusted capture compares
against that exact subject and retains the original Work subject separately.

Publish the author profile with its actual reasoning harness in `tool`, the
installed trusted capture identity in `result_tool`, `change-set-manifest.json`
as the primary Artifact under `spec_manifest`, and these required outputs:

- `change-set.bundle`: capture Artifact from that same capture tool/plugin.
- `specification_handoff`: bounded Report under `spec_handoff`.
- `specification_facts`: independently verified Report under `spec_facts`, from
  a qualified verifier distinct from the reasoning harness. It binds the exact
  handoff bytes and input digest, checks that the declared contract exists in
  the candidate, and reports its bounded validity period.

The exact output schemas and tool/plugin identities belong to the published
Build binding. They are not inferred from node names or model prose. The
capture executor verifies the canonical manifest and bundle before publishing
any common completion; the gate separately checks the handoff's Work, original
base, predecessor, candidate, input and byte identities against retained facts.
It also checks the trusted publication deadlines of every required output.

The node's `capture_write_paths` declares disjoint portable directory roots,
narrows the selected profile and is frozen in its Build intent. Every
materialized signed Job must carry both that exact capture whitelist and the
same explicit `factory.source_write_paths`; ordinary Build admission rejects
either missing or substituted selection. Agent projects the rest of source
read-only and exposes only the signed directory roots as writable overlays.
Missing directories are prepared
before credential provisioning; symlink/reparse redirects, files used as
writable directory roots, overlapping roots and unqualified projection fail
closed. Scratch and declared outputs retain their separate bounded roots.
The supplied whitelist is `openspec/changes` and `openspec/specs`. Edit only
the author's `capture_write_paths` to select another repository's contract
layout. The author input projects this resolved setting through
`node_configuration`; the gate reads it from the retained Build profile's trusted
metadata and checks every changed path and handoff path against that list.
Provider handoff data cannot replace the scope.

The signed ChangeSet capture filter and signed writable directory roots have
separate meanings. `change_set_capture.policy.allowed_path_prefixes` filters
captured paths and may select individual files or overlapping prefixes. It never
creates mount roots. Optional `factory.source_write_paths` narrows an explicitly
granted source read/write mount to sorted, nonempty, disjoint directory roots;
absence retains the signed mount mode. Thus existing file filters do not create
directories in source or acquire different execution semantics.

For feature Work, the supplied gate requires added specification and change
paths matching the typed handoff, with nonempty acceptance criteria. A feature
may use a new change's delta specification as its contract. For defect Work,
the verifier must identify an existing violated contract and the handoff must
include nonempty regression criteria plus a captured change. Every changed
path must be inside the configured specification scope. These are supplied
policy predicates, not runtime branches on Work kind.

Successful authoring enters the configured independent review with a
`review_pending` outcome. It does not freeze Accepted Work or dispatch the
implementation route. Specification publication remains task 10.4;
production owner/capability composition and real
provider qualification remain 16.1 and 16.4. No fallback selects a broader
write scope or advances Work when required output evidence is missing.

Generic gate programs may declare one shared `precondition`. False or unknown
facts select the explicit fallback before any routing rule. This removes
repeated identity/freshness checks while preserving the same byte, predicate,
depth and evaluation ceilings. `repository_path_within` compares validated
portable paths by components; the predicate grants no filesystem authority.
`repository_path_within_any` applies the same comparison to a selected list of
roots. Invalid roots stay unknown even after an earlier match, and each root
consumes the common evaluation budget.

## Supplied independent specification review

`specification_review` selects its own reasoning Build profile. Publish its
reviewer task/prompt, model/tool configuration, plugin and Build configuration
as exact immutable identities. The result output is the bounded
`specification_review` Report under `spec_review`. Require the independently
qualified `specification_review_facts` Report under `spec_review_facts` as
additional evidence; it verifies the exact review bytes and input digest and
declares its accepted freshness deadline. An author profile or session is not
an implicit input to the reviewer.

The reviewer receives only explicit projections: original Work/base identities,
exact candidate revision, retained manifest and bundle references and digests,
authoring acceptance/regression criteria, and the original Work acceptance
Artifact. The original acceptance reference comes directly from immutable Work;
author output cannot replace it. Source access for qualified review execution
must be read-only, with scratch and declared outputs separately writable and
no forge or publication credentials.

The common `context_binding` declares the selected candidate revision and a
closed map of required logical sources. Each source has an exact category and
an explicit mapping from frozen input to an immutable Artifact, repository
range or receipt-bound fragment reference. `selected_sources` and
`candidate_revision` let a trusted collector resolve these choices without
dispatching by node name. The collector still verifies content bytes, sizes,
subject and retrieval receipts. Input publication, Build intent creation and
restoration reject missing, extra or substituted entries and another selected
revision. The existing candidate materializer additionally binds the complete
ChangeSet subject, manifest and bundle before source execution.

The supplied contract selects the manifest, bundle and original acceptance
Artifact. At publication, add required reviewer-specific skill/configuration
documents as `policy` entries with exact constant references, or explicitly
project their frozen references into the reviewer input schema. Publish that
complete closed contract together with the reviewer profile. Control ordering
does not transfer author context, hidden transcripts, arbitrary sibling outputs
or undeclared repository discovery. Retries reuse the same frozen manifest and
profile rather than recollecting current documents.

`specification_review_gate` checks exact Work, base, candidate, manifest/bundle,
input and report identities and both trusted output deadlines. The supplied
quality criteria are completeness, consistency, feasibility, testability and
scope. Their names and conditions are configuration choices. Typed findings
carry bounded criterion references, explanations and remediation; they are not
hidden reasoning or a provider-selected transition. A violation selects
`changes_required`; uncertainty selects `needs_human_input`; missing or stale
verification selects `invalid`. Only all satisfied criteria with accepted
verification select `publication_pending`.

Operators can insert additional deterministic gates, a separately scoped
reviewer, a nested review flow or a human gate by changing schemas, bindings and
transitions. The optional human-gate contract test inserts
`contract_owner_confirmation` after successful review without engine changes.
None of the supplied review outcomes creates Accepted Work or dispatches
implementation. The successful terminal `specification_reviewed` retains the
exact review pending the later trusted publication/acceptance path.
