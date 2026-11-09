# Clean-state manual Factory authoring

For the operator procedure, see
[build a Factory pipeline from an empty Project](../operations/factory-pipeline-authoring.md).
This guide describes qualification fixtures and their evidence, not required
business definitions for a deployment.

The Console setup regressions additionally exercise creating an AI step without
editing schemas, explicit Work attachment, computed/manual budgets, access review
and cancellation, and retained advanced controls. Run:

```sh
cd ui
pnpm exec vitest run src/features/factory/StepExecutor.test.tsx src/features/factory/BudgetSettings.test.tsx src/features/factory/FlowGraphEditor.test.tsx src/features/factory/FactoryPublication.test.tsx src/features/factory/validateDraft.test.ts
pnpm exec playwright test tests/browser/factory-setup.spec.ts
```

The browser test uses bounded API fixtures and does not certify a live model or
MCP call. `cargo test --locked -p octacity-server-factory --test role_profiles`
separately checks that the server accepts an empty typed input plus required Work
Context, and rejects either missing contract. The disposable end-to-end fixture
below remains the execution and isolation qualification.

The separate [ticket delivery qualification](#ticket-delivery-qualification)
covers the post-triage path.

`factory_manual_authoring` qualifies tasks 5.1–5.4 of
`enable-manual-factory-pipeline-authoring`. It starts the actual Server and Agent
executables against PostgreSQL and S3 and executes ordinary Jobs in separate
Microsandbox VMs. Only the model CLI is a finite protocol peer; it does not own
admission, routing, persistence, compilation or completion.

## Technical prerequisites

- Current Server and Agent executables built from the revision under test.
  Release CI uses the checksummed installed candidates; a local run can select
  current debug binaries explicitly.
- A verified Agent runtime bundle (source-plugin registry and runtime manifests)
  and the pinned Octa release with its runner, Codex plugin and compatibility
  contract. These are technical capabilities, not saved business definitions.
- Qualified Microsandbox, its firmware and an immutable Linux guest image for
  the selected host/guest architecture. The image needs `/bin/sh`, `head`, `wc`
  and `grep`; no repository, model login or preinstalled workflow is needed.
  Use the qualified runtime repair described in
  [backend contracts](backend-contracts.md) when required by the installed MSB.
- Disposable PostgreSQL with permission to create/drop databases. The journey
  creates a unique database and runs the production migrations on normal startup.
- Disposable S3/MinIO with the `octacity-artifacts` bucket and fixture credentials.
- Private writable workspace/state directories and enough free disk for the
  selected Rust build and two bounded guest workspaces. Use the existing backend
  provisioning; do not point at a personal stand's writable directories.

The headless fixture installs a generic worker Build configuration, pool, Project
policy and generic Codex execution capability. No repository is created. It verifies
empty Flow, Factory Configuration, Build and Factory Run collections after this
technical installation and again checks that authoring has not started a Build.
Roles contain reusable generic guidance; assignments belong to the authored
steps. There is no triage preset, bootstrap Build, copied Run, alternate handler,
direct business-row insertion or stage-name resolver.

## Run

Build the selected binaries with the ordinary workspace commands. Provision the
backend using the existing release/backend-contract setup, then supply absolute
paths and disposable service endpoints:

```sh
export OCTACITY_MANUAL_SERVER_BINARY=/absolute/checkout/target/debug/octacity-server
export OCTACITY_MANUAL_AGENT_BINARY=/absolute/checkout/target/debug/octacity-agent
export OCTACITY_RELEASE_AGENT_ROOT=/absolute/verified/agent-installation
export OCTACITY_CONTRACT_OCTA_RELEASE_ROOT=/absolute/verified/octa-installation
export OCTACITY_RELEASE_BACKEND=microsandbox # linux-microsandbox on Linux
export OCTACITY_POSTGRES_URL=postgres://fixture:fixture@127.0.0.1:15432/postgres
export OCTACITY_MINIO_ENDPOINT=http://127.0.0.1:19000
export OCTACITY_MINIO_ACCESS_KEY=fixture
export OCTACITY_MINIO_SECRET_KEY=fixture-secret
export OCTACITY_CONTRACT_MICROSANDBOX_WORK_ROOT=/absolute/disposable/work
export OCTACITY_CONTRACT_MICROSANDBOX_STATE_ROOT=/absolute/disposable/state
export OCTACITY_CONTRACT_MICROSANDBOX_EXECUTABLE=/absolute/runtime/bin/msb
export OCTACITY_CONTRACT_MICROSANDBOX_LIBKRUNFW=/absolute/runtime/lib/libkrunfw.so
export OCTACITY_CONTRACT_MICROSANDBOX_ENVIRONMENT_IDENTITY=qualified-guest-v1
export OCTACITY_CONTRACT_MICROSANDBOX_IMAGE=registry.example/qualified-guest@sha256:...
export OCTACITY_CONTRACT_WORKSPACE_BYTES=1073741824
# Set MSB_AGENTD_PATH to the qualified guest agent when using a repaired runtime.
CARGO_PROFILE_DEV_DEBUG=0 CARGO_BUILD_JOBS=2 cargo test --locked \
  -p octacity-release-harness --test factory_manual_authoring \
  clean_project_authors_and_executes_two_isolated_agents_without_a_business_preset \
  -- --ignored --exact --nocapture
```

The firmware path must match the host installation (`.dylib` on macOS). The
credentials above are examples for disposable services, never provider keys.
The model peer receives only a private test credential and has no network access.
An explicitly selected journey fails if a prerequisite is absent; it never
silently skips. The existing release vertical-slice action runs it for the two
Microsandbox backends and propagates a failure to the release gate. Its services,
runtime and checked release installations are reused; no second stand is created.

## Assertions and evidence

All documents, Roles/Profiles, bounded schemas, gates, Flow, Configuration and
manual Work are authored through real management HTTP routes. Project document
reads verify exact contents. The two gates explicitly project the accepted
assessment summary and choose the classification route. The finite assessment
retains duplicate search as `not_requested`; it claims no verified search or
reproduction evidence. Components and priorities come from selected Project
documents rather than a second constructor vocabulary.

The model process checks its actual VM input: both steps receive the original
ticket and process criteria; only assessment receives Project goals; only
classification receives component/priority guidance and the accepted summary.
Classification rejects the predecessor's private investigation canary and its
unselected result fields. The source anchor contains no checkout or hidden
bootstrap files, and the explicit MCP configuration is empty.

Public diagnostics must retain four exact node results, two different ordinary
Build/Job identities and separate task, input and Context identities, even though
both agents use the same model. Builds remain explicitly source-free, with no
fabricated repository or revision. Server restart must preserve the terminal
state and exact Work, call, result and Build-link records.

Set `OCTACITY_RELEASE_EVIDENCE_DIR` to an existing private evidence directory to
retain `manual-authoring.json`: backend, selected Server/Agent binary SHA-256 and
public records verified after restart. Release CI supplies this directory and
includes it in its existing evidence artifact.

Successful runs remove their private scenario directory and database. A failed
run retains an `om-*` directory under the evidence directory (or next to the work
root when no evidence directory is supplied) and its unique database for diagnosis; process children stop on drop. Treat retained files as
private test evidence. The caller owns cleanup of disposable services and failed
databases/directories. Do not reuse or delete a personal stand as cleanup.

The first case qualifies headless authoring and isolated execution. The packaged
Console and recovery cases are described below. Additional compatibility matrices
retain their separate tasks. A finite peer does not qualify real-model quality,
provider delivery or production credentials.

## Packaged Console journey

The `packaged_console_authors_and_executes_two_isolated_agents_from_empty_project`
case runs the same production Server/Agent owners through the checksummed Console
and its packaged nginx routing. It creates a fresh database and Project for each
locale (`en`, then `ru`), uses a 390-pixel viewport and keyboard controls, and
creates the Project documents, agents, profiles, graph, conditional routes,
pipeline goals/criteria, Configuration and admitted Work through ordinary UI.
No browser route is intercepted and no business definition is seeded.

In addition to the headless prerequisites, provide matching packaged products,
the installed Playwright browser and nginx:

```sh
export OCTACITY_RELEASE_SERVER_ROOT=/absolute/verified/server
export OCTACITY_MANUAL_SERVER_BINARY="$OCTACITY_RELEASE_SERVER_ROOT/bin/octacity-server"
export OCTACITY_RELEASE_CONSOLE_ROOT=/absolute/verified/console
export OCTACITY_RELEASE_UI_ROOT=/absolute/checkout/ui
export OCTACITY_RELEASE_NGINX=/absolute/nginx
# Optional when nginx's standard MIME path differs from the packaged default:
export OCTACITY_RELEASE_NGINX_MIME=/absolute/nginx/mime.types
cargo test --locked -p octacity-release-harness --test factory_manual_authoring \
  packaged_console_authors_and_executes_two_isolated_agents_from_empty_project \
  -- --ignored --exact --nocapture
```

`OCTACITY_RELEASE_AUTHORING_LOCALE=en` or `ru` selects one locale for focused
feedback; omit it for qualification of both. UI dependencies and Playwright must
be installed in `OCTACITY_RELEASE_UI_ROOT`. The Server binary must be the one in
the selected verified Server package, rather than another local build.

The Console case adds one technical read-only Repository and matching worker
capability. A test-owned Git transport rewrites its HTTPS locator to a disposable
local mirror using the unchanged, verified SourceGit executable. This qualifies
exact source selection and isolated materialization, not remote Git connectivity.
The operator declares that source in the graph and connects only `README.md` at
its exact revision to classification. Assessment explicitly has no code. The
finite process checks the actual VM filesystem: the selected file is read-only,
the assessment source anchor is empty, and neither step receives Git metadata or
unselected files. Work itself retains its repository-free Project/input subject.

Both reasoning steps explicitly select **Task from admitted Work**. This uses
`context_binding.work_task` with a logical input name; it does not add Artifact
metadata to the model's data/result schemas. The existing input owner pins only
the admitted task reference before creating the Node Attempt. Restore checks it
against the original Work. Definitions and inputs without the optional selection
keep their historical wire form. Literal/projected Task Context remains supported.
For steps that need the same Work's acceptance document, also enable
**Acceptance criteria from admitted Work**. It binds the frozen acceptance
reference alongside the original task and does not modify the step's input
schema or the results received from earlier steps. For a manual Issue launch,
that document comes from the Issue's acceptance criteria at admission.
Selecting a document under multiple aliases preserves those aliases in Context;
the native task inventory contains each immutable document once and excludes the
primary task from its own specification list.

The journey also verifies refusal before publication when a native step has a
reference document named `task` but lacks actual Task Context. Exact document
previews and duplicate-name criteria choices do not read or persist bodies until
requested. Retained completion, call, Work and Build records are verified through
REST and compared after Server restart; private structured results are read through
the existing authorized Artifact download operation. Locale-specific evidence is
written as `manual-console-en.json` and `manual-console-ru.json`; the headless
report keeps its existing `manual-authoring.json` name.

A deterministic gate must explicitly select its result data as well as its
predicate and routes. For an unchanged bounded result, enable **Choose result
data** and select the root path; for a summary, choose only that field. New Flow
publication rejects a declarative gate with no mapping for a reachable outcome,
while parsing and viewing historical predicate-only programs remains supported.
This does not introduce an automatic payload/context handoff.

For an offline Microsandbox guest imported from an archive, register the full
immutable image reference used by `OCTACITY_CONTRACT_MICROSANDBOX_IMAGE` in the
disposable cache. A tag for the same image is a different cache lookup. For example,
with the private MSB home configured for this test environment:

```sh
msb image load --input /absolute/qualified-guest.tar \
  --tag "$OCTACITY_CONTRACT_MICROSANDBOX_IMAGE"
```

## Three-step isolation and resources

Two additional headless journeys author an unrelated bulletin workflow:
**compose → translate → audit**. They reuse the same ordinary HTTP, PostgreSQL,
Artifact, Server, Agent and Microsandbox owners:

```sh
CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 \
  CARGO_BUILD_JOBS=2 cargo test --locked -p octacity-release-harness \
  --test factory_manual_authoring unrelated_three_step \
  -- --ignored --nocapture --test-threads=1
```

`unrelated_three_step_repository_free_workflow_retains_only_selected_inputs`
creates no Repository. All three steps use the same Role/Profile and model,
but separate ordinary Builds, Jobs, input identities and Context manifests.
Each step explicitly selects the admitted task, completion criteria and just
its own Project document. The next step receives only the selected scalar
caption or translation. Private result fields, investigation messages and
an unselected Project document have distinct canaries which must be absent.

`unrelated_three_step_workflow_isolates_repositories_a_b_and_no_code` declares
repositories A/B with different exact revisions. Compose has no code;
translate receives only read-only `README.md` from A; audit receives only
read-only `README.md` from B. Both repositories also contain an unselected
file. The finite guest checks the actual mounted content, its read-only mode,
and absence of every other source entry, including Git metadata. Work remains
a repository-free Project/input subject throughout.

The A/B source setup also publishes an independent Project policy edit before
installing either repository. Technical installation retains the exact version
returned by each policy publication for its next `If-Match`; repository counts
are never used as policy versions.

The verified SourceGit executable is copied unchanged with a private transport
observer. The repository-free journey must record no Git invocation. The A/B
journey uses the real SourceGit process and test-owned local transport rewrites;
it qualifies isolation and exact materialization, not remote Git connectivity.

The finite guest also puts copies of its actual protected execution package
and node envelope into its ordinary private trace Artifact. Through authorized
Artifact download, the test validates the package, compares its Context identity
and digest to the retained call, and checks exact document IDs/digests/sizes,
selected payload, original Work task and predecessor result references against
the public journal. These are test probes, not model evidence or a new input
owner. They grant no verified fact or delivery authority. Existing diagnostics
do not expose protected execution documents.

Before valid publication, both validate and publish routes must reject an
unsupported network grant and a result source naming a foreign Run. The A/B
case additionally rejects an undeclared source, broader read scope, write
access and an accepted-candidate reference from A used for B. Rejected drafts
must create no Flow, Run or Build. Completed reasoning results must create no
candidate, verification evidence or delivery record implicitly.

The exact retained records and input probes are compared after Server restart.
With `OCTACITY_RELEASE_EVIDENCE_DIR`, these cases write distinct
`manual-isolation-no-code.json` and `manual-isolation-a-b.json` reports alongside
the existing headless and Console reports. Keep these artifacts private. Both
Microsandbox release jobs run the headless baseline and both isolation cases;
failure of any selected case fails the gate. Recovery is described below;
compatibility matrices remain a separate qualification task.

## Recovery and delivery truth

The recovery journey authors the bulletin Flow with an ordinary HumanGate
between compose and translate. It waits for compose's accepted result and the
gate, verifies that only one invocation/Build exists, then publishes later
same-name task, criteria, glossary and checklist documents. These are new
immutable publications, not edits to the selected Artifacts or an inferred
document version chain.

The Server and idle Agent processes stop and restart with their existing durable
state. The Agent's normal coordinator transport retries are finite; the fixture
restarts its process after Server readiness, without changing that policy. Before
resuming, the management API must return the exact accepted predecessor records.
Only the explicitly declared successors run. Their private guest probes must
retain the original task, criteria, selected documents, projected scalar inputs
and provenance, reject later-document/private canaries, and create no Git calls.
The completed history contains four node results including the gate and exactly
three ordinary reasoning Builds/Jobs. It is checked again after another Server
restart.

For Flow/Configuration publication and the gate response, a bounded transparent
TCP relay forwards the actual HTTP request to the normal Server, consumes its
successful committed response, and closes without returning it to the client.
The test requires a real client transport failure before replaying the original
body, idempotency key and precondition. Publication retains one Flow and one
Configuration, and response replay cannot create another logical successor.
No handler, transaction, journal or workflow outcome is replaced by the relay.

```sh
cargo test --locked -p octacity-release-harness --test factory_manual_authoring \
  manual_workflow_recovers_after_accepted_result_with_original_documents_once \
  -- --ignored --exact --nocapture
cargo test --locked -p octacity-server --test factory_work_source_pilot \
  'postgres::recovery::' -- --ignored --nocapture --test-threads=1
```

The second command uses the existing configured trusted-action, Source,
integration grant, hosted Reporter, PostgreSQL outbox and process-adapter owners.
Only the provider protocol peer is finite. Its tests reconstruct the store and
Reporter worker and read delivery diagnostics through fresh normal Server HTTP
instances. Pending delivery, an applied write with a lost reply, a revoked Project
write grant, permanent provider failure, retry exhaustion and original deadline
expiry remain separate from the accepted action result. Recovery must preserve
Work/Flow and operation identity, perform at most one native write after the lost
reply, perform no provider call after revocation/deadline expiry, and never
restart terminal delivery. The read-only API must expose pending, delivered and
failed states truthfully as one logical effect.

The revoked/expired cases first retain a retryable pending delivery. After
reconstructing the store, they revoke writer access or advance beyond the
original deadline, restore a responsive provider peer and resume with monotonic
time. The provider request log must remain unchanged during that recovery,
while the operation identity and accepted Work/Flow remain exact.

These provider tests qualify the common protocol/ownership boundary, not live
GitHub authentication or the native GitHub adapter. The physical recovery case
qualifies macOS ARM64 with Linux ARM64 Microsandbox guests; Linux host execution
is selected by the corresponding release gate, not claimed by a local macOS run.
Both Microsandbox release jobs select the physical recovery case and durable
Reporter cases and propagate their failures. `OCTACITY_RELEASE_EVIDENCE_DIR`
retains `manual-recovery.json` alongside the isolation reports. Compatibility
qualification is described below.

## Ticket delivery qualification

Use the [operator walkthrough](../operations/factory-pipeline-authoring.md#author-ticket-delivery)
to publish a Flow assembled from an empty graph. On the disposable macOS ARM64
stand, the published Configuration admitted a manual Issue with its exact
earlier triage Run. The Issue showed its selected Project specification,
independent review and confirmed test PR. Automatic intake and Reporter effects
were disabled for that trial.

Run the Console authoring and Issue outcome regressions with
`pnpm exec playwright test tests/browser/factory-ticket-delivery.spec.ts tests/browser/issue-outcomes.spec.ts`
from `ui`. They exercise the operator path with fixture responses.
Run `cargo test --locked -p octacity-server-application --lib ticket_delivery`
to check Flow publication, policy and input validation, and lifecycle behavior.

For the packaged journeys, reuse the checksummed Server, Agent and Octa
installations, qualified Microsandbox and disposable services described above.
The pilot additionally needs the pinned Git executable
(`OCTACITY_CONTRACT_GIT_EXECUTABLE`), the finite model programs
(`OCTACITY_FACTORY_PILOT_CLI` and `OCTACITY_FACTORY_VERIFIER_CLI`) and the
verified GitHub adapter executable (`OCTACITY_FACTORY_GITHUB_PLUGIN`).
Select the pilot programs from the release harness fixtures and use the
qualified guest with its configured Node executable. The pilot uses a
local HTTPS/Git forge and creates its own database. Run the cases serially
when they share the same guest runtime state:

```sh
for test in \
  packaged_delivery_recovers_the_accepted_real_review_after_server_restart \
  packaged_matching_ticket_waits_for_approved_requirements \
  packaged_large_ticket_delivers_the_reviewed_specification \
  packaged_unreproduced_defect_stops_before_test_authoring \
  packaged_review_findings_stop_delivery_after_one_rework
do
  cargo test --locked -p octacity-release-harness --test factory_pilot \
    "$test" -- --ignored --exact --nocapture || exit
done
```

The separate automatic intake case uses the packaged Console proxy settings
above, a private `OCTACITY_RELEASE_EVIDENCE_DIR` and the verified adapter as
`OCTACITY_RELEASE_GITHUB_BINARY`. It tests admission and data handoff with
a deterministic Flow. The pilot above covers ordinary Agent/Build execution
and actual PR publication against the fixture.

```sh
cargo test --locked -p octacity-release-harness --test release_vertical_slice \
  packaged_downstream_source_waits_for_separate_enablement_and_binds_exact_triage \
  -- --ignored --exact --nocapture
```

The qualification on macOS ARM64 observed these Red and Green results:

| Case | Red | Green |
| --- | --- | --- |
| Small ticket and PR recovery | The graph ended at `finished`. | The graph reached `delivered_for_review` and recovered the original PR URL after losing the creation response and restarting the Server. The forge received one creation request. |
| Matching requirements policy | The Run ended before asking for approval. | The original pending request gated private test authoring, and the approved Run delivered. |
| Large ticket | Requirements review began before the specification was captured. | All four OpenSpec files were captured before approval and survived into the published branch. |
| Non-reproduction | An unresolved defect reached test authoring. | An ordinary command checked the admitted revision and retained `cannot_reproduce` before test authoring. |
| Bounded rework | The negative review ended after the first implementation. | A new candidate used the retained remediation and received fresh checks. The second rejection waited for an operator. |
| Separate automatic intake | The downstream Run lacked the immutable triage input. | The exact accepted result passed to the separately enabled downstream input, with one admission retained after restart. |

Use the test PR link to inspect the published revision. A live operator merges
in the forge under the repository's branch rules.

### Regression checks

Run the portable Rust checks from the repository root. The release harness
also has opt-in packaged tests, including the protected test case below.

```sh
cargo test --locked -p octacity-server-application --lib
cargo test --locked -p octacity-server-application --test manual_trigger
cargo test --locked -p octacity-server --lib
cargo test --locked -p octacity-server-factory -p octacity-protocol \
  -p octacity-delivery-protocol -p octacity-work-source-protocol \
  -p octacity-github -p octacity-server-delivery -p octacity-server-work-source
cargo test --locked -p octacity-release-harness --test release_vertical_slice \
  packaged_triage_prepares_raw_unlabeled_input_without_a_canonical_card \
  -- --ignored --exact --nocapture
cargo test --locked -p octacity-release-harness --test release_vertical_slice \
  packaged_factory_source_admission_retains_canonical_issue_and_configured_status \
  -- --ignored --exact --nocapture
cargo test --locked -p octacity-release-harness --test factory_pilot \
  packaged_protected_author_precedes_real_coder_capture_without_private_context \
  -- --ignored --exact --nocapture
```

The protected test case runs on the same qualified Agent and guest used by the
Delivery pilot. It verifies that the real implementation capture follows the
protected test author and receives only its approved inputs.

For the two-profile triage regression, use the
[native Agent prerequisites](../operations/factory.md#qualify-project-policies-with-the-native-agent).
Stage the Agent with the local-stand Codex fixture and use the image manifest
digest reported by Microsandbox for the archive. The finite model peer runs
through native Codex, the installed MCP readers and ordinary Agent Builds.

```sh
OCTACITY_TEST_PACKAGED_TRIAGE=1 python3 -m unittest -v \
  tools.tests.test_factory_triage_pipeline.PackagedTriagePipelineTests.test_source_invokes_both_reviewed_profiles_with_native_mcp_evidence
```

For the browser suite, use `pnpm test:browser` from `ui`. The manual Delivery
test initially reached the default timeout while publishing and opening the
Issue. Giving that journey a dedicated timeout let both publications and the
Issue launch assertions complete. The desktop Project screenshots on macOS
were updated after verifying the Factory settings entry against the Linux
references. Both locales passed again in the full browser suite.

### Native Tester and verification launch modes

On 8 November 2026, the full **Ticket delivery · v1** example was also assembled
and published through Console against the current native macOS Server and real
PostgreSQL/S3, using an isolated copy of the qualification installation. No
reproduction command was installed or selected, and no test pack, manual manifest
or check script was supplied to the editor. The selected Tester author and
baseline/candidate commands provide the protected-test path. Specification and
implementation source scopes and reviewed-candidate consumers were selected
explicitly in the UI.

Flow `5b5cc244-13d8-4421-aeca-a980bbe39fad` v1 passed authoritative validation and
publication. Its launch Configuration `c8f7c203-c97f-440d-9cf3-3b4f156f0286` v1
also passed validation and publication through Console and remained disabled.
This check qualifies assembly and publication, without a new model execution or
PR. The execution qualification for both verification modes is recorded below.

The standard stand installs the test author and the scoped validation Tester
with a separate single-slot validation Agent. The current Console offers
**Verification analysis → Without AI analysis** or a bound **Tester** step.
Factory dispatches the mandatory verification Build in both cases. A bound
Tester starts after its result is retained and reads `verify_candidate` with
`action: status`; its prose cannot override the runner verdict. One active
Flow slot is sufficient. Changing analysis selection does not publish a new
test suite. The execution records below describe the earlier tool-triggered
launch behavior; the mandatory-dispatch regression is recorded separately.
Upload supplementary checks
in the test-author step and freeze their explicit shell entrypoints or
Octafile tasks before publishing the Flow.

The Microsandbox scoped Tester reserves two vCPUs in its signed Job: one
for the model and one for its private reader. Its ordinary Build Configuration
must allocate both vCPUs; raising permission ceilings alone does not allocate
resources. Its installed Runtime, Profile, enclosing Flow, Project access profile
and Agent ceiling must cover
that allocation. The direct verifier and other AI steps keep their own
allocations; the separately registered verification Agent remains single-slot.

For a bounded smoke ticket, request a new file under a permitted source path,
state its exact contents in the original acceptance, and retain the original
triage Run at manual launch. Verify the following through Run details and
the Issue outcome cards:

- Tester publishes the protected handoff before Coder starts. The baseline
  retains an expected behavioral failure, rather than a setup error.
- Coder captures a candidate and a separate Reviewer inspects that revision.
  Reviewer reports the exact frozen node input identity, candidate revision
  and candidate digest. The input identity is the top-level `input_digest` in
  the task envelope, not the Context or role-package digest. Its `findings`
  contain actionable defects only;
  uncertainty remains an indeterminate quality judgment.
- In the verification step, choose **Accepted candidate from another step**,
  its implementation step and the accepted outcome as the step code revision.
  A base-revision checkout cannot carry a candidate-bound Context. The trusted
  publication step also selects that accepted candidate as its read-only
  resource when consuming the bundle and manifest; **No code** deliberately
  carries no repository authority.
- Candidate verification starts after accepted review and consumes the same
  protected handoff digest as the baseline. The scoped mode uses a separate
  ordinary Build and returns only the bounded runner verdict to Tester.
- The trusted publication step records a confirmed draft PR receipt. The
  Issue's PR link belongs to this exact Run and reviewed candidate. No merge
  or external Issue Reporter action is required for this check.

The macOS ARM64 standard stand qualified both modes on 8 November 2026 with
real model calls, native Microsandbox Jobs and GitHub draft publication:

| Mode | Flow | Completed Run | Confirmed draft PR |
| --- | --- | --- | --- |
| CI/CD Build | v40 | `26b8f939-9e4e-4b14-9bdd-db4eb61928d1` | [#4](https://github.com/OctaHive/OctaCity/pull/4) |
| Scoped Tester | v44 | `1744f85a-47c2-4ab8-bf86-0fdf696104b2` | [#5](https://github.com/OctaHive/OctaCity/pull/5) |

Both Runs retained the expected-failure baseline and the same original
protected handoff at candidate verification, accepted independent review,
confirmed publication and the exact PR card on the Issue. GitHub's draft head
and base matched each retained receipt. The base was
`3205c5415ba4b32c4bed71038b593439aeba0b37`; candidate revisions were
`5b88d0dddca11b0de625fd8b81054e4e85539f78` (direct) and
`5a0e818b10bbd5ea4bd4ef7760a2e24c858433ec` (Tester).

The scoped caller Build `5b067ffc-355e-5f38-bca9-f80fa3ea6ab6` and runner Build
`fa5d6e88-9d74-5098-88ba-5c1d476d74b1` had successful Jobs on different Agents.
The original protected handoff digest was
`789fc5239cd9d66497551500812240b96e85ecc0654ec953df35f2b8a4838eda` for both its
baseline and candidate check. The native Console journey passed in 6.3 minutes.
Its scoped Profile v5 used the qualified Codex adapter digest
`0f72eeb3efd443b54c4730384ae0b994455d3a744640c4c1fba6d15a47a66d51`.
Those historical Runs used the source-pinned qualification adapter. The same
workspace and MCP behavior is now published in Octa 0.9.0; the stand pins that
release without a local plugin replacement. See the
[installation and release pin](../operations/local-stand.md).

The native qualification exposed transport and materialization failures.
Each was reproduced before its fix through an existing public boundary:

| Boundary | Observed Red | Observed Green |
| --- | --- | --- |
| Stand integration restart | Replacing the Server image rejected the retained integration because the image shipped another provider binary. | Restart retains the manifest-pinned executable and credentials; changing the installed executable still prevents startup. |
| Delivery provider upgrade | Installing a repaired provider conflicted with the original immutable adapter installation. | The installer retains the original provider and policy identities, installs the new exact provider separately and reuses an unchanged installation on replay. |
| Git publication scratch | The hardened Server had no writable system temporary directory, so the provider rejected candidate publication. | Git publication confines temporary state to its installed private input directory and removes it after rejection. |
| Trusted action deadline | A lost publication response followed by deadline expiry repeatedly returned invalid input and left the Run running. | The successor settles only the original operation through its cancellation boundary and records its measured usage and stopped state without requiring a user cancellation or reporting successful publication. |
| Reconciliation batches | Old failing Runs repeatedly opened the shared worker circuit, so newer Runs outside the first batch could remain admitted. | Individual failures are counted and logged with the exact Run ID, retain their original claim expiry and permit the next batch immediately; shared claim-store failures still fail the worker pass. |
| Built-in fan-out | A native fork looked up an external executor and repeatedly returned invalid input before either verification branch started. | The ordinary Flow owner retains the frozen typed payload and its declared fork outcome without an external executor. A separate cancellation regression verifies that an unfinished fork requires no external stop receipt. |
| Reader time allocation | Preparation consumed part of the Job window, so reserving the entire installed reader time ceiling rejected the scoped Tester before model startup. | The existing controlled execution regression fails at the physical backend boundary with a 29-second remaining Job and a 30-second installed ceiling, then passes when the helper is narrowed to the remaining allocation. Resource partition and isolation checks remain enforced. |
| Microsandbox CPU allocation | A 100-mCPU reader reservation left a 900-mCPU main workload, which Microsandbox cannot run. | Stand installation regressions require a whole vCPU for the reader, two in the scoped ordinary Build and matching local authority. Upgrades retain earlier Build and compiler identities and repair incomplete installations. |
| Unattended scoped MCP approval | Native Codex rejected `verify_candidate` before the host hook because its MCP default required interactive approval under policy `never`. | The public Octa invocation regression requires delegation only for explicitly listed MCP tools when the required blocking host authorizer is installed. Without that authorizer, or without an explicit list, no approval override is emitted. All 146 adapter tests pass, with one optional test ignored. |
| Published Octa installation | The previous pins selected 0.8.0; source staging tried to apply the qualification patch, and release staging rejected the published workspace capability. | The 0.9.0 pin, unmodified source staging, unchanged native bundle installation and common release staging pass. A package without the workspace capability is rejected before publication. Actual released Linux ARM64 bytes and the pinned source archive pass the OctaCity verifiers. |
| Exact access reference review | Console rejected an already covered plugin request when profile and resource responses serialized its fields in different orders. | Draft validation accepts the same identity, version and digest regardless of object key order; a different digest still requires an enclosing grant. The existing grant representation is retained. |
| Git capture | A depth-one checkout with omitted parents could not produce the candidate bundle. | The private capture Git retains the bounded shallow boundary and produces a verified bundle without running checkout hooks. |
| Completed runner accounting | Capture rejection discarded the completed model usage. | The failed Job retains completed runner records and usage without promoting a candidate. |
| Read-only candidate verification | A required-deliverable policy was mistaken for a request to capture another candidate, so Job signing failed. | The verifier retains that policy and all original private inputs while signing a read-only Job with materialization and no new capture. |
| Selected role preparation | Candidate bundle bytes were read as model text. | The selected role uses the exact retained candidate reader, excludes capture bodies and retains its selected textual context. |
| Job event persistence | PostgreSQL JSONB rejected a command's NUL byte before terminal completion. | The JSON payload column preserves the exact event through upgrade, restart, overlap and replay; conflicting bytes remain rejected. |
| Retained native trace | An otherwise successful reviewer was rejected because its JSON-encoded tool output contained NUL. | Original tool output bytes are accepted without rewriting; JSON framing, metadata, sensitive-material, size, sequence and capture-digest checks remain enforced. |
| Agent HTTP event delivery | A reverse proxy's HTML 502 response ended the Agent. | Transient non-protocol responses retry the original fenced batch and idempotency key; permanent authentication failures do not retry. |

Run the focused regression commands from the repository root (the PostgreSQL
case requires the disposable database prerequisites):

```sh
cargo test --locked -p octacity-changeset --lib
cargo test --locked -p octacity-github --lib publication_uses_only_its_installed_private_directory_for_git_scratch
cargo test --locked -p octacity-job --lib capture_rejection_keeps_completed_runner_accounting_without_a_candidate
cargo test --locked -p octacity-server-application --all-features --lib selected_role_prepares_candidate_context_without_reading_capture_files_as_text
cargo test --locked -p octacity-server-application --all-features --lib validation_materializer_publishes_the_invocation_contract_and_signs_every_original_input_once
cargo test --locked -p octacity-server-application --lib an_expired_trusted_action_settles_its_original_operation_and_leaves_running_without_a_user_cancel
cargo test --locked -p octacity-server-application --lib native_capture_preserves_json_encoded_control_bytes_in_tool_results
cargo test --locked -p octacity-server-application --lib factory_reconciliation_tests
cargo test --locked -p octacity-server-application --lib factory::execution::tests::factory_flow_tests
cargo test --locked -p octacity-server-store-postgres --all-features --test store_contract nul_bearing_job_output_survives_upgrade_replay_and_terminal_completion -- --ignored --exact
cargo test --locked -p octacity-coordinator --test coordinator
cargo test --locked -p octacity-lifecycle -p octacity-execution
python3 -m unittest tools.tests.test_local_stand_entrypoint
python3 -m unittest tools.tests.test_factory_delivery_setup
python3 -m unittest tools.tests.test_factory_stand_triage.FactoryStandTriageTests.test_guest_image_contains_only_the_public_ca_and_returns_the_verified_msb_identity
```

The guest image also qualifies local Git from the pinned Rust image before
publishing its immutable identity. The native probe first returned command
not found; the repaired guest ran Git and compared two fixture files. This
adds local candidate inspection without changing the step's read-only source
mount or granting forge credentials.

## Compatibility matrix

Task 5.5 uses the same disposable PostgreSQL/S3 prerequisites. It checks the
existing public storage/application/protocol boundaries; it does not reset a
stand, rewrite accepted definitions or require a real model/provider account.
Run every command to completion: Cargo stops at the first failing test target,
so an earlier partial run does not establish a passing matrix.

```sh
export CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
export CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2
cargo test --locked -p octacity-server-store-postgres --all-features \
  --test factory_flow_management --test factory_discovery \
  --test store_contract --test configurations --test internal_triggers \
  --test migrations -- --ignored --test-threads=2
cargo test --locked -p octacity-protocol -p octacity-server-factory \
  -p octacity-server-store -p octacity-server-job --all-features --lib
cargo test --locked -p octacity-server-job --all-features --test job_spec
cargo test --locked -p octacity-server-application --all-features \
  --test manual_trigger --test agent_registration
cargo test --locked -p octacity-server-application --all-features --lib \
  'factory_admission_tests::'
cargo test --locked -p octacity-server-application --all-features --lib \
  'factory_configuration_cqrs_tests::'
cargo test --locked -p octacity-server --all-features \
  --test factory_work_source_pilot -- --include-ignored --test-threads=1
pnpm --dir ui exec vitest run \
  src/features/factory/FactoryDefinitions.test.tsx \
  src/features/factory/FactoryPublication.test.tsx \
  src/features/factory/FactoryWorkForm.test.tsx \
  src/features/factory/FlowBindings.test.tsx \
  src/features/factory/FlowDocuments.test.tsx \
  src/features/factory/FlowCodeResources.test.tsx
```

| Boundary | Compatibility evidence |
| --- | --- |
| Saved Flow/Configuration versions | Exact publication replay and replacement preserve old versions; historical definitions remain readable when current execution is disabled or choices change. |
| Historical Work/Run upgrade | Frozen pre-backfill rows migrate without changing original Work, Run or current state. New Work cannot enter legacy backfill without explicit verified native import. |
| Repository-backed and source-free Builds | Mixed history survives additive migrations, independent pools, retry and cancellation; old Job templates and Trigger replay identities remain exact. |
| Source preparation | Managed input publication, retained preparation, provider-neutral selectors, revocation and lost acknowledgements preserve the admitted target and original Run. |
| Context and step resources | Legacy Context bytes omit new optional fields; exact task, selected source pins and accepted result provenance survive restore without granting sibling access. |
| Protocol/signing | Checked-in v1–v3 bytes and signatures remain stable; explicit v4 source-free intent negotiates separately, and incompatible revisions are rejected. |
| Ordinary CI/CD | Manual, schedule, webhook and internal Trigger paths retain ordinary Build causality, retry, suppression and terminal-outbox semantics independently of Factory. |
| Operator inspection | Component tests retain exact saved versions and inputs. The durable Source/Reporter pilot also checks real HTTP and Console inspection after Server restart with two provider vocabularies. |

Migration 0112 adds a typed 16 MiB bound to retained `flow_code_pins`, using the
existing Flow-publication storage ceiling. It preserves historical SQL NULL and
all valid bounded exact references. The migration neither edits 0109/checksums
nor rewrites Run documents, resource identities or replay digests. Its storage
regression starts from schema 0111, retains an original Run across upgrade and
repeated migration, rejects oversized/non-array updates and verifies the exact
snapshot again through a reconstructed store.

The durable pilot explicitly imports the native Issue before waiting for its
queued Work associations. A running normal Server performs bounded recovery;
test setup never seeds associations or broadens the one-time legacy migration.
This ordering also means import uses the already selected Issue workflow: the
only later business-status changes are the two explicitly reviewed transitions.

Local qualification covers macOS ARM64 with actual disposable PostgreSQL/S3 and
finite Source/Reporter peers. Physical isolated execution remains qualified by
the separate 5.1–5.4 journeys above; this matrix does not imply another host or
backend qualification or live-provider authentication. Common-authoring cutover,
the optional stand example and retained-client compatibility are documented in
[authoring migration](../reference/factory-pipeline-authoring-migration.md).

### Mandatory verification dispatch regression (2026-11-08)

The dispatch, ordering, one-slot admission, Console selection and installed
Tester instructions were changed through separate Red/Green cycles. Before
implementation, the tests observed missing mandatory dispatch, premature Tester
eligibility, the two-slot restriction and the old tool-triggered instructions.
The focused regression commands are:

```sh
cargo test --locked -p octacity-server-factory --lib
cargo test --locked -p octacity-server-application --all-features --lib factory::validation::tests::factory_protected_test_tests
cargo test --locked -p octacity-server-application --all-features --lib scoped
python3 -m unittest discover -s tools/tests -p test_factory_delivery_setup.py
cd ui
pnpm exec vitest run src/features/factory/VerificationLaunch.test.tsx src/features/factory/FactoryAuthoring.test.tsx src/features/factory/FlowGraphEditor.test.tsx
pnpm build
```

Green results: 348 Factory tests, eight protected-test adapter tests, 16 scoped
execution tests, 20 stand tests (one platform-specific skip), and 39 Console
tests. Clippy with `--all-features --lib --tests -- -D warnings`, Rust formatting,
Console lint/formatting, architecture checks and strict change validation pass.
The documentation changes have no executable behavior; strict OpenSpec
validation and whitespace checks cover them.

For a small implementation qualification, assign only the requested source
change to Coder. Candidate bundles, manifests and their identities belong to
trusted Agent capture. The model must not manufacture these publications; an
absent accepted specification remains absent.

The native run also exposed an expired-dispatch regression: when no Build had
been accepted, reconstructing private inputs after the node deadline prevented
settlement. The adapter regression first failed with `InvalidInput` for both
source-only and captured-deliverable cases, then passed after expired missing
operations delegated directly to the common fenced settlement path. Existing
Builds still require independently verified evidence; this is not a successful
validation result or a renewed execution window.

Native qualification used disposable PostgreSQL/S3, the standard Delivery image,
Octa 0.9.0 and Microsandbox 0.7.6 on macOS ARM64. Factory used the standard stand's
30-second ownership window. Both Flows admitted one active node and completed
all 14 nodes without stops:

| Scenario | Factory Run | Retained runner verdict | Resolved terminal |
| --- | --- | --- | --- |
| Accepted candidate | `8cc8e463-48ed-4c59-a3d2-8c02f4dfc168` | `acceptance: passed` | `verified_candidate` |
| Frozen operator check fails | `6c5b0fd8-3cf9-489b-a8c2-41b3eb544c5e` | `acceptance: failed` | `protected_validation_gate_failed` |

Each Run retained exactly one mandatory candidate-check Build. Its verified
completion preceded the first Tester attempt. The baseline check input, candidate
check input and final report named the same exact protected inventory within each
Run. Tester reported the actual verdict; its successful analysis completion did
not turn the second runner's failure into a pass. Both resolved terminals were
also confirmed in the persisted lifecycle checkpoint. No PR or GitHub update was
published, and the user's running stand, volumes and pipelines were unchanged.

The failure-path Run used the additively installed current Tester compiler and
Profile through the ordinary Runtime, Profile and Flow publication APIs. The
successful Run also confirms compatibility with an older pinned Tester Profile:
its redundant `start` call reused the single already accepted Build.

An earlier diagnostic deliberately gave the old compiler contradictory
instructions to omit tools and claim success. It timed out without final resource
accounting and entered an explicit wait for settlement; it produced no accepted
analysis or successful verification. This is fail-closed evidence, not a claim
that an optimistic AI response completed that diagnostic, nor qualification of
resource-accounting recovery. The completed failure-path Run above uses the
current installed instructions and the actual retained runner failure.

### One Tester profile, independent step execution

The stand publishes one current **Tester · Delivery** profile with the same
model, harness and AI settings for test preparation and optional CI analysis.
Each step selects its installed execution mode and reviews its own access.
Legacy profiles omit `execution_binding` in canonical serialization; their
content identities and existing Flow pins remain unchanged. Earlier stand data
is retained rather than rewritten.

Observed Red/Green boundaries for this correction:

- The core role-profile regression first rejected a reusable profile. It now
  accepts one exact profile in two steps with different qualified Builds and
  independent permissions.
- Application binding resolution initially rejected the second mode. Invocation
  and recovery tests then exposed inherited tools, credential-purpose coupling
  and the default Build pin. The same public boundaries now resolve and recover
  the actual selected Build while retaining the exact shared profile.
- The stand publication test initially observed three Tester publications rather
  than one; installation lacked the two shared modes. Repeated publication now
  keeps one current default, with independent source and private-reader compilers.
- Console tests initially displayed the common profile ceiling as required access,
  offered the author Build for CI analysis, and omitted the selected mode summary.
  They now require explicit per-mode access review, select the exact analysis
  Build and display the actual step request without implicit draft changes.

Commands used (Rust runs use a dedicated temporary target, debug info disabled,
incremental compilation disabled and two build jobs):

```sh
cargo test --locked -p octacity-server-factory
cargo test --locked -p octacity-server-application --lib
cargo test --locked -p octacity-server-application --lib factory::profiles
cargo test --locked -p octacity-server-api-rest --test v1_handlers factory_roles
python3 -m unittest discover -s tools/tests -p test_factory_delivery_setup.py
cd ui
pnpm exec vitest run src/features/factory
pnpm build
```

The full application run passed 1,065 tests; after the final diagnostics
correction, the 116-test profile suite and 17 REST role tests passed. The Factory
Console suite passed 453 tests, and the stand script suite passed 200 tests with
34 platform/environment skips. These are automated contract and fixture tests;
this correction did not rebuild or modify the operator's running stand or repeat
the native AI/PR journeys recorded above. The existing mandatory-CI and protected
inventory regressions remain in the passing application suite. Production bundle
budgets, all-target/all-feature Clippy, architecture and strict OpenSpec checks
are also required before completion. Documentation has no executable behavior;
its validation uses OpenSpec and whitespace checks.

The mode-switch regression first accepted a preparation/analysis request with
the other mode's credentials and tools still attached. The core publication
boundary now rejects it and accepts the selected mode's own request. Console's
draft-validation regression first omitted that blocker; it now reports
`execution_mode_permissions` at the step permissions without changing the draft.
The focused Red/Green commands were:

```sh
cargo test --locked -p octacity-server-factory --test role_profiles switching_a_reusable_profile_mode_rejects_access_left_from_the_other_mode
cd ui
pnpm exec vitest run src/features/factory/validateDraft.test.ts -t 'rejects leftover access'
```

After Green, the complete Factory core suite passed 407 tests, the application
profile suite passed 116 tests, the application validation suite passed 37 tests,
and the Factory Console suite passed 455 tests. Coverage includes mandatory CI
before analysis, independent protected evidence and rejection of AI attempts to
override failed checks. Core all-target/all-feature Clippy, Console lint and
production bundle budgets, architecture, formatting and strict change validation
passed. No native stand rerun was performed for this publication-boundary fix.
