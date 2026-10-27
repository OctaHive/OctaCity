## Context

See [proposal.md](proposal.md) for motivation. The existing integration model already separates safe immutable connection metadata, protected credential publication, Project grants and bounded read-only checks. The simplification must preserve those boundaries while removing their exposure as prerequisite screens.

Observed implementation constraints:

- `server/application/src/integration_connection.rs` accepts resources only from verified installed declarations; it neither resolves a project address nor handles credentials.
- `server/app/src/runtime/integration_management.rs` installs fixed resource, check and credential-publisher targets. Adding an address field to the UI cannot create a usable resource under that contract.
- `tools/factory_integrations.py` resolves a public repository without authentication and produces an immutable bundle for that repository. `tools/factory_local_stand.py` currently defaults to the demo repository when no installation exists.
- `ConnectionEditor.tsx` publishes metadata, `CredentialSetup.tsx` provisions credentials, and `ConnectionDetails.tsx` exposes activation and checks separately. Existing secret entry deliberately avoids browser persistence and mutation-cache retention.
- Source, Reporter and existing optional task observations already use native resource identities and separate live purpose grants. Pending reporting retains exact target/version/deadline information.

## Goals / Non-Goals

**Goals:** Make one form sufficient to create a usable connection to an operator-selected external project, including a private GitHub repository; keep Project authorization a separate understandable operation; provide understandable disable, archive and guarded permanent-delete actions while preserving lifecycle and identity guarantees.

**Non-Goals:** A general workflow/orchestration engine, arbitrary adapter installation from the browser, automatic expansion of host policy, native GitLab/Jira implementations, task import, pipeline activation or internal card implementation. Task connection permissions do not configure code checkout or Delivery.

## Decisions

### 1. Two operator operations, with explicit meanings

The primary setup form contains:

| Field | Meaning |
| --- | --- |
| Name | Display name in OctaCity, independent of native identity |
| External project address | Repository/project URL, interpreted by a compatible installed adapter |
| Access token | Password input; write-only, transient, never prefilled |
| Read tasks | `task_read`, selected by default when supported |
| Modify tasks | `task_update`, initially unselected; covers configured ticket effects, not code pushes |

**Check and save** is the explicit submission of these visible choices; the safe summary is in the same form, not another technical review wizard. Server-authoritative dependencies between purposes are explained before submission. If an installed provider is ambiguous, show a human-readable adapter choice; do not ask for installation IDs, digests, native IDs, endpoints or credential handles. Unsupported URLs get an actionable error, never an automatic adapter installation.

After success, show name, canonical address, selected permissions, enabled/archive state and per-purpose check results, with Edit, Change token, Disable, Archive and Delete permanently as applicable. A connection is enabled for its declared purposes by successful setup; a separate activation click is unnecessary. Unknown write readiness remains visible, including after success. Disruptive replacement, permission removal and lifecycle changes keep explicit consequence/dependency confirmation. Keep ordinary administration on the connection detail, not a new setup wizard; explain unavailable deletion using safe dependency reasons.

**Project access** separately selects an OctaCity Project, an existing connection/resource and permitted operations. It requires both existing connection-grant and target-Project administrative authority. Its choices are a subset of the connection's declared purposes and installed ceiling. Pipeline users then select the granted integration without another URL or token. These operations do not start polling, import tasks or enable execution.

Alternative rejected: only relabel the current forms. It leaves resource selection and credential/activation sequencing unresolved. Putting grants in the creation form was also rejected because it conflates reusable credentials with Project authority.

### 2. Add a bounded enrollment seam to verified installed providers

Separate **installation policy** (executable/manifest pins, allowed service/API hosts, authentication support, enrollment eligibility and ceilings) from **enrolled resources** (verified native ID, canonical display locator and immutable capability/profile references). Default policy denies enrollment unless explicitly configured. Deployments may restrict enrollment to explicit repositories or an owner/organization; personal Factory stand policy explicitly allows token-authorized repositories on the approved GitHub service, subject to configured bounds. The form cannot change that policy.

Extend the existing verified adapter contract with a versioned setup/resolve capability and typed safe result. The application seam uses open provider/native-container identities; URL grammar, GitHub response types and credential class translation remain in the native adapter. The runtime composes this seam alongside existing metadata, publisher, check and authorization services. It does not execute `tools/factory_integrations.py` from HTTP or accept arbitrary provider config/filesystem paths.

Before sending a token, the adapter validates the URL against installed address grammar and enrollment policy and constructs its approved API request. Never fetch the submitted URL directly. Reject embedded credentials, unexpected ports, schemes, traversal, unsupported paths and unapproved redirect/pagination destinations. Resolution is bounded by configured time, bytes, requests and concurrency. GitHub resolves the exact repository with the submitted token, verifies its native ID and canonical repository context, and supports private repositories. No public repository lookup, `gh` session or ambient environment token is a prerequisite or fallback.

Persist newly verified resources as additive immutable managed declarations/profiles under installed policy. Pin executable, profile, endpoint and native resource identity as current Source/Reporter consumers require. Existing installed declarations/private bundles remain immutable and supported. Resource enrollment must also become usable by runtime Source, Reporter and check resolution after restart, without a per-repository rebuild or a startup-only registry snapshot. Use the existing registry/store/profile mechanisms with the narrow additional resource persistence needed; do not introduce another independent integration model.

Alternative rejected: treating a user URL as an installed resource or expanding the installed allowlist directly. Both bypass verification and risk silently redirecting pending work. An immutable single-repository installation can remain supported without limiting the new explicitly enrollment-enabled installation.

### 3. One protected setup operation over existing services

Expose a typed setup command plus safe status/receipt reads through REST/OpenAPI, available to non-UI clients as well. The exact endpoint names are implementation details. Metadata-only publication stays credential-free; raw token submission goes only to the protected setup/rotation boundary with redacted bodies. Authorize connection administration, credential provisioning and resource enrollment before private preparation or native resolution. Project grant authority is not implied.

Use one stable administrative operation identity and current-version/admin-revision preconditions. A small durable setup state records safe intent and references to protected preparation, verified resource publication and completion; reuse existing credential generation fencing and publication recovery. The sequence is authorization/policy validation, private preparation, bounded resolution/check, immutable resource/profile preparation, publication of connection and credential references, then activation/completion. Only the final usable connection is selectable. Failure leaves a new connection inactive/unavailable; failed replacement leaves the prior working version current.

No database transaction spans provider I/O. Fence concurrent completion and recheck current installation policy, connection version and revocation before activation. Expose only safe pending/failed/completed status with request correlation. An uncertain transport response is recovered by operation status before any new submission. A retry resumes the same preparation rather than creating another connection or credential generation. Reject conflicting reuse of the key, including a changed token; reuse protected credential-intent binding without storing raw tokens, unkeyed token digests or token-derived values in public metadata/audit. Recovery must not require a browser-retained token. Clean up unreferenced preparation within bounded lifecycle rules without deleting active or historical referenced material. Old replay cannot resurrect a disabled connection or revoked credential.

One submitted token may provision separate private purpose-scoped bindings selected by the user. Source and Reporter still resolve independent handles and cannot borrow Delivery/repository authority. Preserve the existing private credential store/publisher; do not create a second general secret storage system. Browser handling reuses the transient input approach and sends the token outside ordinary query/mutation caches; form unmount, submission and navigation clear it. Safe operation receipts survive retry without the value.

Alternative rejected: chaining existing HTTP calls only in the browser. It fails under refresh, lost responses and process restart, and cannot safely enroll previously unknown resources.

### 4. Permission selection and permission evidence are distinct

For each purpose, effective authority is bounded by installed policy, connection selection, current Project grant and actual remote credential authority. A UI checkbox requests a local ceiling; it never edits remote token permissions. The adapter reports confirmed, denied, unverified, unavailable or deferred evidence as supported, without pretending all providers expose permission introspection.

Checks are read-only: no temporary label, state, branch, comment or PR write. Authentication or required read failure prevents successful setup. A known-denied requested purpose produces a field-level outcome and no silent downgrade. If write authority cannot be proven without mutation, saving the explicitly selected purpose is allowed with a visible unverified result, not a claim that writing works. Later writes remain subject to normal authorization, live revocation, provider rejection and bounded recovery. Throttling/outages are classified and do not count as successful empty discovery.

Alternative rejected: test writes or inferring all write permissions from a successful read/token scope string. Those change external state or overstate readiness.

### 5. Preserve identities and optional consumers

Renaming a connection or rotating its token preserves logical connection/native resource/input identities. A changed project URL is resolved and explicitly published as a new resource/version; it cannot retarget an existing Work, input snapshot or pending effect. Compatible canonical aliases verified as the same native ID do not invent a new task identity. Two different connections to one repository retain their own credentials and grants; enrollment does not make one connection a credential fallback for the other.

Removing a connection purpose or revoking access blocks subsequent operations for that purpose, including retained intent retries. Existing Project grants cannot exceed the current connection ceiling, and adding the purpose later does not silently revive invalidated grants. Already accepted requests cannot be recalled; retained execution/reporting history is unchanged. Existing explicit adoption/advanced metadata APIs remain available; ordinary setup is the default UI entry.

Future internal cards consume connection/resource/grant references and the existing execution-neutral observation seam. This change neither creates cards nor maps their statuses. Manual cards, when enabled, and current manual Artifact-backed Factory intake require no integration.

### 6. Factory stand installs capability before any real project

Change bootstrap to install the pinned adapter, protected gateway-to-server channel, credential publisher capability and explicit bounded enrollment policy independently of a repository. Remove the implicit real OctaCity resource from the default setup path; optional demo/legacy explicit repository bootstrap remains compatible. No token or unauthenticated GitHub repository lookup is required to open Integrations. Reuse verified installations instead of rewriting bundles or deleting credentials.

Launcher readiness verifies the actual setup/catalog and connection routes through the gateway, not only process health. Direct unverified server access remains forbidden. Test packaged source/build context, generated config, restart and retained state, and the real composed UI route. The adapter-only startup must work with GitHub unavailable; subsequent **Check and save** reports provider availability separately.

### 7. Disable, archive and delete have distinct consequences

**Disable** is the existing temporary operational pause. Explicit reactivation still requires current credentials, policy and authority. **Archive** retires the connection: it is disabled, excluded from the default list and all new-use selectors, and visible to authorized administrators through an Archived filter and retained detail/history links. This change does not add an unarchive workflow. Archive can retain dependencies; it neither deletes nor retargets Project grants, input versions, pending effects, Work/Run history or future internal-card links. It invalidates their authority for subsequent provider dispatch, checks, new grants and input publication. Already accepted remote requests cannot be recalled; manual tasks/intake and accepted local execution history remain independent.

**Delete permanently** is restricted to a never-used connection with no retained dependencies. Eligibility requires no Project grant or input binding (including retained versions), Work/Run or observation/card reference, pending effect, in-progress setup/credential publication or other retained consumer. The connection may be unconfigured, disabled or archived; archival does not make a used connection eligible. Completed setup, credential configuration and administrative read-only checks alone do not count as task/execution use, but accepted runtime use must remain a deletion blocker even if it produced no Work. Deletion requires provably safe cleanup. Completed administrative audit/idempotency receipts are retained as minimal safe tombstone evidence and do not themselves make an otherwise unused connection undeletable. Existing connection/native identities are never reassigned to another connection.

Authorize each action and dependency inspection under existing management boundaries. Return a bounded dependency summary and authoritative eligibility, showing visible dependents and a generic blocker for protected dependents without leaking their identities. Require current metadata-version/admin-revision preconditions, scoped idempotency and explicit confirmation. The server atomically rechecks eligibility when deleting; an earlier UI preview is not authorization. Fence concurrent grants/input references, setup/rotation completion and stale activation/replay against the same lifecycle state. Archive can invalidate a prepared operation; later completion must fail safely and use bounded cleanup. Retried completed commands return their original safe receipt without restoring a connection, credential or grant.

Preserve existing exact historical metadata/reference reads under their authorization rules after archive. Delete only the unreferenced connection data and exclusively owned, unreferenced private credential generations; never remove shared installed handles, adapter installations, shared resource/profile declarations or material required by other retained consumers. Release ownership only after the old private binding is safely retired, so a new connection neither conflicts forever with a deleted owner's handle nor inherits its credential. Reuse generation-fenced cleanup/recovery and minimal deletion evidence across restart; do not hold a database transaction over private-store or provider I/O. Deletion completes only after safe private cleanup, with recoverable status if interrupted. Neither archive nor delete performs external writes, deletes tickets/repositories or revokes the user's token at the provider.

These lifecycle contracts are provider-neutral and shared by legacy and newly enrolled connections. Future internal cards register retained references through the same dependency boundary; their implementation is outside this change. No archive/delete operation requires a provider for manual creation or changes task status mappings.

Alternative rejected: relabeling Disable as Delete, cascading deletion of referenced history, or always allowing hard deletion after archive. Each obscures retained dependencies or risks breaking recovery and historical links.

## Risks / Trade-offs

- [Broader resource enrollment than today's static list] → Explicit default-deny installation policy, bounded authenticated resolution and exact native pins; deployments retaining a static policy remain restricted.
- [Crash across private files and database publication] → Extend existing generation-fenced preparation/recovery; test each externally observable interruption and concurrency boundary with actual stores/publisher.
- [A token's write authority cannot always be checked safely] → Preserve unverified status, explain it in place, and enforce/reconcile actual writes through existing Reporter rules.
- [Legacy and managed resources coexist] → One authoritative merged lookup under installed pins; reject conflicting declarations and preserve old referenced versions. Do not migrate identities by display name.
- [Simpler labels could hide stronger purposes] → Read/modify refers to tasks only. Repository-read and Delivery remain separately advertised and granted; unsupported purposes are never silently bundled.
- [Archive/delete races with setup, grants or pending effects] → Authoritative lifecycle fencing and atomic dependency checks; preserve retained references, fail stale completion and recover exclusively owned credential cleanup without resurrecting authority.

## Migration Plan

1. Add compatible versioned setup/provider contracts and durable resource/operation persistence with authorization and secret-handling tests. Retain existing APIs and immutable data; do not rewrite completed archived changes.
2. Wire runtime publication and post-restart resolution, then qualify GitHub and a synthetic second provider through the shared boundary. Unsupported older adapters remain selectable through their supported legacy path with a clear capability message.
3. Replace the primary UI setup path and add separate Project-access entry plus archive/delete controls; keep existing connections usable without reentering tokens. Existing connections start unarchived; no migration auto-archives or deletes them. Rotate only on explicit token replacement.
4. Install the adapter-only policy in clean Factory stands. Existing explicit installations retain their ceilings and resources; moving to broader enrollment is an explicit policy change with safe dependency checks, never an automatic widening of grants.
5. Verify a packaged composed stand: create a non-demo integration through the form against a hermetic provider fixture, grant Project read separately, select it in a pipeline, archive it and verify retained history and blocked subsequent use across restart. Separately delete a never-used connection and verify safe credential cleanup and fresh setup without ownership collisions. Prove manual intake still works. No real-provider writes are part of default qualification.
6. Roll back UI/runtime features by disabling new enrollment and new setup submissions while retaining readable managed resources and operation history. An older binary lacking the new resource format is not a supported in-place downgrade; retain backups and existing schema compatibility policy rather than deleting enrolled data to force rollback.

### Shared packaged qualification environment

Reuse the Source runtime's `SourceEnvironment` in
`server/tests/octacity-release-harness/tests/release_vertical_slice/source_environment.rs`
and the [packaged qualification guide](../../../docs/testing/source-runtime.md).
It already supplies checked Server/Console packages, nginx with protected Unix
ingress, disposable PostgreSQL/S3 endpoints and the verified native adapter
against the shared approved HTTPS provider fixture. This change adds its own
browser enrollment/archive/delete journey instead of duplicating that harness;
its qualification tasks remain pending.
