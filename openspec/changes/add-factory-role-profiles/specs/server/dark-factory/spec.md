## MODIFIED Requirements

### Requirement: Provider-neutral Decision Signals are bounded non-authoritative inputs
The server MAY request probability-backed Decision Signals through a provider-neutral versioned port for `routing`, `tool_risk`, and `profile_selection` purposes. A request SHALL contain bounded redacted canonical state, versioned typed questions with finite answer or score domains, subject and policy digests, deadline, budget, and a stable logical identity. Provider capability discovery SHALL identify supported purposes, question kinds, input media, limits, data-handling class, exact adapter and model version, and probability or confidence semantics without exposing provider wire types to the factory core. JEV SHALL be the initial adapter, and another decision-model or Decisions API adapter SHALL be replaceable through immutable Factory Configuration rather than domain changes. Purpose-specific consumers SHALL retain their own criteria, capability qualification, calibration, validation, and application policy; the common provider bridge SHALL NOT own a universal selection or lifecycle decision engine.

Every completed or terminal request SHALL produce an immutable Decision Signal Receipt that binds the canonical input, question-set and policy digests, exact provider, adapter and model identities, typed answers, probability or confidence semantics, usage, terminal classification, and consuming deterministic disposition. An admitted Factory Run SHALL retain its selected exact identities even after configuration replacement. Unknown-response recovery and replay SHALL reuse or observe the recorded receipt and SHALL NOT silently query a different model for the same logical decision.

#### Scenario: Provider is replaced for later runs
- **WHEN** an operator publishes a new Factory Configuration selecting another compatible decision-model adapter or exact model version
- **THEN** later Factory Runs use the replacement while admitted runs retain their original provider, model, questions, policy, and receipts

#### Scenario: Decision Signal call cannot be reproduced safely
- **WHEN** the configured provider times out, returns an invalid result, lacks a required capability, falls below its calibrated confidence floor, or has an unknown outcome with no observable receipt
- **THEN** deterministic policy applies the configured fail-closed denial or escalation and does not substitute another model implicitly

#### Scenario: One installed provider serves distinct consumers
- **WHEN** routing and profile-selection requests use the same installed provider adapter
- **THEN** they share the bounded provider transport and receipt contract but retain separate purposes, answer mappings, calibration, consuming policies, and operation identities

#### Scenario: Profile selection receives a failed signal
- **WHEN** a profile-selection request cannot produce an accepted consuming recommendation
- **THEN** its failed signal is retained and the stage applies its separately pinned reserve-profile or manual-selection policy without treating that recovery as provider approval

### Requirement: Immutable Factory Task Envelope and Codex execution
Each reasoning or Build-backed node SHALL receive an immutable versioned Factory Task Envelope containing Flow Definition, Flow Run, node, attempt, and subject identities, exact revision, mode, task and specification artifact references, Context Manifest identity and digest, prior typed findings, permissions, budgets, expected result schema, deliverables, and policy, prompt, plugin, executable, model, and input digests. When a node uses a Role Profile, the retained invocation SHALL additionally bind its exact Project rules, role/profile versions, selection record, effective supported model/reasoning settings, complete skill bundle dependencies, and execution-package digest. The first reasoning implementation SHALL execute Codex through the official pinned Octa `codex` task and generic runner protocol; OctaCity server and Agent protocols SHALL NOT contain Codex-specific request, event, or result types.

#### Scenario: Codex implementation is dispatched
- **WHEN** a valid Task Envelope selects the pinned Codex implementation capability
- **THEN** the trusted boundary compiles it to a normal Octa task with explicit prompt, result schema, environment mappings, revision, and deliverables and records the selected identities in provenance

#### Scenario: Pinned executable is unavailable or incompatible
- **WHEN** the selected Agent cannot prove the required Codex plugin or supported executable identity
- **THEN** the Job is not started and no prompt, secret, or workspace record is exposed to that executable

#### Scenario: Profile package changes between selection and compilation
- **WHEN** materialized role, skill, instruction, model, or reasoning-setting content differs from the retained invocation identity
- **THEN** the ordinary Build cannot execute under the substituted package and recovery retains the original binding

## ADDED Requirements

### Requirement: Profile-selected nodes preserve immutable Flow and Build authority
A model-backed Flow node SHALL be able to declare a profile-selection binding instead of one static execution binding. Publication SHALL pin every candidate's role/profile and installed execution references, selection inputs/policy, stage contract, and failure behavior in the immutable definition closure. Runtime selection SHALL choose only one declared compatible binding for the exact Node Attempt and SHALL retain that choice before committing an ordinary Build dispatch intent. No profile name, template, model output, or provider result SHALL create a node kind, mutate the admitted graph, widen authority, change the stage result schema, or bypass a required gate. Existing static node bindings and Flows without model-backed roles SHALL remain executable unchanged.

#### Scenario: Different profiles implement the same stage contract
- **WHEN** two compatible coding profiles are selectable for an operator-named implementation node
- **THEN** the selected profile changes only its declared execution package and ordinary Build binding, and both executions must satisfy the same stage result and downstream verification contracts

#### Scenario: Manual selection waits without changing the graph
- **WHEN** the node's configured selection policy requires an operator choice
- **THEN** the exact pending selection is durable under fenced ownership without inventing a HumanGate node, mutating Flow edges, or creating an execution Build

#### Scenario: A profile name resembles a business phase
- **WHEN** an operator creates a role/profile with an arbitrary name or renames `coder-pro`
- **THEN** only its explicit immutable binding selects execution behavior and no name-based dispatcher or mandatory role is introduced

#### Scenario: Role features are absent
- **WHEN** a deployment executes an existing static Factory node or ordinary CI/CD Build without a role catalog or Decision API
- **THEN** existing Build creation, execution, control, and result behavior remains available
