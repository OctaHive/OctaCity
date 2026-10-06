## Why

OctaCity already provides a durable CI/CD execution substrate and Octa now has an official Codex task plugin, but there is no durable product lifecycle that can accept coding work, coordinate implementation and independent evaluation across Builds, capture an exact candidate, and deliver an approved change. Adding that lifecycle now turns the existing execution platform into an opt-in dark factory without making ordinary CI/CD depend on an LLM or replacing the established Build, Agent, and Octa contracts.

## What Changes

- Add an opt-in Dark Factory mode alongside the existing standalone CI/CD mode; ordinary Triggers, Builds, Attempts, Jobs, and Results continue to work without a Factory Configuration, model provider, or coding harness.
- Add provider-neutral Work Envelopes, versioned Factory Configurations, durable Factory Runs, append-only Stage Attempts, bounded budgets, WIP limits, fencing, cancellation, retry, escalation, and restart reconciliation above ordinary immutable Builds.
- Adopt the official Octa Codex plugin in the pinned OctaCity release path and compile an immutable Factory Task Envelope into a normal Octa task while keeping Codex semantics out of the Agent and server protocols.
- Add deny-by-default Factory Permission Sets that intersect Factory policy, immutable task intent, local Agent grants, and backend capabilities for plugins, tools, commands and arguments, filesystem roots and mounts, network hosts, secret and workload-identity profiles, descendants, resources, and outputs.
- Add trusted ChangeSet capture for an exact base and candidate revision, deterministic validation and evidence production, read-only evaluator connectors, versioned criterion packs, immutable assessments, and a deterministic Decision Engine. Program code owns loops, branches, joins, retries, budgets, and stop conditions; model calls return bounded typed values and never authorize lifecycle transitions.
- Add a provider-neutral Decision Signal boundary for optional probability-backed routing and tool-risk assessment. JEV is the first adapter; later decision-model or Decisions API adapters can be selected through immutable capability-based configuration without changing factory domain types. Signals may choose only among predeclared routes or narrow an already-authorized tool action, and deterministic policy remains authoritative.
- Add a write-capable delivery boundary that can idempotently publish an accepted candidate and create or update a pull request without giving coding or evaluation Jobs forge write credentials. The first release keeps merge human-controlled.
- Add complete REST and Operator Console management workflows for guided Factory Configuration authoring and immutable-version publication, manual Work admission, Factory Run, stage, candidate, evaluation, decision, escalation, and delivery inspection and commands; every workflow remains available headlessly through REST.
- Preserve exact revision, generic Artifact/report, audit, retention, visibility, idempotency, optimistic concurrency, secret-redaction, and same-origin trusted-network contracts.
- Keep automatic merge, unrestricted unattended execution, dynamic infrastructure provisioning, provider-authored control flow, and a universal coding-plugin abstraction outside the initial release. A shared harness interface is considered only after a second production harness proves a real common seam.

## Capabilities

### New Capabilities

- `server/dark-factory`: Provider-neutral work admission, durable Factory Run coordination, code-owned agentic workflow, optional typed Decision Signals, Codex execution, ChangeSet capture, evaluation, deterministic decisions, bounded rework, escalation, and delivery.

### Modified Capabilities

- `server/project-runs`: Preserve the independent CI/CD lifecycle while allowing a Factory Stage Attempt to create an ordinary immutable Build with exact factory causality and narrowed effective policy.
- `server/agent-scheduling`: Extend placement and pre-spawn enforcement with the typed Factory Permission Set and fail closed when a requested restriction cannot be enforced.
- `server/vcs-integration`: Add a protocol-separated, write-capable and idempotent delivery adapter for accepted exact candidates while keeping ordinary VCS access read-only.
- `server/operations`: Recover and reconcile Factory Runs after restart and include factory transitions, evidence, decisions, and delivery in audit, observability, backup, and shutdown contracts.
- `server/rest-control-plane`: Add authorization-safe, bounded, idempotent Factory Configuration and Factory Run management and diagnostics without making the UI mandatory.
- `ui/operator-console`: Add a Dark Factory activity and contextual explorer for runs, stages, candidates, evaluations, decisions, escalations, and delivery while preserving existing CI/CD activities.

## Impact

- Adds factory domain/application modules, store ports, PostgreSQL migrations and workers above the current Build application; existing Build orchestration, placement, and execution remain the only Job execution path.
- Extends signed execution intent and Agent admission with factory permissions, and updates the pinned Octa release/local-stand inputs to include `octa_plugin_codex` and its verified distribution metadata.
- Adds Work Source, Decision Signal, evaluator, and delivery boundaries plus initial manual intake, JEV, Codex, evaluator, and pull-request implementations. Provider SDKs and wire types stay in infrastructure adapters; a future Decisions API implementation plugs into the same Decision Signal contract.
- Adds REST/OpenAPI resources and an optional Operator Console management surface with typed configuration forms, generated client types, release documentation, security contracts, and released-product vertical slices; the console remains replaceable because every operation is available through REST.
- Introduces no breaking change to ordinary CI/CD API behavior; factory resources and routes are additive and opt-in.
