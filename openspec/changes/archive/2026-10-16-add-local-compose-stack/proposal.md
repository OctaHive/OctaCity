## Why

OctaCity currently requires operators to assemble PostgreSQL, S3-compatible storage, server configuration, Agent enrollment, the console proxy, and a qualified execution backend by hand. OrbStack cannot expose nested KVM on the target Apple Silicon Mac, so a useful local stand must keep the service plane in Compose while running the real Microsandbox Agent natively on macOS.

## What Changes

- Add one root-level Compose application that builds and starts PostgreSQL, MinIO, the OctaCity server, initialization jobs, and nginx serving the operator console and proxying every public stand endpoint.
- Add one repository-owned `tools/local-stand` launcher whose `up` command initializes the stand, starts Compose, performs idempotent bootstrap, starts one native macOS Microsandbox Agent, and waits for registration; its `down` command gracefully stops the Agent and Compose without deleting state.
- Stage pinned, integrity-checked native Agent, Microsandbox 0.7.6, firmware, Linux ARM64 Octa runner, and source-plugin inputs instead of running Microsandbox inside OrbStack.
- Generate local-only credentials as private host files, create the object-storage bucket and dedicated Agent Pool, and issue the Agent's single-use enrollment credential without printing secrets.
- Gate native Agent startup on `msb doctor` and configuration validation; fail clearly rather than falling back to a weaker backend.
- Persist PostgreSQL and MinIO in named volumes and native Agent, Microsandbox, and credential state in a private host directory.
- Keep only nginx published for browser and native-Agent access; database, object storage, and raw server listeners remain private to the Compose network.
- Document prerequisites, `up`, `down`, status/log inspection, explicit destructive reset, troubleshooting, and the distinction between this development stand and release-qualified production deployment.

## Capabilities

### New Capabilities

- `deployment/local-compose-stack`: Defines the launcher-managed hybrid local deployment, security boundaries, bootstrap lifecycle, native Microsandbox prerequisite gate, persistence, health, and operator workflow.

### Modified Capabilities

- None.

## Impact

- Adds root-level Compose and container build assets, a native stand launcher, local deployment configuration/templates, bootstrap tooling, and operations documentation.
- Consumes the existing server REST enrollment API, PostgreSQL and S3 adapters, Agent configuration, native macOS Microsandbox backend, and static console artifact without changing their public contracts.
- Introduces local container-image dependencies, source-built MinIO and mc inputs, plus pinned native Agent, Microsandbox, Octa runner, and plugin inputs; all mutable upstream inputs require explicit version, revision, checksum, or digest policy.
- The stack targets the current Mac's OrbStack Docker-compatible engine for services and macOS virtualization for execution. It does not claim that Microsandbox runs inside OrbStack or that the stand is a single container lifecycle.
