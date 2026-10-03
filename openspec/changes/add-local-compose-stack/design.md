## Context

See [proposal.md](proposal.md) for motivation and the [local Compose stack specification](specs/deployment/local-compose-stack/spec.md) for observable behavior. The target host is Apple Silicon macOS with OrbStack. OrbStack provides a Docker-compatible Linux ARM64 service environment but, by its documented design, cannot expose nested KVM on Apple Silicon. The already-supported native macOS Microsandbox provider instead uses the host virtualization framework and runs Linux ARM64 guests without weakening the isolation contract.

The repository currently packages host-native server and Agent archives but has no local Compose topology or unified stand lifecycle. The native Agent requires a separately versioned Linux ARM64 Octa runner bundle, a packaged source plugin, private enrollment state, the server signing public key, and pinned Microsandbox runtime and firmware. Server and Agent URL validation permits HTTPS for non-loopback services, while artifact transfer returns presigned URLs that must be reachable from both containers and the macOS host. The console builds to static Vite output and expects a trusted reverse proxy to keep management API calls same-origin.

## Goals / Non-Goals

**Goals:**

- Make `tools/local-stand up` and `tools/local-stand down` the normal lifecycle interface for the entire hybrid stand.
- Exercise the real Linux ARM64 server, REST bootstrap, native macOS Agent registration, Octa runner, source plugin, MinIO transfer, and Microsandbox virtualization path.
- Preserve listener, credential-file, signing, provider-neutral execution, and non-root container boundaries.
- Fail early and diagnostically when OrbStack, native Microsandbox, or staged release inputs are unavailable.
- Make rebuilds cacheable and ordinary stop/start cycles durable and idempotent.

**Non-Goals:**

- Production deployment, high availability, externally trusted TLS, backup automation, or remote exposure.
- Running Microsandbox inside OrbStack or claiming that Compose alone owns the native Agent lifecycle.
- Falling back to Host, Native, containerd, Apple VF, or simulated execution when Microsandbox is unavailable.
- Installing a persistent system-wide launch daemon or requiring administrator privileges for normal stand lifecycle.
- Replacing release-candidate packaging or the Agent Ready release gate.

## Decisions

### 1. Split the service plane and native execution plane behind one launcher

The root `compose.yaml` will contain `postgres`, `minio`, `minio-init`, `server`, and `gateway`. A repository-owned `tools/local-stand` launcher owns initialization, Compose invocation, management bootstrap, and one native Agent process. `up` starts Compose detached, waits on declared health, performs replay-safe bootstrap, starts the Agent in a separate process group, and waits for registration. `down` validates its ownership record, requests bounded graceful Agent shutdown, escalates only that verified process when necessary, and then runs ordinary `docker compose down`.

The launcher stores an exclusive lock, supervisor PID plus process-start identity, an authenticated private Unix control socket, logs, and generated state below one configurable private host directory. The supervisor remains the native Agent process-group leader until the group is empty; `down` asks that live owner to stop its own group and never calls `killpg` from a later launcher using a persisted numeric PID. This closes the PID-reuse check/signal race and lets forced shutdown account for descendants that outlive the Agent leader. Repeated `up` and `down` calls converge safely; stale ownership files are diagnosed and repaired without signalling an unrelated process. `status` and `logs` expose both lifecycle halves, while a separately confirmed `reset` removes named volumes and host state.

```text
tools/local-stand up
        │
        ├── initialize private host state and validate immutable inputs
        ├── docker compose up --build --detach
        │       ├── postgres ───────────────┐
        │       ├── minio ──> minio-init ──┼──> server ──> gateway
        │       └── gateway/TLS ────────────┘
        ├── bootstrap Pool and enrollment through gateway
        └── native Agent ──> agent/cache/object gateway origins
                    └── native Microsandbox ──> Linux ARM64 guest
```

Trying to launch a host process from a container or mounting the Docker socket into a supervisor was rejected because both expand privilege and make process ownership ambiguous. A persistent launchd service was rejected for the local stand because it adds machine-wide state and outlives the repository workflow.

### 2. Build narrow service images and stage one verified native execution bundle

A repository-owned multi-stage container build will produce narrow server and gateway targets. Shared build stages compile the server and the source plugin once; the gateway target builds `ui/` with locked Node and pnpm versions and copies only static output plus nginx configuration into its runtime image. The Rust builder receives a checksum-verified Octa source archive as a named build context, places it beside the OctaCity tree, and therefore satisfies the workspace's versioned path dependencies without a developer sibling checkout. MinIO Community and mc are built in repository-owned targets from their last official source releases, exact Git revisions, and digest-qualified Go and Alpine bases because upstream no longer maintains Community binary images. The launcher verifies local image architecture, source labels, runtime identity, and executable contract before reuse; missing or stale products share one staged source context per upstream source family rather than downloading the same archive once per target.

The launcher stages a native macOS Agent bundle, Microsandbox 0.7.6 runtime and firmware, the exact Linux ARM64 Octa release, and the source plugin into a versioned local installation directory. Every downloaded or built input is checked against version, source revision, manifest, platform, and SHA-256 metadata before activation. The server JobSpec policy is generated from the same runner and plugin manifests consumed by the native Agent, preventing signer/executor drift.

Alternatives considered were building from a developer-owned sibling Octa checkout, which couples the stand to one directory layout, and downloading mutable latest artifacts during every `up`, which makes restart network-dependent and unverifiable. The selected named context is staged from the revision URL and checksum in the immutable-input manifest; it is not the developer checkout and is safe to cache offline after verification.

### 3. Run the Agent natively with only the Microsandbox provider

The Agent runs as the invoking macOS user with separate state, work, cache, and log roots. Its generated configuration enables only the revision-2 Microsandbox virtualization provider, advertises a macOS ARM64 host and Linux ARM64 guest, and records the exact Microsandbox environment identity. The launcher runs the pinned `msb doctor` and Agent configuration validation before bootstrap can lead to registration.

The native Agent never receives the Docker socket and does not require `/dev/kvm`, `sudo`, or a privileged container. A failed preflight prevents Agent startup and registration; it never changes the advertised mode or starts an unisolated runner.

### 4. Put all routable service origins behind one TLS gateway

nginx will listen only on loopback-published host ports and use distinct `.localhost` hostnames on a shared TLS port:

- `octacity.localhost` serves the SPA and proxies management and health routes;
- `agent.localhost` proxies only the Agent protocol listener;
- `cache.localhost` proxies only the cache protocol listener;
- `objects.localhost` proxies the S3-compatible MinIO API.

The gateway receives these names as Compose network aliases, while macOS resolves them to loopback. This keeps presigned object URLs valid for the native Agent and internal service probes without publishing MinIO or raw server listeners. The host initializer creates a local CA and leaf certificate. Containers mount only the public CA and required key material; the native Agent trusts the same public CA in addition to normal roots. Optional browser trust remains a documented operator action, not an `up` side effect.

One unencrypted internal network was considered, but the existing server and Agent deliberately reject non-loopback HTTP endpoints. Weakening those validators would alter a production trust boundary, so the stand supplies TLS instead.

### 5. Generate private host state once and mount least-privilege subsets

The launcher creates its configurable state root outside the repository with owner-only directory permissions. It generates distinct database, MinIO, signing, enrollment, and cache credentials only when their target files do not already exist, and derives the signing public key needed by the Agent. Compose mounts only the subset required by each service, read-only wherever mutation is unnecessary. Because local Compose file-backed secrets cannot guarantee the server UID owns or can read each mounted file, a narrowly scoped root entrypoint uses only `CHOWN`, `DAC_OVERRIDE`, `SETGID`, and `SETUID` to copy the server's explicit secret allowlist into an owner-only tmpfs, assigns UID/GID 65532, and immediately replaces itself with the non-root server process. The Agent sees only its credential, public trust material, configuration, and its own mutable roots.

No credential is passed on a command line or logged. Compose interpolation is limited to non-secret paths, ports, and resource bounds. Committed `.env.example` content contains no functional credentials. A repository-local secret directory and static development passwords were rejected because they are easy to commit or reuse outside the local boundary.

### 6. Make host-side REST bootstrap replay-safe and registration-aware

The launcher waits for gateway readiness, then uses the server's explicitly acknowledged trusted-network management contract without sending a decorative credential. It submits a stable Pool definition with stable non-secret idempotency keys. Enrollment uses another stable identity and binds to the returned Pool version, macOS ARM64 host, and Linux ARM64 Microsandbox target. The response body is handled in memory or a protected staging file; only the credential is atomically installed without being echoed.

If an Agent registration credential already exists, bootstrap preserves it. If a first-run response was lost, replay with the same idempotency key recovers the original result. Concurrent launchers are excluded by the lifecycle lock. Bootstrap will be a bounded repository-owned program or script with fixtures and log-redaction tests; ad-hoc shell interpolation of JSON and secrets is not acceptable.

### 7. Separate durable container and host state with explicit reset semantics

Database and object bytes use distinct named volumes. Generated credentials, native Agent identity, Microsandbox state, work/cache roots, lifecycle metadata, and logs use distinct directories below the private host state root so permissions and cleanup can be checked independently. Ordinary `down` preserves both storage classes.

The launcher exposes an explicitly confirmed destructive reset that first performs a safe `down`, then removes the known Compose volumes and the resolved stand-specific host state directory. It refuses unresolved, root, home, repository-root, or otherwise broad deletion targets and lists the affected state before deletion.

Workspace and cache limits remain bounded in Agent configuration. The design does not claim hard quota enforcement beyond what the native macOS filesystem and existing Agent validators actually provide.

### 8. Validate portable structure and native behavior separately

Portable CI validates Compose rendering, pinned references, container builds, generated configurations, gateway routing, secret-log redaction, bootstrap fixtures, launcher state transitions, and absence of host-published dependency ports. Existing Apple Silicon self-hosted coverage validates the native Microsandbox contract. The target Mac runs the complete hybrid vertical slice through `tools/local-stand up`, including a real job, restart, and `down`.

## Risks / Trade-offs

- **[The stand spans container and host process lifecycles]** → Hide the sequencing behind one locked launcher, route native shutdown through the still-live process-group supervisor, and provide unified status and logs.
- **[A generated local CA is not automatically trusted by the host browser]** → Export the public CA, document optional Keychain trust and removal, and retain CLI health verification with the explicit CA.
- **[Host and container callers share gateway origins]** → Use `.localhost` names, Compose aliases, and routing tests that cover both sides and preserve S3 signing inputs.
- **[Native runtime downloads can drift or disappear]** → Pin version, revision, platform, digest, and manifest; stage atomically into versioned directories and reuse verified installations offline.
- **[Native state can outlive Compose volumes]** → Put all state below one resolved root and make destructive reset enumerate and remove both halves only after confirmation.
- **[Current console implementation is still evolving]** → Build the checked-in `ui/` state and test the proxy contract independently; the stand does not expand UI feature scope.

## Migration Plan

1. Replace the failed nested-KVM assumption with a pinned native Microsandbox preflight on the target Mac.
2. Add and validate pinned container build targets plus the native execution bundle without changing release packages.
3. Add host-state initialization, the single launcher, Compose topology, replay-safe bootstrap, health checks, and operations guide.
4. Run portable structural and security checks plus the existing native backend contract.
5. Run the full hybrid local vertical slice through `up`, restart, and `down`, retaining concise diagnostic evidence.

Rollback stops the verified native Agent process and removes the launcher and Compose assets. Durable state remains recoverable unless the operator explicitly executes the confirmed destructive reset.
