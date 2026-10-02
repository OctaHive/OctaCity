## 1. Project Discovery Read Models

- [x] 1.1 Add typed current-definition summaries, page inputs, Build summary/filter/cursor types, and management authorization mappings in the application layer; verify constructor, page-bound, invalid-cursor, resource-target, and visibility contract tests pass.
- [x] 1.2 Implement Pipeline, Repository, Build Configuration, and Trigger current-version discovery in the deterministic in-memory store path; verify only current Project-owned visible versions are ordered and paginated before projection, including hidden rows around page boundaries.
- [x] 1.3 Implement the four current-definition collections in PostgreSQL and add only indexes justified by their predicates and ordering; verify migration reentrancy, prior-schema upgrade, representative query plans, and PostgreSQL parity contracts pass.
- [x] 1.4 Register the four project-scoped definition routes, DTO pages, schemas, and OpenAPI operations; verify handler, authorization inventory, response-fixture, route/schema drift, cursor, empty-page, and existing detail-endpoint compatibility tests pass.
- [x] 1.5 Implement project Build discovery and its newest-first composite cursor in the application and in-memory store path; verify Project ownership, configuration/state filters, equal-timestamp tie ordering, invalid cursors, visibility-before-pagination, and concurrent-insert behavior.
- [ ] 1.6 Implement project Build discovery in PostgreSQL and add or prove the required ownership/filter/order indexes; verify migration, query-plan, hidden-boundary, filter, equal-timestamp, and in-memory/PostgreSQL parity contracts pass.
- [ ] 1.7 Register the project Build collection route, summary/page DTOs, parameters, schemas, and OpenAPI operation; verify handler, authorization inventory, route/schema drift, bounded pagination, filter validation, and unchanged Build detail contracts pass.

## 2. Console Toolchain and API Boundary

- [ ] 2.1 Create the single `ui/operator-console` React, TypeScript, Vite, and pnpm package with pinned supported tool versions, strict compiler/lint/format/test scripts, minimal dependencies, ignored generated/build output, and no JavaScript workspace root; verify a locked install and empty production build succeed.
- [ ] 2.2 Add the deterministic development-only Rust OpenAPI exporter and frontend generation script using the same `openapi_document()` as the running server; verify generation from a clean checkout is reproducible and generated TypeScript compiles without committing generated schema output.
- [ ] 2.3 Implement the single typed REST client boundary with relative `/api/v1` URLs, `credentials: 'omit'`, normalized stable errors/request correlation, bounded `Retry-After`, idempotency, and strong precondition headers; verify unit tests prohibit direct absolute origins and cover every stable error class used by the UI.
- [ ] 2.4 Implement the router/query application shell with the original light `evergreen` tokens, approximately 224-pixel collapsible dark sidebar, compact white sticky top bar, system typography, CSS-module boundary, trusted-network security banner, readiness indicator, navigation, route-level error handling, and deep-link fallback assumptions; verify token contrast, shell snapshots, keyboard navigation, unavailable readiness, narrow supported width, and all declared routes without a login path or theme switcher.

## 3. Project Navigation

- [ ] 3.1 Implement bounded root/child Project hierarchy navigation and breadcrumbs using existing Project endpoints; verify pagination, nested ancestry, selection deep links, loading, empty, not-found, and stale-refresh states in component tests.
- [ ] 3.2 Implement the Project view sections for current Pipelines, Repositories, Build Configurations, and typed Trigger summaries using the new collections and existing detail reads; verify each section paginates independently and never asks for a manually entered UUID.
- [ ] 3.3 Implement the recent Project Builds section with URL-backed configuration/state filters and newest-first pagination; verify filter round trips, copied URLs, equal-time results, empty filters, and navigation to `/builds/:buildId` in browser tests.

## 4. Build Diagnostics

- [ ] 4.1 Implement the Build and current Attempt view with Job selection, state/failure details, deterministic SVG DAG layout, and an equivalent accessible dependency table; verify failed, active, succeeded, skipped, and mixed dependency fixtures render identical graph/table relationships.
- [ ] 4.2 Implement one cancellable bounded-wait Job event follower that advances only from the last contiguous sequence and stops on terminal state, route change, or Job change; verify fake-timer and browser tests cover empty waits, appended pages, retry backoff, cancellation, and absence of duplicate events or parallel pollers.
- [ ] 4.3 Implement scoped redacted log search with freshness warnings and URL-backed filters; verify literal/full-text modes, cursor pages, delayed projection, empty authoritative results, and safe snippets without raw-response logging.
- [ ] 4.4 Implement Artifact listing/download, cache-session diagnostics, and Build Result retention panels; verify short-lived download URLs are neither rendered nor persisted, cache pages remain secret-free, and absent/held/expired retention states are distinct.

## 5. Safe Operator Commands

- [ ] 5.1 Implement the shared confirmed-intent mutation helper that creates one UUID key, retains an immutable request for safe transport retry, prevents duplicate submission, applies `If-Match`, and performs targeted query invalidation; verify lost-response replay, explicit abandonment, new intent, conflict, and rate-limit tests pass without browser persistence.
- [ ] 5.2 Add manual Build triggering from a selected Build Configuration plus Build cancellation and retry from Build views; verify target-specific confirmations, one-command submission, replay disposition, updated Project/Build state, validation failures, and request correlation in component and browser tests.
- [ ] 5.3 Add permanent/time-bounded Build Result hold placement and preconditioned release; verify reason/expiry validation, stale-version handling, replay, retention refresh, and audit correlation against REST fixtures.

## 6. Capacity and Audit Workflows

- [ ] 6.1 Implement paged Agent and Agent Pool lists and deep-linked detail views with inventory, readiness, assignment, drain, version, and current execution information; verify empty, unavailable, stale, and incompatible-capacity fixtures are distinguishable without client-side scheduling inference.
- [ ] 6.2 Add confirmed Agent drain and idle-Agent pool reassignment through the shared mutation helper; verify active/ineligible conflicts, strong preconditions, duplicate prevention, targeted Agent/Pool invalidation, and audit correlation.
- [ ] 6.3 Implement the audit view with URL-backed actor, operation, target, request-identity, and time filters plus cursor pagination; verify request-identity handoff from an error/action, bounded filter encoding, empty pages, and absence of authority decisions derived from audit data.

## 7. UX, Accessibility, and Security Hardening

- [ ] 7.1 Apply reusable loading, incremental-refresh, empty, stale, rate-limited, forbidden, not-found, conflict, unavailable, and internal-error presentations across every data boundary; verify a state inventory test covers every route and preserves prior data after refresh failure.
- [ ] 7.2 Complete keyboard, focus, semantic-label, token contrast, non-color state, reduced-motion, narrow-width, graph-table accessibility, and deterministic visual-regression coverage at representative widths; verify automated accessibility and screenshot checks plus documented manual keyboard journeys pass on every primary route.
- [ ] 7.3 Enforce frontend architecture and bundle budgets, dependency allowlisting, tree-shaken SVG icon imports, absence of copied template assets, analytics, browser persistence, icon fonts, remote fonts, direct `fetch`, and scattered timing/query-key literals; verify repository checks fail on representative forbidden imports, oversized bundles, external font requests, direct network calls, and duplicated styling constants.

## 8. Deployment and Release

- [ ] 8.1 Document and test the supported same-origin reverse-proxy contract, including private management binding, SPA fallback exclusions, `/api/v1` and `/health` routing, cache rules, CSP, framing, referrer, MIME, TLS, and trusted-network warnings; verify configuration fixtures serve deep links while API and health misses never return `index.html`.
- [ ] 8.2 Add deterministic `octacity-console` packaging with content-hashed assets, release manifest, checksums, license, deployment guide, no source-bearing maps, no Node runtime, and no environment-specific origin; verify repeated packaging produces equivalent manifests and the archive verifier detects tampering or forbidden files.
- [ ] 8.3 Integrate locked frontend generation, format, lint, type, unit, component, accessibility, build, bundle-budget, package, and provenance steps into CI/release without weakening existing Rust gates; verify CI fixtures reject stale lockfiles, generation failures, test failures, and invalid console archives.

## 9. End-to-End Verification and Documentation

- [ ] 9.1 Add deterministic Playwright operator journeys and stable visual snapshots for Project discovery, Project resources, Build/Attempt/Job diagnostics, one idempotent mutation replay, retention, Agent capacity, audit correlation, deep-link refresh, keyboard use, and the supported narrow layout; verify the suite runs repeatably with fixed data, viewport, motion, and system-font assumptions and without external services or hidden test ordering.
- [ ] 9.2 Add a released-product browser slice that serves the packaged console through the documented proxy against a released PostgreSQL-backed server; verify same-origin requests carry no browser credentials, no CORS allowance exists, live Job following is bounded, and an omitted console leaves all REST workflows operational.
- [ ] 9.3 Update management reference, server architecture, threat model, release contract, and operator deployment documentation, then run OpenSpec validation, Rust formatting/Clippy/docs/tests/coverage/contracts, locked frontend checks, security scans, package verification, and released-server/console slices; verify every required gate passes and the trusted-network no-login limitation is explicit in all operator-facing entry points.
