## MODIFIED Requirements

### Requirement: Provider-neutral Work admission
The server SHALL accept bounded Work Submissions through versioned Work Source adapters and normalize accepted input into an immutable Work Envelope containing a scoped source identity, Project-owned subject, task artifact, process criteria or explicitly selected acceptance input, specification references, priority, risk, and bounded provider metadata. The subject SHALL distinguish an exact repository-backed subject from an explicitly repository-free Project/input subject. Admission SHALL authenticate the source where required, apply Project visibility and Factory policy, and durably deduplicate one Factory Run per scoped Work identity. When a repository subject is selected, admission SHALL resolve its allowed base reference to an exact immutable revision. A repository-free submission SHALL require no synthetic Repository, branch or empty checkout. Source-authored implementation acceptance SHALL remain separate from process-completion criteria.

#### Scenario: Source retries the same Work Submission
- **WHEN** an authenticated source repeats a submission with the same scoped identity after losing its response
- **THEN** admission returns the existing Factory Run without resolving mutable source state or creating another run

#### Scenario: Repository reference cannot be resolved safely
- **WHEN** an accepted submission names a mutable reference that cannot be resolved under the selected Repository policy
- **THEN** no Factory Run is admitted for ambiguous source state and the failure is reported without executing repository code

#### Scenario: Work needs only documents and reasoning
- **WHEN** an authorized submission selects a repository-free pipeline with exact task, criteria and document inputs
- **THEN** ordinary admission retains one Project-scoped Work/Run without a Repository, VCS lookup or fabricated revision

### Requirement: Build-backed flow nodes execute as ordinary Builds
A command, implementation, validation, evaluation, or rework Node Attempt SHALL create or select ordinary immutable Builds through the existing Build application. The Factory Run SHALL record exact Build, Attempt, Job, configuration version, effective policy, output and terminal-observation references, together with the step's exact selected code resources or explicit source-free mode. Factory coordination SHALL observe authoritative Build state and SHALL NOT duplicate Job DAG orchestration, placement, lease, cancellation, retry, event or output truth. A repository-free reasoning step SHALL use qualified source-free ordinary execution rather than a dummy repository or a separate executor.

#### Scenario: Implementation node starts
- **WHEN** the reconciler admits an implementation Node Attempt
- **THEN** it creates one causally linked ordinary Build whose immutable input contains the exact selected code revision and narrowed Factory execution intent

#### Scenario: Build is cancelled outside the factory view
- **WHEN** an operator cancels a Factory-owned Build through the existing Build API
- **THEN** the Factory reconciler observes the authoritative terminal state and applies the configured stage failure policy without inventing a second Build state

#### Scenario: Reasoning step has no code input
- **WHEN** its declared inputs consist of retained text and accepted result fields and its installed capability supports source-free execution
- **THEN** its ordinary Build performs no repository lookup, checkout or code mount and retains normal lease, output, cancellation and retry semantics

### Requirement: Immutable Factory Task Envelope and Codex execution
Each reasoning or Build-backed node SHALL receive an immutable versioned Factory Task Envelope containing Flow Definition, Flow Run, Work, node and attempt identities, its Project/input subject and exact selected resource subjects, mode, task and specification artifact references, Context Manifest identity and digest, prior typed findings, permissions, budgets, expected result schema, deliverables, and policy, prompt, plugin, executable, model and input digests. A code resource SHALL identify its exact repository revision or accepted candidate; absence SHALL be explicit. When a node uses a Role Profile, the retained invocation SHALL additionally bind any selected controlling Project rules, exact role/profile versions, selection record, effective supported runtime/model/thinking settings, ordered instruction layers, explicit environment/CLI settings, authorized MCP/integration bindings, complete retained skill bundle dependencies and execution-package digest. Secret values SHALL remain behind the existing credential boundary rather than enter this retained content identity. The first reasoning implementation SHALL execute Codex through the official pinned Octa task and generic runner protocol; OctaCity server and Agent protocols SHALL NOT contain Codex-specific request, event or result types.

#### Scenario: Codex implementation is dispatched
- **WHEN** a valid Task Envelope selects the pinned Codex implementation capability
- **THEN** the trusted boundary compiles it to a normal Octa task with explicit prompt, result schema, environment mappings, exact selected resources and deliverables and records those identities in provenance

#### Scenario: Pinned executable is unavailable or incompatible
- **WHEN** the selected Agent cannot prove the required Codex plugin or supported executable identity
- **THEN** the Job is not started and no prompt, secret or workspace record is exposed to that executable

#### Scenario: Profile package changes between selection and compilation
- **WHEN** materialized role, skill, instruction, runtime/model/thinking, environment/CLI, resource or MCP/integration-binding content differs from the retained invocation identity
- **THEN** the ordinary Build cannot execute under the substituted package and recovery retains the original binding

## ADDED Requirements

### Requirement: Invocation retains explicit step instructions separately from input data
Each agent invocation SHALL bind the exact reviewed Flow step task and expected result contract independently of reusable Role/Profile instructions and incoming Work/Context data. The compiled instruction package SHALL preserve that distinction, controlling Project policy and effective permission ceilings. Untrusted Issue text, reference material and predecessor output SHALL NOT become step instructions by inference. Step-specific business duties SHALL NOT depend on stage names or hidden runtime instructions. The existing Flow/task/package owners SHALL retain task content identity for replay and recovery without a second instruction store; compatible legacy encodings SHALL retain their meaning.

#### Scenario: Incoming Issue attempts to replace the step assignment
- **WHEN** an Issue or accepted predecessor field contains instructions to skip the configured task or expand access
- **THEN** the invocation retains its reviewed step assignment and policy, treats that incoming content as data and grants no additional tool or resource access

### Requirement: Step code resources are explicit and independent of Work correlation
The versioned pipeline definition/configuration SHALL declare available code sources by authorized Project repository reference, revision-selection rule and access/scope ceiling without creating a separate source owner. Each step SHALL declare no code resource or one exact primary repository/candidate selection from those sources, with supported read/write mode and any declared path/range restriction narrowed by the source ceiling and current effective policy. Different steps SHALL be able to select different declared sources; the declaration MAY be empty. Pipeline availability and a Work's task-source correlation or repository subject SHALL grant no automatic checkout, code visibility, mount, network permission or credential. Mutable selections SHALL be frozen before dependent I/O and dispatch; retries SHALL use those exact retained selections. Cross-repository reuse of candidate/evidence identity SHALL require a declared compatible projection and SHALL NOT be inferred from a shared Project or node name.

#### Scenario: Repository is available but not attached
- **WHEN** repositories A and B are declared for the pipeline and a step explicitly selects only A
- **THEN** that step receives only its narrowed exact A binding, no B checkout or access, and steps with no code attachment receive neither source

#### Scenario: Source declaration changes
- **WHEN** a draft removes a source still selected by a step, selects an undeclared source or widens access beyond its declared ceiling
- **THEN** validation requires explicit reconciliation rather than silently detaching, remapping or broadening that step; later publication never rewrites admitted resource bindings

#### Scenario: Only the second agent needs code
- **WHEN** the first step selects only task/documents/MCP and the second selects repository A for reading
- **THEN** the first receives no code resource and the second receives only the selected revision under its own narrowed permissions

#### Scenario: Successor uses a different repository
- **WHEN** two steps select authorized repositories A and B with different exact revisions
- **THEN** each execution and its evidence retain the correct repository identity, and a result for A cannot masquerade as a candidate or verification for B

### Requirement: Process criteria are editable immutable inputs
An operator SHALL be able to author and edit pipeline goals and completion criteria through the common definition draft, distinguish them from Project goals, step result schemas and ticket-authored implementation acceptance, and explicitly bind their frozen content only to selected steps. Publication SHALL reuse existing immutable definition and managed-input owners. Changed goals/criteria SHALL require a new reviewed version and SHALL NOT alter an admitted Work or invocation. This text SHALL confer no tool, transition, acceptance or delivery authority.

#### Scenario: Criteria change after a Run was accepted
- **WHEN** an operator publishes new criteria and explicitly selects them for later execution
- **THEN** later admission uses the new exact input while the original Run and its recovery retain their accepted criteria bytes and provenance
