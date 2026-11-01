# Clean-state manual Factory authoring

`factory_manual_authoring` qualifies tasks 5.1 and 5.2 of
`enable-manual-factory-pipeline-authoring`. It starts the actual Server and Agent
executables against PostgreSQL and S3 and executes two ordinary Jobs in separate
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
Console case is described below. Full lifecycle/negative cases and other backends
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
