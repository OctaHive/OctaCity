# Ordinary-stand UI authoring follow-up

Date: 15 November 2026. Base revision: `3f681fc743175cbef08faf462d2067255785dc36`, with the uncommitted section 8 changes. Earlier tasks 7.1–7.5 remain historical execution/material qualification; their harness-authored pipelines do not prove full UI authoring.

## Implemented behavior and observed Red/Green

- Ordinary installation advertises `stand.issue-classification` under the same verified management principal as the integration gateway. The writer is a Server-local capability, excluded from Agent plugin permissions. Its explicit Triage ceiling uses a new immutable version; retained ceilings/publications and custom reviewed policies remain intact. Installer tests first failed for the missing writer, then passed. Recovery, set-order normalization, preserved operator renames and retained immutable ceiling versions each have observed failing/passing regressions.
- `--no-example-pipeline` installs technical workers, Runtime, reusable agents/Profiles and policies with no Factory Flow, Configuration or Source input. A public installer/recovery test first rejected the missing option, then passed creation and restart without calling the pipeline publisher. The ordinary CI worker remains a technical executor.
- The snapshot editor supports explicit priority scores, risk mappings and allowed choices from selected required classification fields. Observed component Red/Green covers authoring, reopen and projection preservation, plus alias changes without lost mappings. EN/RU keyboard checks and invalid score feedback pass.
- Delivery v2 advertises release-owned field aliases/sources through discovery, sharing the source definitions with its resolver. The UI joins those sources to Project-owned shapes/choices and collects admission selections before opening an editable Flow. Discovery first failed for absent metadata, then passed. Console tests first failed for missing v2 admission controls, then passed editable publication for v1/v2 with optional/required reproduction commands.
- Bootstrap network names now come from the Compose owner instead of duplicated fixed project names. The process-boundary test first observed the wrong network, then passed; this permits the qualification to own separate networks/volumes.
- Native authoring exposed a resolver defect with the ordinary reusable Tester profile: it copied the shared permissions ceiling instead of the selected execution mode's request. A public resolver regression first reproduced the `/profiles` rejection, then passed with the selected mode's permissions. It checks that permissions for another mode do not become step authority.
- Native command execution exposed insufficient template budgets: a 60-second / 1-MiB node could select a compiler requesting 300 seconds and 64 MiB of combined artifact/report capacity. The resolver regression first observed the unchanged 60-second limit, then passed with limits derived from the selected request. A separate component regression first observed missing permissions/budget in the **Use command** draft update, then passed with the installed request, preserved existing grants and sufficient elapsed/output limits. Larger reviewed node limits remain intact; Project access review still applies.
- Native implementation then exceeded the inherited 150,000-token triage budget (508,860 tokens, and 324,620 after a compact assignment). Delivery now reads its own `deployment/local-stand/factory-delivery.json` stage budget, with a 1,000,000-token default; triage remains unchanged. Installer Red/Green covers the separate budget, additive immutable compiler/profile updates after policy changes, replay, and ordinary restart selecting the required upgrade. The 21-test Delivery installer suite passes with one platform opt-in skip. This is an allowance, not evidence that a run consumed its full budget.
- With sufficient budget the candidate was captured, but the model treated missing `.git` metadata as defective task requirements and took the guarded requirements-defect route. The installed Coder compiler now explains the trusted post-execution capture lifecycle, including that it owns the bundle/manifest/facts and revision identity. The installer regression first observed missing guidance, then passed; earlier compiler instructions remain available through the existing immutable update mechanism.
- The next guarded route retained an indeterminate public-test review: Reviewer verified the exact requested bytes but tried unavailable workstation tools and did not run the candidate's public check. Its installed compiler now explains the read-only snapshot, retained manifest, POSIX/Node tools and applicable public-test verification, while preserving independent protected validation. The installer contract first failed for absent guidance, then passed. Neither failure was counted as successful delivery or bypassed by changing gate predicates.
- Following that guidance exposed intentional raw-manifest exclusion from model Context. A separate verified candidate change summary now projects scoped paths/statuses/file metadata with an explicit completeness flag; range-only/out-of-scope selections omit whole-file details. The public compiler test first failed for the missing summary, then passed while preserving exclusion of binary/raw capture content. The summary restriction API first failed as missing, then passed path/range/out-of-scope checks. Nine candidate-context regressions pass. The qualification assignment explicitly retains a public shell regression script for independent review.
- Successful review observations exposed two missing report-contract instructions: the review must copy this invocation's exact `input_digest` from the protected task envelope, and `findings` contains defects rather than passed-check narration. The unchanged gate rejected a mismatched digest, then requested changes for positive narration in nonempty findings. Both installed-guidance regressions have observed Red/Green. Gate predicates, quality requirements and protected validation were not weakened.

Commands observed during these cycles:

```sh
python3 -m unittest tools.tests.test_factory_stand_triage.FactoryStandTriageTests.test_native_triage_installation_keeps_model_login_and_project_mcp_separate
python3 -m unittest tools.tests.test_factory_stand_policies
PYTHONPATH=tools:tools/tests python3 -m unittest test_factory_stand_recovery
pnpm --dir ui exec vitest run src/features/factory/IssueFieldAuthoring.test.tsx src/features/factory/IssueAdmission.test.tsx src/features/factory/FactoryAuthoring.test.tsx
cargo test --locked -p octacity-server-application --lib delivery_discovery_offers_issue_snapshot_v2_without_a_required_producer
cargo test --locked -p octacity-server-application --lib issue_delivery_uses_selected_step_permissions
cargo test --locked -p octacity-server-application --lib issue_delivery_command_budget
pnpm --dir ui exec vitest run src/features/factory/CommandStep.test.tsx src/features/factory/FactoryAuthoring.test.tsx
python3 -m unittest tools.tests.test_factory_delivery_setup
cargo test --locked -p octacity-server-application --lib candidate_context
pnpm --dir ui exec vitest run src/features/factory/NativeEvidence.test.ts
```

The socket-using Python tests require loopback/Unix sockets outside the filesystem sandbox. A denied socket bind was treated as an environment failure, not Red.

## Regression checks

Observed: all 1159 application unit tests, including the 17 preset/resolver and candidate-context cases; 45 REST preset tests; strict Clippy for both changed server libraries; architecture check (64 packages / 264 edges); OpenAPI generation; Console typecheck/lint/format; 46 script tests and 1253 Console unit tests, including EN/RU keyboard cases, installed-command budgets and private evidence retention. The final complete tools suite passed 685 tests with 43 environment/platform skips. Both legacy Delivery browser journeys passed. Full native results are recorded below.

The production bundle initially exceeded the existing aggregate ceilings after adding admission controls. The reviewed JavaScript/static budgets advance by 16 KiB, to 1,630,208 / 1,720,320 bytes. The real-production-build test and independent near-ceiling rejection test pass under these bounds; entry JavaScript, CSS and asset-count ceilings are unchanged.

## Full ordinary-stand qualification

`test_console_authors_triage_and_delivery_without_api_pipeline_preparation` uses verified native assets (Octa 0.9.0, Microsandbox 0.7.15 with the pinned agentd repair) and separate development Server/Console images built from the current working tree. The committed source revision remains the base identity; the local working-tree patch is retained separately for this development qualification.

The normal installer receives `--no-example-pipeline`. Test-side HTTP observes only; every field, Flow, local effect, admission selection, Configuration and execution request is authored through browser controls. The explicitly authorized publication target is OctaHive/OctaCity, branch `main`, with an unused test file under `tools`. Credentials remain in their selected private file and are not included in the source context or reports.

The complete workflow passed on 15 November 2026: one native test in 366.685 seconds, exit code 0. Project fields, both Flows, their exact Configurations, snapshot admission, scoped code bindings, public-test assignment and the publisher's **Show on Issue → Pull request** mapping were authored through the Console. Triage applied all eight fields with pipeline origin. Delivery independently admitted the Issue snapshot without a producer selector, completed protected test preparation, implementation, independent review, mandatory protected validation and confirmed publication, and displayed the PR link in Issue Overview.

```sh
OCTACITY_TEST_FACTORY_STAND_UI_INPUT=/private/tmp/octacity-ui-authoring/native-input.json \
python3 -m unittest tools.tests.test_factory_stand_native.FactoryStandNativeTests.test_console_authors_triage_and_delivery_without_api_pipeline_preparation
```

Final Run: `f353e6a1-4be4-4806-96e6-2247a899d4b8`. Confirmed [draft PR #7](https://github.com/OctaHive/OctaCity/pull/7), base `main` at `3f681fc743175cbef08faf462d2067255785dc36`, head `593ada58faa8a5c43e3a175a0dff21f419f6fefe`. An independent GitHub read confirmed open/draft state and two added files (marker plus public regression check). [Draft PR #6](https://github.com/OctaHive/OctaCity/pull/6) remains from the preceding successful publication whose UI test had omitted the explicit Issue display mapping; neither PR was merged.

Development image identities: Server `sha256:60e254aabb6e20420b7540a31fec9c6961f25ef690a9f8ac8f4a4d94c5505bd7`, Console gateway `sha256:45ce12570aa7c2d807dd52e2948c3ea0d5a990a6ee83cbb65f40611c67664fdf`. Private evidence under `/private/tmp/octacity-ui-authoring` retains the native log, observed Run/Build/Job records, model results/traces and independently confirmed receipt. The owned Compose project `octacity-ui-23a0920987b3`, its Agents, temporary root and volumes were removed through normal reset; Docker inspection confirmed no remaining services or volumes for that project.

Successful list-reporter runs discard buffer-only attachments. A subsequent retention-only Red/Green regression adds a private `confirmed-delivery.json` output file with the Run, PR URL and checked completions. Its filesystem test and a real passing Playwright process both confirm retained bytes and mode `0600`; this change adds no business API calls or qualification-proof state. The full native business workflow above preceded that isolated reporter-retention adjustment. Preserve `ui/test-results` together with the selected private installer evidence directory in later CI runs. Tasks 8.5–8.6 are complete.

## Post-review regressions

Three review findings have separate observed Red/Green cycles:

- The snapshot form rejected required `property` fields bound to Project classification slots. The public form regression first failed because the bound Impact property was absent from admission choices, then passed authoring, risk/allowed mappings, reopen, unchanged save, projection preservation and alias edits. Both the semantic `classification` source and its bound `property` representation are covered. Eligibility now consults the published Project bindings; arbitrary unbound properties remain excluded.
- Ordinary restart skipped instruction-only Coder/Reviewer upgrades when budgets, plugins, guests and verifiers were already current. All four missing/old-instruction subcases first failed to select an update, then passed. Installation and recovery now share the exact compiler instruction comparison. The same test verifies that an already-current installation remains a no-op; existing immutable-update/replay regressions remain passing.
- Evidence failures skipped disposable reset and image-tag restoration, masking the journey failure. Five injected cases first failed cleanup assertions: an occupied evidence path, disk exhaustion during copy, Docker-log timeout, reset failure and image-tag restoration failure. They now attempt the remaining cleanup operations and retain the original journey exception with secondary error notes. The unchanged successful-evidence case remains passing. The regression mocks external Docker/installer I/O and needs no credentials or real publication.

Observed Red commands, followed by passing Green runs after the corresponding fixes:

```sh
pnpm --dir ui exec vitest run src/features/factory/IssueFieldAuthoring.test.tsx -t 'authors and reopens admission'
python3 -m unittest tools.tests.test_factory_delivery_setup.FactoryDeliverySetupTests.test_standard_startup_upgrades_stale_coder_and_reviewer_instructions_with_current_budgets
PYTHONPATH=tools:tools/tests python3 -m unittest test_factory_stand_native_cleanup
```

Post-fix checks: all 1254 Console unit tests pass; the Delivery installation suite passes 22 tests with one opt-in skip; 34 recovery, policy, triage and cleanup tests pass. The cleanup regression covers all six injected/baseline subcases. Console typecheck, lint, touched-file formatting and architecture checks pass. Documentation-only edits have no executable TDD behavior and are checked with strict OpenSpec validation and whitespace checks.

```sh
pnpm --dir ui exec vitest run
pnpm --dir ui typecheck
pnpm --dir ui lint
pnpm --dir ui check:architecture
pnpm --dir ui exec prettier --check src/features/factory/IssueAdmission.tsx src/features/factory/IssueFieldAuthoring.test.tsx src/features/factory/FlowIssueSnapshot.tsx src/features/factory/FactoryExample.tsx
python3 -m unittest tools.tests.test_factory_delivery_setup
PYTHONPATH=tools:tools/tests python3 -m unittest test_factory_stand_recovery test_factory_stand_policies test_factory_stand_triage test_factory_stand_native_cleanup
openspec validate add-issue-classification-snapshots --strict
git diff --check
```

The full native business workflow recorded above was not rerun for these review fixes; another such run would create another draft PR. These follow-up results establish the changed form, restart and failure-cleanup boundaries, not a second full native qualification.

### Classification-bound property warning

A subsequent review found that the admission warning still validated without the Project classification bindings, even though field eligibility and the parent save check already used them. The form regression now asserts no alert after valid risk/allowed mappings and after reopening, for both `classification` and bound `property` sources. Red observed one failing property case with the actual invalid-admission alert and one passing classification case. Passing the same catalog bindings to the warning validator produces Green: all 22 tests in the three related Console suites pass, including existing EN/RU invalid-score alerts and keyboard checks. No policy or stored snapshot normalization changed.

```sh
# Red before the fix: 1 failed, 1 passed, 10 unrelated cases skipped.
pnpm --dir ui exec vitest run src/features/factory/IssueFieldAuthoring.test.tsx -t 'authors and reopens admission'
# Green after the fix: all 22 tests pass.
pnpm --dir ui exec vitest run src/features/factory/IssueFieldAuthoring.test.tsx src/features/factory/IssueAdmission.test.tsx src/features/factory/FactoryAuthoring.test.tsx
```
