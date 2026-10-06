## ADDED Requirements

### Requirement: Factory permission capability placement
Placement for a Factory-owned Job SHALL require an Agent and execution backend that advertise the protocol and enforcement capabilities needed by the immutable Factory Permission Set, including plugin and executable identity, command and argument policy, descendant-process control, filesystem and mount policy, network policy, secret and workload-identity delivery, resources, and output bounds. Capability matching SHALL remain provider-neutral and SHALL NOT select an Agent merely because it can start the requested executable.

#### Scenario: Agent lacks one required enforcement capability
- **WHEN** a ready Factory-owned Job requires a filesystem, network, process, or identity restriction that an otherwise compatible Agent cannot enforce
- **THEN** that Agent cannot acquire the Job and unrelated compatible work remains schedulable

#### Scenario: Equivalent qualified backends exist
- **WHEN** different Agent backends prove the same required semantic permission capabilities and execution guarantees
- **THEN** either may accept the Job without exposing its concrete backend name in factory policy

### Requirement: Pre-spawn permission revalidation
The Agent SHALL verify the signed Factory Permission Set, pinned plugin and executable identity, local grants, resolved paths, mounts, network hosts, secret and workload-identity profiles, resources, outputs, and selected backend capabilities immediately before source or process activity. Every effective permission SHALL be no broader than the signed Job intent and local policy. Failure SHALL be typed, secret-safe, and terminal for that execution attempt unless server policy explicitly classifies a provider-independent retry.

#### Scenario: Local grant changed after placement
- **WHEN** a Job is leased but a required local tool, path, profile, or backend grant is no longer available before spawn
- **THEN** the Agent rejects execution before revealing source, prompt, or credentials and reports the missing capability without falling back to Host

#### Scenario: Repository requests an extra permission
- **WHEN** repository content or a harness request attempts to add a command, argument, path, mount, host, child process, secret, or output outside signed intent
- **THEN** the Agent or backend denies that operation and cannot persist the requested widening as a new grant

### Requirement: Factory secrets remain stage scoped
Model, evaluator, source, and delivery credentials SHALL use distinct logical profiles and SHALL be delivered only to the trusted component and Stage Attempt authorized to consume them. Coding and evaluation execution SHALL NOT receive a delivery credential, and a delivery adapter SHALL NOT receive model session credentials or unredacted model traces.

#### Scenario: Coding stage requests delivery identity
- **WHEN** a coding Task Envelope or child process attempts to resolve the configured forge delivery profile
- **THEN** secret resolution is denied before value materialization and the denial is recorded without exposing the credential
