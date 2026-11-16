# Task 10.9 — unrelated CI analysis through Console

Status: complete. Task 10.10 retains the final ordinary-stand triage/Delivery and draft-PR qualification.

The existing packaged manual-authoring journey gains a CI-analysis scenario.
Its browser publishes custom text, choice, Boolean, integer and date fields,
creates a Role/Profile and authors Command → Decision Signal → AI agent. The
successful agent result selects the five Project fields. The driver reopens
that form and renames its step/result before publication. It authors a separate
snapshot consumer and starts it from the Issue without a predecessor Run.
Business mutations use Console controls; test-side HTTP calls only observe them.
The installation supplies technical Build capabilities, a retained-report
collector, the existing finite HTTPS JEV fixture and the guarded local writer.
No second fixture runner, mapping editor or output-schema authoring path is added.

## Required product corrections

An empty Project can select an installed routing Signal directly from existing
authoring capabilities. Its canonical input, finite outcomes, exact action and
permission declarations come from the installation. Private provider plans and
credentials stay installed. A hidden adapter or permission dependency removes
the descriptor; generic choice discovery omits it. Selection requests access and
never replaces authoritative publication/admission checks. Local draft validation
recognizes a routing action without requiring a deterministic gate.

Retained Command verification now handles explicit No code execution just as the
ordinary Codex verifier already does: source-free Build/Job facts have no
repository/revision. It checks the Build source mode and complete signed input,
program and resource provenance; repository-backed commands retain their exact
resource checks. Substituting a repository into a source-free Build is rejected.

## Observed TDD

| Boundary / command | Red before implementation | Green |
| --- | --- | --- |
| `cargo test --locked -p octacity-server-application --lib authoring_discovers_installed_signal_contract_without_a_published_flow` | The installed routing descriptor was not accepted/discovered without a published Flow. The extended visibility case then still advertised a hidden adapter sharing the policy reference. | One test passes, including both visible and hidden adapter cases. |
| `cargo test --locked -p octacity-server-api-rest --test v1_handlers installed_command_compiler_facts_match_the_public_authoring_contract` | The public response contract rejected undocumented `decision_signals`. | The response validates the installed Boolean input and finite output contracts. |
| `pnpm exec vitest run src/features/factory/FlowPrimitives.test.tsx -t 'installed finite Decision Signal'` | The installed-policy selector was absent. | Selection uses the installed canonical schemas/action without provider or schema authoring. |
| `pnpm exec vitest run src/features/factory/validateDraft.test.ts -t 'installed routing action'` | A configured routing action incorrectly produced `/root/nodes/0/gate: unavailable_reference`. | The configured action is accepted; an unbound Signal reports the missing action. All 14 validation tests pass. |
| `cargo test --locked -p octacity-server-application --lib retained_source_free_command_report_recovers_without_repository_provenance` | A valid signed source-free command report was rejected by the retained-output boundary. | Two retained reads verify the same literal report; repository substitution is rejected. All 30 command-output tests pass. |

Rust commands use `CARGO_INCREMENTAL=0`, `CARGO_PROFILE_DEV_DEBUG=0` and
`CARGO_PROFILE_TEST_DEBUG=0`. UI commands run from `ui/`. Fixture/compiler and
local runtime setup errors are not counted as behavioral Red evidence.

## Final regression checks

- Complete application library suite: **1,176 passed**, no failures or skips
  (`cargo test --locked -p octacity-server-application --lib`, 512.29 s).
- Application Issue suite: **203 passed**, including explicit producer-result
  execution/recovery, Source identity, snapshots and field application.
- Installed Source suite: **68 passed**, including frozen completion input and
  original ticket/acceptance retention across retries.
- Accepted-analysis/guarded-write replay regression: **1 passed**.
- REST Role/authoring suite: **17 passed**.
- Factory Console unit suite: **507 passed**.
- Console production build, TypeScript and bundle architecture budgets passed.
- Production Factory/application/REST library Clippy passed after the source-free
  verifier correction. The packaged manual-authoring harness also passes Clippy.
- Architecture, Rust/Console formatting, Console lint and strict OpenSpec checks passed.

The CI report cases are passed, failed and absent. The finite model must receive
only selected report values; an unselected raw-log canary must not reach it or the
Signal. A failed test report is valid analysis input. An absent required report
must not run the analyst or save fields. These checks qualify composition and
isolation, not real-provider quality or independent execution of arbitrary CI.

## Native result

The complete ignored packaged journey passed on macOS ARM64 with Microsandbox
0.7.15 and Octa 0.9.0, disposable PostgreSQL/S3, the checksummed Server/Console,
the installed Agent and the existing finite model/JEV peers. Duration: **239.51 s**.

```sh
cargo test --locked -p octacity-release-harness --test factory_manual_authoring \
  packaged_console_analyzes_ci_reports_and_starts_an_independent_snapshot_consumer \
  -- --ignored --exact --nocapture
```

The current compiled test executable was invoked directly with those same test
arguments for the final iteration, after `cargo test ... --no-run` and packaging
the current Server/Console. All three browser phases passed through actual UI.
The passed and failed cases each saved five values with the renamed invocation,
accepted result, form and operation identities. The absent report produced the
existing `stopped` Run state and `execution_failed` receipt, with no analyst
completion or field update. Two authenticated Signal requests carried exactly
`true` and `false`; the unselected raw-log canary was absent. Both used the
configured shadow baseline route `analyze`.

The independent consumer captured the expected five scalar values from both
Issues without any predecessor selector, retaining the original producer Run
attribution. The journey compared classifications, planning values and all five
execution snapshots after Server restart. No execution invoked Git. The verified
aggregate is retained in `manual-ci-analysis.json` under the private evidence
root; successful scenario files and its database were removed by the harness.
No PR, provider write or change to the operator's stand was made.

## Post-review corrections

See the [Signal authoring correction report](verification-10.9-review.md) for
runtime-derived contracts, preserved outcome settings, exact policy versions and
their separate Red/Green regressions. Native results above retain their original
source and packaging identities.
