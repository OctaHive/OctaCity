## Why

The local Factory stand exposes Codex model discovery and Runtime authoring, but installs a deny-all capability ceiling with no MCP executables or executable Factory profile. Operators can see an MCP form without a usable installation behind it, and the separate model/reasoning editor is hard to find.

## What Changes

- Add an explicit supported setup path for the local stand to use operator-approved Runtime capabilities and independently approved Agent policy, reusing the existing Factory catalog and Agent installation mechanisms.
- Support both HTTP MCP endpoints and verified installed stdio programs through existing Runtime/Profile settings and the ordinary compiler/materializer. The selected AI plugin supplies the reviewed connections to its Agent-side executor's native MCP client in that executor's supported format. Ready-made compatible servers keep their native tools and schemas; connecting GitHub, GitLab or Jira requires configuration and qualification, not a provider-specific OctaCity wrapper or another MCP client.
- Give each authenticated MCP connection its own authorized secret reference, credential purpose and resource/access scope, separate from AI-provider authentication and server Source/Reporter authority. Enforce read-only investigation through actual server/tool and credential restrictions, not prompts alone. Qualify secret isolation, revocation, bounded cancellation and retention of actual tool-call evidence.
- Preserve operator settings, profiles and history when applying an installation update and on ordinary restart; retain the current metadata-only behavior when execution setup is absent.
- Explain installation limits and missing prerequisites in Runtime settings, distinguish configuration validation from an MCP execution result, and provide a direct Project-scoped path to profile model/reasoning selection without losing open profile drafts.
- Qualify the actual AI-plugin/native-client HTTP/stdio path with finite MCP peers and existing tests, without a paid model dependency, live repository writes or another test matrix. Report unsupported executor/authentication/capture capabilities explicitly instead of borrowing ambient settings or credentials.
- Preserve the existing reservation while stopped execution usage is unknown, using the existing waiting states so one unsettled Run cannot block other Runs from reconciling or stopping. This repair was approved after packaged qualification exposed the shared-owner failure.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `deployment/local-compose-stack`: explicit Runtime/Agent setup, AI-plugin/native-client MCP execution, isolated connection credentials and evidence, and state-preserving installation updates.
- `ui/operator-console`: actionable Runtime setup states and a clear handoff to model/reasoning selection in the existing AI-agent profile editor.

## Impact

- `tools/factory_local_stand.py`, `factory_local_runtime.py` and existing server/Agent configuration and private-bundle helpers.
- Existing installed Factory catalog/executor bindings, qualified executable inventory, Runtime REST/settings persistence and Agent/Microsandbox permission intersections.
- Existing ordinary Build node observation/cancellation and Factory reconciliation; no new accounting state or journal.
- Console Runtime/profile navigation, localized guidance, existing component/browser fixtures and operator docs.
- Builds on completed `configure-factory-runtimes` and existing execution, Project authorization and secret owners. `add-factory-triage-pipeline` consumes this change in 4.3–4.4; its 4.2 owns the OctaCity MCP search/read server and scoped server-side Issue authority. That server is not a prerequisite for this change's finite-peer qualification. Existing server Source connectors and Reporter/outbox remain responsible for automatic ticket intake and accepted updates. No new Runtime/credential registry, provider SDK, browser software installer, hardcoded model list, or automatic task/profile creation. Native qualification exposed an Octa 0.7.2 CLI/event incompatibility; the operator approved the minimal upstream repair and an immutable dependency update. Task 1.4 qualified Octa 0.7.3; task 1.5 superseded it with the published Octa 0.8.0 pin for retained native capture. Manual Factory intake remains independent; support for one AI plugin does not certify all others.
