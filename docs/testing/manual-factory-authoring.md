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
