# Task 8.2 — Console triage qualification

## Boundary

The journey extends the packaged qualification in
[task 8.1](packaged-triage-verification.md). Chromium operates the packaged Console
through the same-origin nginx gateway and real management routes. PostgreSQL,
object storage, native Source/Reporter adapter processes and the qualified Agent
remain the ordinary owners. No REST interception, alternate execution handler,
new credential owner or new installation updater is introduced.

The operator publishes Project goals/rules, selects two distinct exact
Role/Profile versions and the reviewed Build/Runtime, reviews declared
permissions and bounded budgets, configures the native Source and Reporter,
resolves/publishes immutable child Flows and the root Configuration, previews the
unlabeled native task, publishes disabled intake, independently imports its
canonical Issue and explicitly enables intake as version 2. Disabled intake
creates no execution. Triage retains the original Issue association without
inventing implementation acceptance.

Result inspection uses both English and Russian, keyboard operation and a
390-pixel viewport. The stage decisions, accepted evidence/provenance and
independent delivery receipt are read from their existing management resources.
The UI displays the deterministic gate outcome separately from the analyst's
recommendation. Long immutable evidence references wrap without horizontal
scrolling; they remain available in full.

## Corrections established by tests

| Public behavioral boundary | Observed Red | Green implementation |
| --- | --- | --- |
| Installed catalog → ordinary Console permission editor | Required network permission checkbox absent; file installer rejected choice metadata | Preserve validated `FactoryChoiceMetadata` in the existing file installer |
| Permission editor command labels | Three executable grants with different arguments had indistinguishable labels | Include quoted exact arguments and distinguish wildcard arguments |
| Packaged preset → Console → configuration validation | Browser rounded i64 bounds; server rejected its own preset with HTTP 400 | Exact API schema bounds outside the JavaScript integer range are decimal strings |
| Domain `FlowDataSchema` input | Decimal-string i64 bounds failed deserialization | Accept canonical signed-i64 decimal text; domain serialization and digest stay numeric |
| Schema editor | Maximum bound was rounded on edit | Keep exact bound text in the editor |
| Flow schema compatibility validation | `'10'..'99'` was incompatible with `'2'..'100'` under lexical comparison | Compare schema bounds as exact integers |
| Model capability HTTP response | Model-specific maximum/minimum remained JSON numbers | Convert the declared model-catalog parameter schema slot |
| Action list / observation HTTP response | Response schemas still emitted numeric i64 limits | Convert admitted response schema slots in all three list scopes and exact action observation |
| Packaged retained-result inspection | Deterministic `requirements` decision was absent; long evidence references exceeded the narrow viewport | Render the recorded outcome and wrap immutable references |

The approved wire refinement is recorded in the REST delta spec and design.
Only declared schema positions are converted. Actual node values, arbitrary
JSON, stored schemas, immutable references and digests are unchanged. HTTP tests
round-trip both i64 extrema and assert the independently known retained digest
`cf6037a1bca746f5423c6b01bf78ea84376ed75a5f0565596ca2a3ed15b9aba3`.
Overflow and noncanonical decimal forms are rejected.

During fixture qualification, the shortened five-second Factory claim clipped
an independently budgeted native Reporter dispatch. The fixture now uses the
ordinary thirty-second claim window. Restoring fixture inputs, correcting the idle-Agent observation baseline and
keyboard-driver waits/selectors and restoring the temporary object-storage
bucket are environment/test corrections, not production Red evidence.

## Installation continuity

The same Console-published execution is inspected before and after the supported
`factory_installation_update` owner stages a compatible reviewed catalog update.
The existing lifecycle lock, preflight and atomic configuration exchange are
used. The fixture coordinates only shutdown/startup of its native processes.
The active server references the generated catalog path so the actual exchanged
configuration is consumed on restart. The active Agent consumes the generated
identity, complete Factory/MCP authority and credential/work/state/cache paths
before its first start and again after exchange. The existing native fixture
retains only its transport/backend adaptations. The credential is a real issued
enrollment, not the metadata-only placeholder; MSB loads the exact qualified
image archive into this installation's own generated state path. Server API
inspection requires the newly reviewed environment declaration after restart,
so retaining the previous catalog cannot satisfy the test.

An invalid candidate must fail before stopping the usable server. The valid
update adds an optional public environment declaration and widens no credential
or execution grants. After restart the test requires the original Agent ID,
successful authenticated re-registration and private Agent credential retained by the update, retained
installation secrets, exact Source selection, original Issue/Run, accepted node
completions, Build/call links and effect receipts. Normal Agent registration may promote its persisted credential to the resumed
session; byte equality is checked before registration, authentication and stable
Agent identity afterwards. The resumed browser retains Russian language and dark theme. The finite model peer and native Reporter
assertions continue to require six original model requests and exactly one
provider mutation across the complete journey.

## Verification commands

Cargo uses `CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2`, `--locked -j2`.

- `cargo test -p octacity-server-api-rest --test v1_handlers`: 233 passed.
- `cargo test -p octacity-server-factory flow_data_tests`: 24 passed.
- `cargo test -p octacity-server --lib factory_profile_file_tests`: 11 passed.
- `cargo test -p octacity-release-harness --test release_vertical_slice`:
  25 passed; external-environment qualifications remain explicitly ignored by
  the ordinary invocation and are run separately with their required inputs.
- `python3 -m unittest tools.tests.test_configure_local_stand_agent
  tools.tests.test_factory_source_installation`: 37 passed.
- `python3 -m unittest
  tools.tests.test_factory_triage_pipeline.FiniteTriageResponsesTests`: 2 passed.
- Scoped Vitest schema/editor/flow/effects/Issue tests: 36 passed; separate
  Run-diagnostics/Issue-result regression: 14 passed (overlapping Issue tests).
- Console typecheck, lint and production build, including frontend architecture
  and bundle budgets, passed.
- Workspace architecture check, strict OpenSpec validation and formatting passed.

Targeted Red/Green commands (with the same Cargo environment) include:

```sh
cargo test --locked -j2 -p octacity-server --lib installed_choice_permissions_remain_available_to_ordinary_console_authoring
cargo test --locked -j2 -p octacity-server-factory decimal_schema_bounds_restore_original_integer_storage_and_digest
cargo test --locked -j2 -p octacity-server-api-rest --test v1_handlers plugin_model_catalog_http_
cargo test --locked -j2 -p octacity-server-api-rest --test v1_handlers response_action_schemas_
pnpm --dir ui exec vitest run src/features/factory/SchemaEditor.test.tsx src/features/factory/validateDraft.test.ts
pnpm --dir ui exec vitest run src/features/factory/ResourceEditors.test.tsx src/features/factory/FlowGraphEditor.test.tsx
```

The specific Red/Green HTTP invocations use filters `plugin_model_catalog_http_`
and `response_action_schemas_`; their observed failures compared a numeric i64
boundary with the required exact decimal string. Core and UI Red/Green filters
are the test names described above. Compiler/environment failures are excluded
from the Red claims.

## Exact retained evidence

Base commit remains `11069366a8a9c49e7b0072211d990b51c901ab7c`; the working-tree
changes are not committed. The complete Russian setup and bilingual before/after
inspection passed with:

- Report directory: `/tmp/oc81-evidence/baseline/source-7fd241ab-08d3-4246-beb4-957d1d97a355`.
- Original Run: `4b5833a9-15d9-4411-b0d8-7d1ec944b447`.
- Original Issue: `d4f6d2dd-2617-42ac-a3db-94a4415507a8`.
- Agent: `b657659e-0dc5-5bdb-ac8b-e2ec9e2f7fce`.
- Configuration v1: `3b5bf881-ce34-4563-9cce-525356857333`, digest
  `2919a46e28d59cc6308b648e09fd4868045cf374cd7ff8d03371d09b837d3ef1`.
- Enabled Source input v2: `bc040386-6d8e-4bf8-a792-3f3fb0dd3dd1`.
- Delivered effect:
  `284dcc8da0748c5699ea0d43f3d86509dfb84cc25cf7c7b046ef0cd0c20ef5f2`.
- Packaged server SHA-256:
  `4c78f4de53bc4010630cf634a89f43a6db14f168a7c51e148abe876286cd1dc3`.
- Packaged Agent SHA-256:
  `a3f4c6d70e6f1ad617e48c5982047df5c18e069ee7f73ca8893c1229241299a0`.

`triage-console-restart.json` retains equal public before/after snapshots and the
failed-preflight proof. The Python qualification passed all assertions, including
six original model requests, distinct exact profiles and one native Reporter
mutation across restart. English setup also passed independently before the
restart extension. The existing import-only packaged regression passed again
without admitting a Factory execution.

The separate defect-result qualification passed with Run `c52d8a84-e400-4da6-943a-a7f55f582ff5`;
its report is `/tmp/oc81-evidence/baseline/source-4645b684-da80-4576-a270-d73562fa80f8/triage-profile-results.json`.
It executes three ordinary Builds, retains the accepted independent reproduction
observation and two distinct profile results, confirms one delivered native
label effect, and passes bilingual narrow-screen/provenance inspection.

Both packaged Python test methods passed:

```sh
python3 -m unittest tools.tests.test_factory_triage_pipeline.PackagedTriagePipelineTests.test_console_configures_the_packaged_triage
python3 -m unittest tools.tests.test_factory_triage_pipeline.PackagedTriagePipelineTests.test_console_reads_actual_retained_triage_results
```

These require the explicit verified environment described in task 8.1 and
`OCTACITY_TEST_PACKAGED_TRIAGE=1`; the complete setup/restart proof used
`OCTACITY_RELEASE_TRIAGE_LOCALE=ru`. Native execution qualifications ran
sequentially on the selected shared backend. Failed concurrent fixture runs are
not counted as production Red evidence. The shared import helper's regression
also passed via `packaged_source_only_import_creates_one_visible_canonical_issue_without_execution`.
Clippy passed for the changed Rust packages with warnings denied; its harness
check passed again after the registration-observation correction.

Disk checks stayed above 17 GiB free. Removing 22 inactive task-owned server
package snapshots recovered approximately 6 GiB; active packages, verified
runtime inputs, source changes and qualification evidence were preserved.

## Review corrections: observed Red/Green

- `python3 -m unittest tools.tests.test_factory_triage_pipeline.PackagedTriagePipelineTests.test_console_configures_the_packaged_triage`:
  Red at the real Agent API (`triage-…` instead of generated `local-stand-agent`);
  Green after consuming the installation before both starts. Updated exact
  evidence above includes the successful complete Russian/bilingual restart.
- `cargo test --locked -j2 -p octacity-release-harness --test release_vertical_slice installed_agent_configuration_reloads_authority`:
  Green for reloading changed authority and owned paths, clearing superseded
  credential references while preserving native transport.
- `python3 -m unittest tools.tests.test_configure_local_stand_agent.LocalStandAgentConfigurationTests.test_staging_validates_without_claiming_the_running_agents_cache`:
  Red when the validating CLI attempts the running cache's actual OS lock;
  Green with disposable validation stores. Published credential/cache/work/state
  paths still point to the original installation. Full CLI validation remains
  required; authority and immutable executables are not replaced with stubs.
- `OCTACITY_TEST_PACKAGED_TRIAGE=1 python3 -m unittest tools.tests.test_factory_triage_pipeline.PackagedTriageProbeTests`:
  Red when `health()` returns 200 while the real HTTP endpoint returns 500;
  Green with an observed `GET /health`, its response status and counted request
  event. The full three-Build defect qualification also passed afterwards.
- Claim simplification is behavior-preserving: the existing ignored public test
  `packaged_factory_source_admission_retains_canonical_issue_and_configured_status`
  passed before and after removal of the unused wrapper/parameter. The same
  thirty-second claim remains in one place; no artificial Red is claimed.
- Cleanup registers owned SDK state before image load and uses the existing
  supervisor's process-group termination/inspection. Real-process tests cover
  failure before Source readiness and timeout with a child process, then verify
  released state. `TriageFixtureProcessTests`: 2 passed.

Final scoped regression: 26 harness tests passed (20 opt-in cases ignored by the
ordinary command), 42 Python tests passed, targeted Rust Clippy passed with
warnings denied. The HTTP probe's pinned Node test and complete Console/defect
qualifications passed separately. Restoring the exact original Docker image
archive and canonicalizing macOS `/var` aliases are environment/test corrections,
not claimed production Red. `OCTACITY_FACTORY_PILOT_IMAGE_ARCHIVE` must name the
`image.docker.tar` produced by the existing image preparation tool; each isolated
installation checks the imported immutable digest before running its Agent.

## Qualification limits

The finite model/GitHub peers establish protocol, isolation and retained-state
semantics with the selected native binaries. This does not claim paid-model or
live-GitHub execution, every executor/provider, or the adversarial interruption
matrix reserved for task 8.3. All changes remain uncommitted.
