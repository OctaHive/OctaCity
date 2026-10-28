# Task 2.1 — Runtime-to-profile guidance

## Delivered boundary

The existing Project Runtime view links directly to its AI-agent/profile journey
for model, plugin-declared reasoning and budget selection. The existing mounted
profile editor retains its draft during the round trip. Returning refreshes offered
choices but neither selects a new binding nor publishes or creates a profile.

Installation guidance identifies the reviewed Factory catalog, independent Agent
policy and non-destructive update flags. It distinguishes model discovery from
plugin execution permission, network authority and installed stdio executables.
The required AI-plugin/native-client transport, authentication, enforced read-only
resource and native capture qualification are explicit administrator prerequisites.
The view does not infer worker readiness, connectivity or capture support from
authoring choices, and does not introduce a readiness service or browser installer.

Runtime rows can explicitly select an installed authenticated MCP contract. Its
exact transport, logical credential reference, purpose and resource scope are
copied for review; secret values are never requested. Unqualified bearer/secret
mappings or changed installed transport/authentication contracts cannot enter
review. Publication and dispatch retain the authoritative server checks and
independent Agent ceilings; selecting a connection creates no credential or grant.
AI-provider login, MCP reading and server Reporter writing remain distinct.

The existing authorized profile-capability content read now batches resolution
of its currently offered exact MCP pins through the retained Runtime settings
store. Profile controls display the same read-authority summary for those exact
references. Withdrawn choices are absent; no current-head substitution, secret
resolution or additional browser query is used. OpenAPI describes the existing
Runtime authentication fields and the added optional profile-content projection;
generated TypeScript remains derived from that contract. Token-value fields are
rejected both by the documented schema and the real HTTP parser.

Review and success messages explicitly confirm configuration only: no MCP
connection test or execution occurred. There is no new model editor, provider
wrapper, permission registry, browser persistence or automatic task creation.
The full installed stand/native-client journey remains task 3.1.

## Observed TDD cycles

Each focused test was run before its implementation and again after it.

| Public boundary / command | Red → Green |
| --- | --- |
| `pnpm exec vitest run src/features/factory/ProfileRuntimeSelection.test.tsx -t 'returns from Runtime setup'` | Missing model/reasoning handoff link → Project-scoped keyboard-accessible return, preserved draft and explicit binding adoption. A later cycle first failed on absent credential/scope text, then passed with the shared summary in the existing profile editor. |
| `pnpm exec vitest run src/features/factory/RuntimeSettings.test.tsx -t 'gives installation steps'` | Missing installation guidance → reviewed catalog/Agent flags, plugin/network prerequisites and capture qualification visible without hiding profile authoring. |
| `pnpm exec vitest run src/features/factory/RuntimeSettings.test.tsx -t 'explicitly selects installed MCP read authority'` | Missing installed-connection selector → exact reference/scope selection and reviewed reference-only request. |
| `pnpm exec vitest run src/features/factory/RuntimeSettings.test.tsx -t 'blocks an unqualified MCP'` | No unsupported-authentication reason or refusal → explicit installation reason and disabled review, with no validation/publication request. |
| `pnpm exec vitest run src/features/factory/RuntimeSettings.test.tsx -t 'reviews and explicitly saves'` | Missing configuration-only feedback → review and success explicitly exclude connection testing and execution. |
| `cargo test --locked -p octacity-server-api-rest --test v1_handlers runtime_mcp_authority_is_documented -j2` | OpenAPI rejected the real supported `authorization` property → typed reference/scope metadata, semantic refusal for an uninstalled credential and schema/HTTP refusal of a token-value field. |
| `cargo test --locked -p octacity-server-application --lib authenticated_runtime_settings_require_exact_installed_peer -j2` | Profile-capability content omitted the exact connection authority → retained-pin metadata returned for available choices and removed from offered content after authority withdrawal. |

Cargo commands used two build jobs and
`CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0`.
No paid model call, live MCP/provider mutation or personal stand change was used.

## Regression and inspection

- `pnpm test:unit`: 36 Node tests and 848 Vitest tests passed (106 Vitest files).
- Focused Runtime/profile component regression: 22 passed.
- `pnpm exec playwright test tests/browser/runtime-settings.spec.ts tests/browser/factory-profiles.spec.ts`:
  7 passed. Both Runtime locales use 390 px, keyboard confirmation/navigation,
  explicit MCP adoption, preserved draft and unchanged browser storage. Overflow
  is checked in the expanded Runtime form, profile editor and final view.
- Runtime application regression: 13 passed.
- Role/profile application regression: 9 passed.
- Full REST handler/OpenAPI regression: 185 passed.
- UI lint, TypeScript, Prettier and production bundle checks passed; Rust Clippy
  for application/REST libraries, server compilation, formatting and architecture
  validation passed. Architecture remains 63 packages and 261 dependency edges.
- Screenshots were inspected from `/tmp/runtime-settings-editor-ru.png` and the
  Runtime/profile EN/RU browser journey. Existing form/layout components wrap
  credential and scope metadata without additional styling.

The first production build exceeded the prior total-output budgets with the new
forms, localized guidance and contract metadata. Measured output is 1,332,148 JS
bytes and 1,423,713 total static bytes. Following the existing feature-budget
convention, only those two budgets increased by 16,384 bytes; entry JS, CSS,
asset-count limits and dependency allowlists are unchanged. The resulting entry
chunk is 304,779 bytes, below its existing 460,800-byte limit. This is a documented
feature-size allowance, not a claimed bundle-size optimization.

Disk checks preceded and accompanied Rust/type generation and UI builds. Removing
only obsolete duplicate Rust test executables recovered 0.86 GB; retained libraries,
stand data and unrelated work remained intact. Subsequent checks stayed above
10 GB. Documentation uses OpenSpec/format validation rather than fabricated TDD.
At completion 10.4 GB remained free. No commit was created for task 2.1.

## Review follow-up

Profile capability details now include authenticated MCP metadata supplied by the
installed catalog before any Runtime settings publication. Resolution reuses the
same authorized catalog snapshot, matches full immutable references and batches
only unresolved retained pins. It neither resolves secrets nor substitutes a
current configuration for an installed pin.

The public query test first observed a selectable installed pin with missing
metadata, then passed after the fix:

```shell
cargo test --locked -p octacity-server-application --lib profile_details_include_exact_installed_mcp_authority_before_runtime_publication
```

Runtime review now also requires the exact installed Project integration binding.
Clearing its selection, selecting a different access version or removing the
binding disables review and explains how to restore it. The rendered form tests
also restore the correct binding and observe review becoming available again,
without validation or publication requests:

```shell
pnpm exec vitest run src/features/factory/RuntimeSettings.test.tsx -t 'required integration changes'
```

All three cases failed before the fix because review remained enabled, then
passed. Final regression commands and observed results:

- `cargo test --locked -p octacity-server-application --lib factory_runtime_settings_tests`: 14 passed.
- `cargo test --locked -p octacity-server-application --lib factory_role_profiles_tests`: 9 passed.
- `cargo test --locked -p octacity-server-application --lib factory_profile_diagnostics_tests`: 1 passed.
- `pnpm exec vitest run --maxWorkers=2`: 851 passed across 106 files.
- `pnpm exec vitest run src/features/factory/RuntimeSettings.test.tsx`: 17 passed after final type corrections.
- `pnpm exec playwright test tests/browser/runtime-settings.spec.ts`: 2 passed, EN/RU.
- UI TypeScript, ESLint and production bundle checks passed without another budget increase.
- `cargo clippy --locked -p octacity-server-application --all-targets --all-features -- -D warnings`: passed.

Behavioral Red/Green logs and final check output are retained under
`/tmp/octacity-review-*.log`. Documentation-only evidence updates do not require
an executable TDD cycle.
