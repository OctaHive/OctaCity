## Context

See `proposal.md` for motivation and scope. Current authoring uses `FactoryRoleProfileCapability` and `FactoryConfigurationChoices`; the file installer supplies per-Project declarations and `ProjectCatalog` caches plugin model discovery. Capability REST operations read/refresh inventory, but there is no Runtime settings mutation or persistent Runtime configuration owner. The local-stand installer publishes Codex model discovery and a deny-all authoring permission declaration.

`FactoryRoleProfileExecution`, `FactoryRoleMcpBinding`, `FactoryRoleIntegrationBinding` and `FactoryPermissionSet` already describe profile selections. The application has structured HTTP/stdio MCP configuration and exact pinning in `factory_codex_mcp.rs`; ordinary compilation currently resolves only installation-owned `FactoryCodexBuildProfile.mcp_configurations`. Existing managed connections own credentials, resources and Project grants. Console agent/profile editors currently live under Factory.

## Goals / Non-Goals

**Goals:** Keep one source of configured choices and one compiler path; separate installation support, administrator-approved Project configuration and profile selection. Make setup reachable in a separate AI settings area while preserving draft state.

**Non-Goals:** Installing arbitrary binaries from the browser, inheriting workstation MCP settings, creating a model/credential/connection registry, changing role ownership, redesigning the full workbench, changing CLI argument support or issuing an Octa release. No background MCP connection checks or LLM calls on save/read.

## Decisions

### 1. Reuse existing AI-agent views in one AI settings section

Add **AI settings / Настройки AI** to navigation with **AI agents / AI-агенты** and **Runtimes / Runtime** views. Keep Project context explicit, Role/Profile identities unchanged and profiles inside their agent card. Reuse existing components and route Factory shortcuts to them. Keeping two independent copies under Factory and Settings would create divergent drafts and forms. CI/CD enrollment and pools keep their current owner and navigation.

### 2. Configure an installed Runtime, not a second Runtime entity

Use `(Project, installed Runtime alias)` to identify settings and bind each revision to its exact installed toolchain. Keep the capability source as installation inventory. Add a bounded, append-only Project settings record through operation-shaped store ports and the shared management transaction; do not write deployment files from REST or use a Role/Profile as a hidden Runtime configuration.

Settings reuse ordinary execution bindings and permission values. Store only configured additions and exact retained transport content, not a copied model catalog, full Role/Profile definition or cached discovery result. An absent settings revision preserves installation behavior. Changed installation pins require explicit revalidation rather than silently adopting a different executable.

Installation support and physical execution grants remain independently owned. An administrator may configure Project choices only within the installed supported contract; implementation MUST NOT replace local grants with requested values or mark all permission categories enforceable to make validation pass. If the installation lacks a supported permission or transport, REST and Console explain that limit. Profile validation and actual dispatch continue to apply their existing intersections.

### 3. Reuse structured MCP contracts and managed integration grants

Expose supported transport fields through typed REST schemas: HTTP endpoint and optional logical bearer environment name, or an installed absolute executable, explicit argv and mapped environment names for stdio, plus the existing optional finite tool selection. Reuse exact hashing, validation and compiler translation from the existing MCP mechanism. Extract shared transport value contracts only where required for provider-neutral use; retain provider-specific translation at the adapter boundary rather than duplicating Codex logic in a generic service.

MCP entries may reference an existing selected integration alias. Populate integration/resource/capability choices through the existing authorization-filtered connection and Project-access services. No raw credential fields, hand-entered immutable digests or automatic creation of Project grants. Credentials and secret environment mappings stay under their existing owners.

### 4. Publish and consume one reviewed configuration

Reuse Project administration authorization, bounded content reads, current-version preconditions, scoped idempotency, audit/outbox transactions and safe error classifications. PostgreSQL retains immutable settings and transport versions; the memory adapter follows the same behavioral contract. Replay compares the original client intent before resolving mutable installation or integration state so later updates do not change an accepted receipt. Avoid new generalized persistence or event frameworks.

Apply settings once through the common capability boundary used by profile lists, details, validation and admission. Separate configuration metadata from currently usable choices: revoked integrations remain visible with a safe explanation to authorized administrators, but are absent from selectable profile choices. Configuration never calls a remote provider or starts an MCP process.

### 5. Resolve exact transport pins through the existing compiler

Profiles continue to contain exact `FactoryRoleMcpBinding` and integration references. Extend the existing retained lookup to resolve configured MCP transport versions alongside installed transports; use that lookup consistently for initial materialization and retained replay. Verify content and current integration/permission authority through existing checks. Runtime edits never rewrite published profiles, a frozen package or model selection. A removed/revoked option cannot silently select its replacement.

This closes the current gap where a binding could appear selectable while the ordinary compiler knows only installation-file transports. It does not create another execution route or weaken Agent/Microsandbox isolation.

### 6. Keep the editor guided and bounded

Runtime settings use named typed rows and existing resource/permission editors, server-published choices and limits. Explain supported versus unavailable categories, integration requirements and the distinction between configuration and execution authority. Profile sections use selections only, replace empty frames with explanations and offer a setup action. Keep the profile component/draft mounted while opening Runtime setup; refresh choices on return without applying new defaults automatically. Reuse the confirmed-command helper and targeted query invalidation; no drafts, transport documents or secret locators in URLs/browser persistence.

## Risks / Trade-offs

- **Authoring-only success** → Acceptance includes configuring a transport, selecting it in a profile and observing its exact delivery through the existing finite compiler/process fixture, not only a saved form.
- **Permission broadening or hidden integration leakage** → Preserve independent installation/Agent/backend bounds and reuse protected-use filtering before profile choices and dispatch.
- **Mutable settings break old Runs or retries** → Immutable transport content, exact profile pins and original-intent mutation replay; current revocation checks still apply.
- **Local stand overwrites operator settings** → Keep persistent Project settings outside generated installation inventory; restart/installer refresh preserves them and never imports workstation MCP configuration or enables sample grants implicitly.
- **Growing setup surface** → Reuse existing agent, integration, permission and publication components. No parallel model lists, connection forms or one-off test frameworks.

## Migration Plan

Add the settings persistence/API without backfilling or changing existing profiles. Read the installation declaration whenever no reviewed Project settings exist. Enable the AI settings navigation using the existing agent views and retain Factory shortcuts. Update local-stand installation metadata only to expose qualified supported contracts; administrators configure real choices explicitly. Existing static profiles, model discovery, ordinary CI/CD and manual Factory intake remain usable. Disable new settings publication/navigation for rollback while retaining immutable content required by admitted invocations; never delete transport history to roll back the UI.
