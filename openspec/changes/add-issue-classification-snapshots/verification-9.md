# Project sizes and Delivery specification threshold

Verified on 15 November 2026 against the working tree based on `02f5049d`.
This follow-up supersedes the fixed size vocabulary in tasks 2.1 and 6.6;
earlier qualification reports remain historical evidence.

## Behavior and compatibility

Project field definitions now own an ordered `sizes` catalog, initially
`small`, `medium`, `large`. Size uses the existing choice publication,
retirement, historical-value and replay contracts. Custom size IDs flow through
manual and trusted classification and selected execution snapshots. New size
attribution pins the definition version; legacy size references without that
version still decode. The standard size list is omitted from serialized
definitions and selected snapshot fragments to preserve existing publication
and replay bytes. Custom selected size vocabularies are retained explicitly.
Existing JSONB storage is reused; no SQL migration is needed.

The Delivery v2 example requires the operator to select **Create specifications
starting at size**. The form shows Project display names in published order
while submitting stable IDs. Optional `choice_labels` on the field discovery
contract supplies names without duplicating the allowed IDs or order in `shape`.
The resolver compiles the selected active size and all greater active choices
into the ordinary `requirements_needed` gate. Smaller sizes follow the prepared
requirements path. The separate requirements approval gate remains independently
editable. Execution uses the published rules and admitted snapshot; subsequent
Project reordering does not change either one. Inspect or change the compiled
rules in the ordinary Flow editor and publish a new Flow to change policy.

Old resolver clients may omit the threshold only with the compatible active
order `small`, `medium`, `large`, retaining the released default route. A custom
catalog requires an explicit active threshold. Missing, unknown or retired
thresholds produce `/issue_snapshot/specification_from_size` diagnostics.
Released preset JSON, existing Flow/Configuration publications and retained Runs
are not rewritten.

## Observed TDD cycles

Tests preceded each behavior change. Environment or compilation/setup failures
were corrected separately and were not counted as behavioral Red.

| Public boundary | Observed Red | Observed Green |
| --- | --- | --- |
| Issue catalog/edit/classification/replay | Custom `sizes` rejected as unknown; new size attribution lacked its definition version | Ordered custom IDs accepted, foreign/retired new selections rejected, unchanged retired values and original receipts retained |
| Selected execution snapshot | Custom selected size vocabulary absent | Exact custom size vocabulary retained; legacy standard serialization unchanged |
| Authoring field discovery | Fixed three-size enum instead of the active ordered Project catalog | Active custom IDs in Project order |
| Delivery resolution and execution | `specification_from_size` rejected as unknown | Seven routing cases across default and custom thresholds; later catalog reorder leaves admitted routing unchanged |
| REST/OpenAPI | New catalog/threshold values absent from the closed contract | Additive public contracts accepted, including versioned size attribution |
| Constraint generator | Missing built-in choice contract when planning size no longer had a fixed enum | Defaults derived from the Project catalog schema |
| Project/Issue Console | Missing Sizes controls | EN/RU keyboard editing, publication, custom XL selection and save |
| Catalog conflict review | Local order discarded; concurrent different orders not offered for review | Local order preserved across unrelated remote changes; explicit local/current decision for conflicting orders |
| Delivery Console | Missing threshold selector | Explicit selection required before opening the example; selected stable ID sent to resolver |
| Threshold display names | Discovery returned no XL name; two v2 UI cases could not find the Project display name | Discovery returns the Project name; form shows `Project medium` while sending `medium` |

Focused Red commands were rerun after implementation and passed:

```sh
cargo test --locked -p octacity-server-store --lib builtin_size_uses_ordered_project_choices_and_retains_history_and_replay
cargo test --locked -p octacity-server-application --lib authoring_size_contract_uses_the_active_ordered_project_catalog
cargo test --locked -p octacity-server-application --lib issue_delivery_size_threshold_routes_the_frozen_project_order
cargo test --locked -p octacity-server-api-rest --test v1_handlers project_size_catalog_and_delivery_threshold_have_closed_public_contracts
pnpm --dir ui exec node --test --test-name-pattern='derives default size' scripts/issue-constraints.test.mjs
pnpm --dir ui exec vitest run src/features/issues/ProjectSizes.test.tsx
pnpm --dir ui exec vitest run src/features/factory/FactoryAuthoring.test.tsx
```

The final focused Console run of both files passed all 12 tests. The final
authoring display-name regression passed at the application boundary as well.

## Regression checks

Rust builds used `CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=4` to limit local build storage.

```sh
cargo test --locked -p octacity-server-store --lib
cargo test --locked -p octacity-server-application --lib issue_
cargo test --locked -p octacity-server-api-rest --test v1_handlers
cargo test --locked -p octacity-server-store-postgres --test issues -- --ignored
cargo check --locked -p octacity-server --all-targets
cargo check --locked -p octacity-server-store-postgres --all-targets
cargo clippy --locked -p octacity-server-store -p octacity-server-application -p octacity-server-api-rest -p octacity-server-store-postgres --lib -- -D warnings
cargo fmt --all -- --check
python3 tools/check_architecture.py
python3 -m unittest tools.tests.test_verify_local_stand_context.LocalStandContextTests.test_ui_api_generators_load_from_the_filtered_docker_context
pnpm --dir ui api:generate
pnpm --dir ui test:unit
pnpm --dir ui typecheck
pnpm --dir ui lint
pnpm --dir ui format
pnpm --dir ui build
pnpm --dir ui exec playwright test tests/browser/project-issue-fields.spec.ts tests/browser/issue-properties.spec.ts tests/browser/factory-ticket-delivery.spec.ts tests/browser/issue-execution-snapshots.spec.ts
openspec validate add-issue-classification-snapshots --strict
git diff --check
```

Results: 129 core-store tests, 277 affected application tests, 289 REST tests,
83 PostgreSQL Issue tests, 47 Console script tests and 1258 Console unit tests
passed. PostgreSQL coverage includes custom size round trips, retirement, store
recreation and original publication/edit replay. The database was a separately
owned disposable PostgreSQL 18.6 container; it was removed after testing.

The first browser run exposed an obsolete Russian assertion that expected the
old fixed-size translation. Project display names now come from published data.
The updated Project scenario publishes and reorders XL, preserves it across a
publication conflict and retry, then selects and saves it on an Issue at 390px
in both languages. The final four-suite browser run passed all 9 tests.

API/client regeneration, typecheck, lint, formatting, production build and
existing bundle budgets passed. After adding display-name metadata, focused
application/Console tests, the complete REST suite, typecheck, lint, formatting
and strict library Clippy were rerun successfully. Architecture validation
passed for 64 packages and 264 dependency edges. Documentation/planning edits
have no executable behavior; strict OpenSpec and whitespace checks validate them.

## Qualification and deployment limits

The released UI authoring journey now selects the explicit `Large` threshold.
The full native workflow that creates a draft PR was not rerun for this
follow-up; section 8 records its earlier successful qualification. The checks
above establish the changed catalog, UI, resolver, interpreter, persistence and
compatibility boundaries without claiming another native publication.

No commit, stand image update or ordinary-stand restart was performed. The
running stand must use updated Server and Console builds before the new Sizes
editor and threshold control appear. No new installer flag is required.

## Post-review fixes: independent conflicts and frozen size names

Two separate behavioral Red/Green cycles cover the review findings:

- A valid size choice with ID `order` previously shared the `sizes/order`
  decision key with a catalog order conflict. Two UI scenarios first failed
  because the form exposed duplicate conflict controls. Order decisions now use
  a separate key containing `@`, which cannot appear in a choice ID. Both mixed
  resolutions are tested through publication: local name/current order and
  current name/local order. The complete ProjectSizes file passed all 6 tests.
- Snapshot preview and execution detail previously rendered only the custom
  size ID despite having its original display name in retained definitions.
  Four UI scenarios first failed to find the original name. The view now includes
  sizes in its existing retained-catalog choice lookup. Both `selected_v1` and
  full legacy definition encodings show the original name and ID after a live
  Project rename. They never substitute the current name. The complete snapshot
  file passed all 25 tests; records lacking retained labels keep the ID fallback.

```sh
# Red: 2 failed (duplicate conflict controls), then Green: all 6 passed.
pnpm --dir ui exec vitest run src/features/issues/ProjectSizes.test.tsx -t 'resolves a size named order'
pnpm --dir ui exec vitest run src/features/issues/ProjectSizes.test.tsx
# Red: 4 failed (missing frozen name), then Green: all 25 passed.
pnpm --dir ui exec vitest run src/features/issues/IssueExecutionSnapshots.test.tsx -t 'retains the original size name'
pnpm --dir ui exec vitest run src/features/issues/IssueExecutionSnapshots.test.tsx
```

The two fixes change only Console reconciliation and rendering. They add no
runtime policy, catalog migration, server contract or reinterpretation of
historical values. The operator walkthroughs document independent conflict
decisions and frozen size labels. Documentation-only edits require link,
formatting and whitespace checks, rather than an executable TDD cycle.

Post-fix regression checks passed: 1264 Console unit tests across 144 files,
47 script tests, typecheck, lint, formatting and production build with unchanged
bundle budgets. The two affected Playwright suites passed all 4 EN/RU scenarios
at narrow layout. Architecture validation, strict OpenSpec, the walkthrough's
4 local links and whitespace checks passed. No Rust behavior changed in these
review fixes, so the previously recorded Rust/PostgreSQL results remain separate
evidence rather than a claimed rerun. Native PR qualification was not repeated.

A final test-data cleanup replaced `size-xl` with the valid Project choice ID
`size_xl` in the snapshot regressions and their expected labels. All 25 tests in
`IssueExecutionSnapshots.test.tsx` passed again; touched-file formatting and
whitespace checks passed. This cleanup changes no runtime behavior.

```sh
pnpm --dir ui test:unit
pnpm --dir ui typecheck
pnpm --dir ui lint
pnpm --dir ui format
pnpm --dir ui build
pnpm --dir ui exec playwright test tests/browser/project-issue-fields.spec.ts tests/browser/issue-execution-snapshots.spec.ts
python3 tools/check_architecture.py
openspec validate add-issue-classification-snapshots --strict
git diff --check
```

## Stand restart with a profile catalog larger than a fixture receipt

An ordinary restart failed after Delivery installation growth because both
classification-upgrade catalog reads reused the 128 KiB fixture receipt limit.
The installed private catalog was 151036 bytes, a regular file with mode `0600`,
within the Server's existing 4 MiB profile catalog limit. The original read
reproduced the exact `Factory fixture receipt has an invalid size` diagnostic.

The public CLI recovery regression first failed with that same diagnostic on a
valid installed catalog padded above 128 KiB. The separate oversized-receipt
characterization passed before the fix. Both catalog reads now explicitly use
the existing Server catalog bound and the catalog diagnostic name, while other
receipt reads retain their original default. After the fix both tests passed:
two repeated CLI resumes retain the catalog and original receipt bytes and
publish no Source configuration. The installed 151036-byte catalog also reads
successfully through the corrected call without any write to the user's stand.

```sh
# Red: 1 failed, 1 passed. Green: both passed.
PYTHONPATH=tools:tools/tests python3 -m unittest \
  test_factory_stand_recovery.FactoryStandRecoveryTests.test_default_resume_accepts_a_catalog_larger_than_a_fixture_receipt_without_republishing_state \
  test_factory_stand_recovery.FactoryStandRecoveryTests.test_default_resume_keeps_the_smaller_fixture_receipt_size_limit
```

The local-stand runbook explains the distinct limits and recovery by rerunning
the original command/root after updating launcher code. No catalog truncation,
stand reset or republishing of retained pipeline definitions is required. Full
native startup of the user's stand was not performed for this fix.

Related regression suites completed successfully: 57 tests run, with 3
environment-dependent skips. Python compilation, strict OpenSpec validation and
whitespace checks passed. Documentation changes have no executable TDD behavior.

```sh
PYTHONPATH=tools:tools/tests python3 -m unittest \
  test_factory_stand_recovery test_factory_stand_triage \
  test_factory_stand_policies test_factory_local_stand
python3 -m py_compile tools/factory_local_stand.py tools/tests/test_factory_stand_recovery.py
openspec validate add-issue-classification-snapshots --strict
git diff --check
```
