## Why

Factory stages currently bind low-level execution profiles without a Project-owned way to express reusable agent responsibilities, model settings, skills, and instructions. Operators need arbitrary roles and multiple profiles per role, with manual, criteria-based, or Decision API selection and an explicit stage policy when selection fails.

## What Changes

- Add Project-owned, immutable versioned **Factory Roles** and **Role Profiles**. A role has a name, uploaded raster avatar, description, ordered shared instructions, shared versioned skill folders and an agent-wide concurrency limit; its profiles package runtime, model and thinking level, additional instructions, environment variables, CLI arguments, MCP servers, external integration bindings, tools and resource limits through compatible installed execution capabilities. These are model-task definitions, distinct from enrolled worker Agents and Agent Pools.
- Present these definitions in Console as a Project-scoped **AI agents** catalog (`AI-агенты` in Russian). One agent card represents one Factory Role with shared instructions and skills, and its Role Profiles expose execution settings. Reuse the same identities and REST contracts; keep CI/CD worker Agent/Pool management separate.
- Import a custom skill folder directly from Console without closing the AI-agent editor, then browse the retained file inventory and bounded plain-text previews. Retain its reviewed `SKILL.md` and supporting files as one immutable Project-owned bundle through existing content storage, then select its exact version on the AI agent; local folder changes never update saved definitions automatically.
- Supply copy-on-create starter templates for `coder`, `reviewer`, and `architect`; the coder template contains `default` and `pro` profiles, displayed as `coder` and `coder-pro`. Projects can create, rename, and configure arbitrary roles and additional profiles. Template updates do not modify existing definitions, and `pro` grants no special authority.
- Materialize a complete, reproducible execution package from pinned Project rules, role instructions, profile settings and skill bundles, stage task, and explicitly projected Context. Enforce Project rules and the existing permission intersection independently of instructional text.
- Add per-stage selection policies: a configured/manual profile, deterministic evaluation of supplied typed criteria, or provider-neutral Decision API selection with JEV as the initial adapter. Support explicit operator overrides before dispatch, within the stage's pinned compatible candidates.
- Reuse the Decision provider bridge for profile selection and existing Factory routing, with separate purpose-specific contracts, criteria, validation, calibration, consumption, and failure policies. Do not introduce a universal decision engine or move lifecycle authority into the bridge.
- Configure selection failure per stage as either one exact reserve profile or a durable wait for manual selection. Missing/inconclusive criteria, invalid choices, provider errors/timeouts, and insufficient qualified confidence are explicit failures. An unusable reserve leads to manual selection without recursive fallback.
- Persist selection inputs, policy and profile versions, matched rule or Decision Signal receipt, override/reserve reason, and final compiled execution identity. Recovery reuses the selection; a profile change cannot replace a dispatched invocation or bypass required gates.
- Add headless management and Console authoring, validation, selection controls, waiting-state diagnostics, and history. Existing static Build bindings remain supported without role definitions or Decision API availability.

- Add a follow-up Project settings workflow for supported model-provider connections and available Runtime/Model choices. Reuse managed credentials and qualified runtime installation authority; let `default`, `pro`, or any named profile pin an independently selected model and supported thinking settings. Agent identity/instructions/skills remain authorable without a configured provider. This follow-up is planned in section 10 and is not implemented by the current Console correction.

## Capabilities

### New Capabilities

- `server/factory-role-profiles`: Project-scoped role/profile publication, starter templates, reproducible execution packages, bounded stage selection, reserve/manual recovery, and retained selection provenance.

### Modified Capabilities

- `server/dark-factory`: Pin allowed role/profile choices in immutable Flows, bind chosen profiles into ordinary Builds and Task Envelopes, and add the bounded `profile_selection` Decision Signal purpose without granting lifecycle authority.
- `server/rest-control-plane`: Add authorized role/profile/template discovery and publication, skill-folder import, stage selection validation, explicit selection commands, and safe selection diagnostics under existing concurrency and replay contracts.
- `ui/operator-console`: Add Project AI-agent cards and profile management over Role/Profile contracts, stage selection and failure-policy editing, manual overrides, and pending selection/history views through REST.

## Impact

- Extends Factory core and application modules, operation-shaped memory/PostgreSQL stores and migrations, configured Flow publication/materialization, runtime profile installation, existing retained-content/object storage and Build-capacity enforcement, Decision Signal core and JEV adapter, REST/OpenAPI/generated clients, and `ui/src/features/factory/`.
- Preserves generic Flow node kinds, signed JobSpec v3, ordinary Build/Job execution, fenced ownership, hidden-test isolation, read-only evaluation, credential separation, and independent CI/CD operation. Existing installed technical profiles remain the authority for executable capabilities; user-authored roles cannot install arbitrary software or grant credentials.
- Adds no automatic migration of current runs or configurations, global live-inherited roles, fixed business-role enum, automatic profile upgrades after execution failure, new Agent kind, or Octa-local decision plugin. The separate quickstart, Work card, and GitHub source proposals retain their scope; integration is additive and does not require their implementation.
- Implementation uses behavioral Red/Green slices and the existing shared application/store, REST/Console and released execution-package contracts. Sections 2–9 have recorded implementation evidence; the Console correction adds uploaded avatar and shared skill authoring/reading without a new store or execution path. Section 10 remains the planned provider/runtime/model configuration workflow.
