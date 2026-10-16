## Why

Factory currently accepts manual Work and can publish GitHub delivery branches and pull requests, but cannot discover GitHub Issues or project configured Work statuses back to them. Operators need to attach label-selected Issue sources to pipeline inputs, including an assessment pipeline followed by separate approved defect and feature pipelines, without implementing another lifecycle or duplicate-detection engine.

## What Changes

- Add versioned, Project-owned Work Source input bindings that select an installed GitHub repository, bounded Issue-state and label predicates, and an exact target Factory Configuration/root Flow. An input belongs to its target pipeline; there is no deployment-wide hardcoded triage dispatcher.
- Support all-label, any-label and excluded-label sets, including an explicitly unrestricted label selection. Supply a two-step example: assess previously unassessed Issues and report `approved` or `rejected`; independently admit `approved + bug` and `approved + feature-request` into different pipelines.
- Extend the existing `octacity-github` provider plugin with independently advertised Source and Reporter capabilities, separate scoped credentials and protocol contracts, and bounded restart-safe polling. Webhook ingress is deferred.
- Reuse authoritative Work admission, replay, repository resolution, configured eligibility/business duplicate checks, and the existing reporting outbox. Use a stable input-binding identity so the same Issue can enter assessment and implementation once each without polls, label churn or configuration replacement creating more runs.
- Let every configured stage/outcome, including intermediate and nested stages, select a source-update action with explicit label additions/removals. Reuse ordinary trusted actions and the reporting outbox; do not restrict updates to pipeline completion or assessment. Translate arbitrary configured `set_status` values through exact managed-label mappings and optional Issue open/close effects; preserve unrelated labels and observe uncertain writes before replay.
- Add typed management REST and guided console input configuration, validation/preview and safe Source/Reporter diagnostics. Credentials remain operator-installed; existing manual intake and ordinary CI/CD remain independent.

## Capabilities

### New Capabilities

- `server/github-work-source`: Installed GitHub Issue discovery, label selectors, bounded polling, normalized intake, pipeline-scoped replay and non-authoritative status reporting.

### Modified Capabilities

- `server/dark-factory`: Define stable pipeline input bindings, cross-pipeline Issue identity and source-update actions after arbitrary stage outcomes using existing admission and lifecycle authority.
- `server/rest-control-plane`: Expose versioned source bindings, safe provider choices, bounded previews and connector diagnostics.
- `ui/operator-console`: Configure source selectors at pipeline inputs and label-update actions on individual stage outcomes through typed guided forms.

## Impact

- Extend `server/infrastructure/octacity-github`, common verified adapter/process contracts, the server composition root, application admission/status seams, and PostgreSQL-backed source progress/binding persistence as needed.
- Extend management OpenAPI, generated clients, REST fixtures and `ui/src/features/factory` configuration surfaces; keep provider wire DTOs and credentials outside Factory core and the browser.
- Add public-boundary TDD coverage for selectors, staged intake, existing replay/duplicates, reporter convergence, scopes, restart and failures; update operations/reference documentation and retain a controlled provider-boundary pilot.
- No new provider SDK is planned: reuse the existing pinned Octocrab transport. No change to Agent execution, Flow primitives, paid model qualification or the user's running local stand.
