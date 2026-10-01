## Why

OctaCity exposes a complete trusted-network management workflow through REST, but routine operation still requires hand-written HTTP requests and prior knowledge of opaque resource identifiers. A focused operator console can make existing projects, builds, execution diagnostics, capacity, and audit evidence discoverable without introducing login, sessions, or a second business-logic path.

## What Changes

- Add a separately deployable browser operator console for the trusted management network, available through the same origin as `/api/v1` and explicitly operating without login, sessions, RBAC, or forwarded identity.
- Add bounded project-scoped discovery reads for current Pipelines, Repositories, Build Configurations, Trigger definitions, and recent Builds so the console never requires operators to enter known UUIDs.
- Add project and Build navigation, Build/Attempt/Job diagnostics, bounded live event following, log search, Artifact and cache diagnostics, retention inspection, Agent and Agent Pool views, and audit browsing.
- Add guarded operator actions for manual Build triggering, cancellation, retry, Build Result hold management, Agent drain and pool reassignment, using existing idempotency and concurrency contracts.
- Generate the browser API client and types from the published OpenAPI contract and preserve stable URLs, bounded pagination, explicit loading/empty/error states, and request correlation.
- Keep authoring editors for Pipelines, Repositories, Build Configurations, Project Policy, schedules, and webhooks outside this first UI release.
- Keep public Internet exposure, operator authentication, sessions, RBAC/ABAC, and cross-origin browser access outside this change.

## Capabilities

### New Capabilities

- `ui/operator-console`: Trusted-network browser navigation, diagnostics, safe operator actions, deployment boundary, accessibility, and browser-facing failure behavior for the initial operator console.

### Modified Capabilities

- `server/rest-control-plane`: Add authorization-safe, cursor-paginated discovery endpoints for project-owned definitions, Trigger definitions, and Builds required by the operator console while preserving REST-only headless operation.

## Impact

- Adds a React and TypeScript SPA built with Vite, a generated OpenAPI client, client-side routing and query state, focused component styling, and browser unit and end-to-end tests.
- Adds application queries, store ports/adapters, PostgreSQL and in-memory projections, REST handlers, schemas, OpenAPI operations, and contract tests for the new discovery reads.
- Adds release packaging and reverse-proxy documentation for serving static console assets and routing `/api/v1` to the private management listener under one trusted origin.
- Does not embed UI assets into the Rust server, change existing command semantics, add browser credentials, or make the management listener safe for public exposure.
