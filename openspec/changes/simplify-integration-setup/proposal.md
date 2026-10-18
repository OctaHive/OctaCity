## Why

Integration setup currently exposes installed resource profiles, separate credential administration and activation instead of the operator's task: connect a particular external project. The Factory stand also preselects OctaCity as the installed GitHub repository, preventing the expected address-and-token setup, especially for private repositories.

## What Changes

- Provide one **Add integration** form: internal display name, external project address, write-only token and permitted task operations (read / modify), with one **Check and save** submission. Select a compatible preinstalled adapter through server-advertised address support; expose provider selection only when necessary to disambiguate.
- Add protected, recoverable server-side setup that resolves the selected public or private project using the submitted token, verifies native identity, enrolls a bounded resource and publishes usable connection metadata and scoped credentials. Installing executables, network policy or deployment ceilings remains an operator responsibility.
- Keep **OctaCity Project access** as a separate operation selecting the saved connection and a subset of its permitted operations. Saving a connection neither grants Project access nor enables polling, synchronization or execution.
- Present readable connection details and check results, including unverified write permission; support editing, token replacement and disablement without exposing private handles or credential values.
- Add explicit connection lifecycle actions: reversible disablement, archival that hides a connection from the ordinary list and blocks further use while preserving history, and permanent deletion only for never-used connections with no dependencies. Show safe dependency reasons before confirmation, fence stale operations and clean up only exclusively owned unreferenced credentials; never delete external projects or revoke remote tokens implicitly.
- Bootstrap `tools/factory-stand` with the installed adapter and enrollment policy, without a required real repository, token or unauthenticated repository lookup. Preserve existing installations and accepted execution history.
- Preserve independent task-read, task-update, repository-read and Delivery authority behind the simpler task-oriented labels. GitHub is the first native implementation; future installed providers use the same setup contracts. Manual task creation and manual Factory admission remain independent.

## Capabilities

### New Capabilities

None; extend the existing integration administration capabilities.

### Modified Capabilities

- `server/rest-control-plane`: typed protected setup, verified resource enrollment, recoverable write-only credential publication, connection-bounded Project grants and guarded archive/delete operations.
- `server/github-work-source`: authenticated address-based repository setup within installed ceilings, preserving native identity and independent credential purposes through archival and deletion.
- `ui/operator-console`: one setup form, simple connection details, understandable archive/delete actions and a separate Project access form with truthful readiness.
- `deployment/local-compose-stack`: adapter-ready Factory stand startup without a preselected repository, durable lifecycle state and real integration readiness checks.

## Impact

Application integration handlers, verified adapter/provider contracts, credential publication, durable stores and migrations where needed, runtime composition, REST/OpenAPI and generated UI contracts, Integrations UI, Factory stand bootstrap and setup documentation/tests. Existing metadata, credential and grant APIs remain supported; any versioned provider-contract extension must reject unsupported installations explicitly.

This change does not implement GitLab/Jira adapters, internal task cards, pipeline canvas, task synchronization, new pipeline actions or login. It consumes established native resource identities without changing `add-factory-work-card`, its completed tasks or manual creation requirements. No commit or implementation is part of this planning change.
