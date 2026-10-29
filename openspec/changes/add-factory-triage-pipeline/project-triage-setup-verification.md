# Project triage setup verification (task 7.4)

Verification date: 2026-10-29. This is Console authoring and contract evidence;
packaged execution qualification remains task 8.x.

## Delivered setup and ownership

The Project Factory workbench loads the triage editor lazily and keeps ordinary
authoring and triage editing mutually exclusive. Closing returns keyboard focus
to its opener. The editor resolves reviewed process-completion criteria, selects
exact installed stage profiles and code Repository, assigns goals/rules/named
documents separately to each stage, reviews Source preparation and finite
reporting effects, and publishes ordinary child/root Flows and Configuration.
Partial publication is resumable with its original references. Stale-parent
errors preserve the draft. Import and automatic intake remain independent,
initially disabled operator choices; preview does not enable either.

Runtime/MCP handoffs show safe connection identity, credential-reference and
read-only scope metadata. They never expose token values or treat AI-provider
authentication as Source/Reporter authority. Exact-subject readiness observations
expire; late responses for a previous selection cannot establish readiness for
the current selection. Missing installation, Agent, authenticated scope, capture
or label evidence retains explicit remediation and blocked/unknown status.

No browser storage retains drafts. Published definitions and existing Work/Run,
Context, Artifact and outbox owners continue to retain execution state/results.
Each reasoning stage receives only its selected context and declared projections;
the UI introduces no ambient predecessor history or shared agent state. The
existing `triage_stage_packages_materialize_only_their_declared_context_and_mcp_connections`
behavior remains covered by the application suite.

The approved first-setup extensions use existing owners:

| Contract | Owner and restrictions |
| --- | --- |
| Installed Build compilation binding | Existing capability choices expose the exact server-derived tuple; the browser does not invent toolchain hashes. Unsupported executors retain explicit missing-installation diagnostics. |
| Source preparation mapping/criteria | Existing Source choices expose preparation metadata for the selected Project. Foreign-Project completion documents do not break the remaining choices. |
| `GET /api/v1/projects/{project_id}/input-documents` | Bounded Project-visible metadata catalog: name, exact Artifact reference, publication and retention times; no private document bytes. |
| `POST /api/v1/projects/{project_id}/input-documents` | Explicit confirmed publication of name/content with scoped idempotency. Project Publish and Artifact visibility policies apply; publication is audited once and starts no Work. |
| Installed Reporter action in Source choices | Native `set_status`, exact plugin pin, closed `work.status.receipt` v1 schema and `queued` outcome come from the same declaration as the installed producer. Budgets, retries and permissions remain explicit operator policy. |

Named documents extend `ProjectTextIntake` and the existing Project input journal,
private object publication, fencing, retention and cleanup. The migration widens
the existing object-role constraint; it creates no table. Publication needs no
earlier Build, Flow, Configuration, repository revision or admission. Work and
Document intents share scoped replay keys and reject purpose collisions as
conflicts. Exact bytes remain private and recoverable after an unknown write;
replay never renews retention. Common lease/object identity and verified byte
publication rules are factored once without changing retained serialization.

The native Reporter producer is installed before the ordinary executor registry
is frozen. Its metadata projection contains no executor backreference. Installed
execution requires an outbox, explicit Source update and managed connection pins;
ordinary dispatch and delivery still recheck current grants and provenance. A
queued receipt does not prove a native write. Existing alias collisions preserve
the original capability inventory. Legacy explicitly constructed extension
actions keep their prior contract.

Finite outcome/dimension editors use the existing ordinary Flow compiler and
validation. Enum cases and bounded integer bands require complete coverage;
splitting a band preserves its reviewed effects. Dimensions remain independent
rather than generating a Cartesian product. Turning an effect off writes `null`
and retains no hidden label override. Shared generated limits bound aliases,
retry and graph sizes; supporting aliases account for the `document.` namespace.

## TDD evidence

All Cargo commands use `--locked -j2`, `CARGO_PROFILE_DEV_DEBUG=0`,
`CARGO_PROFILE_TEST_DEBUG=0`, `CARGO_INCREMENTAL=0`. API generation additionally
uses `CARGO_BUILD_JOBS=2`. Tests were added and observed Red before their behavior
was implemented. Refactoring used passing existing tests first. Missing test
imports, incorrect fixture translations and environment failures are excluded
from the Red evidence.

| Public test boundary / command | Observed Red | Observed Green |
| --- | --- | --- |
| `cargo test -p octacity-server-application --lib project_text_intake_tests` | Missing standalone document publication/query contracts; missing Project was accepted; publication had zero audit events | Exact named bytes, no admission/revision, bounded metadata discovery, absent-Project refusal, replay/recovery and one audit event; 12 tests passed |
| `cargo test -p octacity-server-store-postgres --test issue_execution standalone_project_documents -- --ignored` | Existing SQL constraint rejected the Document role; Work lookup of a Document purpose returned unavailable instead of conflict | Same journal publishes without admission; purpose collision is a conflict, original retained identity remains intact |
| `cargo test -p octacity-server-api-rest --test v1_handlers` | Missing document routes/component and missing native Reporter schema | Metadata-only document routes, authorization/inventory parity and native action contract; 229 handler tests passed |
| `pnpm --dir ui exec vitest run src/features/factory/FactoryTriageSetup.test.tsx` | Missing managed-document publication/selection; retained controls disappeared for an empty catalog; alias bound exceeded the namespace limit | Confirmed exact publication, separate stage selection, retained-reference removal and shared alias limit |
| `cargo test -p octacity-server-application --lib factory_flow_action_tests` | Missing installed technical action contract; installed action allowed absent outbox or Source/connection pins | Exact queued receipt, no provider write and explicit managed-source requirements; 40 action tests passed and remain covered in the full application suite |
| `cargo test -p octacity-server --lib runtime::source_profiles_file_tests` | Installed producer absent from execution; optional alias collision made the whole inventory unavailable | First setup works without a prior Flow; exact producer/metadata identity and original inventory preserved; 14 tests passed |
| `pnpm --dir ui exec vitest run src/features/factory/TriageReportingEffects.test.tsx` | Missing native contract, enum and integer controls; disabling customization retained its hidden effect | Five tests pass for native metadata, explicit finite effects, safe band splitting, stale contract removal and controlled effect clearing |
| `pnpm --dir ui exec vitest run src/features/factory/TriageReadiness.test.tsx src/features/factory/FactoryDefinitions.test.tsx` | Expired/wrong-subject observations and overlapping editor/focus behavior were not handled | Exact subject and original expiry, stale response omission, exclusive editing and focus restoration |
| `pnpm --dir ui exec playwright test tests/browser/factory-triage.spec.ts` | Goals/rules selectors had accessible names but no visible field labels | English/Russian keyboard setup at 390/640 px: visible labels, confirmed publication, explicit stage assignment, no draft persistence or horizontal overflow; two tests passed |
| `node --test ui/scripts/verify-architecture.test.mjs` | The measured lazy setup exceeded the old aggregate JavaScript/static allocation | Measured setup fits the explicit 64 KiB allocation; a further KiB near either ceiling is rejected; complete Node suite: 38 passed |

The implementation also exercised Red/Green for resolution, exact Build metadata,
foreign-Project Source mapping, frozen criteria, Source remediation and stale or
partially published drafts through the existing preset/component boundaries.
The original temporary logs are `/tmp/octacity-74-*.log`; this document records
their durable outcomes rather than depending on temporary files as deliverables.

## Regression, review and limits

Executed regression commands:

```sh
cargo test --locked -j2 -p octacity-server-application -p octacity-server-factory -p octacity-server-store
# 1450 passed, one opt-in native MCP test ignored
cargo test --locked -j2 -p octacity-server-api-rest --test v1_handlers
# 229 passed
cargo test --locked -j2 -p octacity-server --lib runtime::source_profiles_file_tests
# 14 passed
OCTACITY_POSTGRES_URL=postgres://octacity:octacity@127.0.0.1:55474/postgres \
  cargo test --locked -j2 -p octacity-server-store-postgres --test issue_execution -- --ignored --test-threads=2
# 21 passed, one unrelated test filtered
pnpm --dir ui exec vitest run
# 875 passed
pnpm --dir ui exec playwright test tests/browser/factory-triage.spec.ts tests/browser/factory.spec.ts tests/browser/factory-inputs.spec.ts tests/browser/factory-actions.spec.ts tests/browser/factory-profiles.spec.ts
# 25 passed; final visible-label changes separately rechecked in both triage locales
```

Read-only standards/spec reviews checked the shared document and native Reporter
extensions. Findings corrected through behavioral Red/Green included duplicated
publication/lease rules, the supporting-alias namespace bound, inaccessible stale
selections with an empty catalog, optional Reporter alias collisions and hidden
effect overrides. Both reviewers confirmed their final Reporter findings fixed.
Clippy additionally prompted compact boxed variants in the in-memory journal;
this representation-only refactoring retains existing behavior tests.

Final gates passed:

```sh
cargo clippy --locked -j2 -p octacity-server-application -p octacity-server-api-rest -p octacity-server --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
python3 tools/check_architecture.py
# 64 packages, 263 dependency edges
pnpm --dir ui lint
pnpm --dir ui format
pnpm --dir ui build
# TypeScript, production output and frontend architecture/bundle budgets
openspec validate add-factory-triage-pipeline --strict
git diff --check
```

After the final representation/ref-name/visible-label changes, the 12 Project
input application behaviors, 28 focused Console tests and two locale keyboard
browser scenarios passed again. The disposable PostgreSQL container was stopped.
Documentation/checklist-only edits have no executable behavior requiring a new
TDD cycle; strict OpenSpec and whitespace checks validate those final edits.

No dependency or package was added. The final lazy setup increases aggregate
JavaScript by about 61 KiB over the measured pre-7.4 baseline. The policy assigns
64 KiB to aggregate JavaScript and static output only; entry, CSS and asset-count
ceilings remain unchanged. Final output is 1,394,834 JavaScript bytes, 90,943 CSS
bytes and 1,486,399 HTML/CSS/JavaScript bytes. Entry JavaScript is 305,431 bytes.
The budget test checks the measured fixture and rejects further growth near both
ceilings; splitting the editor merely to hide aggregate growth was not used.

Browser tests use management fixtures; PostgreSQL tests use a disposable local
PostgreSQL 18.6 instance. Neither proves a normal-stack live LLM/GitHub journey.
No paid model invocation, live provider write, implicit permission grant or stand
reset was performed. Native-write readiness remains independently unknown until
qualified. Task 7.5 and packaged 8.x remain pending. Implementation and review
corrections were completed before the user's separate request to commit them.

Disk was checked throughout. Task-created incremental caches were removed after
their run completed; older caches and stand data were preserved. Final free space
is approximately 38 GiB after the final review corrections, above the 10 GB cleanup threshold.

## Final review corrections

The initial Source input is a one-time handoff to ordinary publication. Adding
another input starts a fresh identity rather than reusing that handoff. Successful
publication consumes it in the containing pipeline-input view, so hiding and
reopening automatic intake cannot turn a published input into an unsaved create
request. Published inputs remain editable through their ordinary catalog/version.
No draft or consumption state is written to browser storage.

Repository detail responses and errors are applied only for the latest explicit
selection request. Earlier requests cannot overwrite the operator's later choice.
Resolution diagnostics now identify the affected stage/parameter and distinguish
required selection from unavailable/incompatible dependencies. Exact Build
bindings link to Runtime setup, profiles to AI-agent setup and Source/effect
dependencies to Integrations. Both locales use the existing safe path/reason
contract; raw parameter paths and hidden reference identities are not rendered.

| Command from `ui/` | Observed Red | Observed Green |
| --- | --- | --- |
| `pnpm exec vitest run src/features/factory/SourceInputEditor.test.tsx -t 'publishes the reviewed initial input once'` | A later added input still used `reviewed-input`; a second slice restored the consumed initial editor when reopening automatic intake | Fresh identity, original create precondition, and no restored published draft. Source editor/pipeline-input regression: 59 tests passed |
| `pnpm exec vitest run src/features/factory/FactoryTriageSetup.test.tsx -t 'retains the last selected code Repository'` | Late success changed B back to A; late failure displayed an obsolete error | Both cases retain B and send B in the next resolution request; setup regression: 12 passed |
| `pnpm exec vitest run src/features/factory/FactoryTriageSetup.test.tsx -t 'explains resolution blockers'` | No accessible dependency list or specific Build/context/effect reason in either locale | Both locales expose the stage, subject, safe reason and appropriate setup links; setup regression: 14 passed |

Logs for these corrections are `/tmp/octacity-74-review-*.log`. Corrections to test
imports, indexed generated types and an older test's global unique-link assumption
are verification repairs, not additional behavioral Red evidence. The existing
workbench legitimately exposes both its navigation link and a diagnostic handoff.

Corrections touch Console code/tests and this evidence only. Server/store contracts
and the earlier Rust qualification remain unchanged. No dependencies, bundle
allowances or new state owner were added. Browser regression passed 18 scenarios
across `factory-triage.spec.ts`, `factory-inputs.spec.ts` and `factory.spec.ts`.
`pnpm build` passed TypeScript and the existing architecture/bundle budgets;
`pnpm lint`, `pnpm format`, architecture and strict OpenSpec validation also passed.
Disk free space during the correction runs remained 38–40 GiB; no cleanup or
commit was performed.

The final `pnpm test:unit` run passed all 38 Node checks and all 879 Vitest tests
in 110 files. `git diff --check` passed. Documentation-only evidence updates were
checked with `openspec validate add-factory-triage-pipeline --strict`; they add no
executable behavior requiring another TDD cycle.
