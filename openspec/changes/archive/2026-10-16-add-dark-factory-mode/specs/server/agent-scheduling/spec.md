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

### Requirement: Optional tool-risk assessment cannot widen execution authority
Before any optional Decision Signal assessment, a model-proposed tool or command action SHALL be intercepted by a trusted blocking pre-execution hook and normalized into bounded executable identity, arguments, resolved paths, network destinations, credential profiles, descendants, resources, and expected outputs. An Agent-local authorization broker bound to the current Job, lease and fence SHALL apply the signed Factory Permission Set, Task Envelope, Project policy, local Agent grants, backend capabilities, and deterministic hard-deny rules before the action can execute. Deterministic hard-allow rules MAY authorize an in-envelope action without a provider call. Only an ambiguous action already inside every authoritative envelope MAY be sent as a redacted `tool_risk` request to a server-side Decision Signal provider. Its consuming code policy MAY allow the unchanged action, deny it, or escalate, but the signal SHALL NOT add or broaden any authority. The Agent and backend SHALL revalidate and enforce the final unchanged action immediately before the protected operation. Provider credentials SHALL NOT be delivered to the Agent workload or harness.

#### Scenario: Proposed command is outside the whitelist
- **WHEN** a harness proposes a command, argument, path, host, credential, descendant, resource, or output not present in the effective permission intersection
- **THEN** deterministic policy denies it before any Decision Signal request and no provider response can authorize it

#### Scenario: Ambiguous in-envelope action is assessed
- **WHEN** an action fits every authoritative permission boundary but is not covered by deterministic hard-allow or hard-deny policy
- **THEN** the configured exact `tool_risk` provider and policy may assess the normalized redacted action, after which code applies its threshold and fail-closed disposition

#### Scenario: Tool-risk provider is unavailable
- **WHEN** bounded control requires a tool-risk signal but its provider is unavailable, invalid, or below the configured confidence floor
- **THEN** the action is denied or escalated according to immutable policy and execution does not fall back to an unassessed allow

#### Scenario: Harness lacks a blocking tool hook
- **WHEN** a Factory Job requires `tool_risk` bounded control but its pinned plugin or harness cannot prove the required pre-execution interception capability
- **THEN** placement or pre-spawn validation rejects the Job and no action runs through an advisory or best-effort fallback

#### Scenario: Tool authorization loses its fence
- **WHEN** the Job is cancelled, its lease or fence becomes stale, the authorization broker disconnects, or the returned receipt does not match the blocked proposal
- **THEN** the pending action is denied and cannot execute using a disposition from another Job or proposal

### Requirement: Factory secrets remain stage scoped
Model, evaluator, source, and delivery credentials SHALL use distinct logical profiles and SHALL be delivered only to the trusted component and Stage Attempt authorized to consume them. Coding and evaluation execution SHALL NOT receive a delivery credential, and a delivery adapter SHALL NOT receive model session credentials or unredacted model traces.

#### Scenario: Coding stage requests delivery identity
- **WHEN** a coding Task Envelope or child process attempts to resolve the configured forge delivery profile
- **THEN** secret resolution is denied before value materialization and the denial is recorded without exposing the credential
