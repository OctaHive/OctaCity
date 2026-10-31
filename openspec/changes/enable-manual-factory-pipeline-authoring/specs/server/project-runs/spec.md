## MODIFIED Requirements

### Requirement: Versioned build configurations
A build configuration SHALL belong to exactly one project and store a sibling-unique name, enabled state, explicit source policy, pipeline version, parameter schema and defaults, trigger policy, agent requirements, allowed pool restriction, provider-neutral execution-mode and target-platform policy, cache policy, output policy and retry policy. Source policy SHALL distinguish the existing repository-backed selection from explicitly qualified source-free execution. Existing repository-backed configurations SHALL retain their required source resolution and policy. Each accepted mutation SHALL create a new configuration version without rewriting builds created from an older version.

#### Scenario: Build configuration is updated
- **WHEN** an operator changes a configuration's pipeline or execution policy using its current version precondition
- **THEN** subsequent builds use the new configuration version while existing builds retain the prior snapshot

#### Scenario: Disabled configuration is triggered
- **WHEN** a manual, scheduled, external, or internal trigger targets a disabled build configuration
- **THEN** the server records a suppressed trigger outcome without creating queued work

#### Scenario: Executor lacks source-free support
- **WHEN** a configuration selects source-free execution under an incompatible protocol or backend
- **THEN** validation/readiness blocks it without substituting a repository or an unisolated executor

### Requirement: Immutable build input
Creating a build SHALL bind the project, build-configuration version, pipeline version, effective project policy, explicit source policy, parameters, trigger occurrence, priority and creation cause into an immutable build record. Repository-backed inputs SHALL retain their Repository and exact immutable source revision. Source-free inputs SHALL record absence explicitly and SHALL NOT inherit a source from another Build, Factory Work or host workspace. Existing input encodings and replay fingerprints SHALL retain their historical meaning.

#### Scenario: Manual build is accepted
- **WHEN** a client submits valid parameters and an exact revision for an enabled repository-backed build configuration
- **THEN** the server durably creates the build, its first attempt, the jobs materialized from its pipeline and the initially ready queue entries

#### Scenario: Mutable revision is unresolved
- **WHEN** a repository-backed trigger names a branch or tag that cannot be resolved to an immutable revision
- **THEN** the server rejects or defers the build without leasing ambiguous source state

#### Scenario: Source-free input is retried
- **WHEN** an accepted source-free Build is retried or restored after process loss
- **THEN** its signed execution intent retains source absence and the same bounded execution inputs without a VCS request or implicit checkout

### Requirement: Factory-owned Builds preserve CI/CD semantics
A Factory Stage Attempt SHALL create an ordinary immutable Build through the existing Build application and SHALL record the Factory Run, Factory Configuration version, stage identity, Stage Attempt number, Task Envelope digest, exact resource subjects or source-free mode, and parent ChangeSet or Decision identity as bounded creation causality. Factory causality SHALL NOT replace the Build's Project, Build Configuration, Pipeline, effective policy, explicit source policy, Attempt, Job, Trigger, cancellation, retry, output or terminal-state contracts. Repository-backed Builds SHALL retain their exact source revision; source-free execution SHALL require explicit ordinary support. An ordinary Build SHALL NOT require factory causality.

#### Scenario: Factory stage creates a Build
- **WHEN** the Factory Controller dispatches an implementation, validation, evaluation, or rework Stage Attempt
- **THEN** the resulting Build is queryable and operable through the existing Build APIs and additionally identifies its exact Factory causality

#### Scenario: Ordinary Build is triggered
- **WHEN** a manual, scheduled, webhook, or internal Trigger creates a Build outside a Factory Run
- **THEN** the Build retains the existing CI/CD representation and behavior without a synthetic Factory Run or Task Envelope
