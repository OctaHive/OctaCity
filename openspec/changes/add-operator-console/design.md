## Context

See `proposal.md` for motivation. The server is a layered Rust modular monolith whose management REST adapter publishes a generated OpenAPI 3.1 document at `/api/v1/openapi.json`. Management requests already cross application-layer authorization, but the only current policy grants the canonical anonymous actor unrestricted access and is safe only on a trusted network. There is no frontend toolchain in the repository.

The current REST surface supports the desired detail reads and commands, but it cannot discover recent Builds or most current Project-owned definitions: Pipeline, Repository, Build Configuration, and several Trigger resources can only be read after their identities and versions are already known. The UI therefore needs additive read models before it can provide navigation without inventing a parallel data source.

The existing release system produces deterministic checksummed native server bundles. The console is platform-independent static content and must remain optional so a server deployment without it preserves the complete headless product surface.

## Goals / Non-Goals

**Goals:**

- Establish one browser-to-server boundary: the published management REST API.
- Deliver a small data-dense operator SPA with deep links, accessible alternatives, and explicit asynchronous states.
- Add only the bounded discovery queries needed by that SPA and keep authorization visibility inside application/store reads before pagination.
- Preserve mutation idempotency, optimistic concurrency, audit correlation, and secret-safe transfer behavior in browser workflows.
- Produce a reproducible platform-independent console release artifact and a documented same-origin proxy contract.

**Non-Goals:**

- Embedding static assets, frontend routing, or UI-specific business decisions in `octacity-server`.
- Login, sessions, identity forwarding, cookies, RBAC/ABAC, tenant selection, permissive CORS, or public exposure.
- A general design system, plugin UI framework, GraphQL/BFF layer, WebSockets, or a console-specific database.
- Authoring Pipeline DAGs, Repositories, Build Configurations, Project Policy, schedules, webhooks, or Agent Pools in the first release.
- A separate mobile application or pixel-equivalent layouts for small phones.

## Decisions

### 1. Deploy a separate SPA behind one trusted origin

The console will live directly in `ui/` and build to immutable static assets. Because it is the repository's only UI application, an additional single-app directory would add structure without defining a useful boundary. A trusted reverse proxy will serve the SPA at `/`, route `/api/v1/*` and `/health/*` to the private management listener, and return `index.html` for non-file UI routes only. API and health misses must never fall through to the SPA.

The supported browser configuration has no runtime API-origin setting: requests use relative paths. The documented proxy configuration will disable caching for `index.html`, allow long immutable caching for content-hashed assets, and set a restrictive Content Security Policy, `frame-ancestors 'none'`, `Referrer-Policy: no-referrer`, `X-Content-Type-Options: nosniff`, and an explicit trusted-network exposure warning. Browser API requests use `credentials: 'omit'`; the server does not enable CORS or consume forwarded identity headers.

This is preferred to embedding assets in the Rust server because console and server packaging can evolve independently and server-only installations remain unchanged. A separate browser origin was rejected because it would require CORS now and complicate a later session-based authentication design.

### 2. Build one deliberately small React application

`ui/` is the root of a standalone browser application, not a container for a component library or a JavaScript workspace. It owns its `index.html`, application bootstrap, root React tree, router, development server, production build, tests, package manifest, lockfile, and static output directory. It will use TypeScript, React, Vite, React Router, and TanStack Query, with pnpm and a locked Node/pnpm toolchain in CI. A JavaScript workspace root or another nested application directory is not introduced until a second JavaScript project actually exists.

The package manifest is build metadata for the application rather than a published library contract: it has no library-mode Vite configuration, package export map, reusable component entry point, registry publication workflow, or runtime dependency on Node. Its supported consumer is a browser loading the generated `index.html` and content-hashed assets from the separately deployed static bundle.

The quality boundary combines strict TypeScript, deterministic formatting, and ESLint rules for JavaScript, TypeScript, React, React Hooks, and browser DOM usage. Unused lint suppressions fail the check so exceptions cannot silently outlive their justification. Repository-level EditorConfig and Git attributes normalize UTF-8 text, final newlines, whitespace, and LF line endings across Rust, Markdown, and frontend sources instead of creating UI-only editor policy.

CSS Modules and a small set of semantic design tokens will own layout, typography, spacing, state colors, and focus treatment. No general component framework or global client-state library is added. Server state belongs to TanStack Query, route/filter state belongs in URLs, transient interaction state stays in the owning component, and shared components are introduced only after a second real use.

The source tree is organized by product feature (`projects`, `builds`, `capacity`, `audit`) around a thin `app` shell and one `api` boundary. Features may depend on `api` and small shared primitives, but not on one another's internal components. This keeps the UI aligned with operator workflows rather than mirroring every backend DTO.

### 3. Establish an original evergreen operator visual system

The console will adapt the calm green, data-dense character of the supplied visual references without copying their branding, assets, screen composition, authentication pages, or template code. The result is an OctaCity-owned token system, provisionally named `evergreen`, with one light workspace theme in the first release. A full dark theme and a theme switcher are deferred until a second complete palette has its own contrast and state validation.

The initial semantic palette is:

| Token role | Initial value | Use |
| --- | --- | --- |
| Canvas | `#F5F7F6` | Application background |
| Surface | `#FFFFFF` | Cards, tables, dialogs, and top bar |
| Subtle surface | `#EEF2F0` | Grouped rows and secondary panels |
| Evergreen 950 | `#10251D` | Primary sidebar |
| Evergreen 800 | `#1C3B30` | Sidebar selection and dark feature panels |
| Brand action | `#2F765E` | Primary controls and accessible links |
| Brand accent | `#4BA786` | Progress, focus accents, and decorative data marks |
| Brand soft | `#DFF2EA` | Selected, positive, and contextual backgrounds |
| Primary text | `#263247` | Main content text |
| Muted text | `#77827E` | Secondary metadata after contrast validation |
| Border | `#DCE4E0` | Dividers and component outlines |
| Warning | `#D49A35` | At-risk and delayed states |
| Danger | `#C65A55` | Failed, blocked, and destructive states |
| Information | `#477FAE` | Neutral active and informational states |

All final foreground/background pairs must pass the declared accessibility checks; token values may be adjusted centrally to meet contrast without changing component APIs. Green is not a generic status color: it represents brand emphasis and successful/healthy state, while warning, danger, and information retain distinct semantic tokens plus icons and text labels.

The desktop shell uses an approximately 224-pixel collapsible dark sidebar, a compact white sticky top bar, and a fluid light canvas. Cards use 8–10 pixel radii, one-pixel neutral borders, and shadows only when elevation communicates behavior. Tables remain the primary representation for operational collections; cards group summaries and actions rather than turning every value into a dashboard metric. Project and Build identity may use one dark-evergreen or soft-mint context panel, but the root still redirects to Projects and no unsupported KPI dashboard is invented.

Typography uses the native system sans-serif stack to avoid a remote font request or another shipped font dependency. One tree-shaken SVG icon package supplies the consistent outline vocabulary; icon fonts, copied template icons, ornamental avatar imagery, and a general theme customizer are excluded. Visual regressions are tested at representative desktop and narrow supported widths, with motion disabled and deterministic data.

### 4. Generate types from the Rust-owned OpenAPI document

`octacity-server-api-rest` will expose a small development-only OpenAPI export entry point that serializes the same `openapi_document()` used by the running route. `pnpm api:generate` will invoke that exporter and generate TypeScript schema types into an ignored build directory using a pinned `openapi-typescript` toolchain. The generated file is not hand-edited or committed; CI regenerates it before type checking and building, avoiding a second checked-in copy of the contract.

One narrow API module will combine the generated types with `openapi-fetch`, normalize `ErrorResponse` and `X-Request-Id`, apply `Idempotency-Key` and `If-Match` where the OpenAPI operation declares them, and enforce relative `/api/v1` paths. Feature code will not call `fetch` directly. Existing OpenAPI route/schema drift tests remain authoritative on the Rust side, while the frontend build proves all used operations still type-check.

Generating handwritten endpoint clients was rejected because it creates schema duplication. Generating a large method-per-operation SDK was rejected because the typed-fetch layer provides the required safety with less generated code and a smaller review surface.

### 5. Add five project-scoped collection routes

The REST adapter will add these additive reads:

- `GET /api/v1/projects/{project_id}/pipelines`
- `GET /api/v1/projects/{project_id}/repositories`
- `GET /api/v1/projects/{project_id}/build-configurations`
- `GET /api/v1/projects/{project_id}/trigger-definitions`
- `GET /api/v1/projects/{project_id}/builds`

Every route accepts the existing bounded `limit` and exclusive `cursor` convention. The Build collection additionally accepts optional exact `configuration_id` and `state` filters. Definition pages contain only current-version summaries; exact immutable content continues to come from existing version-addressed detail endpoints. Trigger summaries use a tagged `kind` (`manual`, `scheduled`, or `internal`) rather than merging their full definitions into an untyped object.

Definition collections use stable identity ordering. Builds use `(created_at DESC, build_id DESC)` and a bounded versioned cursor carrying that composite position. Cursor decoding is strict and does not authorize a resource. The owning Project and optional configuration filter are validated before querying.

Application queries receive typed Project ownership, filters, page bounds, and authorization visibility. Store ports return UI-neutral summary projections rather than REST DTOs. In-memory and PostgreSQL adapters apply ownership, visibility, filters, ordering, cursor comparison, and limit in that order; REST performs no post-fetch security filtering. PostgreSQL adds only the indexes justified by the selected predicates and ordering, with migration and query-plan contract coverage.

A global dashboard endpoint and aggregate counts were rejected for this slice: Project-scoped collections are sufficient for the agreed navigation and avoid creating a premature reporting model.

### 6. Compose screens from existing detail resources

The initial route map is:

- `/projects` for paged hierarchy navigation;
- `/projects/:projectId` for current definitions, Trigger summaries, and recent Builds;
- `/builds/:buildId` for Build state, current Attempt DAG, selected Job diagnostics, logs, Artifacts, cache sessions, and retention;
- `/agents` and `/agents/:agentId` for inventory and Agent actions;
- `/agent-pools` and `/agent-pools/:poolId` for current pool state;
- `/audit` for bounded filtered audit facts.

The root route redirects to `/projects`; it is not a synthetic dashboard. Resource identity and user-selected tab/filter/cursor state are encoded in the URL where sharing or refresh matters. Query keys are centralized by resource identity and filters so a mutation invalidates only affected data.

The Attempt graph renders as SVG using a small deterministic, UI-independent DAG layout module and also provides the same nodes, states, and edges as an accessible table. Keeping this narrow module inside the application avoids a graph-library runtime dependency while isolating and directly testing topology and geometry policy. The first slice will not introduce a canvas editor. Job-event follow uses one cancellable bounded-wait loop for the selected active Job, advances only from the last accepted sequence, aborts on route or selection change, and backs off on retryable failure. It does not open one poller per Job.

### 7. Treat mutations as explicit user intents

One shared mutation helper creates a UUID idempotency key when the operator confirms an intent and retains that key and immutable request in memory until a definitive response or explicit abandonment. Transport retries reuse both; a new user action receives a new key. Automatic mutation retries are disabled unless the helper can prove the same key and body are retained.

Confirmation dialogs name the target and consequence for cancellation, retry, hold release, Agent drain, and pool reassignment. Versioned actions use the version already returned by the relevant detail resource as a strong `If-Match`; a precondition failure stops and offers a refresh rather than silently applying to newer state. Pending actions disable duplicate submission. Successful responses invalidate the narrow Build, Project Build page, Agent, Pool, retention, and audit query keys implicated by that operation.

Persistent replay journals were rejected for the initial release because storing arbitrary command bodies in browser storage broadens the data-retention and secret-review surface. The console guarantees safe retry while the confirmed intent remains active; a later durable outbox would require a separate explicit security design.

### 8. Keep browser handling secret-safe and policy-neutral

The console never stores API responses, mutation bodies, download capabilities, or query caches in local or session storage. Error rendering uses the stable code, safe message, and request identity only; raw response bodies, headers, and URLs are not sent to browser logging or analytics. No analytics SDK is included.

Artifact download obtains the existing short-lived capability immediately before navigation, uses `noopener`/`noreferrer` behavior where a new context is required, and never renders or copies the private URL. Audit data is displayed as evidence, not interpreted as an authorization grant. A `403` remains a terminal denied operation and never starts a login flow.

### 9. Verify backend, browser, and released deployment independently

Backend discovery work follows the existing contract layers: application query tests, in-memory store behavior, PostgreSQL adapter and migration contracts, REST handler tests, authorization coverage, OpenAPI drift, and architecture checks. Fixtures cover hidden rows around every page boundary, exact filters, invalid cursors, empty visible pages, and concurrent inserts at equal timestamps.

Frontend verification includes TypeScript strict checking, lint/format checks, unit tests for API error and idempotency behavior, component tests for loading/empty/stale/error/keyboard states, accessibility assertions, and Playwright flows against a deterministic management fixture. A released-product browser slice will serve the built assets through the documented same-origin proxy and exercise Project discovery, Build navigation, one safe mutation replay, bounded Job following, and absence of CORS or browser credentials.

The release workflow will build one platform-independent `octacity-console` archive containing content-hashed assets, a release manifest, checksums, license, and deployment guide. It will be versioned from the same release tag and source revision as server artifacts but remain a separate downloadable and deployable product. The archive contains no Node runtime, source map with repository sources, or environment-specific API address.

## Risks / Trade-offs

- **[Unauthenticated console is mistaken for public-safe software]** → Display the security mode in the shell and deployment guide, keep the management listener private, ship restrictive proxy examples, and add a released deployment check that no credential or CORS path is required.
- **[New collection queries become expensive]** → Require bounded pages and typed filters, add predicate/order indexes, inspect representative PostgreSQL plans, and omit aggregate dashboard queries.
- **[OpenAPI generation couples frontend builds to Rust tooling]** → Keep the exporter small and deterministic, pin JavaScript generators, generate once before frontend checks, and package only static output.
- **[Live diagnostics create excessive request load]** → Follow only the selected active Job, use the server's bounded wait, cancel hidden views, honor `Retry-After`, and centralize timing constants instead of scattering intervals.
- **[SPA state drifts from authoritative state]** → Keep durable selection in URLs, server state in bounded queries, and use targeted invalidation plus visible stale/error states rather than optimistic domain transitions.
- **[Separate console release adds deployment work]** → Provide a checksummed static archive and copy-ready reverse-proxy contract; server-only deployments remain valid and rollback is removing the optional static route.
- **[DAG visualization adds bundle weight]** → Keep the deterministic layout behind one small internal module with custom accessible rendering, enforce a bundle budget, and retain the table as the functional baseline.
- **[Visual inspiration becomes copied template surface or dependency]** → Own all tokens and layouts in OctaCity, use no reference assets or code, and review visual snapshots for product-specific information hierarchy rather than pixel imitation.
- **[Green styling obscures operational severity]** → Reserve semantic warning, danger, and information colors, pair every state with text and an icon, and enforce contrast and non-color accessibility tests.

## Migration Plan

1. Ship the additive REST discovery endpoints and OpenAPI contract; existing routes and clients remain unchanged.
2. Build and verify the console against that exact generated contract, then publish its separate checksummed static archive.
3. Operators deploy the archive and add same-origin static, `/api/v1`, and `/health` proxy routes while keeping the management listener private.
4. Rollback removes the static/proxy UI route or restores the prior asset archive. The additive REST endpoints can remain because they do not alter existing request or response contracts.
