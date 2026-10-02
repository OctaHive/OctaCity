## Why

OctaCity currently requires operators to assemble PostgreSQL, S3-compatible storage, server configuration, Agent enrollment, the console proxy, and a qualified execution backend by hand. A single local Compose stack will provide a reproducible test environment on the project's Apple Silicon development machine while preserving the real Microsandbox virtualization boundary when Docker exposes usable KVM.

## What Changes

- Add one root-level Compose application that builds and starts PostgreSQL, MinIO, the OctaCity server, an idempotent bootstrap job, one Microsandbox Agent, and nginx serving the operator console and proxying the management API.
- Add purpose-built server, Agent, bootstrap, and console container images instead of embedding multiple long-lived responsibilities in one container.
- Generate and mount local-only credentials as files, create the object-storage bucket, create a dedicated Agent Pool, and issue the Agent's single-use enrollment credential without printing secrets.
- Gate Agent startup on an in-container Microsandbox preflight that verifies a usable `/dev/kvm`; fail clearly when Docker Desktop does not expose nested virtualization rather than falling back to a weaker backend.
- Persist PostgreSQL, MinIO, Microsandbox, and Agent state in named volumes, with health checks and dependency ordering that make `docker compose up --build` the sole startup command.
- Keep only nginx published for browser access; retain database, object storage, server management, Agent, and cache listeners on the private Compose network except where an explicitly documented local diagnostic port is required.
- Document supported-host prerequisites, first startup, readiness verification, reset semantics, troubleshooting, and the distinction between this development stack and release-qualified production deployment.

## Capabilities

### New Capabilities

- `deployment/local-compose-stack`: Defines the one-command local deployment topology, security boundaries, bootstrap lifecycle, Microsandbox prerequisite gate, persistence, health, and operator workflow.

### Modified Capabilities

- None.

## Impact

- Adds root-level Compose and container build assets, local deployment configuration/templates, bootstrap tooling, and operations documentation.
- Consumes the existing server REST enrollment API, PostgreSQL and S3 adapters, Agent configuration, Microsandbox backend, and the static console artifact without changing their public contracts.
- Introduces local container-image dependencies for PostgreSQL, MinIO, nginx, and the pinned Microsandbox runtime; all mutable upstream inputs require explicit version or digest policy.
- The stack targets Docker environments that can pass `/dev/kvm` to a Linux container. On the current Apple Silicon Mac, Docker Desktop nested-virtualization support is therefore an acceptance prerequisite, not an assumed capability.
