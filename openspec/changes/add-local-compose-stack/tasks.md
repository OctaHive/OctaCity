## 1. Host Gate and Immutable Inputs

- [ ] 1.1 Start Docker Desktop on the target Apple Silicon Mac and run the pinned official Microsandbox container with `--device /dev/kvm` and `doctor`; record the exact Docker, host, image, and preflight result, and do not claim this machine is supported unless the check passes.
- [ ] 1.2 Pin digest-qualified Linux ARM64 base images for Rust, Node, nginx, PostgreSQL, MinIO, the MinIO client, and Microsandbox plus the exact Octa version and source revision; verify a repository check rejects tags without the expected digest or release metadata.
- [ ] 1.3 Add a bounded `.dockerignore` and BuildKit-compatible build context that includes only OctaCity sources and lockfiles; verify the context excludes Git metadata, Rust/Node build outputs, local generated secrets, and unrelated developer files.

## 2. Container Build Products

- [ ] 2.1 Add the shared Rust builder and minimal non-root server runtime target; build with locked dependencies and verify the image runs `octacity-server --version` and `octacity-server validate` against a fixture configuration.
- [ ] 2.2 Add the Agent runtime target containing the built Agent and source plugin, the checksum-verified Linux ARM64 Octa release, and the pinned Microsandbox CLI and firmware; verify every manifest/digest and run the offline Agent installation/configuration validation fixture.
- [ ] 2.3 Generate the server JobSpec policy from the exact runner and plugin assets copied into the Agent image; verify a drift test fails if either image would use a different version, protocol, platform, or SHA-256.
- [ ] 2.4 Add the gateway build target using the locked `ui/` toolchain and a minimal nginx runtime; verify `pnpm build` succeeds and the runtime image contains static output but no source tree, package-manager store, or development server.

## 3. Local Trust and Configuration

- [ ] 3.1 Implement a bounded first-run initializer that creates distinct database, object-store, signing, enrollment, and cache credentials plus a local CA and gateway certificate only when absent; verify file type, ownership, modes, SANs, key separation, and idempotent reruns in automated tests.
- [ ] 3.2 Generate server configuration for private listeners, explicit trusted-network acknowledgement, PostgreSQL, the TLS object origin, signing/enrollment files, the TLS cache origin, and the generated JobSpec policy; verify both valid generation and fail-closed malformed/missing-secret cases with `octacity-server validate`.
- [ ] 3.3 Generate a Linux ARM64 Agent configuration with only the Microsandbox virtualization provider, distinct bounded roots, TLS Agent/cache/object origins, the derived server public key, conservative local resource limits, and no Host/Native/containerd fallback; verify it with `octacity-agent validate`.
- [ ] 3.4 Add narrowly scoped server and Agent entrypoints that install only the generated public CA, fix owned-volume permissions, and drop permanently to fixed non-root users; verify process identity, effective capabilities, mounted-secret visibility, and failure on unsafe credential permissions.

## 4. Gateway and Infrastructure Services

- [ ] 4.1 Configure nginx virtual hosts for `octacity.localhost`, `agent.localhost`, `cache.localhost`, and `objects.localhost`; verify SPA fallback is limited to console routes, unknown API/health paths remain upstream 404s, protocol hosts cannot reach management routes, and S3 requests retain signing-relevant host/path/query data.
- [ ] 4.2 Define PostgreSQL and MinIO services with pinned images, named volumes, bounded health checks, no host-published ports, and private credential files; verify data survives an ordinary Compose down/up cycle.
- [ ] 4.3 Add the idempotent MinIO bucket initializer and complete object-lifecycle readiness dependency; verify bucket creation can replay and the server remains unready while required PUT/GET/COPY/DELETE behavior is unavailable.

## 5. Server, Bootstrap, and Microsandbox Agent

- [ ] 5.1 Add the server service, private listener aliases, read-only configuration/secret mounts, health check, shutdown grace, and dependency conditions; verify only the loopback gateway port is published and both liveness and readiness propagate accurately through nginx.
- [ ] 5.2 Implement the bounded REST bootstrap client with stable idempotency identities, Linux ARM64 Pool admission, exact Pool-version enrollment, atomic private credential installation, registration-aware restart behavior, bounded retries, and secret-safe logs; verify clean start, lost-response replay, restart, corrupt state, and concurrent invocation tests.
- [ ] 5.3 Add the `agent-microsandbox` service with only `/dev/kvm`, required volumes, the pinned guest image, and no Docker socket or blanket privileged mode; verify its entrypoint runs Microsandbox preflight as the final Agent user before registration and exits clearly without advertising fallback capabilities on every preflight failure.
- [ ] 5.4 Assemble the single root `compose.yaml` with health-based and successful-init dependencies for all services; verify `docker compose config --quiet` succeeds, a policy test enumerates the expected services/networks/volumes, and no fixed startup sleep or unbounded restart loop exists.

## 6. Verification and Operations

- [ ] 6.1 Add portable CI checks that build every target, render Compose for ARM64, validate pins and configurations, scan committed and emitted logs for secret material, exercise gateway routing, and prove dependency services are not published to the host.
- [ ] 6.2 Add a KVM-capable integration gate that starts the clean Compose project, waits for declared health, verifies the UI and server readiness, observes one registered Linux ARM64 virtualization Agent, runs a minimal successful Microsandbox job through PostgreSQL and MinIO, restarts the stack without duplicate resources, and checks bounded cleanup.
- [ ] 6.3 Write the local-stand operations guide covering prerequisites, the single startup command, URLs, CA trust and removal, status/log inspection, readiness and job verification, ordinary shutdown, the explicitly destructive volume reset, KVM troubleshooting, and non-production limitations; verify every documented command against the target Mac.
- [ ] 6.4 Run formatting, linting, unit tests, Compose policy tests, container vulnerability/dependency checks, and the full local vertical slice; retain concise evidence and confirm the change adds no release-qualified or production claim beyond the new local deployment specification.
