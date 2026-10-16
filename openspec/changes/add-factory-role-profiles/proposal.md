## Why

Factory stages currently bind low-level execution profiles without a Project-owned way to express reusable agent responsibilities, model settings, skills, and instructions. Operators need arbitrary roles and multiple profiles per role, with manual, criteria-based, or Decision API selection and an explicit stage policy when selection fails.

## What Changes

- Add Project-owned, immutable versioned **Factory Roles** and **Role Profiles**. A role describes responsibilities; its profiles package exact model and reasoning settings, skills and supporting files, instructions, tools, resource limits, and compatible installed execution bindings. These are model-task definitions, distinct from enrolled worker Agents and Agent Pools.
- Supply copy-on-create starter templates for `coder`, `reviewer`, and `architect`; the coder template contains `default` and `pro` profiles, displayed as `coder` and `coder-pro`. Projects can create, rename, and configure arbitrary roles and additional profiles. Template updates do not modify existing definitions, and `pro` grants no special authority.
- Materialize a complete, reproducible execution package from pinned Project rules, role instructions, profile settings and skill bundles, stage task, and explicitly projected Context. Enforce Project rules and the existing permission intersection independently of instructional text.
- Add per-stage selection policies: a configured/manual profile, deterministic evaluation of supplied typed criteria, or provider-neutral Decision API selection with JEV as the initial adapter. Support explicit operator overrides before dispatch, within the stage's pinned compatible candidates.
- Reuse the Decision provider bridge for profile selection and existing Factory routing, with separate purpose-specific contracts, criteria, validation, calibration, consumption, and failure policies. Do not introduce a universal decision engine or move lifecycle authority into the bridge.
- Configure selection failure per stage as either one exact reserve profile or a durable wait for manual selection. Missing/inconclusive criteria, invalid choices, provider errors/timeouts, and insufficient qualified confidence are explicit failures. An unusable reserve leads to manual selection without recursive fallback.
- Persist selection inputs, policy and profile versions, matched rule or Decision Signal receipt, override/reserve reason, and final compiled execution identity. Recovery reuses the selection; a profile change cannot replace a dispatched invocation or bypass required gates.
- Add headless management and Console authoring, validation, selection controls, waiting-state diagnostics, and history. Existing static Build bindings remain supported without role definitions or Decision API availability.

## Capabilities

### New Capabilities

- `server/factory-role-profiles`: Project-scoped role/profile publication, starter templates, reproducible execution packages, bounded stage selection, reserve/manual recovery, and retained selection provenance.

### Modified Capabilities

- `server/dark-factory`: Pin allowed role/profile choices in immutable Flows, bind chosen profiles into ordinary Builds and Task Envelopes, and add the bounded `profile_selection` Decision Signal purpose without granting lifecycle authority.
- `server/rest-control-plane`: Add authorized role/profile/template discovery and publication, stage selection validation, explicit selection commands, and safe selection diagnostics under existing concurrency and replay contracts.
- `ui/operator-console`: Add Project role/profile management, stage selection and failure-policy editing, manual overrides, and pending selection/history views through REST.

## Impact

- Extends Factory core and application modules, operation-shaped memory/PostgreSQL stores and migrations, configured Flow publication/materialization, runtime profile installation, Decision Signal core and JEV adapter, REST/OpenAPI/generated clients, and `ui/src/features/factory/`.
- Preserves generic Flow node kinds, signed JobSpec v3, ordinary Build/Job execution, fenced ownership, hidden-test isolation, read-only evaluation, credential separation, and independent CI/CD operation. Existing installed technical profiles remain the authority for executable capabilities; user-authored roles cannot install arbitrary software or grant credentials.
- Adds no automatic migration of current runs or configurations, global live-inherited roles, fixed business-role enum, automatic profile upgrades after execution failure, new Agent kind, or Octa-local decision plugin. The separate quickstart, Work card, and GitHub source proposals retain their scope; integration is additive and does not require their implementation.
- Implementation requires behavioral Red/Green slices, memory/PostgreSQL recovery and concurrency contracts, provider conformance, public REST/Console journeys, and released execution-package evidence. This change currently creates planning documents only.
