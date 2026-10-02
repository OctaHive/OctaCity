## Purpose

Defines a reproducible, one-command local OctaCity deployment that exercises the real server, storage, console, enrollment, and Microsandbox Agent path without presenting development conveniences as production guarantees.

## ADDED Requirements

### Requirement: One Compose application starts the complete local stand
The repository SHALL provide one root-level Compose application whose normal startup command builds and starts PostgreSQL, S3-compatible object storage, the OctaCity server, bootstrap automation, one Microsandbox Agent, and a reverse proxy serving the operator console. A successful startup SHALL require no separately launched OctaCity or Agent process.

#### Scenario: Operator starts a clean stand
- **WHEN** an operator with a supported Docker environment runs the documented Compose startup command against empty named volumes
- **THEN** every required service is built or pulled, initialized in dependency order, and reaches its documented healthy or successfully-completed state

#### Scenario: Operator restarts an initialized stand
- **WHEN** the operator stops and starts the same Compose project without deleting its named volumes
- **THEN** the server and Agent reuse durable state and bootstrap does not create duplicate logical resources or invalidate the current Agent registration

### Requirement: The stand exposes one bounded browser gateway
The local stand SHALL publish only its reverse-proxy gateway on loopback host interfaces by default. The gateway SHALL serve the console, route known management and health paths to the management listener, route the authenticated Agent and cache origins to their dedicated listeners, and route the object-transfer origin to object storage. PostgreSQL, MinIO, and the server's raw listeners SHALL remain private to the Compose network, and unknown management or health paths SHALL NOT fall back to the SPA.

#### Scenario: Browser opens the local console
- **WHEN** the operator opens the documented loopback console URL
- **THEN** the gateway returns the built SPA and same-origin management requests reach the server

#### Scenario: Client requests an unknown API route
- **WHEN** a client requests an unknown path below `/api/v1` or `/health`
- **THEN** the gateway returns the upstream not-found response and does not return the SPA entry document

#### Scenario: Host scans dependency ports
- **WHEN** a process on the host attempts to connect directly to the configured PostgreSQL, MinIO, management, Agent, or cache container port
- **THEN** no direct host-published listener exists for that dependency

### Requirement: Microsandbox capability is proven before Agent registration
The Agent service SHALL use the pinned Microsandbox virtualization backend and SHALL receive only the device and permissions required to access a usable `/dev/kvm`. Before starting or registering the Agent, the service SHALL verify the device, runtime, firmware, and ability to initialize Microsandbox. It SHALL NOT silently fall back to Host, Native, containerd, or an unisolated process backend.

#### Scenario: Docker exposes usable KVM
- **WHEN** the Agent container can access `/dev/kvm` and its pinned Microsandbox installation passes the runtime preflight
- **THEN** the Agent starts with only the virtualization provider enabled and advertises the exact Linux host, Linux guest, runtime, and environment identity it actually provides

#### Scenario: Docker does not expose usable KVM
- **WHEN** `/dev/kvm` is absent, inaccessible, or rejected by the Microsandbox preflight
- **THEN** the Agent service exits unhealthy with an actionable diagnostic and no weaker execution capability is registered

### Requirement: Bootstrap is idempotent and secret-safe
Bootstrap SHALL wait for server readiness, create or recover one dedicated local Agent Pool through the management API, issue or replay one enrollment credential bound to that Pool version and expected Agent platform, and place the credential in an Agent-readable private file. Repeated bootstrap with unchanged durable state SHALL converge on the same logical Pool and valid Agent identity. Enrollment credentials, registration credentials, signing material, object-store credentials, and presigned URLs SHALL NOT appear in Compose output or service logs.

#### Scenario: First bootstrap succeeds
- **WHEN** the server first becomes ready with an empty application database and no Agent credential file
- **THEN** bootstrap creates the Pool, obtains one single-use enrollment credential, writes it with private ownership and permissions, and allows the Agent to register

#### Scenario: Bootstrap response is interrupted
- **WHEN** bootstrap retries a mutation after the server committed it but before bootstrap received the response
- **THEN** it reuses the stable idempotency identity and recovers the original logical result without creating a duplicate Pool or enrollment

#### Scenario: Stand is restarted after registration
- **WHEN** an existing Agent registration credential and application database are present
- **THEN** bootstrap preserves the credential and the Agent reconnects without issuing an unnecessary replacement enrollment

### Requirement: Local transport and credentials preserve production trust boundaries
All non-loopback origins accepted by the server or Agent SHALL use TLS. The stand SHALL generate or provision local-only credentials as files with bounded ownership and permissions, use distinct signing, enrollment, cache, database, and object-store secrets, and run long-lived OctaCity processes as dedicated non-root users after any narrowly scoped initialization. No committed secret SHALL be suitable for a non-local deployment.

#### Scenario: Service configuration is validated
- **WHEN** the server and Agent validate their generated local configuration
- **THEN** every required credential is a private regular file, all configured service origins satisfy transport validation, and the Agent trusts only the local stand certificate authority in addition to its normal roots

#### Scenario: Repository is inspected for deployment secrets
- **WHEN** the Compose assets are checked out on another machine
- **THEN** they contain no generated private key, enrollment token, registration token, or object-store password from a previous run

### Requirement: State and destructive reset are explicit
PostgreSQL data, object-store data, Agent credentials and state, and Microsandbox runtime state SHALL use named volumes that survive an ordinary Compose stop and restart. The documented reset operation SHALL identify every local volume it destroys and SHALL require an explicit destructive command; ordinary startup and shutdown SHALL NOT erase durable state.

#### Scenario: Ordinary shutdown preserves state
- **WHEN** the operator runs the documented non-destructive Compose shutdown and later starts the project again
- **THEN** prior management resources, Agent identity, and retained object data remain available

#### Scenario: Operator requests a clean reset
- **WHEN** the operator runs the explicitly documented volume-removing reset command
- **THEN** all stand-owned durable state is removed and the next startup follows the clean bootstrap path

### Requirement: Health reports usable stand state
Each long-lived dependency SHALL provide a bounded health check, and startup dependencies SHALL use health or successful-completion conditions rather than timing sleeps. The gateway health surface SHALL distinguish process liveness from server readiness, and the documented verification SHALL prove the console, server readiness, Agent registration, and Microsandbox-backed job path.

#### Scenario: Object storage is not ready
- **WHEN** MinIO is running but its bucket initialization or required object lifecycle has not completed
- **THEN** the server remains unready and bootstrap and Agent admission do not proceed as if the stand were usable

#### Scenario: End-to-end verification succeeds
- **WHEN** all services are healthy and the operator runs the documented verification
- **THEN** it observes the UI entry point, ready server, registered Agent with virtualization inventory, and a terminal successful job executed by Microsandbox

### Requirement: Inputs are pinned and the development scope is explicit
Base images, Microsandbox, Octa, and package-manager inputs used by the Compose build SHALL be version-pinned and integrity-checked according to repository policy. Documentation and observable metadata SHALL identify the stack as a local development and integration environment; passing it SHALL NOT by itself claim release qualification, production hardening, backup coverage, or availability guarantees.

#### Scenario: Upstream artifact identity changes
- **WHEN** a downloaded Octa or Microsandbox artifact does not match its pinned version, source revision, manifest, or digest
- **THEN** image construction or startup fails before the server or Agent is reported ready

#### Scenario: Operator evaluates deployment guarantees
- **WHEN** the operator reads the local-stack documentation or diagnostics
- **THEN** the supported Docker/KVM prerequisites and non-production limitations are stated without implying that the Compose stand replaces the release Agent Ready matrix
