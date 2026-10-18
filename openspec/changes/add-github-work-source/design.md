## Context

See [proposal.md](proposal.md) for motivation and the [GitHub source contract](specs/server/github-work-source/spec.md) for observable behavior.

Implementation baseline and remaining seams:

- `server/application/src/factory_admission.rs` preserves manual admission and replay-before-resolution; completed tasks 1–4.5 have added separately authorized source admission and reporting without changing manual identity.
- `server/core/octacity-server-store/src/factory_admission_model.rs` already scopes external identities by source and accepted security scope and retains immutable admission intent. Extend that authority; do not add a second deduplication database or reimplement the pipeline's business duplicate analysis.
- `server/application/src/factory_work_status_outbox.rs` already dispatches accepted `set_status` effects under independent outbox fences and retry policy through `FactoryWorkStatusProjector`.
- `server/infrastructure/octacity-github` now implements Delivery, Issue Source and Reporter over separately versioned verified process contracts and private scoped grants. Tasks 5.1–5.6 provide reusable connection metadata, protected credential publication, recoverable authorized administration/REST, explicit Project/resource/purpose grants with managed Source/Reporter enforcement, bounded read-only connection checks through independently advertised provider operations, and typed immutable automatic input/group publication with safe installed choices. Task 5.7 provides bounded read-only Source preview and retained Source/Reporter diagnostics. Task 5.8 aligns OpenAPI, canonical wire examples and generated client contracts. Tasks 5.9–5.10 provide dedicated connection/credential/check administration plus Project grants and visible retained dependency review. Guided pipeline input and status authoring remain pending from 5.11 onward.
- Existing VCS configuration uses integration identities and private credential handles, while the secret domain separates logical references from transient values. Reuse these contracts where applicable; do not expose Job-secret delegation as authority to manage provider credentials.
- `ui/src/features/factory/ConfigurationEditor.tsx` already selects visible Factory dependencies through generated REST types. Add an input section there or in its adjacent configuration detail rather than create a second pipeline designer.
- `ui/src/features/factory/FlowGraphEditor.tsx` and `FlowBindings.tsx` already author ordinary trusted-action bindings; use those surfaces for stage-local source effects.

The existing [extension boundaries](../../../docs/architecture/factory-extension-boundaries.md) deliberately reserved this change. The approved generic Flow and non-authoritative reporting contracts remain controlling.

The revised `add-factory-work-card` plans a persistent internal Issue above immutable Work. In this document, **external task** means the native GitHub Issue (or another installed provider's task); **internal Issue** means that optional Project-owned card. Existing references to an Issue's labels/state describe the external task unless explicitly qualified. This update preserves all completed task entries and verification evidence; the new handoff seam is pending work, not a claim about the implemented Source protocol.

## Goals / Non-Goals

**Goals:**

- Configure a reusable provider connection once, explicitly grant selected resources/capabilities to OctaCity Projects, and select it from multiple pipelines without copying secrets.
- Attach an operator-configured Issue source to a pipeline input and make its selectors, exact target and reporting effects inspectable before activation.
- Support the same Issue across independently configured assessment and implementation inputs while reusing existing admission, eligibility, duplicate handling, Flow execution and reporting.
- Keep GitHub payloads and network access in the GitHub provider; retain authorization and durable orchestration in the server.
- Reuse authorized native observations for optional internal-task consumers without making pipeline execution depend on cards or duplicating Source/Reporter machinery.

**Non-Goals:**

- No hardcoded labels, universal approval vocabulary, defect/feature dispatcher, automatic model decision, or new Flow primitive.
- No webhook ingress, GitHub Projects board/status-field API, Issue comment/attachment crawling, automatic label creation, arbitrary search-query language, or cross-forge implementation in this slice. Polling plus Issue labels and optional open/closed state supplies the requested behavior.
- No new business duplicate detector, credential entry in pipeline forms, implicit re-run on an Issue edit, live paid-model qualification, automatic merge, redesign of the whole console, or manipulation of the user's running stand.
- No general-purpose secret vault, browser plugin installation or promise of native GitLab/Jira or OAuth/App authentication in this slice. GitHub token authentication is the initial concrete administrative path; other installed adapters can advertise their own supported mechanisms.
- No internal Issue aggregate, revision editor, card execution request, standalone import/link-refresh lifecycle, field-ownership resolver or card UI in this change; those belong to `add-factory-work-card`. Manual Issue creation stays independent of integrations when that capability is available.

## Decisions

### 1. A stable input binding selects an exact Factory target

Introduce a Project-owned Work Source input aggregate with immutable versions and one current enabled version. Its stable identity survives selector and target-version edits. A version binds the exact authorized connection/resource and Project-access revision, installed Source capability/configuration, code Repository, exact target Factory Configuration, source/base policy, bounded input mapping, selector, poll policy and optional Reporter mapping. The reusable connection owns authentication; input selectors and Flow effects remain pipeline configuration. The selected Factory Configuration already pins its root Flow closure. In this document, “pipeline input” means this entry into existing Factory execution, not a replacement for the ordinary CI/CD Pipeline aggregate.

Provider-neutral application records carry exact references and admission provenance. Typed GitHub selector configuration is validated by its installed capability; GitHub wire DTOs, issue-state enums and SDK types do not enter the Flow interpreter. Store new binding/progress facts through narrow existing-style ports, with additive PostgreSQL migrations and reusable contract tests. Factory-disabled deployments do not install or start these workers.

Alternative: put a single poller and triage switch in the Factory controller. Rejected because the operator must independently select sources for several pipelines and change their labels without changing runtime code.

### 2. Explicit predicates, no privileged label names

The label predicate is:

```text
all_of is a subset of observed labels
AND (any_of is empty OR at least one any_of label is observed)
AND none_of has no observed label
```

All three empty sets are unrestricted; unlabeled Issues match. Issue state is independently `open`, `closed` or `all`, with `open` as the explicit initial form value. Validate bounds, duplicate entries, contradictory required/excluded labels and an unsatisfiable alternatives set; reject unsupported predicates. Labels are literal provider-resolved names, not regexes, expressions or instructions. Recheck actual returned labels locally after any provider-side narrowing; do not rely on an incomplete search result to enforce exclusions.

A runnable example configuration will illustrate the following mappings. **Every name and predicate below is an editable example, not a built-in value or required sequence.** Removing exclusions from the first input intentionally makes it select all Issues in its configured state.

| Example input | Required labels | Alternative labels | Excluded labels | Target |
| --- | --- | --- | --- | --- |
| Assessment | none | none | `approved`, `rejected` | Operator-selected assessment configuration |
| Defect implementation | `approved`, `bug` | none | `rejected` | Operator-selected defect configuration |
| Feature implementation | `approved` | `feature-request`, `enhancement` | `rejected` | Operator-selected feature configuration |

Assessment uses ordinary configured eligibility/duplicate/classification nodes. Its accepted trusted actions can project configured approval and classification label groups; the source neither assesses nor classifies tickets. The operator can instead use entirely different labels, more stages, a single pipeline, or unrestricted intake. The same controls exist through REST and the console.

### 3. Preserve input-scoped Work admission identity

The trusted connector installation resolves its allowed Project and source security scope. Refactor manual admission only after existing public behavior passes; extract or add a source-neutral application entry that preserves visibility, immutable definitions, input validation, replay-before-resolution, audit and atomic publication. Do not accept arbitrary claimed management identities from the provider process.

Construct a canonical admission identity from source installation, repository immutable identity, Issue stable item identity and stable input-binding identity. Issue number is display/lookup context and is verified against those identities. Binding version, current labels, observed timestamp, token and current target version are deliberately absent from this key. Retain them with the original intake snapshot/intent instead.

For repeated observations, recover the retained intent before preparing a new candidate: a changed Issue body must not cause an exact replay request with fresh artifacts or a new mutable base resolution. New candidates freeze title/body, selected labels/state, update identity and explicit task/acceptance mapping into bounded Artifacts; they then use existing admission. Durable pending preparation and input-owner fencing preserve the first accepted intent across crashes before or after admission. Artifact staging uses stable operation identity and bounded retention/cleanup. Missing acceptance data follows the configured mapping's declared failure, never fabricated criteria.

An external task admitted once to the assessment input may subsequently be admitted once to a distinct implementation input. Repeated observations, relabeling and input replacement cannot re-run either. Existing pipeline business duplicate checks still run when that pipeline first processes Work. The source identity distinguishes intended inputs, not a mechanism to bypass those checks. Diagnostics correlate both admissions to the same external task. An intentional fresh **automatic intake** cycle requires an explicit authorized new input identity; ordinary retry stays the existing Run/attempt mechanism. A later manual execution from an internal Issue uses that card's distinct explicit execution-request/admission identity, not a new automatic input or a changed native observation.

Alternative: use only Issue ID, blocking every later pipeline; or use Issue update/version/labels as dedup keys, creating loops after our own writes. Both fail the requested staged flow.

### 4. Exclusive groups make ambiguous downstream selection visible

Independent inputs may legitimately select the same Issue. Operators can explicitly place alternative implementation inputs in an exclusive group. One bounded selection pass evaluates the group's pinned member selectors against the same candidate snapshot. More than one match produces an ambiguous disposition and no admission; zero is filtered; one is submitted to its selected target. The group's current-version precondition and fenced scan prevent members being evaluated against different group revisions.

This avoids silently choosing a defect or feature pipeline when both label sets are present. No global label-based exclusivity is inferred. A later operator correction can make a previously ambiguous, unadmitted Issue eligible. Already admitted identities remain unchanged; editing labels does not cancel, migrate or replace running Work. GitHub edits cannot be atomically locked with database admission: selection is bound to the retained observation, not a promise that external labels stay unchanged.

### 5. Polling first, with retained progress and reconciliation

Extend the existing GitHub plugin with separate Source and Reporter protocol entry points, independently advertised capability versions and credential classes; reuse its common transport and verified host process mechanisms. Do not create a separate GitHub plugin per stage or allow the Delivery grant to authorize Issue operations. Source requires repository-scoped Issue-read authority; Reporter requires separately granted Issue-write authority. Secrets resolve through private installed handles, never Work metadata. Integration administration provisions or adopts those handles through a trusted publisher; a provider installation does not itself grant Project access.

Use repository Issue listing rather than unrestricted Search. GitHub's [Issues API](https://docs.github.com/en/rest/issues/issues#list-repository-issues) returns pull requests as well as Issues; reject records carrying `pull_request`. Apply local selector validation after decoding bounded responses. Pagination links are untrusted and must stay within the installed host/repository policy. Honor [GitHub retry guidance](https://docs.github.com/en/rest/using-the-rest-api/best-practices-for-using-the-rest-api), original deadlines and finite retries.

Retain per-input/group progress, scan windows, pending candidate intents, owner/fence and safe dispositions. Drain pages within configured per-pass limits; checkpoint only covered work. Use overlapping update windows plus periodic fair bounded full reconciliation, stable observation identities and existing admission replay. Do not assume timestamps or ETags alone provide exactly-once intake or a provider snapshot. Bound concurrency, bytes, pages and retries; expose backlog/cursor age rather than scan without a ceiling. Disabled inputs start no new preparation/admission; already committed Work and pending reporting keep their original policies.

Retain a narrow execution-neutral observation seam over these installed Source operations. An observation carries trusted Project/security scope, installation/native resource/item identity, display context, freshness evidence and bounded snapshot/input provenance; GitHub wire DTOs and secrets stay inside their existing boundaries. Pipeline matching and Work admission consume observations separately. Observations of already admitted tasks must not be suppressed solely by admission replay or exposed only through the last accepted Work snapshot. Existing Work continues to use its original frozen inputs.

The card change owns explicit import/link-refresh configuration and its durable task updates, including refresh of retained links that no longer match an execution selector. It must reuse the authorized Source read/observation and bounded progress/reconciliation contracts. A nonmatching/closed external task is not deleted from an internal card, and an observation alone is not a Run request. Disabling an execution input stops its admissions; separately configured link refresh can continue only with its own enabled policy and current read grants. No optional consumer means no card mutations or extra provider reads, and existing intake does not wait for card availability. Do not add an unbounded cache, implicit observer subscription or second provider polling implementation here.

### 6. Report configured desired effects through the existing outbox

Adapt the existing `FactoryWorkStatusProjector` to the verified Reporter process contract. Resolve only a pinned status mapping and the original source Issue association. A mapping declares logical status value, managed label groups, labels to add/remove and optional Issue state; `approved`, `rejected` or any other spelling is ordinary configuration. No action is inferred from model prose or a terminal outcome unless the Flow explicitly selects that action.

Every stage's declared outcome can transition through an ordinary `trusted_action` node that queues a source update. This includes intermediate/nested stages and configured failure/escalation branches; the Run need not be terminal. In the existing Flow editor the operator selects “update source labels”, selects labels to add/remove, and reviews the resulting exact mapping/action binding. Authoring can resolve a named installed effect preset or publish the reviewed versioned mapping with the selected labels; the admitted action always pins the resolved identity, not a mutable preset. This is presentation over the existing `set_status` and outbox contracts, not an automatic callback that bypasses the graph. Separate outcome routes can select different effects, and a stage with no selected action writes nothing. Retain stage/outcome/action provenance with each projection so reporting progress can be inspected before the pipeline ends.

Use the [GitHub label endpoints](https://docs.github.com/en/rest/issues/labels) to add/remove only managed labels, not replace the entire Issue label collection. Validate mapped labels using bounded repository choices and report missing/renamed labels; do not auto-create them. Multi-request projection is not atomic: retain original desired effects, inspect after unknown responses and converge partial progress. A durable per-Issue managed-group projection order shared by relevant reporters prevents a late older operation from restoring a superseded value. Recheck the current fence and latest authorized intent before each write; if an in-flight stale request lands late, reconciliation restores the newest desired state. Report unreconcilable external edits as conflicts rather than claim atomic compare-and-swap support from GitHub.

Implementation of task 4.4 uses Reporter v2 read-only plans and separately fenced
conditional single-write steps, with no downgrade to bulk v1. Each step is a
commuting subset of the original desired effects. Providers own opaque managed
field identities; GitHub includes native label-id and name-alias identities to
retain overlap across rename/recreation. Native field digests namespace the
canonical provider API instance, container and item, remaining stable across
installations. Memory and PostgreSQL share a Project/native-container/item
technical bucket across installations and assign an atomic
per-item sequence with accepted action/outbox publication, independent of clocks
or worker arrival. Immutable per-operation scopes, latest per-field heads and
revisions, and unresolved write permits fence both mutation and final observation.
Unknown newer scopes block older writes conservatively. Known disjoint fields
remain independent. Unknown remote completion is not cleared merely because a
client deadline expires; it becomes explicit conflict. Known late completion can
release only its own permit. Valid typed definitive failures release that
permit; unknown, invalid, cancelled or transient completion retains its hazard.
Reconciliation never renews the original bounds.
Legacy migration bootstrap is deterministic over retained timestamps/digests;
historical commit order cannot be reconstructed. New acceptance uses a locked
counter without rewriting accepted history. Bootstrap includes only configured
status actions bound to the original source Reporter capability. Shared
accepted-action provenance validation keeps store ordering and application
dispatch consistent. GitHub v2 resolves affected groups through one bounded
repository-label catalog page with exact-name fallback; no persistent cache or
unbounded pagination is introduced.

Share the existing reporting retry/outbox authority; add only the correlation/order state needed for external effects. Freeze adapter/mapping versions per intent. Neither projection completion nor GitHub labels reconstruct accepted Factory evidence. A matching downstream input still creates ordinary Work under its own gates; importing an upstream accepted artifact would require an explicit trusted provenance mapping, not interpreting the word `approved` as evidence.

Task-card field ownership and conflict resolution remain in `add-factory-work-card`, which consumes existing Reporter observations/receipts and reuses this outbox. This change authorizes Work-backed configured Factory actions; any future Issue metadata command is a separately validated producer owned by the card change, not permission inferred from a link, comment or mutable card status. Retain the original native target, action and mapping at acceptance: later card association or mapping replacement cannot retarget a pending operation or retroactively authorize reporting from an unlinked execution. An unavailable provider changes reporting health, not accepted execution or the future task's verification result.

### 7. Reusable integrations, Project access and pipeline selection

Use three distinct records and user journeys:

| Level | Owns | Does not select |
| --- | --- | --- |
| Integration connection | Stable identity, display name, open provider identity, installed adapter pin, endpoint, non-secret settings, credential references and supported capabilities | A pipeline target or its ticket predicates |
| Project access | Exact connection/resources, permitted purposes and use/manage authorization for an OctaCity Project | Credentials or implicit access to every provider resource |
| Pipeline input | Authorized connection/resource references, exact Factory target/code Repository, selectors, input mapping, poll policy and optional reporting effects | Authentication material or new provider authority |

The Integrations section is a reusable administration surface. Connection management
and credential rotation require explicit integration-administration authority;
Project-access publication requires authority over both the connection and target
Project. Pipeline editors receive only safe choices already granted to their
Project. A connection may serve several Projects through separate explicit grants;
each Project may use it in several pipelines. Creation grants no implicit access.

Keep provider identity open and expose only capabilities of verified installed
adapters. GitHub is the native implementation here. Native GitLab/Jira adapters
remain follow-ups, but common contracts/tests must accept their opaque identities
and containers without a GitHub-specific branch in Factory. A GitHub owner/name,
GitLab project or Jira project key is external resource context, not an OctaCity
Project or the code Repository. Native immutable resource identity is authoritative;
connection display names and provider resource names are not replay keys.

Reuse existing VCS/Delivery references and grants where applicable, without
changing their protocol or credential class. Task read, task update, repository
read and Delivery remain separately granted even when shown under one connection.
A token having remote write permission does not grant the pipeline that permission.

### 8. Protected credential setup and immutable publication

Integration administration supports a dedicated write-only GitHub token submission
or adoption of an existing installed credential handle. Provider forms expose only
advertised authentication methods. Raw values travel only in the protected secret
submission request; they never enter ordinary connection/configuration payloads,
returned representations, URLs, audit, logs, provider responses or Work. The UI
keeps secret input transient, excludes it from persistence and query/mutation caches,
and clears it after submission or leaving the form. Existing token contents cannot
be retrieved; show configured/missing status and safe metadata only.

Use a narrow credential-management port backed initially by the existing private
provider credential-file mechanism. The trusted installation publisher owns paths,
confinement, permissions, credential classes and bounded atomic replacement; the
browser supplies neither paths nor executable bytes. Public connection/access
records in the database retain only opaque credential references. Reuse existing
logical-secret types where appropriate, but do not treat the Job grant provider as
a provider-token vault. Managed token writes use authenticated protected transport
and request-body redaction. Do not add a universal vault or deploy a new external
secret service as a prerequisite. Externally installed handles remain supported
without exporting their values to management clients.

Provision private material before activating an immutable connection/profile
version. Since database publication and private-file writes are not one transaction,
use stable administrative operation identities, retained safe preparation state and
bounded orphan cleanup: a partial write is never an active usable connection.
Retries resume the same operation; replay of an old rotation returns its safe receipt
without restoring the old token. Do not retain raw request bodies for idempotency.
Rotation atomically replaces protected material behind the same logical purpose
handle and preserves connection/input/item identity; it does not repin native scope
or create Work. Endpoint, adapter, resource or capability changes require explicit
versioned replacement and validation; they cannot redirect a pending accepted intent.

Project grant revocation and connection disablement apply to subsequent protected
operations, including retained-intent retries. Queued reporting keeps its accepted
history and original finite policy but cannot use revoked write authority. No
fallback credential or automatic switch to a replacement resource is allowed.
Already admitted Work remains retained; other execution continues subject to its
own permissions. Show dependents before retirement, retain referenced versions and
safe audit, and collect private material only when no authorized retained consumer
requires it. Revocation must not be undone by stale administrative replay.

A connection check verifies bounded authenticated access and native resource identity
through the verified provider. Report configuration, authentication, resource access,
rate-limit/unavailable and capability results separately. It makes no test label,
Issue, branch or review mutation, grants no new permission, starts no polling or
admission and advances no production cursor. If write permission cannot be proven
read-only, report unverified/unsupported rather than test a mutation or claim success.
Endpoint validation and the trusted installation network ceiling precede credential
access; arbitrary URLs, redirects and pagination cannot expand that ceiling.

### 9. Guided pipeline configuration and safe diagnostics

The normal path is Integrations → connection/credential setup → Project access →
pipeline input. Pipeline forms select a permitted ready connection and native task
container independently of the code Repository, then configure their predicates,
exact target and effects. Show a link to connection administration for missing
access, not a token field in the pipeline. Manual intake remains a peer choice
requiring no provider configuration. Multiple pipelines reuse the same connection
and logical credentials while retaining their distinct stable input identities.

Expose versioned inputs/groups, connection/access choices and diagnostics through
ordinary typed management commands, bounded queries and confirmed publication.
Recheck current grants at validation, publication and each protected operation;
client choices and earlier checks are not authority. A bounded source preview
reads candidates but cannot change provider data, production cursors or admission.
Keep task-read readiness separate from optional task-write/Delivery readiness.

Task 5.6 authoring selects operator-installed, adapter-validated immutable selector/
mapping profiles. Safe choices describe their literal state/label settings; the
server does not accept executable uploads, private paths or arbitrary provider
configuration JSON. Input-specific settings pin their trusted parent connection
profile separately, allowing distinct selectors to reuse a connection. Managed
Source/Reporter dispatch resolves the selected input connection and fails closed
without its current read/write authority. See [task 5.6 verification](verification-5.6.md)
for publication, visibility, replay and compatibility evidence.

The approved task 5.11 expansion adds optional input-owned normalized narrowing
profiles. An installation owner explicitly advertises permitted opaque states only
for a Source profile whose installed selector is unrestricted. State/all/any/excluded
labels are bounded, typed and frozen in the existing input version, with absent
profiles retaining the old wire/digest shape. The server validates profile support
and state choices; Source preview and admission apply the same literal normalized
predicate after the verified provider read. It can only narrow the original
installed resource/network/credential scope. This uses no new provider protocol,
private configuration upload, mutable global catalog or independent persistence.
Installed provider-owned mapping and polling ceilings remain unchanged. Legacy
profiles remain selectable without claiming to support editing. Manual Artifact
intake and future card ownership stay independent. The browser consumes exact
server-supplied Repository digests rather than reconstructing configuration hashes.

Task 5.7 uses an independently advertised optional Source `preview` operation,
sharing native listing without admission, cursor or task-observation authority.
The hosted reader shares one original authorization/process deadline and narrows
installed page bounds. Safe diagnostics project existing scan, admission and
accepted reporting journals with dependency/Run visibility before pagination.
Native correlation includes the installed security partition and excludes input
identity; automatic replay remains unchanged. Optional visible task links use a
read-only authoritative owner capability, without creating or refreshing cards.
See [task 5.7 verification](verification-5.7.md) and
[Source diagnostics reference](../../../docs/reference/factory-source-diagnostics.md).

Task 5.8 publishes canonical schema-checked wire examples and the existing
`set_status` parameter contract, with shared authoritative retry/credential bounds.
Generated client checks preserve write-only credentials, native correlation,
optional visible Issue links and independent manual admission. See
[task 5.8 verification](verification-5.8.md) and the
[integration contract reference](../../../docs/reference/integration-management-contracts.md).

Task 5.9 adds dedicated Integrations list/detail navigation, reviewed metadata and
credential administration, and explicit resource/purpose checks. Its approved
prerequisite scope adds visible connection enumeration, a bounded safe installed
catalog and pure draft validation. Catalog enumeration requires unrestricted
administration visibility; ordinary connection enumeration filters before paging.
These opt-in management contracts still require verified protected ingress and
trusted installed declarations. No authentication or live deployment configuration
is inferred by the browser. Secret values stay outside drafts, confirmed intents,
query/mutation caches and browser storage; submission/cancellation/navigation clear
the input. Existing review, SelectMenu and query-state components handle keyboard
access, conflicts and failed background refreshes. Both pages share one optional
lazy bundle. Project grants and pipeline input authoring remain subsequent tasks.
See [task 5.9 verification](verification-5.9.md) for Red/Green evidence and measured
bundle-budget changes.

Task 5.10 adds separate Project selection and reviewed resource/purpose grants,
including explicit full revocation, access-version conflict recovery and filtered
use choices. Dependency review reuses the input journal with a connection-scoped
retained-version mode; ordinary current-definition enumeration is unchanged.
A bounded protected Project/connection endpoint applies dependency visibility
before paging, with exact input identity/version cursors. Existing Source diagnostics
supply visible Run/reporting history on demand, including historical inputs after
revocation, without restoring use authority or requiring a task card. The UI
identifies its Project scope and historical enabled flags rather than presenting
these rows as a global dependency graph or current activation proof. See
[task 5.10 verification](verification-5.10.md).

Use existing Factory configuration/detail patterns for effective selectors, exact
versions, preview, source/reporting progress and visible Work/Run correlations.
Keep drafts/preview data in memory and complete English/Russian and keyboard flows.
The existing Flow editor exposes per-outcome label additions/removals and review of
pinned mappings at intermediate, nested and terminal stages. No action means no
write. Reporting progress stays associated with its originating action independently
of overall Run completion. The task card consumes these same facts when enabled;
this change does not implement its editor, run-history view or synchronization controls.

Distinguish automatic input setup from importing/updating an internal task and from
an explicit card execution request. Pipeline input selectors still select an exact
execution target; they are not the global task synchronization policy. Source preview
remains read-only even with cards enabled: it cannot create an Issue/revision, attach
a source, dispatch reporting or reserve execution. Diagnostics can expose an optional
authorized internal Issue link once the card supplies one; absent/backfilling cards
retain native correlation and existing Work/Run links with no read-time creation.
Manual Artifact intake remains available now; manual Issue creation and execution
belong to the card capability and require no GitHub connection. No credential form
or duplicate task-creation form is added to pipeline setup.

### 10. Preserve task identity separately from Work admission identity

| Identity | Scope and purpose | Includes pipeline input identity? |
| --- | --- | --- |
| Native task correlation | Project/security partition plus trusted installation, native container and item; shared with optional internal Issue association | No |
| Automatic Work admission | Existing source scope, native task and stable input binding; one accepted Work/Run per input | Yes |
| Explicit Issue execution request | Card-owned frozen Issue revision and exact target; deliberate later execution | No; uses a distinct request identity |

Display numbers, names, URLs, labels and credentials are not identity keys. Separate
inputs can process the same external task once each and later attach to one internal
Issue without changing completed admission records. Grouping/backfill and unique
Issue associations remain the card change's responsibility, not another deduplication
database here. Repeated observations or our own label writes cannot create a new
Issue execution request. A second provider-neutral fake consumer must demonstrate
the same handoff without a GitHub-specific branch in Factory.

## Risks / Trade-offs

- [Credentials exposed by management] → Separate write-only administration, protected storage, redacted requests/receipts and UI memory-only inputs; test every read/audit/cache/error path for non-disclosure.
- [Connection permission exceeds Project permission] → Explicit Project/resource/purpose grants checked before enumeration and every protected operation, with no implicit write access.
- [Credential publication fails between storage and database] → Stable operation recovery, inactive preparations, atomic private replacement and bounded orphan cleanup.
- [Revoked access or changed connection redirects pending work] → Current authorization plus retained immutable scope, safe dependency inspection and no fallback to another connection.
- [Own label writes trigger new observations] → Stable binding-scoped admission keys and replay prevent loops; downstream bindings are intentional separate consumers.
- [Filters change while discovery is paginated] → Pinned group/binding revisions, overlap and fair reconciliation; never treat an incomplete scan as complete.
- [Multiple pipelines write the same status group] → Explicit managed groups, durable Issue-level ordering and conflict/convergence evidence; preserve unrelated human labels.
- [External labels change between read and admission/write] → Retained snapshot semantics, explicit conflicts and reconciliation; no claim of distributed atomicity with GitHub.
- [Task text is private or adversarial] → Bounded private Artifacts, explicit mapping and permissions, no implicit resource fetching, safe diagnostic projections and secret-redaction tests.
- [Cross-pipeline identity accidentally disables existing duplicate policy] → Assert that staged inputs reuse original admission/lifecycle contracts and configured eligibility checks rather than accept classification from labels.
- [Provider outage blocks Factory] → Input-scoped deferred progress and independent reporting; ordinary Builds and manual intake remain available.
- [Task refresh accidentally replays execution or splits a card per input] → Separate native correlation, optional observation consumption and input-scoped admission; keep card creation/uniqueness in its owning change and qualify both connected and absent-consumer paths.

## Migration Plan

1. Add new input/progress/correlation state and capability contracts without altering existing manual admission identities, retained Runs or Delivery installation semantics. Test migrations and retained-row compatibility through existing store contracts.
2. Add connection/Project-access metadata and protected publication without starting workers. Explicitly adopt existing installed configurations/handles while retaining their original installation, native resource and input identities; migration must not enable inputs, widen grants or produce duplicate Work. Verify pending projections and ordinary VCS/Delivery/manual behavior.
3. Deploy compatible server/plugin versions with source inputs disabled. Configure or adopt a named connection in Integrations, provision separate purpose credentials and explicit Project/resource grants, perform read-only checks, then select the connection at pipeline inputs. Expose only installed supported choices.
4. Validate and preview sample inputs, then enable a controlled disposable-repository pilot with fixture assessment outputs. Verify first admission, status projection, downstream selection, restart/lost responses and ordinary CI/CD without paid model calls. Live GitHub writes require an explicitly selected test repository and credentials; no user's repository is used implicitly. Hermetic HTTPS/process conformance is required independently and is not called live account qualification.
5. Enable real inputs only after their provider-boundary qualification and operator review. Retain exact source/plugin/configuration evidence and ordinary CI/CD regression results.
6. Roll back by disabling inputs to stop new admissions and pausing the affected Reporter dispatch if required; retain pending intents, cursors, Work and audit. Existing Runs continue under their pinned definitions. Do not drop state or reverse externally applied labels automatically; reconcile later using the retained mapping and newest intent.

Card adoption is an additive follow-up: expose the qualified observation/correlation
seam and optional links without changing Work identities, enabling import/refresh,
or backfilling Issues in this deployment. `add-factory-work-card` owns those migrations
and its own task-only pilot. The integration pilot qualifies the seam with a fake
consumer and reruns with real cards only when that capability is implemented; card
absence is not a blocker for the existing Source/admission/Reporter release path.
