# Task 7.5 — retained result and delivery diagnostics

## Scope and ownership

The existing Factory Run and Issue Results views render public reviewed Project
and analyst results. They show gate acceptance/outcome, the original bounded
reason, classification fields, unchanged admission priority/risk, dependencies,
reproduction assessment and separate retained command evidence. Evidence includes
its exact subject, environment/criterion pins, original validity and Build/attempt/
Job identities. Missing evidence remains absent; model prose is not execution.

The public eligibility projection now includes optional native lookup metadata:
consumed connection/resource pins, capture status, unknown coverage and exact
retained observation references. Interrupted capture and unavailable scope are
explicit. Original observation bytes remain behind the authorized Build Artifact
view. The analyst binding copies only the four selected eligibility decision
fields, excluding lookup metadata, claims and private Project Context.

Safe producer provenance comes from the original executing Build and explicitly
selected predecessor record references in the frozen input. Public completion
reads reuse the existing Call projection: original attempt, tool/model/task/Build
binding, selected Role Profile pin, Context identity/digest and permission digest, without Context bytes or
candidate inventory. The UI renders those exact references immediately and links
the corresponding original Call; it never guesses a producer by stage name.
Historical/private outcomes do not gain disclosure retroactively.

Current local Issue business status remains in the existing independently read
Issue header. Original Issue/Run navigation preserves the selected execution.
The new `effect-delivery` collection extends the existing diagnostic endpoint,
port and memory/PostgreSQL adapters. It selects the latest immutable receipt per
logical operation with the same attempt/state/time/identity ordering as recovery.
It does not add an endpoint, state owner, table, worker or retry command. Receipts
omit ownership fences, credentials and input bodies. Only `work.status.projection`
is labelled Reporter delivery; admission effects retain their own technical kind.
Pending/claimed, delivered, and failed forbidden/unsupported/exhausted/conflict
conditions remain separate from accepted AI assessments and local Issue status.

Existing guarded human controls remain responsible for eligibility, original Run
version and confirmation. Result navigation/reload performs read-only operations.
No browser draft/result persistence or hidden-context expansion was introduced.

## TDD evidence

Every behavior-changing slice observed a relevant failure before implementation;
compilation/import/environment failures were corrected and are not Red evidence.

| Public boundary / command | Observed Red | Green verification |
| --- | --- | --- |
| `cargo test --locked -j2 -p octacity-server-application --lib factory_discovery::diagnostics::public_results` | Reviewed Project outcomes were private | Both reviewed gates public; actual authorized reads preserve privacy and snapshots |
| `cargo test --locked -j2 -p octacity-server-application --lib effect_delivery_reads_retained_states` | Unknown `effect-delivery` collection | Safe pending receipt; unchanged snapshot/mutation evidence |
| `cargo test --locked -j2 -p octacity-server-api-rest --test v1_handlers effect_delivery_contract` | Missing safe delivery component | Existing tagged diagnostic page accepts the documented receipt |
| `cargo test --locked -j2 -p octacity-server-application --lib reviewed_eligibility_publishes_lookup_scope` | Public lookup coverage absent | Native observations preserved; analyst receives exactly four selected decision fields |
| `pnpm exec vitest run src/shared/FactoryDiagnosticRecord.test.tsx` | Public results/evidence/delivery/coverage were not rendered | Reviewed summaries, exact evidence/dependencies and independent Reporter states |
| `pnpm exec vitest run src/features/factory/FactoryRunDiagnostics.test.tsx -t 'opens retained assessments'` | Results absent; then provenance pointed at the deterministic gate | Original agent attempt and Context displayed; no control mutation |
| `pnpm exec vitest run src/features/issues/IssueResults.test.tsx -t 'reads the selected original Run triage result'` | Selection fell back to another collection | Exact original Work result and Run delivery link |
| `cargo test --locked -j2 -p octacity-server-application --lib retained_public_results_read_classification` | Public producer provenance absent | Exact retained Call/Context; private outcomes disclose none |
| `cargo test --locked -j2 -p octacity-server-application --lib call_diagnostics_retain_the_exact_selected_role_profile` | Selected Role Profile pin absent from Call metadata | Exact immutable selected pin, without package/instruction/Context bytes; legacy static binding remains absent |
| `pnpm exec vitest run src/shared/FactoryDiagnosticRecord.test.tsx -t 'preserves a free-form'` | Reason `critical` became enum label `Critical` | Original reason retained verbatim |
| `node --test --test-name-pattern='measured triage result' scripts/verify-architecture.test.mjs` | Measured JS/static output exceeded the previous allocation | Dedicated 32 KiB aggregate allocation; another KiB near ceilings rejected |

The native PostgreSQL journal contract traverses the same safe effect read after
restart with limit one, checks exact latest receipt identities and verifies the
snapshot is unchanged. Browser checks use intercepted management fixtures; they
are Console contract evidence, not the packaged upstream qualification of 8.1–8.3.

## Review and validation

Independent specification and standards reviews found provenance and reason issues, corrected with
behavioral Red/Green: gate provenance substituted for agent provenance, the chosen
Role Profile pin was absent from Call metadata, and enum
translation altered a free-form reason. Unused duplicate English choice labels
were removed while keeping the behavioral tests green. No new runtime dependency,
ambient agent context, persistence owner or provider write path was added.

Final validation evidence:

| Command | Result |
| --- | --- |
| `cargo test --locked -j2 -p octacity-server-application -p octacity-server-factory -p octacity-server-store --lib -- --skip factory_release_journey_tests --skip factory_requirements_tests` | Application subset: 864 passed; the 11 unrelated release/requirements qualification cases are explicitly outside this task regression |
| `cargo test --locked -j2 -p octacity-server-factory -p octacity-server-store --lib` | 325 Factory and 109 Store tests passed |
| `cargo test --locked -j2 -p octacity-server-api-rest --test v1_handlers` | 230 passed |
| `OCTACITY_POSTGRES_URL=<disposable> cargo test --locked -j2 -p octacity-server-store-postgres --test factory_discovery -- --ignored --test-threads=2` | 42 passed on PostgreSQL 18.6, including native restart/pagination contracts |
| `cargo clippy --locked -j2 -p octacity-server-application -p octacity-server-api-rest -p octacity-server-store-postgres -p octacity-server-factory -p octacity-server-store --all-targets --all-features -- -D warnings` | Passed |
| `cargo fmt --all -- --check`; `python3 tools/check_architecture.py` | Passed; 64 packages / 263 dependency edges |
| `pnpm typecheck`; `pnpm lint`; `pnpm format`; `pnpm build` | Passed |
| `pnpm test:unit` | 38 Node checks and 890 Vitest tests / 110 files passed |
| `pnpm exec playwright test tests/browser/factory-triage-results.spec.ts tests/browser/factory.spec.ts tests/browser/factory-issue-status.spec.ts` | 10 passed; English/Russian, 390px keyboard navigation, original producer references, hidden-context omission and no read-triggered mutation |
| `openspec validate add-factory-triage-pipeline --strict`; `git diff --check` | Passed |

A broader intermediate application run also passed 872 tests with two very long
requirements qualification cases filtered. The final relevant application subset
above includes the added selected-Profile diagnostic test. Its combined invocation
then found the obsolete classification-only Factory assertion; the corrected
Factory and Store suites were separately rerun successfully as recorded above. These counts do not
claim an unfiltered whole-workspace or packaged upstream qualification. The
existing classification-only disclosure expectation was updated to the reviewed
Project-plus-classification policy; that test maintenance is not additional Red
evidence.

The final build contains 1,417,575 JavaScript bytes, 90,943 CSS bytes and 1,509,140
static bytes in 53 files. Entry JavaScript remains 305,431 bytes. The dedicated
32 KiB aggregate allocation brings JS/static ceilings to 1,436,160 / 1,527,808;
entry, CSS, asset count and dependency limits are unchanged. The ceiling test
retains an independent near-limit rejection case.

Rust checks used `CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
CARGO_INCREMENTAL=0` with `-j2`; API generation additionally used
`CARGO_BUILD_JOBS=2`. The disposable PostgreSQL data directory used a 384 MiB tmpfs
and its container was removed after checks. Free disk stayed at 37–38 GiB; no
unrelated files or caches were removed. No commits were made during implementation. Task 8 remains
explicitly outside this work.
