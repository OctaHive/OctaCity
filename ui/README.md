# OctaCity Operator Console

The operator console is a standalone browser application. It owns its HTML entry point,
browser bootstrap, dependency lockfile, and development and production build lifecycle. It
does not publish a JavaScript package and is not part of a repository-wide JavaScript
workspace.

## Toolchain

- Node.js from `.node-version`
- pnpm from `package.json#packageManager`

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
corepack pnpm check:architecture
corepack pnpm format
corepack pnpm lint
corepack pnpm typecheck
corepack pnpm test
corepack pnpm test:accessibility
corepack pnpm test:visual
corepack pnpm build
```

The automated checks are complemented by the keyboard-only journeys in
[`ACCESSIBILITY.md`](./ACCESSIBILITY.md).

The one-time Playwright install supplies the pinned Chromium runtime used by browser layout and
semantic shell snapshots. `pnpm test` runs both unit/component tests and these browser checks;
`pnpm test:unit` and `pnpm test:browser` remain available for focused local runs.

The production application is written to `dist/` as an `index.html`-rooted static bundle.
`build` also verifies the reviewed dependency allowlists and the byte and asset-count budgets in
`architecture-policy.json`. The same policy keeps browser networking, persistence, query keys,
timing constants, colors, and SVG icon imports behind their intended boundaries and rejects
external fonts, icon fonts, analytics integrations, and copied template assets. Run
`check:architecture` for the source-only gate or `check:bundle` against an existing `dist/`.
`architecture-policy.json` is the single source of truth for the raw-byte and asset-count limits;
the build reports any measured value that exceeds them.

Release packaging consumes this exact `dist/` directory without rebuilding the application:

```sh
python3 ../tools/package_console_release.py package \
  --distribution dist \
  --version 0.1.0 \
  --octacity-revision "$(git -C .. rev-parse HEAD)" \
  --output ../dist/octacity-console-0.1.0.tar.gz
python3 ../tools/package_console_release.py verify \
  --archive ../dist/octacity-console-0.1.0.tar.gz \
  --expected-version 0.1.0 \
  --expected-octacity-revision "$(git -C .. rev-parse HEAD)"
```

The same `dist/` is copied directly into the existing local-stand gateway image; release packaging
therefore adds no second UI build path and is not required by `tools/local-stand up`.

CI installs the exact Node.js and pnpm versions above, rejects lockfile drift, regenerates the API
boundary with Cargo's workspace lockfile, and runs format, lint, type, unit/component,
accessibility, browser/visual, architecture, bundle-budget, package, and archive-verification
gates. Release automation promotes that exact tested archive as the platform-independent
`octacity-console` product and includes it in the repository's GitHub artifact attestation instead
of rebuilding it. The local stand and release workflow therefore share one application build
contract even though the release archive remains optional at deployment time.

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
fallback. The checked-in configuration, TLS boundary, cache policy, security headers, and
deployment verification commands are documented in the
[operator console runbook](../docs/operations/operator-console.md).

The shell uses a compact utility header, a labeled section rail, a contextual explorer, and a
detail workbench. The explorer starts closed as a focus-managed overlay at the supported narrow
width and is resizable from 240 to 480 pixels on desktop. The Projects explorer owns the bounded
root/child Project hierarchy; the detail workbench never duplicates that navigation as a list. The
Builds explorer follows that same hierarchy and lazily loads each Project's Build Configurations
and each Configuration's newest Builds with an independent cursor per expanded branch. A Build
deep link reveals its Project ancestry without flattening nested Projects, and every explorer makes
its bounded-page scope explicit rather than claiming global completeness.

The header exposes readiness, Command/Ctrl+K resource search and safe navigation/presentation
commands, system/light/dark theme selection, and a notification center. Notifications combine
three bounded sources: current-tab definitive command outcomes, favorite-resource attention, and
critical system attention. Notification items and request identities stay in memory; only the
center's last-opened timestamp is stored in the versioned browser-local preference record. The
operator menu owns the infrequently changed language selection plus deployment references. The
console deliberately has no
login route, browser credential storage, fabricated browser identity, or logout action, and it
keeps the unauthenticated trusted-network boundary in deployment and management documentation
rather than a persistent application banner.
Explorer search entries pass typed server resource kinds into the shared command center; clearing
that scope returns the same command center to global search.
