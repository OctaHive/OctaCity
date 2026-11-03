# Task 8.1 — packaged triage qualification

## Boundary and implementation

Qualification uses the packaged server and gateway, PostgreSQL 18.6, S3-compatible
MinIO, the qualified packaged Agent, ordinary signed Builds/Jobs and microsandbox
VMs. Native Codex 0.161.0 uses its own HTTP/stdio MCP clients. The selected external
server is the unmodified official GitHub MCP server 2.0.3 for Linux ARM64.
Finite local Responses and GitHub HTTP peers provide deterministic inputs; no
paid model or live GitHub mutation is claimed. The HTTP bridge forwards OctaCity
MCP requests to the real gateway, including its optional GET method refusal.
It implements neither Issue reads nor authorization.

The production correction is in `FactoryRuntimeResources::retained_artifact_reader`:
attach the existing private Source input reader alongside ordinary Build outputs
and Project inputs. This lets the normal compiler resolve admitted Source Context.
There is no new state owner, compiler, authorization shortcut or execution path.

Test setup composes the supported executable installation and freezes exact
Runtime/Role/Profile/Repository/Context/configuration references before admission.
The eligibility invocation receives the two explicitly selected read connections;
the analyst receives its own Context and accepted handoff without inherited MCP
connections or credentials. Reproduction is an independent command Build with its
own disabled-network configuration. Accepted command metadata supplies the
analyst's reproduction fact; model text cannot substitute for that observation.

Reporter permissions come from the trusted installed Source Reporter declaration
and are registered through ordinary catalog/configuration publication before
admission. The server action alone receives its declared plugin/capability,
secret-profile reference and network hosts. Neither AI profile nor Agent receives
Reporter credentials. Installed public action choices supply the effect contract.

## Observed behavior

The packaged authority test verifies scoped OctaCity search/read, hidden/foreign
resource refusal, invalid/wrong-purpose credentials and denied mutation tools.
The separate ready-made external-server qualification verifies actual search/read,
foreign repository refusal, denied writes and retained protocol evidence.

Both complete pipeline scenarios perform four actual native MCP calls, retain
their accepted lookup observations and trace/provenance identities, execute two
distinct selected profiles and finish without a canonical Issue association.
They use six finite model HTTP requests. No-card operation does not disable the
normal durable Source, Work, Run, Build, result or delivery owners.

The feature scenario creates two ordinary Builds and performs no provider writes
when reporting is unconfigured. The defect begins as an unlabeled native task
with no body or implementation acceptance. It creates three ordinary Builds:
eligibility, an independently installed reproduction command, and classification.
The command checks the exact selected code revision and observes the specified
HTTP 500 symptom. Its accepted report has `finding=matched`. The bounded gate
emits `requirements`: completing triage does not invent implementation acceptance.
The explicit reporting subflow sends exactly one POST adding the existing
`ready / review` label, leaves the native Issue open, retains a delivered
`work.status.projection` receipt and completes the Run. The writer credential is
checked independently from Source, model and external MCP credentials.

## Test-first evidence

Cargo commands use `CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2`, `--locked` and `-j2`.

| Behavioral boundary / command | Observed Red | Observed Green |
| --- | --- | --- |
| `cargo test -p octacity-release-harness --test release_vertical_slice packaged_triage_uses_two_distinct_profiles_before_accepted_results -- --ignored --exact --nocapture` | Admitted Source Run could not prepare the required Source Context through the runtime reader; no accepted stage results | Existing Source reader wired into runtime composition; both selected native profile invocations accepted and Run completed |
| `python3 -m unittest tools.tests.test_factory_triage_pipeline.FiniteTriageResponsesTests` | Finite peer returned a boolean instead of the required structured result; separate invocations did not receive separate plans/results | Two structured-response tests pass through HTTP Responses events |
| `cargo test -p octacity-release-harness --test release_vertical_slice packaged_triage_reproduces_raw_unlabeled_defect_before_classification -- --ignored --exact --nocapture` | Two AI Builds supplied no independently accepted command observation; configured reporting expectation initially observed no native write | Three Builds, accepted symptom-specific command proof, exact label/state effect, delivered receipt and completed Run |
| `python3 -m unittest tools.tests.test_factory_triage_pipeline.PackagedTriagePipelineTests` | Successful execution report lacked required immutable image/identity metadata | Reports bind actual image/executable/catalog/configuration/profile/input/record identities; both complete scenarios pass |

The narrow public packaged tests were also written and run during fixture
development: `packaged_source_gateway_preserves_scoped_issue_mcp_authority`,
`packaged_triage_prepares_raw_unlabeled_input_without_a_canonical_card` and
`packaged_source_uses_the_qualified_agent_and_installed_job_contract` all passed.
Raw-ticket preparation uses reviewed process-completion criteria through normal
publication rather than fabricating missing Issue acceptance.

Restoring verified native toolchain inputs, correcting fixture output/resource
budgets and extending the fixture teardown wait are environment/test corrections,
not claimed production Red evidence. Existing packaged native MCP behavior passed
before and after the shared launcher/Responses-peer refactoring.

## Initial execution evidence (before the HTTP-probe correction)

Base commit: `11069366a8a9c49e7b0072211d990b51c901ab7c`. Changes remain uncommitted;
the executable digest below identifies the tested working-tree server rather
than implying that the base commit contains this correction.

Image: `local.octacity/factory-pilot@sha256:ee1e56d0adfd06f3d219f0e064219acb23deb0c6579bf6f418a75810c489bee9`.

| Executable | SHA-256 |
| --- | --- |
| Packaged server | `7285eb8baee344c961558912a7378edbbbc50c8068535f76c27f87a6257f6830` |
| Packaged Agent | `a3f4c6d70e6f1ad617e48c5982047df5c18e069ee7f73ca8893c1229241299a0` |
| Native Codex, Linux ARM64 | `0129f94f4f1197bd75f6c4265889061f386ed302bbc50c6e57e3e0660dfb28a7` |
| Official GitHub MCP, Linux ARM64 | `f8293e996624d4669ec25bc4aa8130995dfd912055feb439d363d0259c5651b4` |

Successful feature report:
`/tmp/oc81-evidence/baseline/source-c9cdd985-71eb-4503-8852-2ad68930579e/triage-profile-results.json`;
Run `698fc379-df22-4042-a459-32081380fa89`.

Historical defect report (function-only probe):
`/tmp/oc81-evidence/baseline/source-362a5d6b-3f12-4da1-93f7-97777a6f12ee/triage-profile-results.json`;
Run `0341eece-e3fa-4831-a210-176242414064`;
delivered effect `5139170f86dd5477b3d8705c274721e6e21eda06e8f091bccf2f727ad688aacf`.

After the final test-only Clippy refactoring, the complete defect method passed
again in 105.938 seconds through the normal Cargo launcher. Its report is
`/tmp/oc81-evidence/baseline/source-5697b5a9-3f7c-4081-8ec0-9c361bd64fcf/triage-profile-results.json`,
Run `7eef37a3-019d-4472-9bd9-fbe7645e6983`, delivered projection
`3ea48c5a0fce4a7edc54ecd13816007d9e3cf432ae05773aa2aa7cce655d7b39`.
The final log is `/tmp/octacity-81-offline-diagnostic.log`; its name is historical,
and this final run uses the ordinary Cargo test rather than a cached-harness shim.

Each schema-version-1 report contains exact selected Role/Profile refs, factory
catalog digest, configuration, input publication/ID, Source and Repository
bindings, Agent product manifest/toolchain, accepted node completions and final
Run/Build/attempt/call/Flow/delivery diagnostics. Adjacent
`packaged-triage-products.json` records actual server/console/native adapter
manifests and Source profile/configuration hashes. Credential canary assertions
guard retained public evidence. These local qualification artifacts are temporary
machine evidence, not a committed public artifact archive.

The exact same Linux GitHub MCP binary was separately qualified inside the pinned
Rust toolchain container with `--network none`, a read-only filesystem and finite
loopback API. `ReadyMadeGitHubMcpTests` passed; reports are under
`/tmp/oc81-evidence/ready-linux/` and log
`/tmp/octacity-81-ready-linux-mcp.log`. This checks the selected deployment binary,
in addition to the earlier host-native protocol qualification.

## Review correction: actual HTTP reproduction

The initial reports above used a direct `health()` call and did not establish
an HTTP endpoint observation. The current trusted probe starts the server from
the exact checked-out source, performs one loopback `GET /health`, counts actual
server request events and records the actual HTTP response status. Its reviewed
criterion explicitly names that HTTP operation. A pinned-Node behavioral test
separates a misleading `health() → 200` from the endpoint's HTTP 500: it failed
with the old probe and passed after the correction, including the server's
observed method/path.

The complete corrected defect/result method passed with three ordinary Builds,
two distinct exact profiles, independent accepted reproduction and one delivered
native label effect. Current report:
`/tmp/oc81-evidence/baseline/source-4645b684-da80-4576-a270-d73562fa80f8/triage-profile-results.json`;
Run `c52d8a84-e400-4da6-943a-a7f55f582ff5`.

Actual Agent identity, credential and durable paths now come from the generated
installation before initial start and restart; each installation loads the
qualified immutable image into its own SDK state through the existing MSB CLI.
The archive is the original `image.docker.tar` from
`tools/prepare_factory_pilot_image.py`, supplied explicitly through
`OCTACITY_FACTORY_PILOT_IMAGE_ARCHIVE`; imports with a different digest fail.
The completed Console/restart proof and follow-up Red/Green commands are recorded
in [Console verification](console-triage-verification.md).

## Reproduction commands and scoped regressions

Use the verified release/native fixture environment described by the existing
[native MCP qualification](mcp-execution-verification.md) and release harness.
Provide actual packaged release roots, reachable dedicated PostgreSQL/object
storage, qualified microsandbox image/toolchain and native executor inputs.
The opt-in class explicitly executes the otherwise ignored Cargo journeys:

```sh
OCTACITY_TEST_PACKAGED_TRIAGE=1 \
OCTACITY_TEST_GITHUB_MCP_GUEST=/path/to/verified/linux/github-mcp-server \
OCTACITY_FACTORY_PILOT_CLI=/path/to/verified/linux/codex \
OCTACITY_FACTORY_PILOT_IMAGE_ARCHIVE=/path/to/qualified/image.docker.tar \
python3 -m unittest tools.tests.test_factory_triage_pipeline.PackagedTriagePipelineTests

OCTACITY_TEST_GITHUB_MCP_SERVER=/path/to/native/github-mcp-server \
python3 -m unittest tools.tests.test_factory_github_mcp.ReadyMadeGitHubMcpTests
```

| Scoped check | Observed result |
| --- | --- |
| Packaged feature / defect Python methods (each runs exact ignored Cargo journey) | Passed, real native clients and ordinary execution |
| `python3 -m unittest tools.tests.test_factory_mcp_credentials.PackagedMcpTests` with verified fixture environment | Passed, existing native execution/isolation/cancellation/restart regression |
| `cargo test --locked -j2 -p octacity-release-harness --test release_vertical_slice` | 25 passed, 18 explicitly ignored opt-in tests |
| `cargo test --locked -j2 -p octacity-github --test issue_reporting` | 19 passed |
| `cargo test --locked -j2 -p octacity-server-application source_inputs_are_read_as_private_project_owned_data_without_fabricating_build_outputs` | One matching behavioral test passed |
| `python3 -m unittest tools.tests.test_factory_local_stand tools.tests.test_factory_source_installation tools.tests.test_factory_triage_mcp tools.tests.test_factory_triage_pipeline.FiniteTriageResponsesTests` | 31 passed, 4 explicitly skipped native/backend cases |
| `cargo clippy --locked -j2 -p octacity-release-harness --test release_vertical_slice -- -D warnings` | Passed |
| `cargo fmt --all -- --check` | Passed |
| `python3 tools/check_architecture.py` | Passed: 64 packages, 263 edges |
| `node --check server/tests/octacity-release-harness/fixtures/factory/triage-reproduction.mjs` | Passed |
| `pnpm --dir ui exec prettier --check tests/released/sourceJourney.ts` and targeted ESLint with zero warnings | Passed |
| `openspec validate add-factory-triage-pipeline --strict` and `git diff --check` | Passed |

Independent Standards and Spec reviews of tracked and new working-tree files
found no actionable findings. The two review axes included authority ownership,
fixture scope, duplication, fixed business values and documentation. A small
test-only `NativeTools` bundle keeps executable paths and immutable pins together;
Clippy passes without lint suppression.

Disk checks stayed above the 10 GB guard (approximately 22–37 GiB free during
qualification, 24 GiB at final verification). Only identified task-owned disposable
fixtures/server snapshots were removed. No user stand data was removed.

Task 8.2's complete Console/localization/restart journey is recorded separately.
Task 8.3's packaged adversarial matrix and task 8.4's operating guide/full release
gates remain unfinished. This evidence establishes the selected finite packaged
8.1 journey, not paid-model quality, live-provider behavior or all AI plugins.
