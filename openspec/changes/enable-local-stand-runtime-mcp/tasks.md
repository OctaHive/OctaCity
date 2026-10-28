## 1. Explicit local Runtime execution setup

Use repository Red/Green TDD for every executable change and existing public
boundaries. Check free disk space before/during builds; below 10 GB remove only
identified regenerable caches or task-owned disposable resources, preserving
stand data and other work. No automatic commit is authorized. Completed upstream
Runtime settings are reused; triage owns the OctaCity MCP server and consumes
this change's plugin/credential/capture qualification without a dependency cycle.

- [x] 1.1 Through TDD at the existing stand CLI/installer boundary, thread the operator-owned Factory catalog and independent Agent policy into `tools/factory-stand` using current configurators; verify explicit selection, safe preflight failure and unchanged metadata-only/manual-intake startup without introducing another installation schema. Evidence: [CLI Red/Green and regression results](design.md#task-11-implementation-evidence), [operator setup](../../../docs/operations/local-stand.md#select-an-operator-owned-factory-installation).
- [x] 1.2 Through TDD at ordinary installer/compiler/materializer and the selected AI plugin/native-client boundary, compose the qualified executor and exact HTTP/stdio MCP choices from 1.1. Verify actual guest executable availability, native client configuration, absent ambient/unselected MCP settings, network/resource bounds and rejection of mismatched, unsupported or unapproved selections. Ready-made compatible servers retain native tools without provider wrappers or another MCP client; keep models plugin-owned and worker authority independent of Runtime settings. Record which executor/transport/auth methods are actually qualified. Evidence: [installed native programs, MCP configuration and Red/Green verification](mcp-execution-verification.md); full packaged execution remains 3.1.
- [x] 1.3 Through TDD with the existing stand lifecycle fixtures, apply installation updates without destructive reset; verify failed preflight preserves the usable bundle and normal restart retains reviewed settings, profile/transport history, worker identity and demonstration receipts without replaying Work. Evidence: [installation staging, rollback, restart contracts and packaged preflight Red/Green](installation-update-verification.md), [operator update instructions](../../../docs/operations/local-stand.md#select-an-operator-owned-factory-installation).
- [x] 1.4 Through TDD at existing secret binding/materialization and actual MCP invocation boundaries, supply independent per-connection token/credential-purpose and resource/read-only scope bindings. Verify credential isolation between HTTP/stdio peers, AI-provider login and server Source/Reporter; reject absent/expired/revoked or unsupported bindings without fallback. Enforce actual read-only/resource restrictions and keep values out of definitions, prompts, Context, public results and diagnostics. Reuse existing authorization/secret owners and independent Agent ceilings; no credential registry or MCP-specific authentication broker. Evidence: [credential Red/Green, published Octa 0.7.3 pin and actual native qualification](mcp-execution-verification.md#task-14-independent-mcp-credentials).

- [x] 1.5 Through TDD at the selected AI plugin's native execution events/results, retain actual bounded MCP calls and consumed results through existing invocation/Artifact provenance. Verify connection/tool/arguments, observation/content identity, available coverage, timeout/cancellation and malformed/unavailable results remain inspectable without secret values or model-invented evidence. Missing required native capture is an explicit capability blocker; add no per-provider search schema/parser or parallel tool journal. Record the common handoff consumed by triage 4.4. Evidence: [native capture Red/Green, published Octa 0.8.0 pin and common handoff](mcp-evidence-verification.md).

## 2. Runtime-to-profile guidance

- [x] 2.1 Through Console TDD, add a clear Project-scoped handoff from Runtime settings to the existing AI-agent profile model/reasoning editor and actionable installation guidance. Show safe per-connection credential-reference/read-only scope metadata and distinguish AI login, MCP read authority and server Reporter writes; no token values in Runtime/profile content. Verify unsupported plugin/transport/auth/capture reasons, preservation of the open profile draft, explicit selection and configuration-only success feedback. Reuse the existing English/Russian narrow-keyboard browser loop and form components rather than adding another editor. Evidence: [Runtime/profile guidance, typed authority metadata and Red/Green verification](runtime-profile-guidance-verification.md).

## 3. Focused end-to-end qualification

- [x] 3.1 Extend the existing finite packaged/stand journey to configure and execute HTTP/stdio MCP through 1.1–1.5 and the real selected AI plugin/native client, with no out-of-band fixture grants. Observe exact selected settings, independent credential destinations, read-only/resource refusal, revocation, bounded cancellation, retained tool evidence, secret redaction and state after restart. Document setup/use and the exact qualified plugin/server capabilities, plus the handoff to triage 4.3–4.4. Run relevant existing regression/format/lint/build and strict OpenSpec checks, recording Red/Green commands and disk use; add no paid model call, live repository write, release pipeline or duplicate test matrix. Evidence: [packaged native qualification, repairs, Red/Green and disk results](packaged-mcp-verification.md).

  Approved qualification repair: when a stopped ordinary execution has no
  trustworthy terminal usage, retain its exact execution and budget reservation
  through the existing waiting state. Do not invent zero consumption, create a
  semantic result, dispatch another attempt or fail the shared owner in a way
  that prevents other Runs from stopping. Exercise observation and cancellation
  Red/Green at the existing node adapter boundary and in this same packaged
  journey. Late verified consumption must still permit normal settlement.
