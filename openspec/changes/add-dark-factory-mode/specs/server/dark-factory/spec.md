## Purpose

Defines the opt-in Dark Factory lifecycle that turns provider-neutral coding work into an exact, independently evaluated and safely delivered candidate while preserving ordinary CI/CD as a complete standalone product mode.

## ADDED Requirements

### Requirement: Independent CI/CD and Dark Factory modes
OctaCity SHALL continue to accept and execute ordinary CI/CD Triggers and Builds without a Factory Configuration, model provider, coding harness, Factory Run, or delivery adapter. Dark Factory SHALL be an opt-in application lifecycle above ordinary immutable Builds and SHALL NOT introduce a second Job execution path, a special Agent kind, or a different lease protocol.

#### Scenario: Deployment has no factory configuration
- **WHEN** an operator runs an ordinary Build in a deployment with no configured model or factory adapter
- **THEN** the Build uses the existing Build, Attempt, Job, Agent, Octa, Result, and audit contracts without creating a Factory Run

#### Scenario: Factory mode is enabled for one project
- **WHEN** a Project enables a Factory Configuration
- **THEN** other Projects and ordinary Build commands remain independent of that configuration and its provider availability

### Requirement: Versioned Factory Configuration
The server SHALL store Factory Configurations as immutable versions owned by one Project. A version SHALL bind admission rules, the exact root Flow Definition and its reachable version closure, Build Configuration selections, budgets, WIP limits, permission policy, optional Decision Signal profiles and rollout policies, criterion packs, evaluator policy, rework limits, delivery policy, and enabled state. A Factory Run SHALL retain the exact version accepted at admission and SHALL NOT observe later replacements.

#### Scenario: Factory Configuration is replaced
- **WHEN** an operator publishes a valid replacement using the current-version precondition
- **THEN** later Work may use the replacement while admitted Factory Runs retain their original immutable configuration

#### Scenario: Configuration references an unavailable capability
- **WHEN** a proposed configuration requires a plugin, execution guarantee, permission control, evaluator, or delivery capability that cannot be selected under Project policy
- **THEN** the server rejects the version before it can admit Work

### Requirement: Immutable nested Flow Definitions use bounded typed primitives
The server SHALL represent factory behavior as immutable versioned Flow Definitions composed from a closed set of node kinds. Executable nodes SHALL be limited to ordinary Build or command execution, a schema-bounded reasoning call, and a provider-neutral Decision Signal call. Orchestration nodes SHALL be limited to deterministic gates, bounded fan-out and join, human gates, trusted actions, and calls to exact nested Flow Definition versions. A definition SHALL declare its entry node, typed inputs and outcomes, transitions, failure routes, budgets, permission profiles, Context projections, terminal outcomes, and exact subflow references.

Definition validation SHALL reject unknown or provider-created node kinds or edges, incompatible schemas, unreachable required nodes, missing terminal outcomes, cycles without explicit bounded repeat semantics, recursive subflow closures, and any graph whose maximum depth, node count, fan-out, repeat count, budget, or WIP cannot be proven within configured bounds. Admission SHALL persist those exact bounds with the immutable reachable closure. A running Flow Run and every restart SHALL revalidate against the persisted bounds and SHALL NOT substitute defaults or discover newer subflow versions.

Provider-neutral Flow graph structure and algorithms SHALL initially be implemented as a deterministic pure module private to the OctaCity Factory core. The module SHALL be limited to canonical node and edge structure, structural validation, traversal, reachability, component and cycle facts, topological ordering of acyclic projections, and bounded structural measurements. It SHALL NOT own plugins, commands, Builds, Flow node kinds, schemas, context or data authority, permissions, budgets, retries, persistence, fencing, side effects, or transition decisions. Octa SHALL remain unchanged and continue to reject cyclic task DAGs, while the Factory adapter MAY accept only cycles whose explicit bounded-repeat semantics satisfy Factory policy and SHALL record their realized execution as append-only attempts and cycles. Any later shared-kernel extraction SHALL require a separately reviewed change based on at least two working consumers and SHALL NOT be a dependency or release commitment of this change.

#### Scenario: Nested Flow Definition is replaced
- **WHEN** an operator publishes a replacement for a subflow used by an admitted Factory Run
- **THEN** the admitted run continues with its pinned subflow version while later runs may use the replacement

#### Scenario: Provider proposes an undeclared transition
- **WHEN** a reasoning or Decision Signal result names a node or edge absent from the current Flow Definition
- **THEN** the result cannot change control state and deterministic policy rejects or escalates the node attempt

#### Scenario: Flow graph cannot prove bounded execution
- **WHEN** a proposed definition contains recursive nesting, unbounded repetition, or fan-out without a hard maximum
- **THEN** the server rejects the definition before it can become selectable by a Factory Configuration

#### Scenario: Factory structural validation remains locally owned
- **WHEN** a Factory definition is checked against a provider-neutral conformance fixture that also documents comparable Octa and Job graph semantics
- **THEN** the Factory adapter obtains deterministic structural facts from its private pure module, without importing Octa graph code, invoking another runtime, or importing Job execution state

#### Scenario: Server resumes a Flow after process loss
- **WHEN** an active Flow Run is reclaimed after the server process loses all memory
- **THEN** the server resumes only from its authoritative persisted Flow, attempt, fence, and outbox state and does not depend on an Octa executor instance or graph scheduler state

### Requirement: Provider-neutral Work admission
The server SHALL accept bounded Work Submissions through versioned Work Source adapters and normalize accepted input into an immutable Work Envelope containing a scoped source identity, repository subject, base reference, task artifact, acceptance criteria, specification references, priority, risk, and bounded provider metadata. Admission SHALL authenticate the source where required, resolve an allowed repository reference to an exact immutable base revision, apply Project visibility and Factory policy, and durably deduplicate one Factory Run per scoped Work identity.

#### Scenario: Source retries the same Work Submission
- **WHEN** an authenticated source repeats a submission with the same scoped identity after losing its response
- **THEN** admission returns the existing Factory Run without resolving mutable source state or creating another run

#### Scenario: Repository reference cannot be resolved safely
- **WHEN** an accepted submission names a mutable reference that cannot be resolved under the selected Repository policy
- **THEN** no Factory Run is admitted for ambiguous source state and the failure is reported without executing repository code

### Requirement: Typed triage and deterministic phase-ready pools
A Factory Configuration SHALL be able to select a two-phase nested triage flow. The eligibility phase SHALL produce an immutable Eligibility Result from typed duplicate candidates and Project-fit observations with exact provenance. Deterministic policy SHALL map that result only to declared rejection, escalation, or classification outcomes. The classification phase SHALL produce one immutable Triage Result containing `defect` or `feature_request` Work kind, component, severity, size, risk, dependencies, preliminary defect reproducibility, and a recommended route from the finite outcomes declared by the Flow Definition.

Declared route outcomes SHALL be able to include research, requirements, protected test authoring, development, verification-only, already-fixed resolution, escalation, and rejection. Deterministic policy SHALL validate the recommendation against Work kind, risk, accepted evidence, budgets, and the pinned configuration before committing a route. A reasoning or Decision Signal result SHALL NOT commit eligibility, resolution, pool membership, or routing itself.

The server SHALL expose durable phase-ready pools derived from authoritative Flow Run state. Selection from a pool SHALL be deterministic under configured severity, Project priority, age, dependency readiness, required capability, WIP, and budget rules and SHALL preserve the selected policy and input digests. The first root successor SHALL use its accepted triage route's pool; subsequent queued nodes SHALL declare an immutable pool key resolving to the admitted configuration's exact policy and capabilities. Unknown or conflicting bindings SHALL fail admission, and missing or substituted bindings SHALL fail publication without inferring a pool from a node name or kind.

#### Scenario: Duplicate is rejected before classification
- **WHEN** the eligibility phase identifies an accepted duplicate candidate under the configured duplicate policy
- **THEN** deterministic policy records the duplicate resolution and does not dispatch classification or later development work

#### Scenario: Eligible defect needs deeper reproduction
- **WHEN** classification accepts a defect whose preliminary reproducibility is inconclusive and whose declared route is research
- **THEN** the server places the Work in the research-ready pool with the exact Eligibility Result and Triage Result provenance

#### Scenario: Existing change already satisfies the ticket
- **WHEN** classification evidence satisfies the configured `already_fixed` outcome
- **THEN** deterministic policy records that typed resolution without dispatching requirements or implementation

#### Scenario: Small Work bypasses specification authoring
- **WHEN** the triage result classifies admitted Work within the configured specification-bypass bounds
- **THEN** deterministic routing proceeds to Accepted Work Contract creation without creating a requirements-authoring node attempt or skipping later declared gates

#### Scenario: Several items are ready for the same bounded capacity
- **WHEN** multiple Flow Runs satisfy a phase-ready pool and capacity admits fewer than all of them
- **THEN** the server selects them in the configured deterministic order and records the selection evidence

### Requirement: Defect and feature research are isolated typed subflows
A Factory Configuration SHALL be able to route an accepted Triage Result into an immutable Research Flow with its own Context projections, model or command profile, budgets, permissions, attempts, and typed outcomes. A defect research branch SHALL bind the exact subject, symptoms, environment contract, and prior observations and SHALL produce deterministic reproduction evidence plus one declared outcome such as `reproduced`, `intermittent`, `environment_specific`, `cannot_reproduce`, or `needs_human_input`. A feature research branch SHALL produce a bounded proposal Artifact containing source references, assumptions, alternatives, and unresolved questions.

Research output SHALL remain a non-authoritative Stage Handoff. Deterministic policy SHALL map it only to declared requirements, protected test-authoring, verification, escalation, rejection, or terminal-resolution routes. Research prose SHALL NOT make Work implementation-ready, replace required reproduction evidence, widen authority, or create an undeclared transition. Mutable external and repository discovery included in research SHALL be frozen into the Context Manifest and applicable Retrieval Receipts.

Research SHALL use the same ordinary Build creation boundary and generic Flow/Node Attempt identities as other Flows. Per-node profiles SHALL pin the model or command, task, plugin, Build Configuration and result output. Build priority SHALL be frozen in persisted intent and checked against the observed immutable Build. External research execution ordinals and budget usage SHALL remain distinct from generic Flow node numbers, including intervening deterministic gates. Trusted output verification SHALL attest the full schema version and digest of exact bytes; neither a report format name nor a requested schema SHALL constitute evidence of schema validation. Restart SHALL replay the same operation; ordinary Build retries SHALL preserve the owning Node Attempt's aggregate resource ceiling and deadline. Execution failure or cancellation SHALL NOT be interpreted as a negative reproduction observation.

Research wire contracts SHALL use version 2 for the separate external execution ordinal, SHALL bind that version in immutable schema references, and SHALL reject older bytes without interpreting them under the changed provenance shape.

#### Scenario: Stronger defect research reproduces the failure
- **WHEN** a defect research node using its pinned model and budget publishes valid reproduction evidence for the exact subject
- **THEN** deterministic policy records the typed `reproduced` outcome and follows only the declared successor route

#### Scenario: Defect cannot be reproduced within bounds
- **WHEN** completed deterministic checks retain a negative reproduction observation and the finite research budget is exhausted
- **THEN** the observation remains distinct from execution failure, missing inputs and human escalation, while pinned policy selects the declared exhaustion route without silently continuing to implementation or assigning an external status

#### Scenario: Feature research produces a proposal
- **WHEN** a feature research node completes with a schema-valid proposal, sources, assumptions, and unresolved questions
- **THEN** the proposal is retained as an exact bounded Artifact and cannot authorize implementation until the declared requirements or acceptance gates complete

#### Scenario: Research returns Work to a configured phase-ready pool
- **WHEN** manually admitted Work completes eligibility, classification and an optional pinned defect or feature Research Flow, and deterministic gates project its accepted handoff to a declared root successor
- **THEN** the server publishes and selects that exact node under its immutable declared pool binding and frozen configured policy with the predecessor output digest, schema, accepted severity, capabilities and dependencies, while rejecting substituted pools, inputs and terminal targets

#### Scenario: Research resolution survives execution recovery
- **WHEN** a research Build response is lost or an ordinary Build retries, and a replacement fenced owner restores the frozen intent against durable Flow history
- **THEN** the same bounded Node Attempt and Build operation identity are preserved, accepted verification or terminal evidence remains separately required, and a declared root terminal resolves dependency readiness without an implementation dispatch or a research-specific lifecycle state

### Requirement: Flow actions are configurable plugin extensions
Any Flow SHALL be able to compose ordinary `trusted_action` nodes with exact immutable plugin identities, plugin-defined action keys, bounded structured parameters and finite declared results. Adding a plugin action SHALL NOT require a new primitive kind, a Research-specific executor or connector-specific Flow logic. Plugin validation SHALL precede effects; plugins SHALL enforce the scoped permissions, budgets and deadline and deduplicate repeated execution by stable operation identity. Only the owning Flow SHALL authorize readiness, persist attempts and commit completion/routing.

Setting a Work source status SHALL be one reusable action whose status value is operator-configurable, independent of the research observation vocabulary. The selected connector SHALL translate that arbitrary value into its source representation, such as a GitHub label or a Jira native status. External source state SHALL remain non-authoritative, and projection failure SHALL NOT roll back accepted research or authorize an internal transition.

The action adapter SHALL reject an absent exact plugin permission before dispatch. Status requests SHALL preserve the frozen permission set through the connector boundary; the connector SHALL enforce applicable hosts, credential profiles and other grants before external effects and SHALL fail closed on unenforceable authority.

#### Scenario: Research completes with a configured source status
- **WHEN** declared Flow transitions select a status action with an operator-chosen value after accepted Build observations
- **THEN** the exact selected connector receives that value with stable idempotency and source identity, without a hardcoded mapping from `cannot_reproduce` or another research observation

#### Scenario: Another plugin adds a completion action
- **WHEN** an operator publishes a trusted-action binding for another installed plugin and its declared result contract
- **THEN** the same Flow action boundary executes it without changing Research or adding a primitive kind, and rejects substituted plugin versions or undeclared results

### Requirement: Durable code-owned Factory lifecycle
The server SHALL drive Factory Runs by interpreting their pinned Flow Definition closure into explicit persisted Flow Runs and Node Attempts. Program code SHALL own every node creation, nested-flow call and return, loop, branch, join, retry, timeout, budget check, and stop condition. A model result SHALL be treated only as a schema-validated typed value and SHALL NOT create a node, follow an undeclared edge, skip a required gate, or directly complete a Factory transition.

#### Scenario: Harness claims success before validation
- **WHEN** an implementation harness reports a completed result but required validation has not run
- **THEN** the Factory Run advances only to the program-selected validation stage and cannot become accepted or delivered

#### Scenario: Required branch does not complete
- **WHEN** one required parallel evaluator times out or returns an invalid result
- **THEN** the deterministic join policy records the failed branch and applies its configured retry or escalation rule

### Requirement: Provider-neutral Decision Signals are bounded non-authoritative inputs
The server MAY request probability-backed Decision Signals through a provider-neutral versioned port for only `routing` and `tool_risk` purposes. A request SHALL contain bounded redacted canonical state, versioned typed questions with finite answer or score domains, subject and policy digests, deadline, budget, and a stable logical identity. Provider capability discovery SHALL identify supported question kinds, input media, limits, data-handling class, exact adapter and model version, and probability or confidence semantics without exposing provider wire types to the factory core. JEV SHALL be the initial adapter, and another decision-model or Decisions API adapter SHALL be replaceable through immutable Factory Configuration rather than domain changes.

Every completed or terminal request SHALL produce an immutable Decision Signal Receipt that binds the canonical input, question-set and policy digests, exact provider, adapter and model identities, typed answers, probability or confidence semantics, usage, terminal classification, and consuming deterministic disposition. An admitted Factory Run SHALL retain its selected exact identities even after configuration replacement. Unknown-response recovery and replay SHALL reuse or observe the recorded receipt and SHALL NOT silently query a different model for the same logical decision.

#### Scenario: Provider is replaced for later runs
- **WHEN** an operator publishes a new Factory Configuration selecting another compatible decision-model adapter or exact model version
- **THEN** later Factory Runs use the replacement while admitted runs retain their original provider, model, questions, policy, and receipts

#### Scenario: Decision Signal call cannot be reproduced safely
- **WHEN** the configured provider times out, returns an invalid result, lacks a required capability, falls below its calibrated confidence floor, or has an unknown outcome with no observable receipt
- **THEN** deterministic policy applies the configured fail-closed denial or escalation and does not substitute another model implicitly

### Requirement: Decision Signals can select only declared Factory routes
A routing Decision Signal SHALL receive only the finite outgoing route choices declared for the current node by the immutable Flow Definition. Deterministic routing policy SHALL combine the recorded signal with authoritative lifecycle state, budgets, required gates, and configured thresholds to follow one declared edge, use a declared fallback, or escalate. A provider result SHALL NOT name or create an undeclared node, skip a required validation, evaluation, delivery, promotion, or human-review gate, change a hard budget, or directly commit a Factory transition.

#### Scenario: Routing provider returns an undeclared answer
- **WHEN** an adapter returns a route absent from the request's finite choice set
- **THEN** the result is invalid, no transition is committed, and fail-closed policy denies or escalates the routing request

#### Scenario: Routing confidence is sufficient in bounded control
- **WHEN** an exact provider/model policy in `bounded_control` mode returns a declared route that satisfies its calibrated threshold and margin rules
- **THEN** deterministic policy may commit that declared edge only if every mandatory lifecycle invariant and budget still permits it

#### Scenario: Routing policy is in shadow mode
- **WHEN** a Decision Signal profile is configured as `shadow`
- **THEN** the server records comparison evidence while the deterministic baseline selects the route and the signal cannot alter execution

### Requirement: Append-only Flow and Node Attempts with fenced ownership
Every Flow Run node SHALL have monotonically numbered append-only Node Attempts with immutable input digests, selected Build or provider identities, owner, deadline, fence, terminal observation, and consumed budget. A retry, bounded repeat, requirements correction, or implementation rework SHALL create a new attempt or append-only Workflow Cycle and SHALL NOT rewrite prior history. Only the current fenced owner SHALL publish a transition or consume the next node budget.

#### Scenario: Stale reconciler completes after takeover
- **WHEN** an old owner submits a Node Attempt completion after another server replica has acquired a newer fence
- **THEN** the store rejects the stale completion and preserves the current Factory Run state

#### Scenario: Node retry is permitted
- **WHEN** a retryable infrastructure failure occurs within the recorded attempt and budget limits
- **THEN** the reconciler creates a new Node Attempt with the same immutable subject and preserves the failed attempt

### Requirement: Build-backed flow nodes execute as ordinary Builds
A command, implementation, validation, evaluation, or rework Node Attempt SHALL create or select ordinary immutable Builds through the existing Build application. The Factory Run SHALL record exact Build, Attempt, Job, source revision, configuration version, effective policy, output, and terminal-observation references. Factory coordination SHALL observe authoritative Build state and SHALL NOT duplicate Job DAG orchestration, placement, lease, cancellation, retry, event, or output truth.

#### Scenario: Implementation node starts
- **WHEN** the reconciler admits an implementation Node Attempt
- **THEN** it creates one causally linked ordinary Build whose immutable input contains the exact base revision and narrowed Factory execution intent

#### Scenario: Build is cancelled outside the factory view
- **WHEN** an operator cancels a Factory-owned Build through the existing Build API
- **THEN** the Factory reconciler observes the authoritative terminal state and applies the configured stage failure policy without inventing a second Build state

### Requirement: Immutable Factory Task Envelope and Codex execution
Each reasoning or Build-backed node SHALL receive an immutable versioned Factory Task Envelope containing Flow Definition, Flow Run, node, attempt, and subject identities, exact revision, mode, task and specification artifact references, Context Manifest identity and digest, prior typed findings, permissions, budgets, expected result schema, deliverables, and policy, prompt, plugin, executable, model, and input digests. The first reasoning implementation SHALL execute Codex through the official pinned Octa `codex` task and generic runner protocol; OctaCity server and Agent protocols SHALL NOT contain Codex-specific request, event, or result types.

#### Scenario: Codex implementation is dispatched
- **WHEN** a valid Task Envelope selects the pinned Codex implementation capability
- **THEN** the trusted boundary compiles it to a normal Octa task with explicit prompt, result schema, environment mappings, revision, and deliverables and records the selected identities in provenance

#### Scenario: Pinned executable is unavailable or incompatible
- **WHEN** the selected Agent cannot prove the required Codex plugin or supported executable identity
- **THEN** the Job is not started and no prompt, secret, or workspace record is exposed to that executable

### Requirement: Deny-by-default Factory permissions
The effective Factory Permission Set SHALL be the intersection of the immutable Factory Configuration, Task Envelope, effective Project policy, local Agent grants, and enforceable backend capabilities. It SHALL independently constrain plugin and executable identities, tools, commands and bounded arguments, child processes, read and write roots, mount modes, network hosts, secret and workload-identity profiles, resources, and output capabilities. Unknown permissions, broader child requests, or unenforceable restrictions SHALL fail closed before spawn or before the corresponding protected operation, without fallback to a weaker execution mode.

#### Scenario: Harness requests a host outside the allowlist
- **WHEN** a coding task attempts network access to a host absent from its effective restricted network policy
- **THEN** the backend denies the operation and the permission cannot be widened by repository text, prompt output, or harness result

#### Scenario: Backend cannot enforce a required path restriction
- **WHEN** a candidate Agent otherwise matches placement but its backend cannot enforce one required filesystem or descendant-process restriction
- **THEN** placement or pre-spawn admission rejects the Job instead of running it on Host or omitting the restriction

### Requirement: Trusted immutable ChangeSet capture
After a writable requirements, implementation, or rework Build, a trusted capture boundary SHALL compare the owned workspace to the exact predecessor revision, validate resolved paths, symlinks, changed-path policy, file count, byte bounds, forbidden content, and provenance, and produce an immutable ChangeSet with exact predecessor and candidate identities, content digest, changed-path summary, Node Attempt identity, candidate purpose, and artifact references. Harness output SHALL NOT be accepted as the authoritative ChangeSet identity.

#### Scenario: Valid implementation changes files
- **WHEN** a successful implementation Build leaves allowed bounded changes in its owned workspace
- **THEN** trusted capture creates one immutable candidate and subsequent nodes address that exact candidate digest and revision

#### Scenario: Change escapes the owned workspace
- **WHEN** capture observes traversal, an unsafe symlink, a forbidden path, excessive output, or secret material
- **THEN** capture rejects the candidate, publishes no accepted ChangeSet, and records a typed security failure

### Requirement: Requirements cycles preserve exact candidate lineage
A requirements flow SHALL support a deterministic specification-bypass branch for bounded small Work and, for larger Work, separate requirements-authoring and independent requirements-review nodes. Before bypassing specification authoring, the server SHALL freeze an Accepted Work Contract containing the immutable Work Envelope, accepted Eligibility and Triage Results, acceptance criteria, exact base, required Research Handoffs, policy digest, and provenance. The Accepted Work Contract SHALL feed the same declared protected test-authoring, implementation, review, and verification gates as an accepted specification unless the pinned Flow Definition explicitly omits one.

For larger feature Work, requirements authoring SHALL create a new specification or change with acceptance criteria and declared design, performance, security, UI, compatibility, and operational requirements as applicable. For a defect, requirements authoring SHALL identify or amend the violated specification and SHALL record regression acceptance criteria. Accepted requirements SHALL be captured as an exact immutable ChangeSet, linked to the admitted Work and original base, and SHALL be the only specification candidate that can make the applicable Flow Run ready for implementation.

An implementation, review, or verification node MAY return a typed Requirements Defect referencing exact contradictory, incomplete, or infeasible requirements evidence. Deterministic policy SHALL start a new append-only Workflow Cycle at the declared requirements node, retain all prior attempts and candidates, and mark descendants of the superseded requirements candidate ineligible to authorize later delivery or deployment. A new cycle SHALL traverse its declared review and acceptance gates again.

#### Scenario: Requirements author and reviewer are isolated
- **WHEN** the independent requirements-review node is dispatched
- **THEN** its Context Manifest contains the candidate specification and declared acceptance evidence but excludes the author's hidden transcript and undeclared context

#### Scenario: Small Work bypasses specification authoring but not test-first gates
- **WHEN** deterministic policy accepts Work within the configured specification-bypass bounds
- **THEN** the server freezes one Accepted Work Contract and routes it to the declared protected test-authoring or later gates without treating model prose as requirements

#### Scenario: Defect requirements preserve regression intent
- **WHEN** a defect requires a specification correction
- **THEN** the accepted requirements ChangeSet identifies the violated contract and regression acceptance criteria before implementation becomes ready

#### Scenario: Implementation finds a specification defect
- **WHEN** implementation returns a valid Requirements Defect for the exact active specification candidate
- **THEN** the server appends a new requirements cycle and no downstream result from the superseded candidate may satisfy a later gate

### Requirement: Protected test-first development
A test-authoring node SHALL be able to consume an accepted specification candidate or Accepted Work Contract and produce an immutable protected test bundle before implementation. Its immutable plan SHALL declare which integration, end-to-end, performance, UI or visual, security or negative, migration, compatibility, and Project-specific acceptance checks are required. The implementation node MAY have a control dependency on successful test authoring, but its Context Manifest, mounts, Artifact grants, prompts, and Stage Handoffs SHALL exclude protected test source, assertions, expected outputs, and author transcript. A validation node SHALL materialize the exact implementation candidate and exact protected test bundle in a trusted environment and publish reproducible typed evidence.

Deterministic policy or a separately scoped reasoning node MAY classify failed validation as an implementation defect, requirements defect, test defect, or indeterminate only from declared bounded evidence. That classification SHALL route through declared edges and SHALL NOT reveal protected test content to an implementation retry unless policy explicitly publishes a bounded remediation summary.

#### Scenario: Implementation attempts to obtain hidden tests
- **WHEN** the implementation node requests or scans for an Artifact or path outside its declared Context and mount grants
- **THEN** access is denied and the attempt cannot widen its own authority despite being sequenced after test authoring

#### Scenario: Candidate is checked against the protected bundle
- **WHEN** implementation produces a candidate eligible for validation
- **THEN** validation combines that exact candidate with the exact protected test bundle and records both identities in the Evidence Manifest

#### Scenario: Test plan requires several verification classes
- **WHEN** the pinned test-authoring plan requires integration, performance, and UI acceptance coverage
- **THEN** the protected bundle records each required class and provenance and validation cannot report completeness while any required class is absent

### Requirement: Implementation deliverables are explicit and policy-bound
An implementation node SHALL receive an immutable deliverable contract derived from the accepted specification or Accepted Work Contract. The contract SHALL declare permitted production-code changes and required developer-visible unit tests, documentation, and build or deployment script updates when applicable. It SHALL NOT disclose protected test source or assertions. Trusted ChangeSet capture and deterministic validation SHALL verify required deliverable presence and allowed paths before the candidate can enter independent review.

#### Scenario: Implementation omits a required deliverable
- **WHEN** the accepted contract requires a documentation or build-script update and the captured candidate omits it
- **THEN** deterministic validation records the missing deliverable and the candidate cannot become accepted despite a successful model result

#### Scenario: Unit tests remain visible to ordinary developers
- **WHEN** an implementation contract requires developer-visible unit tests
- **THEN** those tests are captured with the implementation candidate while the separate protected acceptance bundle remains unavailable to the implementation call

### Requirement: Exact deterministic evidence
Validation Builds SHALL run against a freshly materialized exact candidate and publish an immutable Evidence Manifest containing typed report and artifact references, digests, producer identities, candidate identity, freshness, and completeness. Compiler, lint, test, coverage, security, dependency, specification, and other deterministic outcomes SHALL reach the Decision Engine as authoritative evidence rather than through an LLM summary.

#### Scenario: Candidate changes after evidence is produced
- **WHEN** rework creates a new candidate revision or ChangeSet digest
- **THEN** the prior Evidence Manifest cannot satisfy a gate for the new candidate

#### Scenario: Required test fails
- **WHEN** a required deterministic test report records failure
- **THEN** the Decision Engine observes that failure even if every evaluator assessment claims satisfaction

### Requirement: Versioned read-only evaluation
The server SHALL create an immutable Evaluation Plan that binds the exact candidate and Evidence Manifest, versioned Criterion Packs, selected evaluator connector identities and digests, budgets, deadlines, data-handling policy, and required quorum. Evaluators SHALL receive read-only candidate access and bounded evidence and SHALL return schema-validated immutable Assessments with `satisfied`, `violated`, or `indeterminate` outcomes, typed findings, evidence references, remediation, fingerprints, and provenance. Transport failure, timeout, and cancellation SHALL remain distinct from an Assessment outcome.

#### Scenario: Evaluators run in parallel
- **WHEN** independent criterion branches are ready
- **THEN** they execute as sibling Node Attempts with scoped inputs and the program joins only their typed terminal results

#### Scenario: Evaluator asks for missing evidence
- **WHEN** an evaluator cannot judge a required criterion from the supplied manifest
- **THEN** it returns `indeterminate` with bounded typed evidence requirements and cannot invoke arbitrary tools or mutate the candidate

### Requirement: Independent verification is a scoped nested flow
A Factory Configuration MAY select an immutable Verification Plan implemented as a nested Flow Definition containing bounded deterministic or reasoning nodes for acceptance, end-to-end, performance, UI, security, staged-environment, deployment, and post-deployment checks. Each node SHALL declare its exact candidate or deployable subject, source visibility, Context and data projections, environment access, credential profile, budget, evidence schema, and required outcome. Deterministic failures SHALL remain authoritative and SHALL NOT be hidden by a reasoning summary.

A verification node configured for black-box operation SHALL receive no repository source, implementation transcript, protected test bundle, or undeclared credential. Verification results SHALL authorize only declared deterministic routes and SHALL remain bound to their exact candidate, environment, plan, and policy identities.

#### Scenario: Black-box verifier checks a staged candidate
- **WHEN** a Verification Plan declares only endpoint, public-contract, environment, and selected evidence inputs for one node
- **THEN** the node runs without source or implementation context and its result remains bound to the exact staged candidate

#### Scenario: Required deterministic verification fails
- **WHEN** an end-to-end, performance, UI, security, or post-deployment gate records a required failure
- **THEN** the Flow Run follows its declared failure route and no reasoning Assessment can convert that evidence into a pass

### Requirement: Durable Stage Handoffs and reproducible Context Manifests
Every completed model-backed Node Attempt SHALL publish an immutable versioned Stage Handoff containing a bounded typed outcome, summary, candidate-affecting decisions and assumptions, unresolved items, changed-component references, validation observations, prior findings, and exact Artifact, ChangeSet, Evidence, policy, and provenance references needed by declared successor nodes. A Stage Handoff SHALL exclude credentials, hidden chain-of-thought, unbounded transcripts, protected tests, and undeclared sibling state.

Before dispatching a reasoning call, the server SHALL construct and persist one immutable Context Manifest with a canonical stable order and digest. Every entry SHALL carry a source kind, logical identity, exact revision or subject binding, bounded Artifact or repository-range reference, content digest, encoded size, inclusion reason, and provenance. Control dependencies SHALL determine readiness only and SHALL NOT grant context or data authority. Construction SHALL use only explicit context and data projections, required exact inputs, selected typed child results or bounded summaries, and policy-authorized context. The same immutable inputs and construction policy SHALL produce identical manifest bytes, and retry or replay SHALL use the recorded manifest rather than repeat discovery.

A future Repository Knowledge subsystem MAY contribute bounded repository fragments only when each result is bound to the exact visible repository revision and accompanied by a Retrieval Receipt identifying the immutable index and embedding identities, normalized query, retrieval policy, stable ranking, ranges, and content digests. Retrieval SHALL remain non-authoritative: it SHALL NOT replace required Stage Handoffs or deterministic evidence, grant permissions, select a lifecycle transition, cross Project visibility, or become an unrecorded input. Mutable retrieval results SHALL NOT change an already dispatched or replayed call. The complete accumulated transcript SHALL NOT be authoritative lifecycle state or an implicit input to every later call.

#### Scenario: Successor node consumes an explicit handoff
- **WHEN** validation or rework follows a completed implementation Node Attempt
- **THEN** its Context Manifest contains the required typed Stage Handoff and exact candidate references without requiring access to the prior provider session or full transcript

#### Scenario: Identical context is reconstructed after restart
- **WHEN** another server owner resumes a call from the same immutable dependencies and context-construction policy
- **THEN** it observes the recorded canonical Context Manifest with the same ordered entries and digest instead of performing mutable discovery again

#### Scenario: A sibling reasoning branch starts
- **WHEN** one evaluation branch starts after another branch has completed
- **THEN** its Context Manifest contains only the declared parent context and explicitly selected typed result or bounded summary, not that branch's full hidden transcript or undeclared state

#### Scenario: A node is ordered after another without receiving its data
- **WHEN** a Flow Definition declares a control edge but no context or data projection between two nodes
- **THEN** the successor waits for the predecessor outcome while receiving none of its inputs, Artifacts, transcript, mounts, or hidden results

#### Scenario: Model session is lost
- **WHEN** an Agent and its provider session disappear after a completed call
- **THEN** another owner can continue from persisted Factory state, exact revisions, Stage Handoffs, Context Manifests, typed results, digests, and Artifact references

#### Scenario: Repository retrieval changes after dispatch
- **WHEN** a Repository Knowledge index or ranking policy changes after a call has been dispatched
- **THEN** retry and audit use the frozen repository fragments and Retrieval Receipt in the recorded Context Manifest, while a later call must create a new manifest to consume the new retrieval result

#### Scenario: Retrieved repository text attempts to control the factory
- **WHEN** a retrieved fragment contains instructions to widen permissions, skip validation, disclose sibling context, or claim success
- **THEN** the fragment remains untrusted call context and cannot modify policy, authority, deterministic evidence, or lifecycle decisions

### Requirement: Deterministic decisions and bounded rework
The Decision Engine SHALL apply a versioned policy to exact deterministic evidence and immutable Assessments and SHALL produce one typed `accept`, `rework`, `escalate`, `reject`, or `cancel` Decision with reasons and input digests. Required failed or indeterminate gates SHALL NOT become accepted. Rework SHALL create a new candidate, Evidence Manifest, Evaluation Plan, and Decision within configured attempt, elapsed-time, token, cost, and WIP bounds.

#### Scenario: Required evaluator is unavailable
- **WHEN** retry is exhausted for a required evaluator and policy does not define a safe substitute
- **THEN** the Decision Engine escalates the Factory Run rather than treating the missing assessment as pass

#### Scenario: Rework budget is exhausted
- **WHEN** another rework would exceed any recorded hard budget
- **THEN** the Factory Run escalates or rejects according to policy without dispatching another model call

### Requirement: Policy-governed idempotent delivery and promotion
Delivery and promotion nodes SHALL use protocol-separated trusted adapters to observe or idempotently act on only the exact accepted candidate and recorded policy. A delivery adapter MAY create or update one pull request or equivalent review target. A deployment trusted action SHALL invoke and observe the existing CI/CD Pipeline or Build path and SHALL NOT create a separate deployment executor. Coding, evaluator, verification, reasoning, and Decision Signal Jobs SHALL NOT receive forge-write or production credentials.

An immutable Factory Configuration SHALL declare every required human gate, deterministic gate, environment progression, and Promotion Plan. A Promotion Plan SHALL bind the exact candidate, environments, rollout strategy, allocation or cohort parameters where applicable, observation windows, evidence requirements, budgets, success and rollback criteria, and finite outcomes. Declared strategies SHALL be able to include stage-only, stage plus human approval, stage plus deterministic evidence, canary, A/B, progressive rollout, and immediate production for an explicitly allowed low-risk class. A model or Decision Signal SHALL NOT merge or deploy directly and SHALL only recommend a route from the declared finite outcomes. Bounded post-deployment verification MAY contribute typed evidence to the Flow Run, while long-running production monitoring SHALL remain an external system that may submit new Work through the source-neutral admission boundary.

#### Scenario: Delivery response is lost
- **WHEN** the adapter may have created a pull request but the response is lost
- **THEN** retry uses the stable delivery identity and observe-before-retry behavior and does not create a duplicate review target

#### Scenario: Candidate no longer matches the accepted decision
- **WHEN** delivery observes a candidate, ancestry, or policy digest different from the accepted Decision
- **THEN** it rejects publication and the Factory Run cannot claim delivery success

#### Scenario: Accepted candidate is promoted through existing CI/CD
- **WHEN** all declared delivery, human, and environment gates are satisfied for an exact candidate
- **THEN** the trusted action submits or observes an ordinary deployment Build and records its exact identity and terminal evidence

#### Scenario: Canary observation fails
- **WHEN** a canary Promotion Plan records a required health or performance failure during its immutable observation window
- **THEN** deterministic policy follows the declared rollback or escalation outcome and cannot promote full production from a model recommendation

#### Scenario: A/B continuation uses a bounded signal
- **WHEN** an A/B Promotion Plan permits a Decision Signal to recommend one declared continuation outcome from recorded experiment evidence
- **THEN** the signal receipt is retained as a non-authoritative input and deterministic or human policy commits the continuation, rollback, or escalation decision

#### Scenario: Black-box verification is isolated from implementation
- **WHEN** a verification subflow evaluates the deployed candidate without source authority
- **THEN** its nodes receive only declared endpoint, contract, environment, and evidence inputs and cannot access implementation transcripts or protected credentials

### Requirement: Source reporting is a non-authoritative projection
The server MAY publish bounded status and result projections through a versioned Work Reporter adapter, but reporter failure or external status SHALL NOT authorize or reconstruct a Factory transition. Delivery and reporting SHALL use independent credentials and idempotency identities.

#### Scenario: Work tracker is unavailable
- **WHEN** a Factory transition commits while its Work Reporter is unavailable
- **THEN** the authoritative Factory Run continues, durable reporting work is retried within bounds, and no transition is rolled back or inferred from tracker state

### Requirement: Self-evolution uses the ordinary candidate path
Any model-produced change to factory workflow code, prompts, criterion packs, policies, or adapters SHALL be represented as an ordinary ChangeSet and SHALL pass the same validation, evaluation, delivery, authorization, and human gates as another candidate. A running Factory Run SHALL NOT mutate its own controlling code or policy through hidden model or session state.

#### Scenario: Harness proposes an improved workflow
- **WHEN** a model generates a modification to a factory function or criterion pack
- **THEN** the proposal becomes a candidate for a separately governed Factory Run or ordinary CI/CD review and does not alter the current run's control flow
