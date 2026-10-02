# OctaCity Operator Console

The operator console is a standalone browser application. It owns its HTML entry point,
browser bootstrap, dependency lockfile, and development and production build lifecycle. It
does not publish a JavaScript package and is not part of a repository-wide JavaScript
workspace.

## Toolchain

- Node.js 22.17.0
- pnpm 10.34.6 through Corepack

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
`/api/v1/openapi.json`, then writes TypeScript declarations to
`.generated/api/schema.d.ts`. The generated directory is intentionally ignored: regenerate it
before type checking or building instead of editing or committing its contents.

## REST boundary

`src/api/client.ts` is the console's only management transport. Feature code uses its generated
OpenAPI client and mutation-header helpers instead of calling `fetch` directly. The boundary
accepts only relative `/api/v1` paths, always omits browser credentials, validates request
correlation, converts server and transport failures to safe `ManagementApiError` values, and
bounds numeric `Retry-After` delays through one console retry-safety policy.

## Application shell

The root route redirects to `/projects`. The first-release shell declares deep links for:

- `/projects` and `/projects/:projectId`;
- `/builds/:buildId`;
- `/agents` and `/agents/:agentId`;
- `/agent-pools` and `/agent-pools/:poolId`;
- `/audit`.

The static host must return `index.html` for these non-file browser routes so a copied deep link
survives refresh. `/api/v1` and `/health` are reserved proxy prefixes and must never use the SPA
fallback. The shell deliberately has no login route, browser credential storage, dark theme, or
theme switcher; it visibly identifies the unauthenticated trusted-network boundary.
