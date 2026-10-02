## Context

See [proposal.md](proposal.md) for motivation. The current development host is Apple Silicon macOS with the Docker CLI installed. Docker containers therefore run as Linux ARM64 processes inside Docker Desktop's VM rather than as macOS processes. Microsandbox supports execution inside a Linux container only when that environment exposes a usable `/dev/kvm`; the physical Mac's virtualization support alone is insufficient.

The repository currently packages host-native server and Agent archives but has no container images or Compose topology. The Agent also requires a separately versioned Octa runner bundle, a packaged source plugin, private enrollment state, and the server signing public key. Server and Agent URL validation permits HTTPS for non-loopback services, while artifact transfer returns presigned URLs that must be reachable by the Agent and potentially by the host browser. The console builds to static Vite output and expects a trusted reverse proxy to keep management API calls same-origin.

## Goals / Non-Goals

**Goals:**

- Make `docker compose up --build` the only command needed to initialize and run every stand component.
- Exercise the real Linux ARM64 server, REST bootstrap, Agent registration, Octa runner, source plugin, MinIO transfer, and Microsandbox virtualization path.
- Preserve the existing listener, credential-file, signing, and provider-neutral execution boundaries.
- Make unsupported Docker/KVM environments fail early and diagnostically.
- Keep rebuilds cacheable and ordinary restarts durable.

**Non-Goals:**

- Production deployment, high availability, externally trusted TLS, backup automation, or remote exposure.
- Claiming the macOS host-provider matrix: the Agent runs inside Docker's Linux VM and truthfully advertises a Linux host platform.
- Falling back to Host, Native, containerd, Apple VF, or simulated execution when Microsandbox is unavailable.
- Replacing release-candidate packaging or the Agent Ready release gate.

## Decisions

### 1. Use one root `compose.yaml` with separate single-purpose services

The stack will contain `postgres`, `minio`, `minio-init`, `secrets-init`, `gateway`, `server`, `bootstrap`, and `agent-microsandbox`. Init and bootstrap services terminate successfully; long-lived services remain one process per container. Named volumes own database, object, generated-secret, Agent, and Microsandbox state.

This satisfies the one-file and one-command requirement without combining unrelated daemons into a supervisor container. A Linux overlay file was rejected because the requested acceptance surface is the current Mac and one Compose application.

The dependency graph is:

```text
secrets-init ───────────────┐
                            v
postgres ───────────────> server ─────> bootstrap ─────> agent-microsandbox
minio ──> minio-init ────> server             ^                    ^
     gateway/TLS ─────────> server             │                    │
                            └── readiness ──────┘          /dev/kvm preflight
```

Compose health and `service_completed_successfully` conditions will express this graph. Fixed sleeps are not startup coordination.

### 2. Build repository-owned images from one multi-target container build

A repository-owned multi-stage build will produce narrow server, bootstrap, Agent, and gateway targets. Shared Rust stages build the server, Agent, and source plugin once. The Agent target also stages the exact Linux ARM64 Octa release named by the repository's pinned version and source revision, validates its release contract and checksums, and copies the pinned Microsandbox runtime and firmware from its immutable upstream image. The server target receives a JobSpec policy generated from the same staged runner and plugin manifests, preventing image drift between signer and executor.

The gateway target builds `ui/` with its locked Node and pnpm versions, then copies only static output and nginx configuration into the runtime image. Production release archives remain unchanged; these images are local deployment products.

Alternatives considered were building from a sibling Octa checkout, which would couple Compose to one developer's directory layout, and downloading mutable latest artifacts at container startup, which would make restarts network-dependent and unverifiable.

### 3. Treat the Compose Agent as Linux ARM64 and require KVM

`agent-microsandbox` maps `/dev/kvm` with Compose `devices` and does not receive blanket `privileged` mode or the Docker socket. A small root entrypoint may install the generated local CA and grant the dedicated Agent group access to the mapped device; it then runs `msb doctor` as the Agent user and permanently drops privileges before configuration validation and Agent startup. Its configuration enables only the revision-2 Microsandbox virtualization provider.

The preflight checks the device, read/write access, pinned CLI and firmware, and Microsandbox initialization. Failure terminates the Agent service with remediation that points to Docker Desktop nested virtualization. It never changes the advertised mode or starts an unisolated runner.

Although the physical machine is macOS, the Agent's observable host is Docker's Linux ARM64 VM. Pool admission and bootstrap therefore bind the enrollment to Linux ARM64, and the environment identity records the exact Microsandbox and Linux guest image family.

### 4. Put all routable origins behind one TLS gateway

nginx will listen only on loopback-published host ports and use distinct `.localhost` hostnames on a shared TLS port:

- `octacity.localhost` serves the SPA and proxies management and health routes;
- `agent.localhost` proxies only the Agent protocol listener;
- `cache.localhost` proxies only the cache protocol listener;
- `objects.localhost` proxies the S3-compatible MinIO API.

The gateway receives these names as Compose network aliases, so server and Agent containers resolve the same origins internally that the host resolves to loopback. This keeps presigned object URLs valid without exposing MinIO directly or rewriting signed requests. A first-run init service generates a local CA and leaf certificate into a private volume. Server and Agent entrypoints add only the public CA to their container trust stores before dropping privileges. The generated CA certificate is exported to a documented host path for optional browser trust; accepting or trusting that local CA is not required for non-browser health verification.

One unencrypted internal network was considered, but the existing server and Agent deliberately reject non-loopback HTTP endpoints. Weakening those validators for Compose would alter a production trust boundary, so the stand supplies TLS instead.

### 5. Generate secrets once and expose them as private files

`secrets-init` creates distinct random database, MinIO, signing, Agent-enrollment, and cache keys only when their target files do not already exist. It also derives the signing public key needed by the Agent and writes server configuration inputs with fixed service identities. The generated-secret volume is not bind-mounted into the repository. Long-lived processes receive only their required subset through read-only mounts, with ownership and modes satisfying existing validators.

No credential is passed on a command line or logged. Compose interpolation is limited to non-secret tuning values. Committed `.env.example` content may select ports and resource bounds but contains no functional credentials.

Static well-known development passwords were rejected because they tend to escape the local boundary and conflict with the repository's credential-file rules.

### 6. Make REST bootstrap replay-safe and registration-aware

The bootstrap image uses the published management contract. It waits for `/health/ready`, then submits a stable Pool definition with stable idempotency keys. Enrollment uses another stable key and is bound to the returned Pool version and Linux ARM64 platform. Bootstrap writes the response body directly to a protected staging file, extracts only the credential without echoing it, and atomically installs the Agent configuration and credential.

The Agent owns the credential file after bootstrap and may replace enrollment material with its registration credential. On restart, bootstrap detects the existing private credential and does not overwrite it. If a first-run response was lost, replay with the same idempotency key recovers the server's original result. Removing all stand volumes deliberately resets both authoritative server state and Agent identity.

Bootstrap will be implemented as a bounded repository-owned program or script with fixtures and log-redaction tests; ad-hoc shell interpolation of JSON and secrets is not acceptable.

### 7. Separate durable volumes and publish explicit reset semantics

Database rows, object bytes, Microsandbox state, Agent state/cache/work roots, and generated credentials use distinct volumes so permissions, capacity, and cleanup can be checked independently. Ordinary `docker compose down` preserves them. The documented clean reset is `docker compose down --volumes` and explicitly lists that it destroys local Builds, object data, Agent identity, and generated trust material.

Workspace and cache limits remain bounded in Agent configuration. The implementation will verify whether Docker Desktop volume capacity can satisfy the configured maximum and choose conservative local defaults rather than claiming hard quota enforcement that the platform does not provide.

### 8. Validate structure portably and behavior on a KVM-capable runner

Portable CI validates Compose rendering, pinned image references, container builds, configuration generation, nginx routing, secret-log redaction, idempotent bootstrap fixtures, and absence of host-published dependency ports. A KVM-capable ARM64 environment runs the real stack, asserts all health conditions, inspects Agent inventory, submits a minimal virtualization job, and verifies cleanup. The current Mac must pass the same preflight before it is accepted as a working local stand.

## Risks / Trade-offs

- **[Docker Desktop may not expose `/dev/kvm` on this Mac]** → Make the official in-container preflight the first acceptance gate and fail the Agent honestly; no Compose setting or `privileged` flag is presented as a workaround.
- **[A generated local CA is not automatically trusted by the host browser]** → Keep startup fully automated, export the public CA, document optional Keychain trust and removal, and retain CLI health verification that uses the exported CA explicitly.
- **[Hairpin routing through nginx adds overhead to object and cache traffic]** → Accept it for the bounded local stand because it preserves one reachable signed origin and production-equivalent TLS validation; measure the vertical slice rather than claim production throughput.
- **[Container builds must obtain the separately released Octa bundle]** → Pin version, revision, platform, checksum, and manifest; use BuildKit cache and fail closed on any mismatch.
- **[Root is briefly needed for CA and KVM device setup]** → Keep that work in a small audited entrypoint, drop to fixed non-root identities before validation or networking, remove unnecessary capabilities, and never mount the Docker socket.
- **[Current console implementation is still evolving]** → Build the checked-in `ui/` state and test the proxy contract independently; the stack does not expand the UI feature scope.

## Migration Plan

1. Add and validate pinned container build targets and generated configuration fixtures without changing existing release packages.
2. Add the single Compose topology, init/bootstrap lifecycle, health checks, and local operations guide.
3. Run portable structural and security checks.
4. Start Docker Desktop on the target Mac and run the `/dev/kvm` plus `msb doctor` gate.
5. If the gate passes, run the full local vertical slice and retain diagnostic evidence. If it fails, keep the core diagnosis visible and do not mark the change complete for this machine.

Rollback removes the Compose assets and local images. Named volumes remain recoverable unless the operator explicitly executes the documented destructive reset.
