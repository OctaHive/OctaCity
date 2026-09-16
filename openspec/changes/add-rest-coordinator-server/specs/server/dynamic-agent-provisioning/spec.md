## Purpose

Defines a provider-neutral, versioned agent-provisioning protocol that can be implemented after the static-agent server is complete without coupling virtualization-provider details to scheduling, agent execution, or JobSpec semantics.

## ADDED Requirements

### Requirement: Versioned agent-provisioning protocol
The server SHALL define a bounded protocol for provision, observe, and terminate operations using idempotency identities, opaque provider machine identifiers, normalized lifecycle states, cancellation, and classified failures.

#### Scenario: Provider implementation is added later
- **WHEN** a future provider advertises a compatible protocol range and passes the contract suite
- **THEN** the server can host it without adding provider-specific fields to jobs, agent inventory, or placement scheduling

### Requirement: Provider-independent machine lifecycle
The protocol SHALL represent requested pool intent, expected platform, bootstrap delivery, observed machine state, and terminal disposal without exposing provider-specific templates, clusters, networks, or datastore types to the core scheduler.

#### Scenario: Provision response is lost
- **WHEN** a future adapter receives the same provision idempotency identity after a lost response
- **THEN** it returns or converges on the same provider machine rather than creating duplicate capacity

### Requirement: Short-lived enrollment bootstrap
A provisioned machine SHALL receive only a single-use, short-lived enrollment credential bound to its pool and expected platform and SHALL connect outbound using the normal agent protocol.

#### Scenario: Bootstrap token is replayed
- **WHEN** a consumed or expired enrollment credential is presented again
- **THEN** registration is rejected and no additional agent identity is created

### Requirement: Provider credentials remain isolated
Provider credentials and sensitive infrastructure configuration SHALL remain owned by the provider adapter and SHALL never enter an agent request, JobSpec, event, or project-readable response.

#### Scenario: Client reads normalized capacity state
- **WHEN** a client reads a future provider machine through a management interface
- **THEN** the response contains normalized identity and lifecycle data but no credential, bootstrap secret, template, network, cluster, or datastore value

### Requirement: No production provisioner in the first release
The first server release SHALL NOT advertise dynamic agent provisioning as available and SHALL NOT include production vSphere, Proxmox, or other virtualization adapters. Static-agent scheduling SHALL operate without loading or configuring an agent-provisioning adapter.

#### Scenario: First-release server starts
- **WHEN** no agent-provisioning adapter is installed
- **THEN** readiness and static-agent scheduling remain unaffected and dynamic agent-provisioning operations report unavailable
