## Context

See `proposal.md` for motivation. The server is a layered Rust modular monolith whose management REST adapter publishes a generated OpenAPI 3.1 document at `/api/v1/openapi.json`. Management requests already cross application-layer authorization, but the only current policy grants the canonical anonymous actor unrestricted access and is safe only on a trusted network. There is no frontend toolchain in the repository.

The current REST surface supports the desired detail reads and commands, but it cannot discover recent Builds or most current Project-owned definitions: Pipeline, Repository, Build Configuration, and several Trigger resources can only be read after their identities and versions are already known. The UI therefore needs additive read models before it can provide navigation without inventing a parallel data source.

The existing release system produces deterministic checksummed native server bundles. The console is platform-independent static content and must remain optional so a server deployment without it preserves the complete headless product surface.

## Goals / Non-Goals

**Goals:**

- Establish one browser-to-server boundary: the published management REST API.
- Deliver a small data-dense operator SPA with deep links, accessible alternatives, explicit asynchronous states, contextual resource exploration, and keyboard-first global navigation.
- Add only the bounded discovery, resource-search, and operator-attention queries needed by that SPA and keep authorization visibility inside application/store reads before classification, ranking, or pagination.
- Preserve mutation idempotency, optimistic concurrency, audit correlation, and secret-safe transfer behavior in browser workflows.
- Provide complete English and Russian presentation plus light, dark, and system-following evergreen themes without introducing authenticated browser identity.
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

The console will adapt the calm green, data-dense character of the supplied visual references without copying their branding, assets, screen composition, authentication pages, or template code. The result is an OctaCity-owned token system, provisionally named `evergreen`, with complete light and dark palettes in the first release. A three-state selector chooses light, dark, or the current operating-system preference; both palettes independently pass contrast, non-color state, screenshot, and accessibility checks before release.

The light palette starts from these semantic values; the dark palette maps the same roles rather than allowing components to branch on literal colors:

| Token role | Initial value | Use |
| --- | --- | --- |
| Canvas | `#F5F7F6` | Application background |
| Surface | `#FFFFFF` | Cards, tables, dialogs, and top bar |
| Subtle surface | `#EEF2F0` | Grouped rows and secondary panels |
| Evergreen 950 | `#10251D` | Activity rail and dark context surfaces |
| Evergreen 800 | `#1C3B30` | Active rail selection and dark feature panels |
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

The desktop shell uses one compact sticky utility header, an approximately 88–96-pixel primary section rail, an independently collapsible and resizable contextual explorer, and a fluid detail canvas. The rail presents large icon-and-short-label controls for Projects, Builds, Agents, and Audit rather than a dense text menu or ambiguous icon-only strip. The explorer defaults to 288 pixels, is pointer- and keyboard-resizable between 240 and 480 pixels, persists only its clamped width, and becomes a focus-managed overlay at the supported narrow width. Its sticky header contains the active section title, a search button, and collapse control; a Favorites section precedes the lazily loaded resource hierarchy. The header owns the global command-center search entry, readiness, direct theme control, notification bell, and neutral operator controls; the lower-frequency language control stays inside the operator menu. Cards use 8–10 pixel radii, one-pixel neutral borders, and shadows only when elevation communicates behavior. Tables remain the primary representation for operational collections; cards group summaries and actions rather than turning every value into a dashboard metric. Project and Build identity may use one dark-evergreen or soft-mint context panel, but the root still redirects to Projects and no unsupported KPI dashboard is invented.

Typography uses the native system sans-serif stack to avoid a remote font request or another shipped font dependency. One tree-shaken SVG icon package supplies the consistent outline vocabulary; icon fonts, copied template icons, ornamental avatar imagery, and an unbounded custom-color editor are excluded. Rail icons always have accessible names and tooltips rather than relying on symbol recognition. Visual regressions cover both palettes at representative desktop and narrow supported widths, with motion disabled and deterministic data.

### 4. Generate types from the Rust-owned OpenAPI document

`octacity-server-api-rest` will expose a small development-only OpenAPI export entry point that serializes the same `openapi_document()` used by the running route. `pnpm api:generate` will invoke that exporter and generate TypeScript schema types into an ignored build directory using a pinned `openapi-typescript` toolchain. The generated file is not hand-edited or committed; CI regenerates it before type checking and building, avoiding a second checked-in copy of the contract.

One narrow API module will combine the generated types with `openapi-fetch`, normalize `ErrorResponse` and `X-Request-Id`, apply `Idempotency-Key` and `If-Match` where the OpenAPI operation declares them, and enforce relative `/api/v1` paths. Feature code will not call `fetch` directly. Existing OpenAPI route/schema drift tests remain authoritative on the Rust side, while the frontend build proves all used operations still type-check.

Generating handwritten endpoint clients was rejected because it creates schema duplication. Generating a large method-per-operation SDK was rejected because the typed-fetch layer provides the required safety with less generated code and a smaller review surface.

### 5. Add bounded discovery, search, and attention-feed routes

The REST adapter will add these additive reads:

- `GET /api/v1/projects/{project_id}/pipelines`
- `GET /api/v1/projects/{project_id}/repositories`
- `GET /api/v1/projects/{project_id}/build-configurations`
- `GET /api/v1/projects/{project_id}/trigger-definitions`
- `GET /api/v1/projects/{project_id}/builds`
- `GET /api/v1/search`
- `GET /api/v1/operator-attention`

Every route accepts the existing bounded `limit` and exclusive `cursor` convention. The Build collection additionally accepts optional exact `configuration_id` and `state` filters. Definition pages contain only current-version summaries; exact immutable content continues to come from existing version-addressed detail endpoints. Trigger summaries use a tagged `kind` (`manual`, `scheduled`, or `internal`) rather than merging their full definitions into an untyped object.

Definition collections use stable identity ordering. Builds use `(created_at DESC, build_id DESC)` and a bounded versioned cursor carrying that composite position. Global search accepts bounded normalized query text, an allowlisted resource-kind set, `limit`, and an exclusive cursor bound to the normalized query and kinds. Typed Project, Build, Agent, and Agent Pool hits rank exact stable identifiers before normalized name-prefix and other normalized name matches, then use resource kind, normalized label, and stable identity for deterministic ties. Cursor decoding is strict and does not authorize a resource. The owning Project and optional configuration filter are validated before querying.

The operator-attention route accepts a bounded set of Build, Agent, and Agent Pool targets, an explicit critical-condition selector, bounded time constraints, `limit`, and a scope-bound exclusive cursor. It returns only safe typed attention-worthy transitions for visible requested targets and server-classified critical conditions. It is not a general activity feed, does not accept or infer a recipient, does not expose arbitrary audit or Job events, and does not assign personal read state. Submitting a favorite identity selects an authorized query target but never grants visibility.

The existing readiness monitor is the initial production source of critical system conditions. A transition to an unavailable or timed-out required dependency opens one idempotent condition with a stable server classification code, an internal runtime-source UUID, and a bounded non-secret summary; a transition away from that exact failure resolves only the same runtime source's condition. The source UUID is persistence-only and is not exposed through REST. This prevents one recovered replica from resolving another replica's active failure. Persistence is bounded by the readiness timeout and remains best-effort when PostgreSQL itself is unavailable, so attention reporting can neither stall the monitor indefinitely nor falsely restore readiness.

Application queries receive typed Project ownership, filters, search inputs, attention scope, page bounds, and authorization visibility. Store ports return UI-neutral summary/search/attention projections rather than REST DTOs. In-memory and PostgreSQL adapters apply ownership, visibility, matching or classification, ranking or ordering, cursor comparison, and limit in that order; REST performs no post-fetch security filtering. PostgreSQL owns one immutable normalization function shared by the production search query and its expression indexes, and representative plan checks execute that exact production query. PostgreSQL adds only the indexes justified by the selected predicates, matching rules, classification, and ordering, with migration and query-plan contract coverage.

A global dashboard endpoint and aggregate counts remain rejected: typed resource search supports navigation without creating a premature reporting model or computing operational KPIs.

### 6. Compose screens from existing detail resources

The initial route map is:

- `/projects` for paged hierarchy navigation;
- `/projects/:projectId` for current definitions, Trigger summaries, and recent Builds;
- `/builds/:buildId` for Build state, current Attempt DAG, selected Job diagnostics, logs, Artifacts, cache sessions, and retention;
- `/agents` and `/agents/:agentId` for inventory and Agent actions;
- `/agent-pools` and `/agent-pools/:poolId` for current pool state;
- `/audit` for bounded filtered audit facts.

The root route redirects to `/projects`; it is not a synthetic dashboard. Resource identity and user-selected tab/filter/cursor state are encoded in the URL where sharing or refresh matters. Query keys are centralized by resource identity and filters so a mutation invalidates only affected data.

The workbench separates global section selection from resource exploration. A compact utility header contains one command-center trigger and search field, readiness, a direct light/dark/system theme control, the notification-center bell, and a neutral operator menu. English/Russian selection lives inside that menu because it is a low-frequency preference. Because the browser supplies no authenticated or forwarded identity, the menu exposes language, preferences, documentation, and deployment information but never fabricates a profile, email address, or logout action. A stable rail of large labeled controls changes among Projects, Builds, Agents, and Audit. The adjacent explorer is contextual, independently collapsible and resizable, keyboard navigable, and does not duplicate the section list.

Project collection summaries include one authorization-aware `has_children` navigation hint computed by the server's bounded collection query. The Project explorer renders a disclosure control only for summaries with visible direct children, so leaf Projects neither open empty branches nor trigger one probe request per visible row. Project detail representations remain unchanged; a deep-linked selected Project reuses the same child collection query that expansion would consume.

The Build explorer lazily composes existing bounded reads as `Project -> Build Configuration -> Build`, shows non-color state labels, and paginates each expanded branch independently. The Agent explorer composes bounded Agent Pool and Agent reads as `Agent Pool -> Agent` with a separate unassigned section. Audit uses contextual filters and favorite views instead of inventing a hierarchy. Every explorer places Favorites before its primary tree or filter content. Its header search button opens the same command center as the global header but preselects the active resource kinds; clearing that scope promotes the search to global rather than creating a second search implementation. Global search queries the typed search route, groups hits by resource kind, and supports navigation plus non-destructive presentation commands. Destructive or state-changing operations remain in confirmed resource context rather than the command center.

Favorites and recents are local shortcuts, not authority or cached domain state. Favorites occupy the explicit section above the explorer hierarchy; recents remain available through the shared command center instead of consuming permanent explorer space. Selecting an explorer or command-center result navigates to its canonical resource URL. Shareable identity and filters stay in the URL; explorer width, visibility, expanded nodes, favorites, recents, theme, and language are presentation preferences.

The notification center is an attention surface, not a firehose or a personal inbox. It merges three bounded sources: definitive outcomes for commands issued during the current browser session, server-published attention-worthy transitions for locally favorited Builds, Agents, and Agent Pools, and server-classified critical system conditions. Current-session command outcomes first use accessible toast/status feedback and remain only in bounded memory; request identities and response payloads are never persisted. Favorite and critical items are fetched from the scoped operator-attention route. Ordinary unrelated successes, all-system Build activity, audit history, and raw Job events are excluded. The badge counts items newer than the browser-local last-opened time and is explicitly not presented as a server-side per-user unread count.

The Attempt graph renders as SVG using a small deterministic, UI-independent DAG layout module and also provides the same nodes, states, and edges as an accessible table. Keeping this narrow module inside the application avoids a graph-library runtime dependency while isolating and directly testing topology and geometry policy. The first slice will not introduce a canvas editor. Job-event follow uses one cancellable bounded-wait loop for the selected active Job, advances only from the last accepted sequence, aborts on route or selection change, and backs off on retryable failure. It does not open one poller per Job.

### 7. Treat mutations as explicit user intents

One shared mutation helper creates a UUID idempotency key when the operator confirms an intent and retains that key and immutable request in memory until a definitive response or explicit abandonment. Transport retries reuse both; a new user action receives a new key. Automatic mutation retries are disabled unless the helper can prove the same key and body are retained.

Confirmation dialogs name the target and consequence for cancellation, retry, hold release, Agent drain, and pool reassignment. Versioned actions use the version already returned by the relevant detail resource as a strong `If-Match`; a precondition failure stops and offers a refresh rather than silently applying to newer state. Pending actions disable duplicate submission. Successful responses invalidate the narrow Build, Project Build page, Agent, Pool, retention, and audit query keys implicated by that operation.

Persistent replay journals were rejected for the initial release because storing arbitrary command bodies in browser storage broadens the data-retention and secret-review surface. The console guarantees safe retry while the confirmed intent remains active; a later durable outbox would require a separate explicit security design.

### 8. Keep browser handling and local preferences secret-safe and policy-neutral

The console never stores API responses, notification items, mutation bodies, download capabilities, query caches, logs, request identities, credentials, or other server-derived content in local or session storage. One versioned preference adapter may persist only language, theme mode, explorer visibility or width, bounded expanded hierarchy identities, bounded recent resource identities, bounded favorite resource identities, and the locally generated notification-center last-opened time. It validates resource kind, identifier shape, timestamps, version, and aggregate limits on every read, discards invalid data, and exposes no general storage API to features. Error rendering uses the stable code, safe message, and request identity only; raw response bodies, headers, and URLs are not sent to browser logging or analytics. No analytics SDK is included.

Artifact download obtains the existing short-lived capability immediately before navigation, uses `noopener`/`noreferrer` behavior where a new context is required, and never renders or copies the private URL. Audit data is displayed as evidence, not interpreted as an authorization grant. A `403` remains a terminal denied operation and never starts a login flow.

### 9. Verify backend, browser, and released deployment independently

Backend discovery work follows the existing contract layers: application query tests, in-memory store behavior, PostgreSQL adapter and migration contracts, REST handler tests, authorization coverage, OpenAPI drift, and architecture checks. Fixtures cover hidden rows around every page boundary, exact filters, invalid cursors, empty visible pages, and concurrent inserts at equal timestamps.

Frontend verification includes TypeScript strict checking, lint/format checks, unit tests for API error, preference bounds, localization completeness, token contrast, notification scoping, and idempotency behavior, component tests for loading/empty/stale/error/keyboard states, accessibility assertions, and Playwright flows against a deterministic management fixture. Workbench coverage exercises large labeled section controls, rail switching, pointer and keyboard explorer resize with min/max clamping, persisted width, collapse and overlay behavior, sticky explorer controls, independent lazy branch pagination, favorites, global versus pre-scoped search through the shared command center, current-session action outcomes, favorite and critical attention items, unrelated-event exclusion, local seen state, both languages, and light/dark/system theme resolution. A released-product browser slice will serve the built assets through the documented same-origin proxy and exercise Project discovery, Build navigation, one safe mutation replay, bounded Job following, and absence of CORS or browser credentials.

The release workflow will build one platform-independent `octacity-console` archive containing content-hashed assets, a release manifest, checksums, license, and deployment guide. It will be versioned from the same release tag and source revision as server artifacts but remain a separate downloadable and deployable product. The archive contains no Node runtime, source map with repository sources, or environment-specific API address.

## Risks / Trade-offs

- **[Unauthenticated console is mistaken for public-safe software]** → Display the security mode in the shell and deployment guide, keep the management listener private, ship restrictive proxy examples, and add a released deployment check that no credential or CORS path is required.
- **[New collection queries become expensive]** → Require bounded pages and typed filters, add predicate/order indexes, inspect representative PostgreSQL plans, and omit aggregate dashboard queries.
- **[Global search becomes an unbounded cross-domain scan]** → Bound normalized input, kinds, page size, result context, and cursor; apply visibility before deterministic ranking and prove representative PostgreSQL plans.
- **[Notification bell becomes a noisy all-system feed]** → Admit only current-browser command outcomes, attention transitions for explicitly favorited targets, and server-classified critical conditions; reject empty server feed scopes and exclude ordinary audit or Job activity.
- **[Local notification state is mistaken for personal delivery]** → Label it as browser-local, retain command history only in memory, persist only a local last-opened time, and defer recipient/read semantics until authenticated actor identity reaches the browser.
- **[OpenAPI generation couples frontend builds to Rust tooling]** → Keep the exporter small and deterministic, pin JavaScript generators, generate once before frontend checks, and package only static output.
- **[Live diagnostics create excessive request load]** → Follow only the selected active Job, use the server's bounded wait, cancel hidden views, honor `Retry-After`, and centralize timing constants instead of scattering intervals.
- **[SPA state drifts from authoritative state]** → Keep durable selection in URLs, server state in bounded queries, and use targeted invalidation plus visible stale/error states rather than optimistic domain transitions.
- **[Separate console release adds deployment work]** → Provide a checksummed static archive and copy-ready reverse-proxy contract; server-only deployments remain valid and rollback is removing the optional static route.
- **[DAG visualization adds bundle weight]** → Keep the deterministic layout behind one small internal module with custom accessible rendering, enforce a bundle budget, and retain the table as the functional baseline.
- **[Visual inspiration becomes copied template surface or dependency]** → Own all tokens and layouts in OctaCity, use no reference assets or code, and review visual snapshots for product-specific information hierarchy rather than pixel imitation.
- **[Green styling obscures operational severity]** → Reserve semantic warning, danger, and information colors, pair every state with text and an icon, and enforce contrast and non-color accessibility tests.
- **[Local favorites become an accidental data cache]** → Persist only bounded validated resource identities and presentation values through one allowlisted adapter; prohibit labels, payloads, request identities, logs, and capabilities.
- **[Theme or translation coverage drifts]** → Require semantic tokens rather than component literals, typed English/Russian message catalogs with identical keys, locale-aware tests, and deterministic snapshots for both complete palettes.

## Migration Plan

1. Ship the additive REST discovery endpoints and OpenAPI contract; existing routes and clients remain unchanged.
2. Build and verify the console against that exact generated contract, then publish its separate checksummed static archive.
3. Operators deploy the archive and add same-origin static, `/api/v1`, and `/health` proxy routes while keeping the management listener private.
4. Rollback removes the static/proxy UI route or restores the prior asset archive. The additive REST endpoints can remain because they do not alter existing request or response contracts.
