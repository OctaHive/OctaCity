## 1. Host Gate and Immutable Inputs

- [x] 1.1 Run the pinned native Microsandbox 0.7.6 runtime and firmware on the target Apple Silicon Mac with `msb doctor`; record the exact host, runtime, firmware, and preflight result, and do not claim this machine is supported unless the check passes. The verified native preflight passes; the earlier OrbStack/KVM rejection is retained as the reason for the hybrid boundary ([evidence](evidence/host-preflight.md)).
- [x] 1.2 Pin digest-qualified Linux ARM64 base images for Rust, Node, nginx, PostgreSQL, Go, and Alpine; pin the last official MinIO and mc source releases and revisions plus the exact native Microsandbox, Agent, and Octa metadata; verify a repository check rejects images, sources, or native artifacts without the expected digest, checksum, platform, revision, and release metadata ([evidence](evidence/immutable-inputs.md)).
- [x] 1.3 Add a bounded `.dockerignore` and staging allowlist that include only required OctaCity sources, lockfiles, and native bundle inputs; verify they exclude Git metadata, Rust/Node build outputs, generated secrets, local stand state, and unrelated developer files ([evidence](evidence/bounded-contexts.md)).

## 2. Container and Native Build Products

- [x] 2.1 Add the shared Rust builder with the checksum-verified Octa source archive staged as its named sibling context, plus a minimal non-root server runtime target; build with locked dependencies and no developer checkout, and verify the image runs `octacity-server --version` and `octacity-server validate` against a fixture configuration.
- [x] 2.2 Add atomic staging for the native macOS Agent, source plugin, checksum-verified Linux ARM64 Octa release, and pinned Microsandbox runtime and firmware; verify every manifest/digest and run offline Agent installation and configuration validation fixtures.
- [x] 2.3 Generate the server JobSpec policy from the exact runner and plugin assets staged for the native Agent; verify a drift test fails if signer and executor would use different versions, protocols, platforms, or SHA-256 values.
- [x] 2.4 Add the gateway build target using the locked `ui/` toolchain and a minimal nginx runtime; verify `pnpm build` succeeds and the runtime image contains static output but no source tree, package-manager store, or development server.

## 3. Host State, Trust, and Configuration

- [x] 3.1 Implement a bounded host initializer that creates the resolved private stand root, distinct database, object-store, signing, management-bootstrap, enrollment, and cache credentials, plus a local CA and gateway certificate only when absent; verify file type, ownership, modes, SANs, key separation, unsafe-path rejection, and idempotent reruns.
- [x] 3.2 Generate server configuration for private listeners, explicit trusted-network acknowledgement, PostgreSQL, the TLS object origin, signing/enrollment files, the TLS cache origin, and the generated JobSpec policy; verify valid generation and fail-closed malformed or missing-secret cases with `octacity-server validate`.
- [x] 3.3 Generate a native macOS ARM64 Agent configuration with only the Microsandbox virtualization provider, distinct bounded roots, TLS Agent/cache/object origins, the derived server public key, conservative local resource limits, and no Host/Native/containerd/Apple-VF fallback; verify it with `octacity-agent validate`.
- [x] 3.4 Implement reusable lifecycle state primitives for an exclusive lock, atomic PID/start-identity record, bounded process readiness and shutdown, stale-state repair, and log paths; verify duplicate `up`, repeated `down`, stale PID, PID reuse, concurrent invocation, and forced-stop tests never signal an unrelated process.

## 4. Gateway and Infrastructure Services

- [x] 4.1 Configure nginx virtual hosts for `octacity.localhost`, `agent.localhost`, `cache.localhost`, and `objects.localhost`; verify SPA fallback is limited to console routes, unknown API/health paths remain upstream 404s, protocol hosts cannot reach management routes, and S3 requests retain signing-relevant host/path/query data from both host and Compose callers.
- [x] 4.2 Define PostgreSQL and source-built MinIO services with pinned inputs, named volumes, bounded health checks, no host-published ports, and private credential files; verify data survives an ordinary launcher `down`/`up` cycle.
- [x] 4.3 Add the idempotent source-built mc bucket initializer and complete object-lifecycle readiness dependency; verify bucket creation can replay and the server remains unready while required PUT/GET/COPY/DELETE behavior is unavailable.

## 5. Server, Bootstrap, Native Agent, and Launcher

- [ ] 5.1 Add the server service, private listener aliases, read-only configuration and secret mounts, health check, shutdown grace, and dependency conditions; verify only loopback gateway ports are published and both liveness and readiness propagate accurately through nginx.
- [ ] 5.2 Implement the bounded host-side REST bootstrap client with file-based management authentication, stable idempotency identities, macOS ARM64 Pool admission, Linux ARM64 Microsandbox target, exact Pool-version enrollment, atomic private credential installation, registration-aware restart behavior, bounded retries, and secret-safe logs; verify clean start, lost-response replay, restart, corrupt state, and concurrent invocation tests.
- [ ] 5.3 Implement native Agent startup that validates staged inputs and configuration, runs pinned `msb doctor`, launches only the Microsandbox provider in its own process group, writes atomic lifecycle ownership, and waits for server-observed registration; verify every preflight and startup failure leaves no stale ownership record or fallback capability.
- [ ] 5.4 Assemble `tools/local-stand up|down|status|logs|reset` and the root `compose.yaml`; verify `up` converges the complete hybrid stand, `down` is graceful and state-preserving, `status` and `logs` cover both lifecycle halves, `reset` requires explicit confirmation, `docker compose config --quiet` succeeds, and no fixed startup sleep or unbounded restart loop exists.

## 6. Verification and Operations

- [ ] 6.1 Add portable CI checks that build every container and native staging target, render Compose for ARM64, validate pins and configurations, scan committed and emitted logs for secret material, exercise gateway and launcher state fixtures, and prove dependency services are not published to the host.
- [ ] 6.2 Add an Apple Silicon integration gate that runs `tools/local-stand up` from clean state, waits for declared health, verifies the UI and server readiness, observes one registered native macOS ARM64 Agent with a Linux ARM64 Microsandbox target, runs a minimal successful job through PostgreSQL and MinIO, repeats `up`, restarts the stand without duplicate resources, runs `down`, and checks bounded cleanup.
- [ ] 6.3 Write the local-stand operations guide covering prerequisites, `up`, `down`, `status`, logs, URLs, CA trust and removal, readiness and job verification, the explicitly destructive reset, native Microsandbox troubleshooting, and non-production limitations; verify every documented command against the target Mac.
- [ ] 6.4 Run formatting, linting, unit tests, Compose policy tests, container/native dependency checks, and the full hybrid local vertical slice; retain concise evidence and confirm the change adds no release-qualified or production claim beyond the local deployment specification.
