## Why

An operator cannot start a first Dark Factory bug fix without assembling a Flow and Factory Configuration and selecting task and acceptance Artifacts produced by earlier Builds. A supported bug-fix preset, direct text intake, and an actionable readiness check should make the first run accessible on a prepared deployment while preserving the existing Factory execution and security contracts.

## What Changes

- Supply one release-versioned **Fix a bug** preset (`bugfix`) that composes the existing generic Flow primitives, protected test-first development, exact candidate capture, independent review, and deterministic acceptance. Resolve Project-owned execution profiles, pools, permissions, and hard budgets through existing visible choices and authoritative validation; publish ordinary immutable Flow and Factory Configuration versions.
- Add bounded manual submission of a bug description and acceptance criteria without prerequisite Builds or uploaded Artifacts. Publish the two documents as immutable Project-owned Work inputs with explicit manual-intake provenance, then admit one ordinary Work Envelope and Factory Run through the existing admission authority.
- Make text publication and admission resumable and idempotent across concurrent requests, server restart, interrupted object-store writes, and lost responses. Retain the original resolved revision, submitted bytes, exact configuration, input identities, and logical outcome; never fabricate Build, Job, Agent, or execution-evidence provenance.
- Add a read-only, configuration-scoped readiness report with safe blocker codes, freshness, and remediation targets covering the reachable execution requirements. Reuse authoritative capability and readiness facts; report unknown required facts honestly and never reserve capacity, resolve credentials, invoke a model, or grant execution authority.
- Provide a guided Console path from a visible Project through preset configuration, text entry, readiness, and target-specific confirmation to the returned Run. Keep every step available headlessly through REST and retain the existing advanced authoring and Artifact-based intake paths.
- Preserve signed JobSpec v3, deny-by-default permission intersection, qualified backend enforcement, scoped Contexts and hidden tests, fenced attempts, exact evidence and decisions, bounded rework, audit, retention, and idempotent operator actions. The initial preset ends with a retained accepted candidate for review and grants no forge-write, merge, or deployment authority.

## Capabilities

### New Capabilities

- `server/factory-quickstart`: Release-versioned bug-fix preset resolution, immutable text Work inputs, recoverable intake, and non-authoritative configuration readiness over the existing Factory substrate.

### Modified Capabilities

- `server/rest-control-plane`: Add bounded preset discovery and resolution, text-intake commands and receipts, and readiness queries with existing authorization, replay, error, and OpenAPI contracts.
- `ui/operator-console`: Add guided bug-fix setup, direct text entry, actionable readiness, and safe submission using the existing REST and confirmed-intent boundaries.

## Impact

- Extends `config/factory/`, Factory application handlers, operation-shaped store ports and PostgreSQL migrations, retained-input reading/transfer/retention, REST/OpenAPI, runtime composition, and `ui/src/features/factory/`.
- Depends on the generic runtime and release-qualified execution path in `add-dark-factory-mode`; its outstanding production composition and qualification tasks remain owned by that change. This proposal does not claim that those prerequisites are complete or modify its artifacts.
- No new executor, business-stage enum, broad Artifact-upload API, provider SDK, Agent wire protocol, or required external service. Ordinary CI/CD and existing Factory clients remain compatible.
- Excludes an installer, infrastructure provisioning, credential setup, other presets, an Attention inbox, an autonomous planner, external ticket ingestion, and delivery/promotion setup. Uses repository-mandated behavioral Red/Green implementation slices and retained release evidence.
