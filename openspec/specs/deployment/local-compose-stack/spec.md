# Local Compose Stack Specification

## Purpose

Defines a reproducible, launcher-managed local OctaCity deployment that combines an OrbStack Compose service plane with a native macOS Microsandbox Agent without presenting development conveniences as production guarantees.

## Requirements

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

### Requirement: Factory stand advertises only composed automatic-intake capability
The supported `tools/factory-stand` path SHALL load compatible trusted Source observation/Issue consumer, optional Factory preparation and optional Reporter/Issue-delivery settings into normal released startup. Selections SHALL derive from explicit installed policy, managed resources and Project configuration, not fixed repositories/Project IDs, credential bytes or browser-supplied private settings. Repeated initialization of a compatible current immutable bundle SHALL retain credentials, Issue workflow pins/mappings, Runtime/Profile choices and history without reset. Earlier metadata-only bundles belong to the pre-release test stand: this change SHALL refuse mismatched native pins without rewriting them and SHALL NOT claim an automatic upgrade of that installation; testing the new bundle uses an explicitly selected fresh disposable root while preserving the earlier stand. Connection setup/grants SHALL not activate import, execution or synchronization. Scoped readiness SHALL distinguish optional absence from advertised-but-omitted protected routes/consumers and SHALL require no live provider call/write at startup. Source-only task import/status push SHALL not require a Factory execution target or unrelated MCP execution setup; provider outage SHALL not block manual task/CI readiness.

#### Scenario: Supported Source-only stand starts with provider unavailable
- **WHEN** installed Source support starts without an enabled import/input and the provider is unreachable
- **THEN** protected choices/catalog/history and retained task/synchronization reads work, consumer installation is reported truthfully and no task, Work or remote effect is created implicitly

#### Scenario: Advertised source handler or consumer is omitted
- **WHEN** an advertised import, Factory input or Issue/Work Reporter installation lacks required composition
- **THEN** its scoped readiness/qualification fails with actionable state instead of relying on health or connection checks

#### Scenario: Retained stand restarts
- **WHEN** the stand restarts with credentials, independent import/input controls, canonical Issues, workflow/mapping pins, Work/Run and pending effects
- **THEN** private initialization succeeds, existing ownership recovers and exact retained state resumes without duplicate tasks/execution or replacement of operator Runtime/Profile settings

### Requirement: Packaged external-task import and reporting have end-to-end qualification
Qualification SHALL exercise the real browser/gateway/normal server startup, PostgreSQL/object storage, verified adapter process and supervised consumers against a hermetic approved provider fixture. It SHALL first import one native task into a visible canonical Issue without Factory configuration or execution, then review an installed status mapping and deliver an authorized manual Issue-status effect through the existing ordered projection. Independently enabled Factory inputs or explicit execution SHALL retain canonical association and original input-scoped Work/Run/results and configured action provenance. Restart/replay, separate consumer/grant disablement, read-only/no-card variants and manual independence SHALL be verified with reused fixtures and controlled boundaries. Browser REST interception or direct use-case/alternate-startup demonstrations alone SHALL NOT qualify the packaged path. Optional live reads SHALL be separate evidence and live writes SHALL require an explicitly authorized disposable target.

#### Scenario: Source-only import and manual status delivery pass
- **WHEN** the actual Console selects the approved fixture resource, previews/enables Project import and reviews a supported push mapping for a permitted manual transition
- **THEN** one native-linked Issue is visible and the exact fixture status effect is delivered without a Factory Configuration, Work or Run
- **AND** repeated reads and a controlled restart preserve canonical identity, workflow/mapping pins and bounded effect history

#### Scenario: Optional execution follows the same native task
- **WHEN** independently enabled Factory inputs process the imported task with valid preparation and qualified execution settings
- **THEN** their distinct original Work/Run and retained results correlate to the same Issue, with no import-triggered execution or duplicate/overlapping status producer

#### Scenario: Browser requests reach omitted composition
- **WHEN** mocked component tests would pass but normal deployed Source/import/delivery composition is missing
- **THEN** packaged qualification fails rather than substituting browser responses, constructing an Issue through an internal test call or reporting a checked connection as completion

### Requirement: Local Factory Runtime setup supplies usable bounded MCP execution
The local Factory stand SHALL provide an explicit documented setup path for operator-approved Runtime installation capabilities and independent worker policy using the existing installation contracts. After that setup, authorized operators SHALL be able to configure HTTP and stdio MCP servers and finite permissions in Console, select their exact bindings in an AI-agent profile and execute them through ordinary Builds in Microsandbox. Stdio choices SHALL identify verified installed guest executables with explicit argument and environment contracts; a host path or executable digest alone SHALL NOT imply guest availability. HTTP endpoints SHALL remain subject to the approved network and credential boundaries. Browser settings SHALL NOT install programs, widen installation/Agent/backend grants, import ambient workstation settings or trigger execution. Model names and reasoning choices SHALL remain supplied by plugins. Without explicit execution setup, the existing model-authoring/manual-intake path SHALL remain usable and execution prerequisites SHALL be reported honestly.

The selected AI plugin SHALL supply reviewed MCP connections to its Agent-side executor's native client in that executor's supported format. Compatible ready-made servers SHALL retain their native tools and schemas without an OctaCity provider wrapper or additional MCP client. Unselected and ambient workstation connections SHALL remain unavailable. Unsupported required plugin, transport or authentication capabilities SHALL block execution without a fallback server. The explicit installation path SHALL use the selected complete catalog and independent Agent policy without merging generated demo authority or creating demonstration Runs; ordinary no-selection startup SHALL retain its existing behavior.

#### Scenario: Operator configures HTTP MCP
- **WHEN** the stand has independently approved network and execution policy and the operator publishes an allowed HTTP MCP and selects it in a profile
- **THEN** an explicitly started ordinary Build invokes the selected endpoint under those bounds without installing another Runtime or requiring an unrelated provider integration

#### Scenario: Operator configures stdio MCP
- **WHEN** the operator selects a qualified installed stdio program, supported arguments and mapped environment names in Runtime settings and then selects that exact binding in a profile
- **THEN** ordinary materialization makes the verified executable available at its declared guest path and executes it without a shell or ambient credentials

#### Scenario: Requested permission exceeds the installed policy
- **WHEN** Runtime or profile settings request an unapproved executable, network host or resource permission
- **THEN** validation or dispatch rejects the unsupported selection without replacing independent worker policy or silently enabling broader access

#### Scenario: Operator supplies a complete execution installation
- **WHEN** the Factory stand command selects an operator-owned catalog, optional Runtime bundle and independently approved Agent policy
- **THEN** the ordinary installer uses those explicit inputs, rejects invalid selections before service activation and creates no extra demonstration Run or implicit provider connection

### Requirement: MCP connections retain independent credentials and enforced access
Each authenticated MCP connection SHALL select an authorized token/secret reference, credential purpose and resource/access scope through existing owners, separate from AI-provider login and server Source/Reporter authority. Secret values SHALL be supplied only to the intended client/server process or approved transport for the authorized execution and SHALL NOT appear in published settings, prompts, Context, public results or diagnostics. Unrelated MCP child processes SHALL NOT inherit one another's credentials. Read-only/resource limits SHALL use actual server/tool and credential enforcement rather than model instructions or annotations alone. Missing or revoked local authority and unsupported authentication SHALL block newly unauthorized use without ambient or other-connection fallback; remote revocation SHALL follow the selected server's enforcement contract. Server Source intake and Reporter delivery SHALL remain independent of agent MCP investigation.

#### Scenario: Two MCP connections use different read credentials
- **WHEN** one execution selects HTTP and stdio MCP connections with distinct approved credential references and resource scopes
- **THEN** each connection receives only its intended credentials, reads only permitted resources and cannot invoke a write using AI-provider or Reporter authority

#### Scenario: A connection loses authorization
- **WHEN** its local credential/connection authority is revoked or its required authentication method is unsupported
- **THEN** newly unauthorized execution is blocked with a safe reason and no fallback token, server or ambient configuration is selected

### Requirement: Native MCP execution produces bounded retained evidence
The qualified AI plugin/native-client path SHALL expose actual MCP calls and consumed results through ordinary invocation events/results and Artifact provenance, identifying connection/tool, arguments, observation time, content identity and available scope/coverage. Missing coverage SHALL remain unknown, and absent native revisions SHALL NOT be invented. Failed, truncated, unavailable and cancelled calls SHALL remain distinct from successful empty results. Execution SHALL obey configured deadlines and bounds, and cancellation SHALL stop or terminate the owned execution through its supported lifecycle rather than leave unbounded tool activity. Model assertions alone SHALL NOT establish tool execution. Missing required native capture or enforcement support SHALL be an explicit capability blocker. Evidence SHALL contain no secret values and SHALL NOT require a new tool journal or provider-specific response parser.

#### Scenario: The executor searches through a finite MCP peer
- **WHEN** a qualified native client performs permitted tool calls and a later call is cancelled or fails
- **THEN** ordinary retained evidence distinguishes consumed results and their exact invocation from the interrupted call without treating failure as empty search or exposing credentials

#### Scenario: A stopped execution has unknown terminal consumption
- **WHEN** a failed or cancelled native execution retains capture but cannot supply trustworthy terminal usage
- **THEN** its exact execution and budget reservation remain waiting without a semantic result or invented zero consumption, while other Runs can still reconcile and stop their owned executions
- **AND** later verified consumption can settle the original execution without dispatching another attempt

#### Scenario: A selected AI profile is cancelled during an MCP call
- **WHEN** the operator cancels an admitted native execution with an explicitly selected immutable AI profile
- **THEN** the owner validates the original selected intent and stops its already accepted ordinary Build without changing the profile, recreating a static intent or dispatching another attempt

### Requirement: Local Runtime setup updates preserve operator state
Applying or replacing an explicitly selected Runtime installation SHALL reuse existing private-file verification, bounded staging and lifecycle mechanisms. Failed preflight SHALL leave the previous usable installation unchanged. Successful setup and ordinary restart SHALL retain PostgreSQL settings/receipts, exact transport and role/profile history, admitted execution content and worker identity without destructive reset or replaying demonstration Work. Installation changes SHALL NOT adopt new model, transport or permission selections into existing profiles automatically; stale or revoked pins SHALL follow the existing qualification and revocation rules.

#### Scenario: Operator enables execution on an existing stand
- **WHEN** valid explicit Runtime and worker installation inputs are applied to a stand containing saved AI agents, profiles and reviewed Runtime settings
- **THEN** the stand preserves its durable state, exposes the newly qualified choices and requires explicit adoption into future profile versions

#### Scenario: Installation preflight fails
- **WHEN** an executable identity, permission contract or worker setup is incompatible
- **THEN** setup reports the failed prerequisite before replacing or stopping the usable installation and does not reset the stand
