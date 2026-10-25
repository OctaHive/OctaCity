## Context

See `proposal.md` for motivation. The completed `configure-factory-runtimes` change already owns reviewed settings, PostgreSQL transport retention, profile capability projection, exact MCP lookup and HTTP/stdio forms. This follow-up consumes those contracts; it does not recreate them. That change's deltas must be synchronized before archiving this dependent change if they have not yet reached the main specs.

Observed local setup is narrower than the authoring API: `tools/factory_local_runtime.py` generates a role/profile capability from `deployment/local-stand/factory-runtime.json`, whose permissions are deny-all; `tools/factory_local_stand.py` publishes a catalog with an empty `codex` executor list. The plugin advertises both transports but no MCP executable is qualified. Existing optional setup already accepts an operator-owned Factory catalog and a separate verified `--factory-agent-profile`; the native compiler and Agent configuration already understand their execution and permission contracts.

`RuntimeSettings` offers configured choices; `ProfileEditor` still contains plugin-owned model discovery, manual identifiers and `FactoryValueInput` reasoning parameters. The practical UI gap is the missing handoff and misleading installation guidance, not missing model-setting storage.

## Goals / Non-Goals

**Goals:** Make an explicitly configured local installation usable end to end for both transports; preserve independent authority and current operator state; clarify Runtime versus profile ownership using the existing editors.

**Non-Goals:** Browser installation of software or worker grants, automatic unrestricted network access, copying workstation MCP/configuration, hardcoded models or reasoning levels, a new connection/permission registry, paid-model qualification, an Octa release or a general Console redesign.

## Decisions

### 1. Reuse explicit installation inputs and separate authority owners

Extend the Factory stand's existing setup composition to accept the existing operator-owned Factory installation catalog and independently verified Agent policy, using the same underlying readers/configurators as `tools/local-stand`. These inputs declare qualified executables, exact transport support, finite permissions and ordinary executor bindings. Reuse shared contract validation and signed-job permission mapping; do not create another generic Runtime policy schema or infer worker grants from a saved HTTP body.

Keep the metadata-only generated installation as the compatible no-setup path. For an executable installation, preflight agreement between Runtime toolchain/build bindings, qualified MCP programs, worker inventory and backend enforcement before activation. Public HTTP MCP need not acquire an unrelated Integration Connection; authenticated/resource-bound uses retain the existing managed integration/credential owner.

Rejected alternatives: globally enabling permissions would erase the independent ceiling; widening only the server capability would leave a misleading form that still cannot execute; creating another REST installation registry would duplicate deployment ownership.

### 2. Use one ordinary materialization path for both transports

Install the selected ordinary executor through the existing file installer/`FactoryRuntimeResources` composition. Configured MCP content remains in PostgreSQL and profiles select its exact immutable reference. Stdio executable paths must refer to actual verified guest programs supplied through the existing tool-executable inventory/materializer, not arbitrary host paths. Retained scripts/supporting files and explicit argv/environment use the current protected-input mechanisms. HTTP uses the existing structured transport, exact allowed hosts and independently scoped credentials. Project settings never stand in for Agent/network/backend policy.

Retain profile validation, current integration revocation, permission intersection and frozen-package recovery. No second launcher, provider-specific Factory branch or external model call is needed.

### 3. Upgrade installation bundles without resetting the stand

Reuse the current private bundle staging/validation and lifecycle ownership helpers. Preflight new inputs before stopping/replacing the running installation; preserve the existing PostgreSQL/object-store volumes, worker registration and evidence receipts. Keep operator Runtime settings outside generated catalog files. Do not automatically migrate profile pins or replay demo admission after an installation update. A changed installation is an explicit deployment action, distinct from a Runtime settings save.

### 4. Make the profile handoff visible rather than copying its fields

Add concise Runtime-level ownership guidance and a Project-scoped action to the existing AI-agent/profile view. Reuse its model suggestions, refresh/manual entry and plugin parameter controls, including `reasoning_effort` when declared by the plugin. Preserve the existing mounted profile draft on the round trip. Distinguish installation instructions from Project configuration instructions and configuration success from actual MCP execution; reuse existing localized form/navigation components.

### 5. Qualify the actual stand composition with a small shared journey

Use existing installer/launcher public contracts for TDD, plus the existing packaged finite MCP journey and two-locale browser loop. Exercise both HTTP and stdio with declared resources and mapped public environment, verify rejection outside the finite policy and one restart that preserves settings/history. The acceptance environment must be produced through the stand setup under test; manually granting extra fixture authority outside that setup would hide the original defect. Reuse current shared authorization/replay/transport tests instead of multiplying their matrix.

## Risks / Trade-offs

- **Operator setup remains necessary** → Document one concrete invocation using existing installation inputs and expose that prerequisite in Console; do not label a metadata-only Runtime executable.
- **Host paths differ from guest paths** → Verify bytes and guest materialization through the same installed inventory used by signed Jobs.
- **Configuration and worker policy diverge** → Preflight installation bindings and retain dispatch-time independent intersections; unsupported categories remain unavailable.
- **Updating generated files discards user data** → Stage only installation bundles and preserve all durable settings, history, credentials and receipts under their existing owners.
- **Qualification consumes disk** → Reuse one Cargo target, pinned assets and existing finite fixtures; check free space before builds and remove only task-owned disposable services/files afterward.

## Migration Plan

Existing stands continue to start in their current metadata-only mode until the operator explicitly supplies execution setup. A valid setup upgrades the installation without a reset, after which Console can publish permitted MCP choices and the operator can adopt them into new profile versions. Restart preserves the installation and reviewed state. Failed preflight retains the current installation. Rollback uses the existing explicit installation path and keeps retained content; it must not delete settings or rewrite admitted Runs.
