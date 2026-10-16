## ADDED Requirements

### Requirement: Factory-owned Builds preserve CI/CD semantics
A Factory Stage Attempt SHALL create an ordinary immutable Build through the existing Build application and SHALL record the Factory Run, Factory Configuration version, stage kind, Stage Attempt number, Task Envelope digest, exact subject revision, and parent ChangeSet or Decision identity as bounded creation causality. Factory causality SHALL NOT replace the Build's Project, Build Configuration, Pipeline, effective-policy, Repository, exact source revision, Attempt, Job, Trigger, cancellation, retry, output, or terminal-state contracts. An ordinary Build SHALL NOT require factory causality.

#### Scenario: Factory stage creates a Build
- **WHEN** the Factory Controller dispatches an implementation, validation, evaluation, or rework Stage Attempt
- **THEN** the resulting Build is queryable and operable through the existing Build APIs and additionally identifies its exact Factory causality

#### Scenario: Ordinary Build is triggered
- **WHEN** a manual, scheduled, webhook, or internal Trigger creates a Build outside a Factory Run
- **THEN** the Build retains the existing CI/CD representation and behavior without a synthetic Factory Run or Task Envelope

### Requirement: Factory policy may only narrow Build authority
The server SHALL derive a Factory-owned Build's immutable effective execution intent by intersecting the recorded Project and Build Configuration policy with the Factory Configuration and Task Envelope. Factory input SHALL NOT add a Pool, execution target, plugin, secret profile, workload-identity profile, network host, cache namespace, output capability, resource allowance, or retention authority excluded by the ordinary effective Project policy.

#### Scenario: Factory requests broader Project authority
- **WHEN** a Task Envelope names an execution capability or provider profile excluded by the Build's effective Project policy
- **THEN** Build creation fails before queue visibility and does not weaken the ordinary CI/CD policy snapshot

#### Scenario: Project policy changes after admission
- **WHEN** Project policy is replaced after a Factory-owned Build has been accepted
- **THEN** that Build and its ordinary retry retain the immutable intersected policy recorded at Build creation while a later Factory Stage performs a new admission decision
