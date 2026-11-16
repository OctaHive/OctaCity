# Task 10.10: final selected-field UI integration

## Scope

Adapt the existing ordinary-stand UI journey, not another qualification harness.
The installer supplies technical capabilities and profiles with
`--no-example-pipeline`; all Project fields, Flows, Configurations, Issue creation
and launches are authored through Console controls. Test-side HTTP only observes
public resources. The user authorized one test draft PR in OctaHive/OctaCity.

The triage agent selects four built-in and four custom fields on its ordinary
result. No semantic Project roles, manually copied schema, field mapper or apply
node are authored. Delivery selects those fields directly, retains their original
assessment attribution and admits an Issue snapshot without a producer selector.

The journey checks the applied save receipt against the producing result,
operation and invocation; retains exact snapshot/configuration policy pins;
requires separate protected-test, implementation, review and validation Builds;
and requires the trusted PR publication receipt and Issue link. Records are saved
before completion/isolation/publication assertions and disposable installation cleanup.

The final-path audit covered `FillIssueFields`, `StepResults`,
`SnapshotFieldSelectors`, `FactoryExample`, the existing selected-form schema/save
contracts, and configured preset field/threshold parameters. Field targets are
advertised Project identities; producer schema and save scope share one form.
Consumers use direct sources and configured local aliases, not semantic slots or
producer Runs. Business field sets and Size thresholds remain editable preset
configuration. No authoring endpoint, runtime framework or compatibility branch
was added by this integration.

## Red / Green

- Existing ordinary-stand browser journey: Red at the installed writer identity,
  expected obsolete `stand.issue-classification`, observed `stand.issue-fields`.
  The obsolete role selectors, schema-copy controls and apply node were removed
  from the driver in favor of the public selected-field controls.
- Broader browser baseline: 25 passed, three failed. Both Project-field cases
  waited for the removed Risk role selector. The Russian classification case
  expected a translated fixed Size label rather than the Project-owned name.
  After adapting the tests, all four affected EN/RU cases passed. The Size case
  now explicitly publishes `Project small` / `Project large` names in its fixture.
- An intermediate native run applied all eight fields and admitted the independent
  Delivery snapshot, then exposed an incorrect new test assertion: the Run read
  uses `configuration_id` / `configuration_version`, not a nested `configuration`.
  The assertion was corrected against the served REST schema before rerunning.
  No PR had been created.
- The next native run completed protected tests, the expected baseline failure,
  implementation, independent accepted review and passing mandatory validation.
  Publication correctly returned `conflict`: the private input still selected
  source `3f681fc…`, while GitHub `main` was already `de4550b…`. The provider's
  existing exact-base check rejected this before branch/PR publication; a separate
  GitHub read confirmed no open PR. Only the private qualification source revision
  was corrected to the verified current `main`; no policy or production check was
  weakened. The stale-base records remain private under `/private/tmp/o1010/evidence`.
- Final native journey: **passed**, including actual trusted publication and
  independent GitHub metadata verification of the single draft PR.

Documentation changes have no executable behavior. They correct stale fixed-Size
and semantic-role setup instructions and describe selected forms, CI/Decision
Signal composition and accepted analysis versus confirmed field application.

## Broader integration checks

Commands ran from the checkout root unless prefixed with `cd ui`:

| Command | Observed result |
| --- | --- |
| `cd ui && pnpm test:unit` | 1268 Vitest tests and 47 script tests passed |
| `cargo test --locked -p octacity-server-api-rest --test v1_handlers` | 296 passed, including generated OpenAPI drift |
| `cargo test --locked -p octacity-server --lib` | 238 passed, 72 explicitly ignored integration cases |
| `cargo test --locked -p octacity-server-store-postgres -- --include-ignored` | 323 passed, no skips, with disposable PostgreSQL/S3 |
| `cd ui && pnpm exec playwright test tests/browser/project-issue-fields.spec.ts tests/browser/issue-classification.spec.ts tests/browser/issue-execution-snapshots.spec.ts tests/browser/factory-canvas.spec.ts tests/browser/factory-ticket-delivery.spec.ts` | Baseline 25 passed; three stale test expectations corrected; focused rerun four passed |
| `python3 -m unittest discover -s tools/tests -p 'test_factory_delivery*.py'` | 24 passed, one platform-specific skip |
| `python3 -m unittest discover -s tools/tests -p test_factory_stand_native_cleanup.py` | Passed |
| `python3 tools/check_architecture.py` | Passed |
| `cd ui && pnpm typecheck` / `pnpm lint` / `pnpm format` | Passed, including final driver edits |
| `openspec validate add-issue-classification-snapshots --strict` | Passed |
| `git diff --check` | Passed |

The full application library's 1176 passing tests, focused Issue/Delivery and
native CI-analysis qualification were already observed in task 10.9; see
[its report](verification-10.9.md). Task 10.10 changes the journey and documentation,
not the application implementation, so that unchanged suite was not repeated.
Rust runs used `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0
CARGO_PROFILE_TEST_DEBUG=0`. The first default PostgreSQL run left opt-in cases
ignored; the reported 323-case run explicitly included them after provisioning
real disposable services. Both services were removed after success.

## Final native result

```sh
OCTACITY_TEST_FACTORY_STAND_UI_INPUT=/private/tmp/o1010/native-input.json \
  python3 -m unittest tools.tests.test_factory_stand_native.FactoryStandNativeTests.test_console_authors_triage_and_delivery_without_api_pipeline_preparation
```

The complete installer/browser/cleanup command passed in **446.951 s**; the
browser reported **1 passed (5.6m)**. Both pipelines were authored through Console
controls on the normal stand, starting with an empty business catalog.

| Fact | Retained result |
| --- | --- |
| Triage Run | `c773815b-9853-42a5-bfaf-2d18d28a0e6e` |
| Field save | `applied`; eight fields match the exact result, form, operation and invocation |
| Delivery Run | `3fe1df90-60c9-4f34-9e51-5eab0c201710` |
| Snapshot/policies | Original attribution retained; admitted snapshot and exact launch/access/configuration pins unchanged after execution |
| Baseline | `baseline_result: expected_failure` |
| Independent review | `code_review_gate: accepted` |
| Mandatory validation | `protected_validation_gate: passed`, verified report retained |
| Publication | `publish_pull_request: published` |
| GitHub | [PR #8](https://github.com/OctaHive/OctaCity/pull/8), open, **draft**, unmerged, two changed files |
| Base | `main` at `de4550b0ac339654e6b6d26a88d6323d0ec72208` |
| Candidate | `db56edefdf46986791e753c828990ccff0e648b7` |

The four required separate Build identities are:

- Protected tests: `1e34f768-94d9-58c7-bd18-86bc42132177`.
- Implementation: `852eb6f8-ab4b-5849-8ac8-8714fe992303`.
- Independent review: `c4c500db-64ae-5cd8-88ad-142e03c0ed5c`.
- Mandatory validation: `0e4fcec9-7028-586c-9422-68c0c3fb29f8`.

`confirmed-delivery.json` and `delivery-records.json` are retained in the private
input's evidence directory as well as `ui/test-results`. The test-owned stand,
VM state and volumes were cleaned up, and original local image tags restored.
The unrelated user's containers and stand were untouched. Nothing was merged.
This native result covers macOS ARM64/Microsandbox; a new Linux native run was not
performed. The 72 explicitly ignored server composition cases were not claimed
as passing; the full PostgreSQL package was separately run including ignored cases.

## Packaged development inputs

The normal Server and Console Dockerfiles built the current working source with
its 10.9 changes. The verified native Agent bundle remains pinned to
`3f681fc743175cbef08faf462d2067255785dc36`; image labels use that qualification
base. The final admitted repository source is the independently verified current
`main`, `de4550b0ac339654e6b6d26a88d6323d0ec72208`. This is a development integration, not a newly published
release-binary claim. Checkout HEAD was
`de4550b0ac339654e6b6d26a88d6323d0ec72208` plus the uncommitted work.

- Server image: `sha256:008495a5987476191737ef5e9c7ce2f1ca9604cd2e6eb4d620e86c7181403b8e`.
- Console image: `sha256:490ddb987a42c3befb12688b27c04e0381eabde3d2bed0ec9c919c5f58530ed6`.
- Installed Octa 0.9.0 and Microsandbox 0.7.15, on macOS ARM64.
- Private installation parameters, source patch, build/test logs and identity
  manifest are under `/private/tmp/o1010`; credentials remain in their selected
  private files and are not included in this report or source staging.

The test owns a unique Compose project, native state and volumes. It resets only
those owners and restores previous local image tags. Unused Docker build cache
was pruned after the builds to recover disk space. No user stand was reset.

## Post-review corrections

See the [Signal authoring correction report](verification-10.9-review.md) for
runtime-derived contracts, preserved outcome settings, exact policy versions and
their separate Red/Green regressions. Native results above retain their original
source and packaging identities.
