# Configurable Factory Flows

`base-flow.json` is the supplied base Factory flow manifest. Edit or clone it
to create another flow. Node keys and outcome keys are operator-defined. Their
names never select Rust execution behavior.

The main file declares incoming schema, entry, execution ceilings and an ordered
list of relative configuration parts. `base-flow/schemas/` contains one editable
contract per file; `base-flow/nodes/` contains one node per file. Transitions and
terminal contracts have their own files in `base-flow/`. Parts may group any
number of contracts or nodes; these directories are an editing convention.

Call `FlowDefinitionTemplate::from_manifest` with a resolver rooted in the selected
configuration directory, then publish through the existing `instantiate` boundary.
The resolver supplies document bytes; the domain performs no filesystem/network
access. Paths must be portable and relative. Composition is flat, bounded, rejects
duplicate imports/definitions and never applies silent overrides. All parts are
resolved before publication. Admitted runs retain the resulting immutable
definition and schemas, so later edits to files cannot change an active run.

Local schema choices may use `{"type":"reference","schema":"choice"}` to reuse
a bounded shape. Publication expands these choices into concrete content-addressed
schemas; missing, recursive or oversized expansions fail closed. Published schemas
contain no mutable aliases.

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
node's closed input schema. `array` builds a bounded ordered list from declared
items; `if_present` selects one declared projection by an RFC 6901 pointer.
Both branches count against structural limits. Explicit null counts as present,
and a failed selected branch never falls back. Context access is separately explicit. A nested node may select `caller_input` to project the exact frozen payload of its parent call; context is never inherited implicitly. The parent retains the exact child terminal result as provenance.

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
protected-test, implementation, review and verification node. Installed technical
capabilities bind to exact published tool/profile identities; the composition
root wires them independently of these example node names. Independent review
and delivery contracts have their own plan tasks.

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
implementation route. Successful review enters a separately authorized trusted
publication action and a final readiness gate. Production owner/capability
composition and real provider qualification remain 16.1 and 16.4. No fallback
selects a broader write scope or advances Work when required output evidence is missing.

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

The supplied contract selects the manifest, bundle, original acceptance
Artifact and every declared requirement Artifact. `artifact_collections` expands
an explicitly selected bounded list into closed logical identities, such as
`requirements.0`. Missing, extra or colliding entries fail closed. At publication,
add required reviewer-specific skill/configuration documents as `policy` entries with exact constant references, or explicitly
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
Review outcomes retain the exact review and do not create Accepted Work. A
confirmed human gate routes to the trusted publication node; refusal terminates
without publication or downstream authority.

## Trusted publication and readiness

Install `FactoryCandidatePublicationAction` under the exact immutable plugin
identity selected by `specification_publication`. Its `publish_candidate` action
selects the bundle and manifest logical Context entries, the operator-owned
target policy and a validity mapping from the accepted independent review.
Grant only the selected publisher authority in both the root and the node; the
supplied deny-all settings do not grant forge credentials.

The publisher port receives the frozen operation, Work, cycle, complete candidate
subject, exact artifact references, narrowed permissions, budget and deadline.
A connector must verify the artifact bytes, candidate identity, original base
ancestry and target policy, and deduplicate the complete request by operation
identity. It observes the original operation before attempting a write, including
after an unknown response. Changed ancestry is a conflict; silent rebasing and
candidate substitution are forbidden. This port does not grant merge authority.
The provider implementation and real forge qualification belong to delivery
integration and production qualification tasks.

The action validates its receipt schema before effects, rejects another
operation or candidate and observes the earlier of review validity and attempt
deadline. The configured readiness gate additionally checks exact candidate and
ChangeSet digest against the receipt and checks review freshness using the
owner clock. Only `implementation_ready` freezes Accepted Work. Optional human
approval must precede publication; stale review, uncertain publication, ancestry
conflict or refusal cannot authorize protected tests or implementation.

## Append-only correction cycles

A transition with `starts_cycle: true` declares a new cycle at its target node.
It requires a positive bounded repeat count, an exhaustion terminal and a single
route for its predecessor outcome. It closes the previous cycle before the new
cycle can execute. The engine records the exact triggering attempt and previous
cycle; it derives the new entry from that admitted edge, including after restart
and when the entry uses a pool. No business-stage name selects this behavior.

The new entry may explicitly project `predecessor` from the exact triggering
result. Other accepted-record sources remain scoped to the new cycle. Previous
inputs, captures, tests, reviews, receipts and Accepted Work remain auditable,
but their cycle cannot execute, append fresh authority or resolve/retry nodes.
The same restriction applies to descendants of a superseded parent cycle.

In the supplied journey, independently qualified `execution_facts` bind the
implementation or verification report, input, candidate and freshness. A typed
`requirements_defect` result with matching proof starts a new requirements cycle;
model prose alone cannot select that transition. The configured correction author
uses the previously accepted specification candidate as its exact predecessor.
It repeats capture validation, independent review, publication, readiness and
all downstream gates. Its fresh Accepted Work contract retains the immutable
original Work and complete accepted ancestry without mutating older contracts.

Feature, defect, researched and small specified Work use the same review and
readiness path. Every declared design, performance, security, UI, compatibility
and operational requirement reference comes directly from immutable Work into
both author and reviewer Contexts. A generic `exact_value` comparison rejects
dropped, reordered or substituted references, and independently qualified
specification facts establish preservation in the actual candidate. Neither
these requirement categories nor the correction routes are engine constants.

Result gates select the producer's frozen input through the generic
`input_payload` record view. Metadata retains its existing bounded identity view.
The supplied gates compare the complete returned specification against that
input and bind qualified execution facts to its exact candidate. A bypass with
no specification requires explicit `absent` predicates for both input and result,
and facts bound to the actual execution revision. Missing comparison operands
continue to mean unknown; optional cases use separate ordered rules.

A Requirements Defect carries a nonempty bounded list of exact artifact
references. The independent verifier must resolve and check those references
against the execution subject and report the qualified references in
`requirements_evidence`; copying the model's claims is insufficient. The gate
compares both lists exactly, together with report/input digests, candidate and
freshness, before permitting correction.

The common interpreter exposes every ready fanout sibling to queue publication.
Readiness inspection does not reserve dispatch capacity again for already
started branches. When a completed branch requests a restart, unfinished
siblings drain before the new cycle and unstarted old-cycle nodes are suppressed.
If several branches request a restart, the lowest attempt number determines
the retained cause, independently of restored row order. Historical boundaries
reconstruct that cause from the preceding execution prefix; later cycles cannot
change it.

## Protected acceptance checks

`../profiles/protected-tests.json` is an editable coverage/retention example,
loaded as parameters of a verified immutable tool profile. Class keys are open
strings: add a Project-specific check without a Rust enum or graph change. The
profile declares file/byte ceilings, retention and `reserved_inputs` for the
inventory, candidate and compiler-managed documents. Policy and frozen Plan use
the same validated coverage contract. The compiler passes its actual managed
documents to `wire_inputs` before signing; the complete manifest must fit both
the reserved slots and protocol count/byte/path limits. Ordinary Job output and
permission ceilings continue to narrow execution.

An author capability resolves its explicitly selected accepted gate record through
`FactoryStoredProtectedTestPlanSource`. `AcceptedWorkContract` retains the complete
accepted ancestry, including an accepted specification when present. The plan binds
that contract digest, exact authoring subject, classes and original attempt-based
retention deadline. Restart reconstructs it from the persisted intent/history and
pinned profile, rather than extending retention or selecting a later contract.
The supplied author and implementation Contexts select the accepted specification
candidate and its exact manifest/bundle, together with the Work task. Their closed
`protected_author_input` retains accepted data and the task reference. The generic
`candidate_when_present` and selection `when_present` pointers make specification
context conditional on its presence in that frozen input. Absent optional data
selects the original Work base and task; present but invalid references fail.
These guards apply to any configured node and artifact collection, without stage
names or implicit grants. The same plan/capture contract verifies either subject.

Publish the author profile with a closed `protected_test_handoff` primary Report
from the installed capture tool and exactly one required private inventory Artifact
(`protected_inventory` in this example). The inventory binds plan/input/attempt
identities and per-file Artifact references. Capture checks actual published bytes,
Job/Lease ownership, exact required-class coverage, unique paths/Artifact identities,
aggregate inventory-plus-source bytes and minimum retention before common completion.
The public result contains only accepted-contract identity and coverage classes.
Raw sources are extra private output documents, not Report facts or handoff data.

The supplied implementation entry selects the author's **input payload** plus
only the verified public accepted-contract digest. It normalizes task, acceptance
and the optional exact candidate parent into a closed implementation input. The
control edge sequences authoring without copying its Context, private inventory,
file references or transcript. Build output Artifact metadata contains no Artifact bytes.
Managed Job downloads mint grants only for explicitly signed input references;
Agent staging uses a private Job root, verifies each downloaded size/digest and
seals inputs read-only. Another Job receives only its own inputs and credentials.
Author logs and trace outputs stay with the producing ordinary Build and are not
part of successor input projection.

The validation example is an ordinary `build_command` with explicit selections for
the implementation bundle, capture manifest and private test inventory. The trusted
compiler restores both verified contracts and resolves retained per-file records;
`factory_protected_validation_materialization` checks exact Context selections,
Artifact identities, retention and read-only candidate/test authority. Candidate
instructions and protected files go only into this signed validation Job. The compiler
adds its configured command/managed Octafile; no test framework or command is selected
by a node name. Production composition uses the same verified materialization source
for Job signing and `FactoryProtectedValidationAdapter` result acceptance.

Publish a closed validation Report under `protected_validation_report`, plus the
independently verified `validation_facts` Report under `protected_validation_facts`.
The installed adapter checks the materialized candidate, complete private inventory
and every declared class before accepting common completion; facts bind the ordinary
input and actual report bytes. The supplied deterministic gate then chooses only its
declared transitions. The report's materialization digest binds the candidate, frozen
test plan and all downloaded inputs; it is distinct from the ordinary node-input digest.
`construct_factory_protected_validation_evidence_manifest` requires the exact
configured facts output, rechecks its retained proof and freshness, retains all
exact Artifact input identities and caps freshness by both qualification and source
deadlines. Evidence cannot extend the independent verifier's validity period.
Repeated materialization reconstructs identical input bytes/digests; changed candidate,
file, inventory, coverage or report identities cannot reuse the previous result.

These are technical capabilities selected through immutable bindings. Nodes can be
renamed, moved or reused in other graphs. Live provider qualification and full
production composition wiring remain separate tasks.


## Validation classification and independent code review

The example's failed validation output projects only the accepted summary, exact
candidate identities, validation-report digest and bounded class outcomes. The
classifier gets that request and the Work task; its Context has no inventory,
source assertions, expected values, implementation transcript or private Artifact
grants. Its closed result offers five configured dispositions. The consuming gate
requires independent facts bound to actual input/report bytes, candidate, original
validation and freshness before following an edge. The classifier cannot supply
a target node. Rework/test-correction edges have explicit repeat ceilings and an
exhausted terminal; requirements correction starts a fresh append-only cycle.
Rework keeps the accepted contract and selects the immediately preceding captured
candidate. Correcting tests reruns authoring and all downstream checks. Correcting
requirements reruns specification review/publication/readiness and downstream work.
The test-correction entry preserves the original accepted summary and projects a
bounded reason, exact failed candidate revision and validation-report digest into
the author's optional `correction` input. It does not grant private test files.
The implementation entry selects only acceptance, task and the public handoff;
the author-only correction is excluded from implementation and review inputs.

Code review selects only the exact current candidate bundle/manifest and Work task.
Install its separately pinned task/skills/configuration profile through the existing
`Evaluate` Task Envelope compiler with read-only source and the scoped evaluation
credential profile. The compiler rejects writable evaluation source or coding/delivery
credentials. No node name selects these capabilities. Independent review facts and
the configured gate bind input, report, candidate and original deadlines. Violated
quality criteria, indeterminate criteria and invalid proof have distinct declared
outcomes; transport failure remains an ordinary execution stop.

## Public candidate deliverables

`capture_deliverables` is an optional immutable profile parameter. Its acceptance
mapping explicitly selects the verified public accepted-contract digest. Its policy
lists permitted/forbidden repository paths and applicable required categories,
each with path alternatives and a minimum added/updated file count. Category names
have no runtime semantics. Choose only applicable documentation or build/deployment
script requirements for the Project; the supplied example deliberately requires all
three developer-visible categories. The capture and validation nodes must use the
same exact policy. The example defines that policy once in
`base-flow/policies/public-deliverables.json` under `deliverable_policies`; both
nodes select `{"reference":"public_changes"}`. Local references resolve at
publication into concrete profile parameters; there is no runtime file lookup.
Missing choices, duplicate definitions, unknown fields and policies broader than
an installed ceiling fail closed. Inline policies remain available for independent
nodes. Neither form grants access to hidden test sources.

Before Build dispatch, the common intent verifies that acceptance can be frozen.
The ordinary Build causality carries the complete frozen public deliverable contract
for trusted task compilation, including the exact acceptance, parent, input and
applicable policy. Templates can narrow an installed capture policy; they cannot
broaden paths, remove exclusions or weaken required categories. Trusted Git capture hashes each changed non-deleted file from the candidate Git tree.
Hashes of current changes reuse the bytes already inspected for capture policy.
The manifest also includes bounded `cumulative_changes` from the original Work
revision (the bundle prerequisite), separately from the current parent's delta.
Materialization verifies both indices against Git and recomputes each unique
candidate blob hash once. `verify_factory_node_candidate_deliverables`
qualifies actual capture bytes and provenance, then checks required files and allowed
paths: the current delta must stay within this node's scope, while required public
updates are counted over the complete Work result. Previously accepted changes may
lie outside this node's write scope. Rework need not touch earlier required files
again; deleted or reverted additions no longer qualify. Profiles declaring
deliverables reject an absent cumulative index or a different original base.
A model's success report cannot substitute for these observations. Validation
requires that exact proof whenever its pinned profile declares deliverables. It
checks candidate/manifest selection, matching acceptance/policy and hashes of the
selected private sources; copying a complete protected file into an allowed public
path fails. The validation materialization digest also binds the frozen deliverable
contract. This digest check addresses exact file copies; broader disclosure prevention
comes from the existing absence of protected inputs/grants in implementation.

Production composition and live external-provider qualification remain tasks 16.1
and 16.4. Owner/store qualification uses deterministic external ports; real Git
capture/materialization and actual-byte Artifact validation have separate contracts.
