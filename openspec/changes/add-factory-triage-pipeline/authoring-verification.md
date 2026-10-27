# Task 2.3: typed label-presence authoring

## Delivered behavior

The management OpenAPI document now exposes the optional `WorkSourceLabelPresence`
enum (`any|none`), exact-profile `predicate_label_presence` choices, effective
selector presence and the bounded `labels_present` preview reason. The enum's
default is `any`; the default lives on the reusable component so generated
clients keep the referencing properties optional. Generated TypeScript accepts
old definitions without this field. Existing field-validation diagnostics remain
the validation boundary; no new endpoint or provider permission is introduced.

Factory Source inputs offer strictly unlabeled selection only from advertised
choices. The form distinguishes it from literal exclusions, retains exclusions,
and blocks a contradictory required/alternative label instead of deleting it.
The publication review identifies the mode. Omitted/default `any` is still
submitted without an additional field. A saved strict selector survives profile,
container and connection changes: an incompatible profile blocks publication
instead of silently widening the filter. Legacy default-mode profile changes
retain their existing behavior.

Project task-import edit and lifecycle reviews preserve and describe a saved
strict predicate. The existing import form continues to retain its predicates,
rather than introducing a second filter editor or changing its raw reader
preview API. An enabled strict import cannot be republished using an unsupported
profile, including the list-level Enable action; explicitly disabling it remains possible. Preview reasons and new form
messages have English and Russian translations.

Effective preview selector descriptions now carry `label_presence: none`, fixing
an omission that previously described a stricter read as unrestricted. Default
selection metadata still omits `any`. Trusted normalized editing also rejects a
fixed unlabeled-only installed base: that base cannot qualify unrestricted
predicate editing.

## Red/Green evidence

Rust commands used `CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2`. UI checks used the repository-pinned
Node 24.21.0 and pnpm 12.8.1 after `pnpm install --frozen-lockfile --ignore-scripts`.
All results below were observed on 2026-10-27.

| Public behavioral boundary / command | Observed Red | Observed Green |
| --- | --- | --- |
| `cargo test -p octacity-server-api-rest --test integration_administration source_label_presence_schema -j2` | New enum component was absent | Optional typed modes, default, qualified choices, legacy omissions and bounded reason schema pass |
| `cargo test -p octacity-server-application --lib preview_describes_strict -j2` | Effective preview selection lost `label_presence: none` | Authorized preview query returns the independently specified complete selector |
| `cargo test -p octacity-server-application --lib editable_predicates_cannot_widen -j2` | A fixed unlabeled-only base incorrectly qualified unrestricted editing | Trusted public builder rejects that widening |
| `pnpm exec vitest run src/features/factory/SourceInputEditor.test.tsx -t 'selects strictly unlabeled'` | Label-presence control was absent after the legacy exclusion-only intent passed | Strict mode is reviewed/submitted separately; exclusions survive; contradictions block validation without clearing text |
| Source editor translated preview reason case | Rendering `labels_present` failed because its translation was absent | Preview explains the reason and preserves unavailable-versus-empty behavior |
| Project import lifecycle review cases with saved strict predicates | Confirmation omitted the strict mode | Enable/disable reviews describe and preserve the exact saved predicate and replay intent |
| Project import `unsupported saved unlabeled` case | No capability warning; invalid activation remained available | Unsupported enabled settings block review; explicit disable keeps the predicate and pins |
| Project import list-level re-enable with a retained strict predicate | The review opened despite missing exact-profile presence support | Activation stops with the existing capability diagnostic; retry succeeds after compatible metadata is restored |
| `node --test --test-name-pattern='measured Source label-presence' scripts/verify-architecture.test.mjs` | Measured JS/static output exceeded previous aggregate budgets | Measured output fits an explicit 3 KiB budget increment, while another 1 KiB still fails |

Additional HTTP regression coverage sends a strict predicate with exclusions
through the normal authenticated validation route, checks its typed publication
against OpenAPI and independently verifies the returned fields. An unknown mode
returns HTTP 400 before application dispatch. Semantic contradictions remain the
existing application field-validation responsibility, not a new transport rule.

## Regression and compatibility results

- `cargo test -p octacity-server-factory -j2`: 303 unit tests, 42 integration
  tests and one compile-fail doctest passed.
- `cargo test -p octacity-server-application --lib -j2`: all 772 tests passed,
  including the 33 Source configuration/preview/reporting tests and immutable
  legacy-version/pin checks. The full suite took about seven minutes; its long
  Factory journeys completed successfully.
- `cargo test -p octacity-server-api-rest -j2`: all 251 tests passed, including
  schema validation, public HTTP transport and existing authorization contracts.
- `cargo test -p octacity-server --lib offline_label_presence -j2`: the public
  offline qualification regression passed; all 261 server unit tests compiled.
- `pnpm api:generate`: passed; generated types retain optional presence fields.
- `pnpm test:unit --maxWorkers=2`: 36 script tests and 842 UI tests passed.
  After sharing the summary module, the two affected UI files passed all 79 tests, including the final list-level
  re-enable correction and typed-select simplification.
- `pnpm exec playwright test tests/browser/factory-inputs.spec.ts tests/browser/project-sources.spec.ts`:
  all 14 passed after the shared-module refactor. New English/Russian cases use
  keyboard selection/review at 390 px, assert no form/modal overflow, and inspect
  the exact published strict predicate with exclusions.
- `pnpm build`, `pnpm lint`, `pnpm format`, architecture/bundle checks and all
  25 architecture script tests passed. Final output: 1,321,122 JavaScript bytes,
  90,943 CSS bytes and 1,412,687 total static bytes. Entry/CSS/asset ceilings are
  unchanged; JS/static ceilings increased by exactly 3,072 bytes, with less than
  1 KiB spare and explicit excess-growth rejection.
- Scoped `cargo clippy -p octacity-server-factory -p octacity-server-application
  -p octacity-server-api-rest -p octacity-server --lib -j2 -- -D warnings` passed.
  `cargo fmt --all -- --check`, OpenSpec strict validation and `git diff --check`
  passed.

No storage schema or migration changes were necessary. The existing predicate
representation already stores the additive optional field; default-skipping
serialization preserves old canonical bytes, digests, immutable versions and
pins. The existing parity/retained-version regressions passed. No migration
history was rewritten or new migration claimed as tested.

## Group 2 code review and resource use

Reviewed domain validation, the provider-owned selector, verified capability
qualification, runtime composition, Source/Issue boundaries, OpenAPI, UI and
installer changes together. Corrected the effective-selection omission and
restricted-base qualification through Red/Green. The final adjacent-action
review also found and corrected the missing list-level activation qualification. Removed duplicate Factory/import
filter descriptions into `shared/sourcePredicateSummary.ts`; preserved existing
review separators and avoided a dependency from one feature into another.
The typed SelectMenu consumes generated enum choices directly. No provider-name
switch, triage-only selector service, additional runtime dependency or business
label/status constant was added. Native and common enums remain in their existing
independent provider/domain owners. Comments document compatibility and authority
boundaries. No further actionable complexity, duplication or coupling issue was
found in the reviewed group.

Free disk space stayed about 14–15 GiB throughout compilation and production builds,
above the 10 GB cleanup threshold. No personal stand, database or volume was
modified, and no paid model or live GitHub call was used. The pinned temporary
Node runtime was SHA-256 verified. No commit was created. Packaged triage setup,
raw-ticket completion and two-agent execution remain later tasks; these checks
do not claim the full preset is operational.


## Review correction: retained state across profile changes

A subsequent review identified a mismatch when changing a strict input between
qualified profiles with different advertised states: the draft still contained
its previous state, but the menu displayed Any state. The form now keeps the
retained value visible and blocks preview, validation and publication review
while that state is unsupported. The operator can explicitly choose an available
state or Any; the strict presence mode and literal exclusions remain unchanged.
This uses one local validity guard and the existing SelectMenu, intent and
publication boundaries, without another component, dependency or backend change.

TDD on 2026-10-27 used the public Source editor boundary:

- Red: `pnpm exec vitest run src/features/factory/SourceInputEditor.test.tsx -t
  'incompatible saved state'` failed because the displayed value was
  `Any state (no extra restriction)` rather than the retained `native-open`.
- Green: the same command passed after the minimal form correction. Coverage
  then expanded to English/Russian, explicit supported-state/Any recovery,
  invalidated prior review, blocked outgoing requests and preserved exclusions.
- `pnpm exec vitest run src/features/factory/SourceInputEditor.test.tsx
  src/features/issues/ProjectSources.test.tsx --maxWorkers=2`: all 81 passed.
- `pnpm exec playwright test tests/browser/factory-inputs.spec.ts`: all 10 passed.
  The existing English/Russian 390 px scenarios now change between qualified
  profiles, show the retained incompatible state and use keyboard selection to
  resolve it before reviewing and publishing the exact predicate.
- `pnpm build` (including TypeScript and architecture/bundle gates), `pnpm lint`,
  `pnpm format`, strict OpenSpec validation and `git diff --check` passed.

Production output is 1,321,460 JavaScript bytes and 1,413,025 total static bytes;
all existing ceilings remain unchanged. Disk checks before and after the build
showed about 15 GiB free; no cache cleanup was needed. Backend compilation was
not repeated for this UI-only correction. No commit was created.
