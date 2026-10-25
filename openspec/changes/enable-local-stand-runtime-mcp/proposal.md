## Why

The local Factory stand exposes Codex model discovery and Runtime authoring, but installs a deny-all capability ceiling with no MCP executables or executable Factory profile. Operators can see an MCP form without a usable installation behind it, and the separate model/reasoning editor is hard to find.

## What Changes

- Add an explicit supported setup path for the local stand to use operator-approved Runtime capabilities and independently approved Agent policy, reusing the existing Factory catalog and Agent installation mechanisms.
- Support both HTTP MCP endpoints and verified installed stdio programs through the existing Runtime settings, exact bindings and ordinary compiler/materializer. Make the selected permissions usable without widening the installation or backend ceilings from browser requests.
- Preserve operator settings, profiles and history when applying an installation update and on ordinary restart; retain the current metadata-only behavior when execution setup is absent.
- Explain installation limits and missing prerequisites in Runtime settings, distinguish configuration validation from an MCP execution result, and provide a direct Project-scoped path to profile model/reasoning selection without losing open profile drafts.
- Qualify the local-stand path with finite HTTP/stdio MCP processes and the existing tests, without a paid model dependency or another test matrix.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `deployment/local-compose-stack`: explicit local Runtime/Agent execution setup, MCP availability and state-preserving installation updates.
- `ui/operator-console`: actionable Runtime setup states and a clear handoff to model/reasoning selection in the existing AI-agent profile editor.

## Impact

- `tools/factory_local_stand.py`, `factory_local_runtime.py` and existing server/Agent configuration and private-bundle helpers.
- Existing installed Factory catalog/executor bindings, qualified executable inventory, Runtime REST/settings persistence and Agent/Microsandbox permission intersections.
- Console Runtime/profile navigation, localized guidance, existing component/browser fixtures and operator docs.
- Builds on the completed `configure-factory-runtimes` change. No new Runtime/credential registry, browser software installer, hardcoded model list, automatic task/profile creation or required Octa release. Manual Factory intake remains independent.
