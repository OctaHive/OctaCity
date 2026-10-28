# Classification contracts and retained public results

Task 5.3 extends the released triage template through existing bounded schemas,
the declarative gate, ordinary immutable records and authorized diagnostics.
It adds no phase dispatch, separate classification entity, mutation handler or
new execution engine.

## TDD evidence

Commands use `CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
CARGO_INCREMENTAL=0`, `--locked` and `-j2` to keep build storage bounded.

| Test command suffix                                                                                     | Observed Red                                                                                                                                               | Observed Green                                                                                                                                                                         |
| ------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `cargo test --locked -j2 -p octacity-server-factory --lib triage_analyst_result_accepts`                | The old closed schema rejects the required `priority_assessment`.                                                                                          | Priority 0/100 accepted; out-of-range/fractional values and unknown vocabularies/fields rejected.                                                                                      |
| `cargo test --locked -j2 -p octacity-server-factory --lib triage_analyst_gate_requires`                 | The old gate input rejects the required applicability contract. A later Red exposed incorrectly requiring the independent verifier to use the model's Job. | Exact assessment/input/Build/Attempt agreement and freshness required; independent verification Job supported. Adding primary-result freshness had its own missing-contract Red/Green. |
| `cargo test --locked -j2 -p octacity-server-factory --lib triage_analyst_gate_checks`                   | A feature with critical severity incorrectly reached `requirements`.                                                                                       | Incompatible feature/defect reproduction and severity combinations require review; unresolved research/input stays distinct.                                                           |
| `cargo test --locked -j2 -p octacity-server-factory --lib triage_public_results`                        | The published classification outcomes lacked explicit disclosure policy.                                                                                   | Only reviewed summaries opt in; default outcomes omit the new field.                                                                                                                   |
| `cargo test --locked -j2 -p octacity-server-application --lib retained_public_results_read`             | The authorized completion query returned no retained classification value.                                                                                 | Exact schema/value/digest read repeatedly without mutations; private outcomes and Context/inputs/evidence omitted.                                                                     |
| `cargo test --locked -j2 -p octacity-server-application --lib triage_analyst_compiled_prompt`           | The actual protected prompt did not explain `priority_assessment`.                                                                                         | Compiled instructions contain the complete contract and a schema-valid feature example.                                                                                                |
| `cargo test --locked -j2 -p octacity-server-api-rest --test v1_contract factory_public_result_contract` | OpenAPI omitted the optional field. A later contract Red caught an OpenAPI default that made generated clients require it.                                 | Optional disclosure flag and bounded public value published without that incompatible default.                                                                                         |

The initial application read fixture lacked the admission discovery index, and
one fixture edit had invalid scope fields. These were setup/compiler failures,
not Red evidence; both were corrected before the behavioral read failure above.
The ordinary record/restore test qualifies existing retention behavior: exact
schema/record identity survives restoration, changed priority bytes fail the
committed completion digest, and every substituted dependency tuple member
requires review. It does not invent a new runtime or a fake command execution.

## Contract and scope

The assessment owns an advisory priority, kind, severity, size, component, risk,
bounded dependency references, preliminary reproduction and finite route. Its
summary retains original admission priority/risk separately. Neither assessment
nor reads change Work/Build scheduling priority or authoritative Issue metadata.
The immutable outcome's `public_result` flag is generic, defaults to omitted
false, and discloses only its schema-checked value. Existing Run/Project and
associated Issue authorization paths remain in place.

The selected binding must still supply a qualified independent `facts` output.
Tests exercise trusted owner records and schema/gate acceptance, not an installed
classification-facts producer or qualified Agent execution. Actual symptom- and
environment-bound positive/negative reproduction verification is task 5.4;
unsupported claims fail closed in this increment. The full operational journey
remains task 8.1. No new permission or readiness follows from these tests.

## Review

Two independent read-only reviews used base
`e0a31261f0a2411d99f4c039155792defbcf76c7` and included untracked files.
Standards review found no actionable complexity, coupling, duplication or
hardcoded-business-policy issue. Spec review found no material task-5.3 defect;
its README drift finding was corrected. Root validation also found and fixed
the generated-client optionality regression through the contract Red/Green.

## Qualification

Baseline: six core triage and thirteen application eligibility tests passed.
The complete core library passed 314 tests. Architecture validation passed with
64 packages and 263 dependency edges. Core Clippy (`--all-targets -D warnings`),
application Clippy (`--tests -D clippy::all`) and strict OpenSpec validation passed.
Final regression commands all passed:

- `cargo test --locked -j2 -p octacity-server-factory --lib`: 314 tests.
- `cargo test --locked -j2 -p octacity-server-application --lib --quiet`: 832 tests.
- `cargo test --locked -j2 -p octacity-server-store --lib --quiet`: 109 tests.
- `cargo test --locked -j2 -p octacity-server-api-rest --test v1_contract --quiet`: 28 tests.
- `cargo clippy --locked -j2 -p octacity-server-factory --all-targets -- -D warnings`.
- `cargo clippy --locked -j2 -p octacity-server-application --tests -- -D clippy::all`.
- `cargo clippy --locked -j2 -p octacity-server-api-rest --all-targets -- -D clippy::all`.
- `pnpm api:generate` followed by `pnpm typecheck` in `ui`.
- `python3 tools/check_architecture.py`, `cargo fmt --all --check`, scoped
  Prettier, `openspec validate add-factory-triage-pipeline --strict`, and
  `git diff --check`.

The application keeps its existing unrelated unused-fixture warning for
`pending_factory_profile_fixture`; no new warning was introduced.

Free disk space was 37 GiB before and 36 GiB after the bounded builds. No cache
or user-file cleanup was needed. Documentation/formatting changes have no
executable behavior and were validated by formatting and OpenSpec checks.
