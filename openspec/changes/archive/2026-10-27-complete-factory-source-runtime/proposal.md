## Why

Source discovery, canonical Issue import, field/status synchronization and Factory intake already have application contracts and tests, but normal server startup does not compose their complete installed runtime. Operators need a supported path from an external task to a visible internal Issue and authorized outbound status updates, with optional Factory execution rather than a mandatory Run.

## What Changes

- Complete normal deployment composition for existing Source management, bounded observations, canonical Issue import/refresh, compatible field/status choices and Source/Reporter diagnostics. Reuse the finished Issue domain, REST conventions and workflow editor rather than implementing another task or workflow system.
- Make explicitly enabled Project Source import/refresh usable without a Factory Configuration, code Repository, execution profile or write credential. Repeated observations resolve one canonical Issue in the Project/security/native namespace; import alone creates no Work/Run. Add only missing protected management/Console controls needed to reach these existing consumers.
- Separately complete explicitly enabled Factory input/group polling, bounded preparation and ordinary admission through the existing supervised worker. Correlate accepted Work/Run with canonical Issues through the existing association/recovery boundary, retaining independent identities and immutable input snapshots. Keep execution-only/no-card intake supported.
- Compose optional Reporter delivery for both existing accepted Issue-status projections and configured Work-backed source-update actions. Reuse exact versioned mappings, current grants, producer-conflict checks and the durable ordered outbox; neither status spelling, workflow publication nor Run completion implies an external write.
- Complete the deployed Console journey: Project task import and synchronization, optional pipeline inputs, actionable localized failures and separate Issue/Run/reporting progress. Use the existing canvas to configure business transitions, with separate publication and default selection; native state/label mapping remains its own reviewed operation.
- Qualify these paths through the packaged browser/gateway/server, PostgreSQL, object storage and verified adapter process against a hermetic provider fixture. Prove import without execution, authorized status push, optional execution, restart/replay, revocation and provider-independent manual operation. Preserve installed connections, immutable profiles, credentials and retained history.

## Capabilities

### New Capabilities

None; finish released composition and qualification of existing task/source capabilities.

### Modified Capabilities

- `server/rest-control-plane`: operational Source/input APIs plus reachable Project-scoped import controls and existing Issue mapping/synchronization operations, without a fabricated Issue or execution target.
- `server/github-work-source`: shared installed observations consumed independently by Issue import/refresh and optional Factory admission; exact authorized Issue/Work reporting and recovery.
- `ui/operator-console`: external-task import, separate workflow/native mapping review, optional automatic execution and truthful synchronization feedback against the deployed runtime.
- `deployment/local-compose-stack`: compatible Source/Reporter installation and scoped readiness, with packaged external-task-to-Issue/status qualification.

## Impact

Normal server composition, deployment-owned Source/Reporter profile loading, existing Issue Source consumers, supervised polling/outbox installation, protected management routes, Console task/source settings, Factory stand setup/readiness and packaged qualification. Extend safe typed contracts only for missing operational controls; retain canonical identities, API compatibility and ordinary Issue/workflow semantics.

The archived `add-factory-work-card`, `configure-factory-runtimes` and `add-visual-issue-workflow-editor` changes are implemented prerequisites, not future owners that exclude task import or status delivery from this change. Their domain rules, Runtime/Profile installation and canvas remain authoritative; completed artifacts/tasks are not reopened. `server/factory-work-collaboration` is consumed unchanged. `simplify-integration-setup` still owns new enrollment and one-form connection/archive/delete UX; this change must work with current installed connections. `enable-local-stand-runtime-mcp` remains a separate execution-installation enhancement and is not required for Source-only import/status push. No new GitLab/Jira adapter, workflow engine, automatic migration, implicit field ownership or live-provider mutation is included. This update revises planning only.
