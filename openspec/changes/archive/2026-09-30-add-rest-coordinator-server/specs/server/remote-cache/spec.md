## Purpose

Defines the namespace-scoped HTTP L2 cache authority consumed by Octa while leaving action keys, result semantics, and local L1 behavior inside Octa.

## ADDED Requirements

### Requirement: Fenced short-lived cache grants
The server SHALL exchange a current lease and signed cache policy for a short-lived cache session restricted to one project namespace and the requested subset of read/write permissions.

#### Scenario: Read-only job begins a session
- **WHEN** a current lease requests read-only cache access allowed by project policy
- **THEN** the server issues a credential that cannot publish cache entries or access another namespace

### Requirement: Exact Octa HTTP cache protocol
The remote cache endpoint SHALL implement the published Octa HTTP cache contract and SHALL treat action descriptors, result metadata, and blobs as opaque validated protocol objects rather than reimplementing task cache-key semantics.

#### Scenario: Agent uses local and remote cache together
- **WHEN** Octa misses its local L1 and presents an authorized remote request
- **THEN** the server returns the matching verified L2 object using the existing Octa protocol

### Requirement: Content integrity and immutable publication
Cache objects SHALL be addressed and verified by their declared cryptographic digest, published atomically, and never served from a partial or mismatched upload.

#### Scenario: Corrupt remote object is detected
- **WHEN** stored bytes no longer match their protocol digest
- **THEN** the server returns no usable hit, quarantines or removes the corrupt generation, and does not alter the caller's namespace

### Requirement: Session revocation and expiry
Cache credentials SHALL stop authorizing new requests immediately after explicit revocation, lease fencing, or expiry; repeated revocation SHALL be idempotent.

#### Scenario: Runner has stopped
- **WHEN** the agent revokes its cache session after runner shutdown
- **THEN** subsequent requests using that credential are rejected without exposing whether another namespace contains the key

### Requirement: Namespace isolation and retention
Lookup, publication, quota, and retention SHALL be isolated by authorized namespace and SHALL preserve objects still referenced by retained action results.

#### Scenario: Equal action keys exist in separate projects
- **WHEN** two namespaces use the same action digest
- **THEN** neither namespace gains metadata or authorization from the other, regardless of physical blob deduplication
