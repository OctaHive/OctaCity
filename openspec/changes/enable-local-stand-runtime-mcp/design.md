## Context

See `proposal.md` for motivation. The completed `configure-factory-runtimes` change already owns reviewed settings, PostgreSQL transport retention, profile capability projection, exact MCP lookup and HTTP/stdio forms. This follow-up consumes those contracts; it does not recreate them. That change's deltas must be synchronized before archiving this dependent change if they have not yet reached the main specs.

Observed local setup is narrower than the authoring API: `tools/factory_local_runtime.py` generates a role/profile capability from `deployment/local-stand/factory-runtime.json`, whose permissions are deny-all; `tools/factory_local_stand.py` publishes a catalog with an empty `codex` executor list. The plugin advertises both transports but no MCP executable is qualified. Existing optional setup already accepts an operator-owned Factory catalog and a separate verified `--factory-agent-profile`; the native compiler and Agent configuration already understand their execution and permission contracts.

`RuntimeSettings` offers configured choices; `ProfileEditor` still contains plugin-owned model discovery, manual identifiers and `FactoryValueInput` reasoning parameters. The practical UI gap is the missing handoff and misleading installation guidance, not missing model-setting storage.

The 2026-10-28 decision keeps server-owned Source polling/import and Reporter
delivery separate from model-driven investigation on the Agent. This change owns
usable AI-plugin MCP execution; `add-factory-triage-pipeline` consumes it and owns
the OctaCity MCP Issue search/read server. Qualified finite peers make execution
setup independently testable, without a dependency cycle on triage.

## Goals / Non-Goals

**Goals:** Make an explicitly configured local installation usable end to end for both transports; preserve independent authority and current operator state; clarify Runtime versus profile ownership using the existing editors.

**Non-Goals:** Browser installation of software or worker grants, automatic unrestricted network access, copying workstation MCP/configuration, hardcoded models or reasoning levels, a new connection/permission registry, paid-model qualification, an Octa release or a general Console redesign.
No provider-specific wrapper for each external MCP, common ticket-search API,
additional MCP client where the AI executor already supplies one, or replacement
of server Source/Reporter connectors is introduced. OctaCity MCP service/token
verification belongs to triage, not this installation change. Additional AI
executors require qualified plugin support; this change does not imply delivery
of a Claude plugin simply because Codex is supported.

## Decisions

### 1. Reuse explicit installation inputs and separate authority owners

Extend the Factory stand's existing setup composition to accept the existing operator-owned Factory installation catalog and independently verified Agent policy, using the same underlying readers/configurators as `tools/local-stand`. These inputs declare qualified executables, exact transport support, finite permissions and ordinary executor bindings. Reuse shared contract validation and signed-job permission mapping; do not create another generic Runtime policy schema or infer worker grants from a saved HTTP body.

Keep the metadata-only generated installation as the compatible no-setup path. For an executable installation, preflight agreement between Runtime toolchain/build bindings, qualified MCP programs, worker inventory and backend enforcement before activation. Public HTTP MCP need not acquire an unrelated Integration Connection; authenticated/resource-bound uses retain the existing managed integration/credential owner.

The explicit `factory-stand` path forwards the operator's complete Factory catalog,
optional private Runtime bundle and independent Agent policy to the ordinary
`local-stand` installer. It does not merge demo declarations into that catalog or
seed demonstration Runs. The no-selection path retains the existing generated
metadata/demo behavior. Validate selected inputs before activating services;
task 1.3 adds explicit installation replacement, while profile adoption remains
an operator-owned publication of a new immutable version.

Rejected alternatives: globally enabling permissions would erase the independent ceiling; widening only the server capability would leave a misleading form that still cannot execute; creating another REST installation registry would duplicate deployment ownership.

### 2. Use one ordinary materialization path for both transports

Install the selected ordinary executor through the existing file installer/`FactoryRuntimeResources` composition. Configured MCP content remains in PostgreSQL and profiles select its exact immutable reference. Stdio executable paths must refer to actual verified guest programs supplied through the existing tool-executable inventory/materializer, not arbitrary host paths. Retained scripts/supporting files and explicit argv/environment use the current protected-input mechanisms. HTTP uses the existing structured transport, exact allowed hosts and independently scoped credentials. Project settings never stand in for Agent/network/backend policy.

Retain profile validation, current integration revocation, permission intersection and frozen-package recovery. No second launcher, provider-specific Factory branch or external model call is needed.

Task 1.2 adds explicit `standalone = true` selection to the existing Agent tool
inventory for independently pinned native programs; omitted settings keep the
plugin compatibility contract. It supplies no workload grant, and cannot bypass
the release-owned Codex executable contract. Role-selected compilation supplies
an explicit MCP map even when empty so the native client cannot adopt ambient
servers. See [MCP installation and process-boundary verification](mcp-execution-verification.md)
for Red/Green commands, supported plugin/authentication scope and the packaged
qualification still required by later tasks.

Each selected AI plugin translates only the exact approved MCP settings into its
executor's native client configuration. Native MCP tools keep their own argument
and result schemas; new compatible servers are configuration choices. Do not
inherit workstation configuration, expose unselected servers or silently switch
clients/transports when support is absent. Record actual qualified transport,
authentication and capture capabilities per plugin rather than claiming universal
support from a transport form or model catalog.

### 2a. Keep connection authority and evidence explicit

Each authenticated MCP connection selects its own logical secret reference,
credential purpose and resource scope through existing credential owners. The
AI-provider login and server Source/Reporter credentials are separate authority;
none is an implicit fallback for an MCP connection. Resolve authorized values only
for the selected execution and deliver them to the intended client/server process
or approved HTTP transport. Keep values out of published definitions, prompts,
Context, generic logs and retained public evidence. Redact configured secrets in
native-client diagnostics and prevent unrelated stdio children from inheriting
one another's environment credentials.

Read-only and resource limits require actual server/tool and credential enforcement,
not an instruction, annotation or model-supplied repository filter. Keep current
Project/connection checks and independent Agent/backend ceilings. Local revocation
blocks newly unauthorized use without a fallback binding; remote revocation follows
the external server's contract, without a cross-provider atomicity claim.

Use existing invocation deadlines, bounded result/budget limits and cancellation
controls. The selected plugin must expose actual MCP-call/result evidence through
ordinary execution events/results/Artifacts, bound to invocation, connection/tool,
arguments, observed time, content digest and available scope/coverage. Missing
coverage remains unknown. Native provider revisions are retained when available;
content observations do not invent immutable remote versions. Model descriptions
of tool calls cannot replace observed evidence. Missing required auth/enforcement
or capture support is an actionable blocker, not another tool journal or a reason
to write a provider-specific response parser.

### 3. Upgrade installation bundles without resetting the stand

Reuse the current private bundle staging/validation and lifecycle ownership helpers. Preflight new inputs before stopping/replacing the running installation; preserve the existing PostgreSQL/object-store volumes, worker registration and evidence receipts. Keep operator Runtime settings outside generated catalog files. Do not automatically migrate profile pins or replay demo admission after an installation update. A changed installation is an explicit deployment action, distinct from a Runtime settings save.

Task 1.3 implements this through the explicit `--update-factory-installation`
path and an atomic exchange of generated server/Agent configuration under the
ordinary lifecycle lock. Native Agent and packaged server validation precede
shutdown; failed startup restores the previous files after candidate shutdown.
The server validator reuses the startup Factory loader, including discovery
executable identity checks without discovery invocation. Existing optional
bundles and integration authority are retained. See
[TDD and packaged verification](installation-update-verification.md).

### 4. Make the profile handoff visible rather than copying its fields

Add concise Runtime-level ownership guidance and a Project-scoped action to the existing AI-agent/profile view. Reuse its model suggestions, refresh/manual entry and plugin parameter controls, including `reasoning_effort` when declared by the plugin. Preserve the existing mounted profile draft on the round trip. Distinguish installation instructions from Project configuration instructions and configuration success from actual MCP execution; reuse existing localized form/navigation components.

### 5. Qualify the actual stand composition with a small shared journey

Use existing installer/launcher public contracts for TDD, plus the existing packaged finite MCP journey and two-locale browser loop. Exercise both HTTP and stdio with declared resources and mapped public environment, verify rejection outside the finite policy and one restart that preserves settings/history. The acceptance environment must be produced through the stand setup under test; manually granting extra fixture authority outside that setup would hide the original defect. Reuse current shared authorization/replay/transport tests instead of multiplying their matrix.

Exercise the real selected AI-plugin/native-client chain with separate finite MCP
peers and credential canaries: verify each intended destination, denied writes and
foreign resources, missing/expired/revoked credentials, cancellation, captured
results and absence of secret values/unselected connections. No paid model call
or live repository write is required. A finite peer proves the generic boundary;
deployment guidance names the ready-made server/plugin versions actually qualified.

Handoff to triage 4.3–4.4 includes installation inputs, supported plugin transports
and auth methods, per-connection secret/scope bindings, actual read-only enforcement,
capture/retention contract and execution evidence from 1.2, 1.4, 1.5 and 3.1. Triage
owns duplicate interpretation and accepted business effects. Server Source/Reporter
installation remains with its existing owner; connecting an MCP server requires no
unrelated Source installation. Do not mark another change complete from this handoff.

Qualification exposed a distinction between unavailable terminal usage and a
failed shared owner pass. The operator approved correcting this in task 3.1.
Once an ordinary execution has stopped, unknown consumption keeps the exact
execution and its reservation in the existing waiting state. It must neither
become zero usage nor prevent reconciliation/cancellation of other Runs. No new
accounting journal or result is introduced; later trustworthy usage follows the
existing settlement path. Actual process stop and verified partial MCP capture
remain independently inspectable while settlement waits.

Cancellation reuses the existing admission validator for the retained static or
selected-role intent. Reconstructing a static intent would discard the selected
profile and reject an otherwise valid stop. A peer Run failure also cannot skip
other already claimed Runs: the bounded reconciler finishes that queue and
reports the first error through its existing retry owner.

## Risks / Trade-offs

- **Operator setup remains necessary** → Document one concrete invocation using existing installation inputs and expose that prerequisite in Console; do not label a metadata-only Runtime executable.
- **Host paths differ from guest paths** → Verify bytes and guest materialization through the same installed inventory used by signed Jobs.
- **Configuration and worker policy diverge** → Preflight installation bindings and retain dispatch-time independent intersections; unsupported categories remain unavailable.
- **Updating generated files discards user data** → Stage only installation bundles and preserve all durable settings, history, credentials and receipts under their existing owners.
- **Qualification consumes disk** → Reuse one Cargo target, pinned assets and existing finite fixtures; check free space before builds and remove only task-owned disposable services/files afterward.

## Migration Plan

Existing stands continue to start in their current metadata-only mode until the operator explicitly supplies execution setup. A valid setup upgrades the installation without a reset, after which Console can publish permitted MCP choices and the operator can adopt them into new profile versions. Restart preserves the installation and reviewed state. Failed preflight retains the current installation. Rollback uses the existing explicit installation path and keeps retained content; it must not delete settings or rewrite admitted Runs.

## Task 1.1 implementation evidence

`factory-stand` now forwards an explicit complete catalog, optional Runtime bundle
and independent Agent policy to the existing ordinary installer, keeping its
preflight and lifecycle owners. See the [operator setup instructions](../../../docs/operations/local-stand.md#select-an-operator-owned-factory-installation).
No-selection demo startup remains on the original path. Subsequent work records
[native MCP materialization](mcp-execution-verification.md) and
[non-destructive installation replacement](installation-update-verification.md);
full MCP credential/capture execution remains tasks 1.4–1.5/3.1.

Public process-level CLI tests in `tools.tests.test_factory_local_stand` observed
these Red/Green cycles, each run with `python3 -m unittest` and its exact test name:

- `FactoryLocalStandTests.test_explicit_execution_setup_rejects_invalid_agent_policy_before_mutating_stand`:
  Red rejected the unknown installation flags; Green reaches the ordinary policy
  validator and preserves the existing stand without exposing its credential canary.
- `FactoryLocalStandTests.test_execution_bundle_or_agent_policy_requires_an_explicit_catalog`:
  Red entered demo startup; Green rejects each incomplete selection at the CLI.
- `FactoryLocalStandTests.test_explicit_installation_does_not_silently_ignore_demo_source_options`:
  Red ignored source options in explicit mode; Green rejects the incompatible options
  before creating a stand root.

Baseline: 83 tests, 76 passed and 7 skipped. Regression: 86 tests, 79 passed and
the same 7 platform/opt-in skips, using `python3 -m unittest` with
`tools.tests.test_factory_local_stand tools.tests.test_factory_local_runtime
tools.tests.test_local_stand_launcher tools.tests.test_configure_local_stand_server
tools.tests.test_configure_local_stand_agent`. This combines the new actual CLI
refusal cases with existing installer/catalog/bundle/policy and default/manual-path
contracts; it does not claim a newly executed packaged stand or native MCP call.
Free space stayed at 18 GiB; this slice required no Cargo/image build or cleanup.


### Task 1.4 dependency qualification update

The operator approved an Octa compatibility fix after native Codex 0.161.0
rejected the pinned plugin's root approval flag after `exec`. Real execution also
exposed the native final-message event contract; the same plugin result owner
now consumes the last bounded sanitized completed assistant message through its
existing schema/result validation. This is a necessary upstream compatibility
repair, not a second MCP client or tool journal. The exact Red/Green evidence,
qualified credential methods and published immutable Octa 0.7.3 dependency pin
are recorded in [MCP execution verification](mcp-execution-verification.md#task-14-independent-mcp-credentials).

### Task 1.5 native capture dependency

A real cancellation test showed that published Octa 0.7.3 removes the trace
after an observed MCP call. The operator approved upstream TDD work to retain
native observations through the existing run-record/Artifact owners. The
candidate uses trace/provenance format 2 and advertises a native-capture
capability; OctaCity verifies it without creating a second journal, provider
search parser or semantic result from partial capture. The exact qualification,
common handoff to triage 4.4 and verified published Octa 0.8.0 pin are recorded in
[native MCP evidence verification](mcp-evidence-verification.md). Task 1.5 is
complete at the native/plugin/Artifact boundary; the full packaged stand journey
remains task 3.1.
