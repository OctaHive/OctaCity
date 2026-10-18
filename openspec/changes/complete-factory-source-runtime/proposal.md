## Why

The released Factory stand can save a provider connection and Project grants, but automatic-input discovery and choices return `503` because their handlers are not composed. Source polling and Reporter components have isolated tests without a qualified browser-to-runtime path, so an operator cannot turn a configured integration into observable Factory execution.

## What Changes

- Complete released server composition for input/group discovery, choices, validation, immutable publication, preview, retained dependencies and Source/Reporter diagnostics through the existing protected API.
- Connect trusted installed Source/preparation/Reporter profiles to Project-authorized connection resources and the existing runtime workers. Reuse durable polling, snapshot preparation, admission, supervised cancellation and reporting recovery; no second execution engine or provider-specific Factory logic.
- Make the existing pipeline-input UI usable against that real composition: select the saved connection, configure predicates and mappings, preview, explicitly enable an input, and follow its Work/Run and reporting results. Return actionable missing-capability/grant/profile and provider-deferred states instead of undifferentiated management errors.
- Qualify the complete packaged `tools/factory-stand` journey through browser, gateway, real server, PostgreSQL, object storage and the installed adapter process. Prove restart/replay, revocation, read-only operation, optional reporting and independent manual intake. Mocked UI success alone is insufficient.
- Preserve already installed connections, credentials, grants, inputs and accepted history. Include the recent dependency-route, outbound-network and retained-volume restart fixes as regression prerequisites, whether they are committed before this change or land with it.

## Capabilities

### New Capabilities

None; complete and qualify existing source-management and execution capabilities.

### Modified Capabilities

- `server/rest-control-plane`: released composition and safe operational readiness for the existing automatic-input API family.
- `server/github-work-source`: installed Source/Reporter profiles are usable by managed inputs in the released supervised runtime, with exact authorization and recovery.
- `ui/operator-console`: an operational automatic-intake journey with actionable runtime feedback and truthful execution/reporting progress.
- `deployment/local-compose-stack`: reproducible source-runtime installation, scoped readiness, restart safety and packaged end-to-end qualification.

## Impact

Server composition and deployment-owned profile loading, existing Source/Reporter supervisors and handlers, protected management routes, Console input authoring/progress, Factory stand installation/readiness and regression evidence. Extend typed deployment or safe public contracts only where the existing seams cannot express the necessary settings; retain existing API and identity compatibility.

This change is independent of the one-form connection setup and archive/delete UX in `simplify-integration-setup`: it consumes their connection/resource/grant contracts without duplicating enrollment or secret storage. It must work with current installed connections first and remain compatible with subsequently enrolled resources. `add-factory-work-card` retains ownership of internal Issues, workflow transitions and field synchronization; its completed tasks and artifacts remain untouched. Manual Factory intake and future manual Issue creation remain provider-independent. No new GitLab/Jira adapter, pipeline canvas, role-profile system or implicit external write is included. This proposal authorizes planning only, not implementation or live-provider mutations.
