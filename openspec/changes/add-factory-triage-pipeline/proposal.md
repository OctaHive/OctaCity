## Why

OctaCity can import GitHub Issues, execute generic Factory graphs and report configured native effects, but operators cannot yet configure and verify a complete two-agent triage journey on the normal stand. The supplied eligibility/classification contracts need executable profiles, trustworthy context, raw-ticket intake and a reviewed reporting policy to become a usable pipeline.

## What Changes

- Deliver a versioned `triage` preset containing two isolated reasoning stages: **Project** assesses Project-goal fit and duplicate candidates; **Systems analyst** assesses priority, severity, size, component, risk, dependencies and preliminary defect reproducibility. Deterministic gates accept only declared outcomes and end with a retained triage result, rejection or human review.
- Reuse existing Roles/Profiles, Flow primitives, ordinary Build/Agent execution and explicit Context/Data projections. Provide bounded reviewed Project-goal, ticket-history and repository context.
- Expose authorized read-only Issue search and exact Issue reads through an OctaCity MCP server. The model conducts duplicate investigation through that server and configured ready-made external MCP servers, such as GitHub; GitLab/Jira remain configuration extensions when compatible servers are qualified. Do not add a duplicate-search REST service, a server-side duplicate planner or provider-specific wrappers around each external MCP.
- Consume agent-side MCP setup from `enable-local-stand-runtime-mcp`: each qualified AI plugin materializes selected connections for its AI executor's native MCP client. Keep each connection's token/credential-purpose binding and access scope separate from AI-provider credentials and server-side Source/Reporter authority. Enforce read-only investigation through actual credential/server/tool restrictions and retain consumed tool evidence.
- Add an explicit, compatible strictly-unlabeled predicate. Admit raw tickets without requiring ticket-authored implementation acceptance: bind a reviewed immutable triage completion document through the shared input service, with distinct provenance and no invented Issue requirements.
- Configure independent Source intake, optional canonical Issue import/status actions and Reporter effects. Review finite existing GitHub labels and open/closed mappings, preserve unrelated fields and recover delivery through the existing ordered outbox.
- Provide headless preset resolution/readiness and a guided Console setup using existing publication and confirmed-command boundaries. Share the generic preset/readiness seams planned by bugfix quickstart rather than create a second resolver or lifecycle.
- Qualify the actual packaged setup, two reasoning invocations, context isolation, reproduction evidence, reporting and restart/revocation behavior. Reuse finite hermetic providers and a qualified Agent; keep paid-model and live-repository writes separately opt-in.

## Capabilities

### New Capabilities

- `server/factory-triage-preset`: Supported two-agent preset, authorized OctaCity Issue MCP reads, agent-side extensible MCP investigation, context/result contracts, truthful raw-ticket intake, readiness and retained triage decisions.

### Modified Capabilities

- `server/github-work-source`: Explicit unlabeled selection and purpose-bound triage input preparation without weakening existing required-acceptance mappings.
- `server/rest-control-plane`: Shared preset resolution/readiness and typed triage setup/results over ordinary publication and execution contracts.
- `ui/operator-console`: Guided two-stage triage setup, independent activation and readable decision/reporting diagnostics.
- `deployment/local-compose-stack`: Reproducible executable triage qualification, including selected agent-side MCP connections, isolated credentials and state-preserving installation prerequisites.

## Impact

`config/factory/`, existing Source mapping/predicate/preparation, Project inputs, the OctaCity MCP boundary over authorized Issue reads, Flow template/authoring, AI-plugin execution-setting contracts and installed runtime boundaries, REST/OpenAPI, generated UI, Console and packaged qualification. Server-owned Source intake and Reporter delivery keep their existing connectors and recovery contracts. No phase-specific engine, journal, provider SDK, common external-MCP search facade or hardcoded model is introduced.

Dependencies: archived `complete-factory-source-runtime` supplies the completed server-side Source/Reporter baseline. `enable-local-stand-runtime-mcp` owns executable Runtime/Agent setup, AI-plugin/native-client HTTP/stdio MCP materialization and safe installation updates; triage consumes its exact qualified connection, secret and permission contracts rather than implementing a second launcher or MCP client. OctaCity MCP is delivered by triage task 4.2 over existing Issue authorization/storage, with scoped token verification required before readiness. Required per-connection credentials, read-only enforcement and result capture must be explicitly qualified at the upstream boundary; configuration forms alone are insufficient. Coordinate server integration enrollment and label-scope maintenance with `simplify-integration-setup`; an agent MCP connection does not require an unrelated Source/Reporter connection. `add-factory-bugfix-quickstart` is a shared-contract consumer, not a prerequisite; full Repository Knowledge, JEV, implementation, PR delivery and deployment are outside this change. All executable work follows repository TDD; this proposal authorizes no implementation or commit.
