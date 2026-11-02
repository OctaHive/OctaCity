## Context

See `proposal.md` for the operator problem. Current implementation boundaries are:

- `tools/factory_triage_setup.py` emits `admission_policy` and `permission_ceiling` installation references. The former has no rule definition; the latter includes typed permission metadata.
- `server/core/octacity-server-factory/src/configuration.rs` resolves both aliases into immutable references. `factory_readiness.rs` checks their presence in current installation choices.
- `factory_admission.rs` and the existing Source/Project-text paths own Work admission. `FactoryAdmissionStore` owns deduplication and the accepted Work/Run transaction.
- `FactoryCapabilitySource` owns installed authority; Runtime settings narrow installed choices. Project configuration must not become a replacement installation source.
- Project settings already host documents, and Factory Configuration authoring already has typed selectors, immutable publication, preconditions and confirmed commands.

## Goals / Non-Goals

**Goals:** Give both selectors inspectable durable contents, allow Project administration through ordinary management contracts, and preserve exact policy provenance through publication, admission and recovery.

**Non-Goals:** A general policy scripting engine, user/role ACL administration, scheduling/queue policy, additional business vocabularies, Runtime/credential installation, or duplicate editors for Configuration WIP and Run budgets. Agent isolation and explicit Context/result projection remain unchanged.

## Decisions

### 1. One Project policy owner, two bounded document kinds

Add a Project-scoped policy owner beside Factory Configuration management, with separate typed documents for launch policy and access profile. Each has identity, display name, immutable version and content digest. Reuse existing management authorization, idempotency, strong preconditions, audit and outbox conventions in store ports and PostgreSQL. Share persistence mechanics where useful without introducing a generic policy framework.

Launch policy initially contains only `allow_manual` and `allow_automatic_source`. Both false is a supported stop-new-admissions policy. Manual covers explicit operator Work and Issue execution requests, even when an Issue originated in GitHub. Automatic covers the existing Source worker. The server assigns the origin from the trusted entry path; request metadata cannot select a more permissive origin. Additional rule kinds require their own reviewed extension.

Access profile reuses the existing Factory permission types and validation/intersection logic, including tools, executable/plugin identities, commands, filesystem, network, secret/workload profile references, resource and output ceilings. It grants neither a secret value nor an MCP connection. Runtime selections, Project authorization, installed limits and independent Agent/backend grants remain separate constraints. Reject unqualified or broader requests with requested/allowed/source diagnostics rather than silently clamping them.

Alternative rejected: storing policy prose or extra installation aliases. Neither supplies enforceable, editable Project rules.

### 2. Pin contents and enforce through existing owners

New Configuration publications resolve Project policy references to exact versions/digests. Admission reads those contents through the same owner and records the selected policy references and effective access with its accepted operation. The common admission path checks the trusted origin before admission-specific source preparation/VCS resolution and before creating Work, Run or Build. Source discovery/preview remains read-only and does not establish admission.

Use typed, explicitly distinguishable Project bindings in the new wire/storage representation. Retain the legacy representation for existing bytes and clients; reject ambiguous requests mixing competing legacy and Project bindings. An older reader must reject an unsupported new binding rather than interpret its identity as an installed alias.

An automatic policy grant does not activate an input; current Source enablement and authorization are independently required. Denial must be a specific policy outcome, not a transient provider error. Accepted-operation replay returns the original receipt under current read/command authorization without a second admission or latest-default lookup.

The access profile narrows the existing Configuration/Flow/step/Profile compilation path and final execution enforcement. Dispatch retains the accepted profile content while current installed revocation or unenforceable restrictions still block execution. Do not resolve a newer profile during retry. Policy updates do not cancel accepted Runs or edit their artifacts.

Alternative rejected: checking only the browser or only the manual endpoint; either allows another existing ingress to bypass the policy.

### 3. Durable defaults, no arbitrary first-option selection

Provision Standard launch policy v1 with both supported launch modes allowed, preserving existing admission behavior; automatic inputs still start disabled or retain their explicit existing state. Provision Standard access profile v1 from the deployment's explicitly designated reviewed default ceiling. The local stand designates its existing qualified ceiling through ordinary provisioning inputs. Do not infer a default from list order, union multiple ceilings or grant unrestricted access. Without such an input, provision a profile with no execution grants and show why execution needs configuration.

Store exact default references in the same Project policy owner. Provisioning and existing-Project backfill are authorized idempotent lifecycle commands, never side effects of GET. Read-only users can inspect already provisioned defaults without needing write access. Repeated initialization must preserve operator edits and selected defaults. Publishing a new version and choosing it as Project default are explicit reviewed operations. Defaults fill only new drafts; existing drafts and published pins remain unchanged.

Alternative rejected: a live virtual default backed by installation order; it changes meaning across restarts and cannot provide stable execution provenance.

### 4. Project editors with a Configuration handoff

Add Project settings → Factory, with **Launch policies** and **Access profiles** catalogs. Each supports create, view and replacement publication; default selection is explicit. Deletion/archival is outside this first change. Standard entries are ordinary editable records, not business-specific reserved names.

Launch editor shows the two launch-mode controls and explains the independent Source activation. Access editor reuses supported finite permission controls and previews effective restrictions and limiting sources. No raw JSON, token entry or manually typed digests are required. Advanced supported controls remain available in collapsed sections.

Configuration fields show visible localized labels, exact names/versions, summaries and **View / Edit** links. New drafts initially use Project defaults and retain explicit overrides. Returning from Project policy settings preserves the mounted Configuration draft; adopting a newly published version is explicit. Keep exact identifiers under Details and distinguish missing, inaccessible and outside-installation choices. A saved policy starts no Run and changes no sibling Configuration.

### 5. Compatibility is explicit and does not rewrite history

Add durable legacy-reference mappings keyed by Project, reference kind and complete immutable reference. Import existing admission markers into concrete launch definitions matching the previous two supported ingress modes. Import access contents only from the exact verified existing installation declaration. Mapping receipts retain the original reference and resulting exact Project policy reference; do not invent missing permissions or reinterpret a foreign alias.

Existing published Configuration bytes/digests remain intact. Their read/admission adapter resolves only the recorded mapping. Missing or incompatible mappings are actionable blockers for new admission, not fallback to Standard. Existing accepted Runs retain their original execution records and recovery semantics. New replacement Configurations use explicit Project policy pins. The ordinary configuration choice resolver keeps legacy clients compatible without name-based business logic.

## Risks / Trade-offs

- **Default could silently broaden authority** → Require an explicit installed ceiling; otherwise no execution grants. Validate against current independent limits.
- **Editing appears to update every pipeline** → Show affected references and explain that a Configuration replacement is required; never repin it automatically.
- **Source and manual paths disagree** → Enforce one server-derived origin contract across manual, Issue, Source and replay boundaries, with durable integration tests.
- **A second permission implementation diverges from Runtime** → Reuse existing typed declarations, capability qualification and compilation; keep transport/credential ownership unchanged.
- **Compatibility hides unresolved authority** → Record exact mappings and block new admission when exact permission contents cannot be recovered.

## Migration Plan

1. Add typed policy persistence and management contracts, preserving all existing immutable rows. Backfill defaults and exact recognized legacy mappings through an idempotent authorized command before exposing the new editor.
2. Enable policy-aware Configuration publication, readiness, admission and compilation together. Verify unchanged accepted-operation replay and existing Run recovery, including narrowed current installation authority.
3. Add generated Console contracts and Project settings, then default selection and handoff in Configuration authoring. Update stand provisioning to use the same commands and retain operator changes across rebuilds.
4. Before a binary rollback, stop new admission and retain matching database/object/private-root backups. Older binaries must not read new policy-bound Configuration versions as legacy aliases. Restore compatible application/data together; do not drop policy tables or reset a personal stand as an upgrade procedure.
