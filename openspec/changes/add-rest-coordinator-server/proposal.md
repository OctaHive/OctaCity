## Why

OctaCity has a production-oriented agent and a complete outbound agent protocol, but no durable control plane that can model pipelines, trigger builds, orchestrate job graphs, fence leases, persist events, or expose operator workflows. A modular self-hosted server is required both to close the Agent Ready release gate and to provide a stable core that can accept REST now and additional management transports later.

## What Changes

- Reorganize the workspace around four product areas: `cli`, `server`, `agent`, and `shared`, with shared code limited to stable contracts used across product boundaries.
- Add a modular Rust server whose API, application/core, and infrastructure layers have enforced one-way dependencies and whose composition root selects concrete adapters.
- Expose a versioned JSON REST management interface and OpenAPI document as an adapter over transport-independent CQRS commands and queries.
- Model versioned pipelines as validated DAGs and model execution explicitly as `Project -> Build Configuration -> Build -> Attempt -> Job`, where an attempt materializes one or more dependent jobs.
- Add a durable trigger engine for manual, scheduled, external, and internal events; keep trigger evaluation separate from placement scheduling.
- Add a server orchestrator that advances build, attempt, and pipeline state machines while agents remain the only executors of repository-controlled build code.
- Group every agent into exactly one pool and place ready jobs from one durable global queue onto compatible accepting agents using expiring fenced leases.
- Separate provider-neutral webhook ingestion from provider-neutral VCS access. Support manually configured webhooks and managed provider integrations, and add a versioned VCS protocol with Git as the first implementation.
- Add explicit modules and interfaces for secret-store integration, cache authority, artifact publication, audit, and server/agent observability.
- Treat each Build Result as one logical aggregate of its immutable execution-configuration snapshot, ordered build log, and produced files and reports while allowing those components to use different physical stores.
- Add bounded full-text and literal search over redacted build logs, with project, Build, Attempt, Job, stream, and time filters, deterministic cursor pagination, context snippets, index freshness, and rebuild support.
- Keep PostgreSQL as the first authoritative state-store adapter and S3-compatible storage as the first artifact-byte adapter without exposing either technology through core interfaces.
- Keep management REST unauthenticated in the first trusted-network release. Agent enrollment and registration credentials, webhook signature verification, JobSpec signing, and secret-store authentication remain mandatory.
- Define a provider-neutral agent-provisioning protocol but do not implement vSphere, Proxmox, or another virtualization adapter in the first release.
- Add black-box release tests that install released agent and Octa bundles and exercise Native, OCI process, and OCI hypervisor paths against the real server vertical slice.
- Explicitly exclude a Web UI, visual pipeline editor, operator authentication, LDAP/TOTP providers, RBAC, production virtualization adapters, Kubernetes execution, and concurrent multi-job execution by one agent from the first release.

## Capabilities

### New Capabilities

- `server/rest-control-plane`: Versioned REST resources, commands, and build-log search over transport-independent application commands and queries for operating the server without a UI.
- `server/project-runs`: Hierarchical projects, versioned build configurations and pipeline DAGs, durable triggers, immutable builds, attempts, jobs, orchestration, retry/cancel semantics, and signed JobSpec construction.
- `server/agent-scheduling`: Agent-pool membership, registration, global durable ready-job placement, capability matching, leases, fencing, heartbeats, drain, event ingestion, and completion.
- `server/vcs-integration`: Separate versioned protocols for provider-neutral webhook ingestion and VCS browsing/resolution, with manual and managed webhook configuration and Git as the first VCS implementation.
- `server/output-storage`: Backend-neutral artifact/report publication and immutable build-log chunk storage with an S3-compatible adapter, integrity verification, retention, and download capabilities.
- `server/remote-cache`: Namespace-isolated cache authorization and the HTTP L2 data plane consumed by Octa's existing cache protocol.
- `server/operations`: Configuration, PostgreSQL migrations, rebuildable build-log search projection, secret-provider integration, agent/server observability, audit, recovery, backup, and graceful shutdown for a trusted-network management deployment.
- `server/dynamic-agent-provisioning`: A versioned provider-neutral agent-provisioning protocol reserved for later machine provisioning without production virtualization adapters in the first release.
- `server/agent-ready-matrix`: Released-machine black-box verification across the supported server, agent, runtime, failure, and lifecycle matrix.

### Modified Capabilities

None. This is the first OpenSpec capability set for the server; existing implemented agent contracts remain unchanged.

## Impact

- Moves the workspace toward `cli`, `server`, `agent`, and `shared` product directories and adds server crates grouped by API, application/core, protocols, infrastructure, and composition.
- Reuses the existing agent protocol and agent execution modules while introducing separate server-side Job, pipeline, trigger, orchestration, scheduling, webhook, and VCS responsibilities.
- Splits backend-neutral artifact contracts from the S3-compatible adapter and prevents REST DTOs, database rows, provider payloads, and domain state from becoming shared models.
- Keeps immutable Build configuration snapshots, event ordering, cursors, logical object metadata, and a rebuildable full-text search projection in PostgreSQL while storing produced files, reports, and large immutable redacted log chunks in the configured object store.
- Introduces PostgreSQL and an S3-compatible object store as initial production adapters; a future external search adapter, Kafka, NATS, Redis, operator identity providers, and virtualization providers are not required for the first deployment.
- Requires trusted-network isolation for the unauthenticated v1 management interface while retaining authentication on every agent, webhook, signing, and secret-store boundary.
- Adds release-machine CI infrastructure and privileged workers for real Native, containerd, and Microsandbox verification.
