# Task 8.3 — packaged recovery and adversarial qualification

## Boundary

This qualification extends the existing packaged Source/Factory journey. It runs
normal packaged Server, Console gateway and Agent executables over disposable
PostgreSQL/S3, ordinary signed Builds and isolated Microsandbox guests. Native
Codex 0.161.0 uses its actual HTTP/stdio MCP clients; the external server is the
unmodified official GitHub MCP 2.0.3 for Linux ARM64. Local finite Responses and
HTTPS provider peers supply controlled data and faults. No production code,
state owner, runtime policy, credential fallback or alternative executor was added.

The new scenario module only selects test inputs and process barriers. The HTTP
bridge forwards real Issue MCP requests; the malformed-tool case deliberately
corrupts one reply after the real router has handled the request. Issue search,
read authorization and stage acceptance still belong to their existing owners.

## Cases and independently specified outcomes

- Admission crash: block the Source acknowledgement after the original Work/Run
  commit, kill the actual server process, restart it over the same journals, and
  release the barrier. Preserve original Work and Run; execute each selected AI
  once.
- Accepted-stage crash: hold the analyst's first model request until the accepted
  Project result is publicly visible, restart Server, and release the request.
  Preserve the exact immutable accepted result and each selected invocation.
- Provider-mutation crash: apply the real adapter's label mutation at the finite
  HTTPS provider, hold the reply, and kill Server before any acknowledgement.
  Preserve both accepted decisions, the logical effect ID and frozen input digest;
  preserve the unrelated human label and issue-open state; perform one native
  write. Under the installed 10-second attempt and 30-second claim lease, recovery
  retains a truthful `failed/exhausted` receipt. It must not renew the attempt
  deadline or manufacture a delivered receipt. `record_id` identifies the latest
  immutable state receipt and changes across the state transition; it is not the
  stable effect ID. Normal delivered behavior remains covered separately.
- Labeled/PR exclusion: native listing contains an unlabeled Issue, labeled Issue
  and PR. Preview exposes the selected and filtered Issues, omits the PR, and
  admission creates one source-only Run with no canonical Issue.
- Out-of-scope and duplicate: retain one Project decision and skip the analyst.
  Duplicate evidence references the exact authorized Issue actually consumed by
  native MCP. Unconfigured closing/reporting cannot occur.
- Missing goals: resolve the otherwise valid installed preset with the Project
  goals reference absent. Public diagnostics report required Context, no drafts
  are executable, and no Run is created. Then resolve the valid preset normally.
- Incomplete lookup: two explicitly authorized candidates with a one-item page
  produce real `has_more=true`. The finite model claims that observed value;
  acceptance routes to review without analyst dispatch or provider writes.
- Malformed model: the model returns an invalid result shape. No fit or analyst
  decision is accepted and the failed Build is retained.
- Malformed tool: corrupt a real MCP detail reply. No failed/unavailable
  observation may authorize classification or reporting.
- Injected model/tool text: return an authority-override canary in an actual
  authorized Issue description and the model's reason field. The reviewed flow,
  two distinct profiles, declared handoff and selected read-only tool scope remain
  in force; the analyst inherits no MCP tools or credentials.
- Failed reproduction environment: the frozen selected revision imports a missing
  dependency. Retain the accepted Project decision and failed command Build;
  dispatch no analyst and report neither a reproduced nor not-reproduced result.
- MCP token/connection revocation: rotate the server token or remove the reader
  after installation while the Agent retains its original selected configuration.
  Observe refusal with the original selected bearer and no unselected fallback,
  analyst or Reporter write. A required native connection can fail before any
  model request. Original execution budgets remain unchanged.
- Independent Reporter revocation: after accepting the Project decision, remove
  write access through the ordinary Project access API while retaining read
  access. Both triage decisions survive; choices retain only task-read; delivery
  records `failed/forbidden` and performs no provider write.

All pipeline cases check actual model/external credential separation, executable
and immutable image hashes, accepted result/Build counts, no canonical association,
and absence of credential canaries in public evidence. The existing scoped MCP
qualification additionally rejects hidden/foreign reads, invalid/wrong-purpose
credentials and mutation tools.

## Test-first workflow

Only qualification code changed. The existing ordinary feature pipeline and 19
GitHub reporting tests passed before extending their fixtures. The one-shot HTTPS
mutation barrier was first tested through the real reporting adapter; Red was the
missing barrier API, Green confirmed state applied while the response was still
held, then one applied write after release. All 20 reporting tests pass afterwards.

Each new packaged scenario was requested through a behavioral test before adding
its scenario selection and fault input. Initial unknown-case failures establish a
missing harness capability, not a production regression. Existing product policies
then passed without production changes. Package revision mismatch, missing native
fixture inputs, watchdog expiration and incorrect fixture expectations are not
claimed as production Red evidence. In particular, an incomplete-page scenario
must claim the observed `true` value; falsely claiming `false` exercises forged
evidence instead. Filtered Issues intentionally remain visible in preview.

Cargo checks use `CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2`, `--locked` and `-j2`.

```sh
# With the verified packaged/native inputs and isolated PostgreSQL/S3 configured:
python3 -m unittest -v tools.tests.test_factory_triage_adversarial.PackagedTriageAdversarialTests.test_restart_after_admission_keeps_one_original_run_and_each_ai_invocation
cargo test --locked -j2 -p octacity-release-harness --test release_vertical_slice packaged_triage_prepares_raw_unlabeled_input_without_a_canonical_card -- --ignored --exact --nocapture
python3 -m unittest -v tools.tests.test_factory_triage_pipeline.TriageFixtureProcessTests tools.tests.test_factory_triage_pipeline.FiniteTriageResponsesTests
cargo test --locked -j2 -p octacity-github --test issue_reporting
cargo clippy --locked -j2 -p octacity-github -p octacity-release-harness --all-targets --no-deps -- -D warnings
cargo fmt --all --check
python3 tools/check_architecture.py
openspec validate add-factory-triage-pipeline --strict
git diff --check
```

Run each packaged method against a fresh disposable database (including between
methods in the adversarial class). The ordinary supported stand deliberately has
a stable Agent identity; another installation with a new pool cannot reuse the
previous installation's enrollment binding. A temporary runner initially reused
one database across methods, leaving the second Job queued for the old Agent/pool;
those interrupted environment attempts are excluded from Green evidence.

Packaged methods are opt-in with `OCTACITY_TEST_PACKAGED_TRIAGE=1`; they require
all normal release-contract inputs plus the selected native Codex and GitHub MCP
binaries and immutable pilot image, as in task 8.1. Missing inputs are blockers,
not a qualified skip. An outer 600-second test watchdog includes installation,
guest startup and cleanup; it changes no signed execution deadline. Private
connection files are not committed or included in public evidence.

## Semantic and platform limits

These finite-model cases qualify schema enforcement, retained evidence, declared
execution, isolation, authority and crash recovery. They do not establish a paid
model's judgment, ability to recognize arbitrary prompt injection, live GitHub
availability or production-quality classification. Injected text is data supplied
to real clients and deterministic responses; no claim of general semantic prompt
injection immunity is made. A failed Build/unresolved stage is not a successful
triage or delivered effect.

Execution was on macOS ARM64 with qualified Linux ARM64 guests. This run does not
qualify native Linux/Windows release matrices. Paid LLM calls and live GitHub writes
remain a separately authorized optional pilot. Task 8.4 remains separate.

## Recorded results

Base commit: `8d206b94` (`fix(ci): restore packaged builds and release checks`).
Only qualification tests and this evidence document differ from that commit.
Normal package tools built/verified current Server and Agent snapshots; Console
assets are unchanged. No synthetic package manifest was used.

| Executable | SHA-256 |
| --- | --- |
| Server | `4c78f4de53bc4010630cf634a89f43a6db14f168a7c51e148abe876286cd1dc3` |
| Agent | `5ccc6fa63147a116e6075c36b3c4a4e4d65f8f0661b79c6551920c73d5b8e047` |
| Native Codex | `0129f94f4f1197bd75f6c4265889061f386ed302bbc50c6e57e3e0660dfb28a7` |
| Official GitHub MCP | `f8293e996624d4669ec25bc4aa8130995dfd912055feb439d363d0259c5651b4` |

Immutable guest image:
`local.octacity/factory-pilot@sha256:ee1e56d0adfd06f3d219f0e064219acb23deb0c6579bf6f418a75810c489bee9`.

Local evidence is under `/tmp/oc83-evidence/source-*/`. Each pipeline directory
retains `triage-profile-results.json`, exact public Build/attempt/Run/result/effect
diagnostics, safe provider paths, package metadata and executable/configuration
hashes. Checkpoints record actual before/after process IDs, original Work and Run,
accepted immutable results and stable logical effects. `triage-missing-goals.json`
records the readiness probe. These are temporary machine artifacts, not a committed
public archive.

Scoped compatibility commands use `cargo test --locked -j2 -p
octacity-release-harness --test release_vertical_slice <method> -- --ignored
--exact --nocapture`. All six passed (log `/tmp/oc83-compatibility.log`):

| Method | Duration |
| --- | --- |
| `packaged_source_only_import_creates_one_visible_canonical_issue_without_execution` | 22.14 s |
| `packaged_factory_source_admission_retains_canonical_issue_and_configured_status` | 66.61 s |
| `packaged_source_gateway_preserves_scoped_issue_mcp_authority` | 13.82 s |
| `packaged_issue_status_mapping_delivers_without_execution` | 36.10 s |
| `packaged_read_only_connection_with_installed_reporter_keeps_status_local` | 21.08 s |
| `packaged_source_uses_the_qualified_agent_and_installed_job_contract` | 14.19 s |

These exercise the existing import/manual Factory, server-owned Source/Reporter
and independent ordinary Job execution paths, including normal public access and
retained configured status. They do not depend on triage role names.

Additional regression/check results:

- `cargo test --locked -j2 -p octacity-github --tests`: 130 passed, zero ignored
  (`/tmp/oc83-github-all.log`). This includes all 20 reporting tests.
- `cargo test --locked -j2 -p octacity-release-harness --test release_vertical_slice`:
  27 ordinary tests passed; 20 opt-in tests remain ignored in that invocation.
  The specifically listed packaged methods were run explicitly.
- Python process cleanup and finite Responses: four tests passed.
- Scoped all-target Clippy with `-D warnings`: passed. Shared listing-only users
  explicitly allow their unused test-only barrier API; no production warnings
  are suppressed.
- Architecture: passed, 64 packages / 263 edges. Format, strict OpenSpec and
  `git diff --check`: passed.

Disk was checked before and during native builds/VM runs. It started around
21 GiB, reached approximately 12 GiB during concurrent isolated guests and stayed
above the 10 GiB working reserve. Only identified inactive task-owned package
snapshots were removed; no global Docker/Cargo prune or user stand cleanup ran.
Final disk and disposable-service cleanup are recorded after the last qualification. No commits.


All 13 adversarial pipeline methods passed individually against isolated
installation databases. The labeled-Issue/PR raw preparation test also passed
(22.62 s). The unmodified feature pipeline passed before fixture extension
(122.761 s). Key final reruns are `/tmp/oc83-mutation-final.log` (239.160 s),
`/tmp/oc83-stage-final.log` (180.978 s), `/tmp/oc83-negative-isolated.log`
(malformed tool 115.987 s; token revocation 69.244 s), and
`/tmp/oc83-final-negative.log` (the first, incomplete-lookup method passed; the
following invalid shared-database attempt was interrupted and replaced).

| Scenario | Report directory under `/tmp/oc83-evidence/` | Original Run |
| --- | --- | --- |
| `out_of_scope` | `source-0c2d1652-36dd-4a01-88e0-8f35bbd4b852` | `f9753a5c-a5ee-455e-98ac-47b760e38631` |
| `duplicate` | `source-77f504e2-6809-4dff-aed0-8f9c49bd5e7c` | `a60c874f-58ac-415b-8250-883762e1faec` |
| `admission_restart` | `source-bb234d11-5687-4312-8cae-e0f2fd2dfdd2` | `d308ab94-0dba-41f5-85c2-3f8360707e38` |
| `stage_restart` | `source-f529265e-255d-4708-9bd1-a4220443dc18` | `b602c46b-30c1-42d0-87fe-3b5aee970a67` |
| `mutation_restart` | `source-10ce4d89-041a-4d77-89f9-a9d3284dd547` | `ed192062-8cd8-41d7-aaea-415bd4d9490f` |
| `malformed_model` | `source-9edda619-fd9c-4ea6-acae-51d4cde2a2f5` | `33512045-b241-40fe-bed6-c4335058a85a` |
| `malformed_tool` | `source-dae50b4d-1079-41b2-8443-4975d4e5684f` | `0b7e5684-c644-4e00-96a7-11eb9940ebb5` |
| `injected_text` | `source-07508eb2-469d-41da-9c9e-e160f9e4120b` | `43ac308c-bc70-4913-8922-0239eed595ad` |
| `incomplete_lookup` | `source-04eb2316-63ba-4ba2-8cd5-5dccfe4e3069` | `a0a81dc4-0cba-48e5-8ba9-86c8e3325bd4` |
| `reproduction_failed` | `source-f49b3290-63d7-4b9d-a273-2ac0a01a13f6` | `e499ee78-32c9-4403-82ac-98fab34bc249` |
| `mcp_token_revoked` | `source-68739d68-938f-45e2-9264-5a44f53d5489` | `1ac30eb5-f05d-4cf6-a171-2cd81debb5ff` |
| `mcp_connection_revoked` | `source-453bbdee-7ba4-4c21-9d05-8dbaa3af64c2` | `a84ee038-542b-4ac7-ad5a-e1f2686f8b9b` |
| `writer_revoked` | `source-7f81e0e9-a271-4dc8-ac9e-543815fe36ec` | `dfa23d26-c9d3-4911-9a04-f3561892fbd5` |

After all packaged processes exited, both task-owned disposable containers
(`octacity-83-postgres`, `octacity-83-minio`) were stopped. Free workspace disk
returned to approximately 14 GiB. The qualified products, immutable image inputs
and safe evidence were retained for subsequent work. No commits were created.
