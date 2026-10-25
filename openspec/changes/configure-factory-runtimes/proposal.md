## Why

AI-agent profiles can select installed MCP, integration and permission bindings, but Console cannot configure those Runtime choices. The local stand publishes model metadata with empty optional choices, so the profile editor shows unavailable functionality without a usable setup path.

## What Changes

- Add a separate **AI settings** Console section with **AI agents** and **Runtimes** views. Reuse the existing Project-owned agent editor; keep profiles inside each agent and CI/CD Agent/Pool management separate. Runtime settings bind existing installed aliases.
- Configure named MCP transports, select existing Project-authorized integration resources, and manage finite Runtime permission choices through reviewed REST operations.
- Retain immutable settings and MCP transport versions with optimistic concurrency, scoped replay and the existing audit/outbox transaction mechanisms.
- Feed the same settings into profile discovery, validation and existing compilation/materialization; profiles select exact bindings rather than copying transport definitions.
- Preserve installed implementation restrictions and independent Agent/backend authority; report unsupported settings explicitly.
- Replace empty selection frames with localized explanations and a setup link while preserving open drafts.

## Capabilities

### New Capabilities

None. Runtime settings extend the existing role/profile capability boundary rather than introducing another Runtime or integration registry.

### Modified Capabilities

- `server/factory-role-profiles`: reviewed Runtime configuration, durable exact bindings, shared profile qualification and compilation.
- `ui/operator-console`: an AI settings section reusing agent/profile editors, Runtime settings and actionable profile selection states.

## Impact

- Existing Factory capability source, role/profile application services, versioned PostgreSQL persistence, REST/OpenAPI and generated Console client.
- Existing MCP transport compiler, managed Integration Connection/Project access and physical execution permission boundaries.
- Local-stand Runtime installation and focused operator documentation.
- No model list, provider credentials, new scheduler, new AI-agent entity or required Octa release. Existing manual Factory intake, static profiles and retained Runs remain supported.
