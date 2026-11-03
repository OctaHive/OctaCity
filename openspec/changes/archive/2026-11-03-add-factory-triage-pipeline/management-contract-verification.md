# Shared management contracts (task 7.3)

Verification date: 2026-10-29.

## Delivered contract and ownership

The ordinary REST operation inventory and typed authorization mappings continue to
own preset discovery, preset resolution and exact Configuration/Repository
readiness. Discovery and resolution require Project View; readiness requires
Configuration View. All three document `no-store`. The POST queries require no
idempotency key or mutation precondition. Existing route/authorization parity,
forbidden-before-dispatch and dangling-reference checks remain in force.

The Factory preset/readiness OpenAPI components reuse ordinary Flow,
Configuration, Repository, Role/Profile, Source and retained-result references.
The context draft schema now matches its existing DTO: `project_id` and
`documents` are required; absent goals/rules remain incomplete draft selections
with ordinary diagnostics. This does not make either document optional for
execution or manufacture missing Project context.

`SourceAcceptanceMapping` is now published as the existing closed preparation
choice: `field`, `body`, `body_section` or explicit `process_completion` with a
`SourceCompletionDocument`. The field/heading byte ceiling is shared with the
existing mapper through `MAX_SOURCE_ACCEPTANCE_SELECTOR_BYTES`; its value and
mapping behavior are unchanged. This type describes the existing reviewed
installation handoff; it adds no preparation endpoint, publication or activation
command. The resolver's existing `completion` response retains its meaning.

The ordinary `ui/scripts/generate-api.mjs` generates the TypeScript declarations.
Its existing schema traversal also retains preset requests,
readiness selections and preparation choices with their reachable server-owned
bounds. The generated `factory-setup.ts` imports common schemas from
`factory-authoring.ts` and adds only setup-specific components. Ordinary editor
bundles do not load this additional export. Generated files remain ignored build
artifacts. No second generator,
contract registry, result format or triage client transport is introduced.

Generated-client tests exercise the shared management transport and exact wire
paths/bodies, without mutation headers. Type assertions preserve optional
`label_presence` with legacy `any` semantics, exact Role/Profile and Configuration
pins, optional canonical Issue correlation and ordinary retained result schema
references. Actual result values remain governed by the admitted result schema;
this task does not introduce a triage-specific execution-read API.

## TDD evidence

All Cargo commands use `--locked -j2`, `CARGO_PROFILE_DEV_DEBUG=0`,
`CARGO_PROFILE_TEST_DEBUG=0` and `CARGO_INCREMENTAL=0`. API generation additionally
uses `CARGO_BUILD_JOBS=2` for its ordinary Cargo export subprocess.

| Boundary / command filter | Observed Red | Observed Green |
| --- | --- | --- |
| REST `--test v1_handlers preset_context_schema_preserves_missing_document_diagnostics` | DTO accepts omitted goals/rules; published schema rejects missing goals | Same incomplete selection matches the schema and retains `MissingProjectGoals` |
| REST `--test v1_handlers source_preparation_schema_preserves_explicit_completion` | Missing `SourceAcceptanceMapping` component | Existing DTO wire representations for all four preparation choices match the published schema |
| `node --test --test-name-pattern='exports preset, preparation' ui/scripts/factory-authoring-schemas.test.mjs` | Preset authoring schema is `undefined` | Preset, readiness and preparation schemas retain their reachable limits |
| `pnpm --dir ui typecheck` with `factorySetupContracts.test.ts` | Missing preparation type and incorrectly required goals/rules | Ordinary `pnpm --dir ui api:generate` followed by typecheck succeeds |

The explicit setup authorization/bounds test characterizes already implemented
7.1/7.2 behavior and passes without changing those route mappings. It is not
reported as a new behavioral Red. The first combined authoring export exceeded the existing bundle budgets by
7,048 JavaScript bytes and 6,965 total output bytes. A new extraction test failed
because setup-only roots leaked into ordinary editor exports; separate exports
using the same traversal then passed that test and the unchanged build budgets.
The selector-limit extraction is a pure
refactoring: its existing Source mapping/preparation suite passes before and
after the extraction. An initial filter collecting zero tests and a missing test
import were corrected and do not count as behavioral evidence.

## Regression and review

The initial REST baseline passed 28 DTO contract tests and 222 handler tests;
the exact Source mapping/preparation baseline passed 30 tests. Final checks passed:

```sh
cargo test --locked -j2 -p octacity-server-api-rest --test v1_contract --test v1_handlers
# 28 DTO contract tests and 225 handler tests passed
cargo test --locked -j2 -p octacity-server-application --lib installed_sources::mapping
# 30 passed before and after the shared-limit extraction
cargo clippy --locked -j2 -p octacity-server-application -p octacity-server-api-rest --all-targets --all-features -- -D warnings
pnpm --dir ui api:generate
pnpm --dir ui typecheck
pnpm --dir ui lint
pnpm --dir ui format
pnpm --dir ui test:unit
# 38 Node tests and 853 Vitest tests in 107 files passed
pnpm --dir ui build
# TypeScript, production build, frontend architecture and unchanged bundle budgets passed
cargo fmt --all -- --check
python3 tools/check_architecture.py
# 64 packages, 263 dependency edges
openspec validate add-factory-triage-pipeline --strict
git diff --check
```

The change keeps the existing validation and authorization owners. Production
changes are limited to contract publication, one shared existing byte limit and
separate setup exports using the existing schema traversal. Common generated
components are imported rather than copied into the setup export. Review found no added runtime
coupling, duplicated mapper, provider/model choice or independent business limit.
Type tests run in the normal UI suite and typecheck; schema extraction tests
already run in both standard UI test commands.

This is metadata/contract verification, not packaged execution qualification.
At this verification stage, tasks 7.4 and later remained pending. Implementation
was completed without commits; committing was separately requested afterward.
Disk space stayed at 45 GiB throughout; no cache or stand-data cleanup was required.
