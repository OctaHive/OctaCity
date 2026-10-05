# OctaCity Operator Console

The operator console is a standalone browser application. It owns its HTML entry point,
browser bootstrap, dependency lockfile, and development and production build lifecycle. It
does not publish a JavaScript package and is not part of a repository-wide JavaScript
workspace.

## Toolchain

- Node.js 24.21.0
- pnpm 12.8.1

Dependency lifecycle scripts are disabled by `.npmrc`. Keep them disabled unless a reviewed
dependency has a documented, narrowly scoped build requirement.

ESLint checks JavaScript, TypeScript, React, Hooks, and DOM usage and rejects stale suppression
comments. Repository-level `.editorconfig` and `.gitattributes` files define shared UTF-8 and LF
text conventions for the frontend and Rust sources.

## Commands

Run these commands from this directory:

```sh
corepack pnpm install --frozen-lockfile
corepack pnpm exec playwright install chromium
corepack pnpm api:generate
corepack pnpm dev
corepack pnpm format
corepack pnpm lint
corepack pnpm typecheck
corepack pnpm test
corepack pnpm build
```

The one-time Playwright install supplies the pinned Chromium runtime used by browser layout and
semantic shell snapshots. `pnpm test` runs both unit/component tests and these browser checks;
`pnpm test:unit` and `pnpm test:browser` remain available for focused local runs.

The production application is written to `dist/` as an `index.html`-rooted static bundle.

`api:generate` runs the development-only Rust exporter for the same OpenAPI document served at
`/api/v1/openapi.json`, then writes TypeScript declarations and the small runtime constraint
projection to `.generated/api/`. The generated directory is intentionally ignored: regenerate it
before type checking or building instead of editing or committing its contents. Filter enums,
UTF-8 bounds, and default page sizes consumed at runtime therefore remain derived from the server
contract rather than duplicated in feature code.

Locked container builds set `OCTACITY_OPENAPI_SCHEMA_FILE` to a bounded regular JSON file that
was exported by an earlier Rust build stage. When the variable is absent, the generator runs the
Rust exporter itself. The override is a build boundary for verified local input, not a runtime API
URL, and the referenced document is limited to 16 MiB.

## REST boundary

`src/api/client.ts` is the console's only management transport. Feature code uses its generated
OpenAPI client and mutation-header helpers instead of calling `fetch` directly. The boundary
accepts only relative `/api/v1` paths, always omits browser credentials, validates request
correlation, converts server and transport failures to safe `ManagementApiError` values, and
bounds numeric `Retry-After` delays through one console retry-safety policy.

## Application shell

The root route redirects to `/projects`. The first-release shell declares deep links for:

- `/projects` and `/projects/:projectId`;
- `/builds` and `/builds/:buildId`;
- `/agents` and `/agents/:agentId`;
- `/agent-pools` and `/agent-pools/:poolId`;
- `/audit`.

The static host must return `index.html` for these non-file browser routes so a copied deep link
survives refresh. `/api/v1` and `/health` are reserved proxy prefixes and must never use the SPA
fallback.

The shell uses a compact utility header, a labeled section rail, a contextual explorer, and a
detail workbench. The explorer starts closed as a focus-managed overlay at the supported narrow
width and is resizable from 240 to 480 pixels on desktop. The Projects explorer owns the bounded
root/child Project hierarchy; the detail workbench never duplicates that navigation as a list. The
Builds explorer follows that same hierarchy and lazily loads each Project's Build Configurations
and each Configuration's newest Builds with an independent cursor per expanded branch. A Build
deep link reveals its Project ancestry without flattening nested Projects, and every explorer makes
its bounded-page scope explicit rather than claiming global completeness.

The header exposes readiness,
Command/Ctrl+K search, system/light/dark theme selection, a neutral notification placeholder, and
an operator menu for browser-local preferences and deployment references. It deliberately has no
login route, browser credential storage, fabricated browser identity, or logout action, and it
visibly identifies the unauthenticated trusted-network boundary.
Explorer search entries pass typed server resource kinds into the shared command center; clearing
that scope returns the same command center to global search.
