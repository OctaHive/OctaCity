## Purpose

Defines a reproducible, launcher-managed local OctaCity deployment that combines an OrbStack Compose service plane with a native macOS Microsandbox Agent without presenting development conveniences as production guarantees.

## ADDED Requirements

### Requirement: One launcher manages the complete hybrid local stand
The repository SHALL provide one `tools/local-stand` launcher and one root-level Compose application. The launcher's `up` command SHALL initialize local state, build and start PostgreSQL, S3-compatible object storage, the OctaCity server, and the console gateway through Compose, perform bootstrap, start one native macOS Microsandbox Agent, and wait for usable stand state. Its `down` command SHALL gracefully stop that exact native Agent process and the Compose project without deleting durable state.

#### Scenario: Operator starts a clean stand
- **WHEN** an operator with supported OrbStack and native Microsandbox prerequisites runs `tools/local-stand up` against empty state
- **THEN** every required container is built or pulled, local state is initialized, bootstrap converges, the native Agent starts, and the launcher returns success only after the documented usable state is observed

#### Scenario: Operator restarts an initialized stand
- **WHEN** the operator runs `tools/local-stand down` and later `tools/local-stand up` without requesting reset
- **THEN** the server and Agent reuse durable state and bootstrap does not create duplicate logical resources or invalidate the current Agent registration

#### Scenario: Operator repeats a lifecycle command
- **WHEN** `up` is invoked for an already-running stand or `down` is invoked for an already-stopped stand
- **THEN** the launcher converges idempotently without starting duplicate Agents, killing an unrelated host process, or reporting a false success

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

### Requirement: Native Microsandbox capability is proven before Agent registration
The native Agent SHALL use the pinned Microsandbox virtualization backend on macOS. Before starting or registering the Agent, the launcher SHALL verify the runtime, firmware, host virtualization capability, and ability to initialize Microsandbox with `msb doctor`. It SHALL NOT silently fall back to Host, Native, containerd, Apple VF, or an unisolated process backend.

#### Scenario: Native Microsandbox preflight passes
- **WHEN** the pinned native Microsandbox installation and firmware pass `msb doctor` on the target Mac
- **THEN** the Agent starts with only the virtualization provider enabled and advertises the exact macOS host, Linux guest, runtime, and environment identity it actually provides

#### Scenario: Native Microsandbox preflight fails
- **WHEN** the runtime, firmware, or host virtualization capability is absent, inaccessible, or rejected by `msb doctor`
- **THEN** `up` fails with an actionable diagnostic, leaves no stale Agent ownership record, and registers no weaker execution capability

### Requirement: Bootstrap is idempotent and secret-safe
Bootstrap SHALL wait for server readiness through the gateway, create or recover one dedicated local Agent Pool through the management API, issue or replay one enrollment credential bound to that Pool version and expected native macOS Agent platform, and place the credential in an Agent-readable private host file. Repeated bootstrap with unchanged durable state SHALL converge on the same logical Pool and valid Agent identity. Enrollment credentials, registration credentials, signing material, object-store credentials, and presigned URLs SHALL NOT appear in launcher, Compose, or service logs.

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
All non-loopback origins accepted by the server or Agent SHALL use TLS. The stand SHALL generate local-only credentials in an operator-private host state directory, mount only each container's required subset, use distinct signing, enrollment, cache, database, and object-store secrets, and run long-lived OctaCity processes under dedicated non-root identities after any narrowly scoped initialization. No committed secret SHALL be suitable for a non-local deployment.

#### Scenario: Service configuration is validated
- **WHEN** the server and Agent validate their generated local configuration
- **THEN** every required credential is a private regular file, all configured service origins satisfy transport validation, and both the native Agent and containers trust only the local stand certificate authority in addition to their normal roots

#### Scenario: Repository is inspected for deployment secrets
- **WHEN** the Compose assets are checked out on another machine
- **THEN** they contain no generated private key, enrollment token, registration token, or object-store password from a previous run

### Requirement: State and destructive reset are explicit
PostgreSQL and object-store data SHALL use named volumes, while Agent credentials, Agent state, Microsandbox state, logs, and launcher ownership records SHALL use a private host state directory. Both storage classes SHALL survive ordinary `down` and `up` operations. The documented reset operation SHALL identify every named volume and host path it destroys and SHALL require an explicit destructive confirmation; ordinary startup and shutdown SHALL NOT erase durable state.

#### Scenario: Ordinary shutdown preserves state
- **WHEN** the operator runs `tools/local-stand down` and later `tools/local-stand up`
- **THEN** prior management resources, Agent identity, and retained object data remain available

#### Scenario: Operator requests a clean reset
- **WHEN** the operator runs the explicitly documented and confirmed launcher reset command
- **THEN** all stand-owned durable state is removed and the next startup follows the clean bootstrap path

### Requirement: Health reports usable stand state
Each long-lived container dependency SHALL provide a bounded health check, and startup dependencies SHALL use health or successful-completion conditions rather than timing sleeps. The launcher SHALL track the exact native Agent process with bounded readiness, shutdown, stale-state, and identity checks. The gateway health surface SHALL distinguish process liveness from server readiness, and verification SHALL prove the console, server readiness, Agent registration, and Microsandbox-backed job path.

#### Scenario: Object storage is not ready
- **WHEN** MinIO is running but its bucket initialization or required object lifecycle has not completed
- **THEN** the server remains unready and bootstrap and Agent admission do not proceed as if the stand were usable

#### Scenario: End-to-end verification succeeds
- **WHEN** all services are healthy and the operator runs the documented verification
- **THEN** it observes the UI entry point, ready server, registered Agent with virtualization inventory, and a terminal successful job executed by Microsandbox

### Requirement: Inputs are pinned and the development scope is explicit
Base images, source-built MinIO and mc revisions, the native Agent bundle, Microsandbox, Octa, and package-manager inputs used by the stand SHALL be version-pinned and integrity-checked according to repository policy. Documentation and observable metadata SHALL identify the stack as a local development and integration environment; passing it SHALL NOT by itself claim release qualification, production hardening, backup coverage, or availability guarantees.

#### Scenario: Upstream artifact identity changes
- **WHEN** a downloaded Agent, Octa, or Microsandbox artifact does not match its pinned version, source revision, manifest, or digest
- **THEN** staging, image construction, or startup fails before the server or Agent is reported ready

#### Scenario: Operator evaluates deployment guarantees
- **WHEN** the operator reads the local-stack documentation or diagnostics
- **THEN** the supported OrbStack/KVM prerequisites and non-production limitations are stated without implying that the Compose stand replaces the release Agent Ready matrix
