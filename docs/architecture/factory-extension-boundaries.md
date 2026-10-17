# Factory integration follow-ups

Factory admits Work through the management application and executes an immutable
Flow. Optional provider-neutral polling composes the verified Source process with
the same admission journal. GitHub implements read-only discovery; GitLab/Jira
can implement the same boundary. Optional Reporter projection composes the same
verified host with the existing outbox and durable managed-field ordering. An
Octa-local JEV task plugin remains a separate implementation change. The GitHub Delivery capability does
not install any of those capabilities.

## Ownership and authority

| Extension | Reads or effects | Existing boundary to preserve | Authority it does not receive |
| --- | --- | --- | --- |
| Work Source | Poll provider candidates or receive verified provider events; submit bounded Work | Authoritative admission, exact Repository/Configuration versions, scoped external identity and replay | Flow transitions, Job placement, permission grants or candidate acceptance |
| Work Reporter | Project an explicitly configured status, label or resolution to its provider | `FactoryWorkStatusAction`, `FactoryWorkStatusProjector`, independently fenced reporting outbox | Reopening a node, changing a Decision, reconstructing a Run from provider state |
| Provider repository read | Read an allowed repository subject | Exact immutable source, explicit host/credential and content bounds | Delivery, merge or deployment |
| Delivery | Publish the exact accepted candidate and its review metadata | Verified delivery process contract and independent credential class | Ticket discovery, Work admission or automatic promotion |
| Octa-local Decision Signal task | Answer declared questions inside one ordinary Job | Octa task inputs/outputs plus the enclosing signed JobSpec and backend ceiling | Server-side Flow state, another Job, a new edge or an acceptance Decision |

Provider SDKs, pagination tokens, webhook signatures, retry headers, and provider
status names belong in the adapter. The Flow owner consumes typed observations
and declared outcomes; it must not acquire a GitHub issue-state enum, Jira
workflow, provider payload, or special business-stage dispatcher.

`octacity-work-source-protocol` defines independently advertised Source and
Reporter roles and version ranges; `octacity-server-work-source` runs their
closed normalized messages through the existing verified adapter host. A source
range cannot disable a compatible Reporter role or confer Delivery authority.
Native ticket containers are independent of the target code Repository, while
selectors and effect mappings stay behind exact installed references. Neither
protocol can submit a management identity, admit Work or drive Flow transitions.
See the [process contract](../reference/protocols/work-source-reporter-v1.md) for
wire bounds, credential separation, negotiation and cancellation. The existing
GitHub provider validates independent Issue read/write installations through
private `github-work.toml` and scoped grant files, sharing API/private-file
validation helpers with Delivery. It checks exact native container, profile and
network ceilings before resolving a token. This provider-specific configuration
adds no GitHub types to Factory; other providers own their installation formats.
GitHub Source reads one bounded HTTPS page, applies its pinned selector locally
and emits opaque native item identities. Pagination, provider DTOs and TLS/API
configuration remain in that plugin. GitHub Reporter implements pinned managed-label
and optional Issue-state effects behind the same provider-neutral process boundary;
Source activation is an explicit trusted runtime installation.

## Ticket discovery and admission

A future connector can poll its own configured candidate states, labels or
resolutions. Those are provider selection data, not Factory lifecycle states.
Discovery alone accepts no Work and starts no execution. The connector must:

1. Authenticate or verify its provider input and apply bounded pagination,
   payload limits, host policy, polling cadence and retry/backoff.
2. Bind a stable provider/installation/repository/item identity within its own
   authorized source scope. Reserve/replay admission before resolving a mutable
   branch again, including after a lost response or restart.
3. Submit ordinary bounded Work with explicit task/acceptance input provenance,
   exact selected Configuration and Repository versions, and allowed source
   selection. The application resolves the immutable base and retains one Run
   per scoped Work identity under current Project policy.
4. Observe the returned immutable Work/Run identities. An external label or
   webhook acknowledgement cannot stand in for the admission receipt.

`WorkSourceInput` defines Project-owned immutable input versions. Publication
checks exact Configuration and code Repository references against selected
resources and freezes the target Configuration's root Flow. Replacement keeps
the input identity, advances only one version, and retains the previous snapshot.
Inputs default to disabled; activation requires an enabled target Configuration.

The ticket container and code Repository are separate selections: a Jira
project, GitLab repository, GitHub repository or another installed provider can
supply Work for the selected code Repository. The core stores exact installation,
capability, configuration and resource references. Provider selectors, input
mappings and polling settings belong to the referenced adapter configuration;
provider payloads and label/state enums do not enter the Flow interpreter.
An optional Reporter requires its own exact Project-scoped capability for the
same installation and ticket container. These pure publication checks consume
trusted installed declarations; they do not resolve credentials, verify a
provider process, persist a binding or start discovery workers.

`WorkSourceGroup` publishes explicit Project-owned exclusive groups with stable
identities, positive immutable revisions and 1–64 frozen input definitions.
Members must have distinct stable input identities, belong to the same Project
and select the same exact Source installation, capability and ticket container;
their selector configurations and target code Repositories may differ. These
scope checks make one provider observation meaningful to every member without
assuming that the ticket container is a Git repository.

A selection pass evaluates enabled members against one borrowed observation
using the adapter's pinned selector configurations. It yields no disposition
until `finish` verifies the complete frozen group against the resolved current
group. Zero matches are filtered, one returns the exact selected input, and
multiple matches return ambiguous input identities without a selected target.
Other groups remain independent; labels and pipeline names confer no implicit
exclusivity. GitLab/Jira adapters can use their own native observation and
predicate types through the same selection interface. These pure checks neither
reserve admission identities nor change running Work. The store resolves the current
group and commits selection and admission under the existing Project lock and scan
fence. This durable ownership cannot lock concurrent external tracker edits.

`FactoryWorkAdmissionIntent` carries the common normalized admission inputs.
`AdmitManualFactoryWorkCommand` remains its compatible management entry point,
retaining the `manual` source kind, original intent digest, transport keys and
replay-before-resolution order. Both entry points publish through the existing
`FactoryAdmissionStore`; there is no additional admission or duplicate authority.

`FactorySourceAdmissionHandlers` requires an explicit `FactorySourceAuthorizer`
bound to a verified installation. That trusted server port resolves an exact
enabled input, Source capability and accepted installation-owner management
context on every invocation. Ticket data supplies none of those authority facts.
The handler checks the resolved input identity/version, capability role/scope
and exact Project/Configuration/Repository selections, then checks Project and
dependency visibility plus execution permission before probing replay. Revoked
visibility therefore cannot disclose an earlier source admission receipt.

New source admission also checks the exact Configuration digest/root Flow and
Repository pin against authoritative definitions before VCS resolution. The
Repository content pin is SHA-256 of the normalized JSON serialization of its
published `RepositoryDefinition`. Installation/Project-derived source and
transport namespaces isolate source requests from manual identities even when
an installation is named `manual`; callers cannot supply either namespace.
Audit retains the context accepted by the trusted authorizer, rather than an
actor claimed in a ticket. GitHub/GitLab/Jira provider DTOs stay outside this path.

`FactorySourceItem` carries bounded opaque native container/item identities.
The trusted authorizer verifies the native container against the pinned resource
definition; an operator resource reference is not itself a native repository or
tracker project id. Native ids retain their exact spelling and case, independently
of any provider's label matching rules.

The application derives the external admission identity from installation,
native container, native item and stable input identity using a domain-separated,
length-framed digest. Project/installation and accepted security scope partition
the source namespace. Input/configuration versions, labels, task content,
observation times and credentials are absent from the item identity. Source
transport retry keys also include that derived item identity, so independently
configured inputs do not collide on a shared polling transport key.

`FactorySourceAdmissionHandlers::recover` checks current source authority and
Project execution access, then reads the original receipt through
`FactoryAdmissionStore::recover_factory_admission`. Before returning it, it
checks visibility of the retained Work's Configuration, Repository and Artifacts.
Discovery calls this lookup before preparing fresh artifacts or resolving a
mutable repository base. An admitted item therefore keeps its original Work,
Flow, base revision and artifacts after a body/label edit, input replacement or
credential/configuration rotation. Explicitly submitting a changed full intent
under the same identity still conflicts and cannot replace that Work.

Memory and PostgreSQL read the same existing admission authority; recovery
reserves nothing and adds no deduplication table. Concurrent identical observers
converge through atomic publication and strict replay, while conflicting intents
preserve the winner. Both may resolve VCS before one wins; recovery does not
provide ownership of unadmitted preparation. Recovered and replayed receipts
retain the initial admission projection even after Run progress; ordinary Run
queries continue to expose the actual current state.

`FactorySourceStore` atomically publishes bounded input/group version batches
with current-version preconditions, exact stored target/repository validation,
and Project ownership. PostgreSQL migration 0059 adds only input versions, group
versions, scan leases/progress, and private preparation/correlation records.
Existing Work admission identities and retained Runs remain authoritative.

An input or exclusive group scan pins its current definition and receives a
bounded lease with a monotonically increasing fence across definition versions.
Every fresh reservation, intent publication, admission and checkpoint rechecks
the current definition and complete lease in the same transaction. A replacement
invalidates old owners; disabled inputs and groups with no enabled members cannot
acquire a scan. PostgreSQL serializes these short source transactions on the
owning Project row and reuses the transaction connection for dependency/replay
reads, including with a one-connection pool. No provider or VCS I/O belongs in
these transactions.

Preparation reserves the first bounded private snapshot and stable operation
identity before artifact or VCS work. Normalization then freezes the complete
provider-neutral `FactoryWorkAdmissionIntent`. Worker takeover reuses both;
changed observations cannot replace them. A new definition's current owner may
resume the original pinned intent for the same stable selected input and
installed resource. Group selection must be reevaluated under the new group's
current revision; an old group claim cannot publish a disposition. Definition
replacement resets scan progress but retains original preparation provenance.

`admit_prepared_source_work` checks that the submitted Work and probe repeat
that retained intent, then atomically publishes through existing admission and
records its original Work/Run association. Recovery of an admission committed
before correlation attaches the existing receipt under a current fence, without
another Work, idempotency outcome, audit or outbox fact. Private pending pages
support restart; checkpoints reject unfinished preparations in their scan.
Safe bounded Issue associations return original input versions and Work/Run ids
without reading snapshot bytes. Their common Issue identity excludes input
identity, while admission remains scoped to each stable input.

`FactorySourceAdmissionHandlers::admit_prepared` connects that fenced store
operation to ordinary admission. It accepts the current installed input and
original preparation identity, rather than a replacement Work payload. Project
visibility is checked before private preparation lookup; all original selected
dependencies are authorized before replay or VCS resolution. Fresh Work shares
the manual/source Repository and immutable configuration validation path. After
VCS returns, trusted owner time is read again before atomic publication.

Direct and prepared ingress share one canonical source admission probe constructor.
Exact replay retains current authorization and scan ownership checks but needs
no retained private bytes; artifact expiry is a gate only for fresh Work.
Snapshot/mapping/store validation uses shared named source input byte and lifetime
ceilings. Installed settings may narrow those ceilings; ticket data cannot raise them.

Each independently configured input can admit the same native item once to its
own exact target and retain their common Issue correlation. Input replacement
and changed labels recover the original Work/Run and journal. Admission adds no
accepted assessment evidence from labels: the target Flow still executes its
configured eligibility and business duplicate gates. Ordinary manual commands
keep their own namespace and need no installed Source, scan claim or provider.

`FactoryWorkSourcePreparer` maps the first retained normalized snapshot into
immutable private task and acceptance documents. Its mapping is pinned to the
exact installed Source binding and selects an explicit text field, entire body
or named body section for acceptance. Missing required data and byte ceilings
produce safe classified dispositions; text is never truncated or used to choose
credentials, repository targets, profiles, priority or Flow transitions. Labels
and update identities remain provenance. There is no comment, attachment or
linked-resource fetch capability in this mapper.

Source materialization reserves exact object identities and cleanup metadata
before byte I/O. Fenced publication exposes the complete verified pair; restart
keeps the first snapshot, mapping digest, object identities, publication time
and admission intent. Inputs are owned by the Project/source preparation and
have no fabricated producing Build or Agent. Explicit context reads and protected
Agent input transfers accept these private publications, while ordinary output
and evidence reads still require ordinary Build publication. Context readers
receive only `SourceArtifactQueryStore`, without scan or admission methods.

Private snapshots have a hard 30-day bound. The configured materialization
lifetime (at most 30 days) can shorten it and never renews on retry. Fresh
admission and context reads reject expired inputs. The bounded
`FactorySourceInputRetentionWorker` deletes reserved physical objects before
clearing snapshot bytes, including unpublished writes and rejected/raw orphans.
Initial expiry and due late-write reconciliation are separate indexed selections.
Cleanup isolates per-preparation failures and returns both the failure count and
page cursor; a failed object cannot block the rest of a sweep. Raw/rejected
preparations with no objects leave the queue after cleanup. Byte reservations
keep a configurable recheck interval that grows with time since their first
cleanup, preserving late-write protection without visiting all historical rows
every sweep. Duplicate acknowledgements do not renew that schedule or rewrite
the frozen preparation. Correlation and immutable provenance metadata survive.
Worker composition must install the trusted recheck interval, observe failures
and wrap the cursor when a sweep ends so failed records are retried.

The installed authorization port has no default implementation and is not
exposed as a management REST command or provider process message. Concrete
process/grant verification and selection reevaluation are composed by the optional
Source worker. It uses the prepared admission seam; diagnostic readers still must
authorize access before exposing retained source state.
The existing manual endpoint is not a ticket poller.

## Bounded source polling

`FactorySourcePoller` runs one finite application slice with an injected clock,
trusted item/page/byte budgets and two provider-neutral ports: a normalized page
reader and a candidate processor. The processor uses existing fenced selection,
preparation and admission; success means durable admission/replay or a classified
disposition. The poller has no second duplicate registry. Concrete verified
process composition, group selection, scheduling, backoff and shutdown belong to
the worker integration, not to this polling state machine.

A complete bounded page is frozen in the existing private scan record before
candidate effects. Checkpoints remove only settled candidates; the covered opaque
continuation advances after the last one. A partial failure or lease takeover
resumes the original snapshots. Candidate retries retain the same page deadline;
the existing retention worker also clears expired pages, including disabled
inputs. Expiry resets progress for a conservative rescan. Legacy checkpoints
without an original pass context also restart from epoch. Existing admission
identities make those repeated observations safe.

Incremental windows are inclusive and overlap the last completed boundary.
A periodic full pass alternates pages with incremental discovery under the same
finite budgets. Each lane retains its own scan identity, window and continuation;
updated timestamps never stand in for admission identity. Budgets count canonical
normalized JSON bytes; the verified process/provider separately bounds raw wire
bytes. No private snapshot or opaque cursor is a diagnostic. Manual management
admission continues independently of these optional source ports.

## Optional Source worker composition

`FactorySourceWorker` binds the application poller to the verified Work adapter
registry and existing source/admission/Artifact journals. Exact installed profiles
supply read handles, provider-owned selectors, mapping, Project attribution and
source policy. No GitHub SDK, label vocabulary or native state enters Factory.
For exclusive groups, every enabled member is resolved at its frozen version;
all selectors share one observation, and the strictest member budgets apply.
Zero/multiple matches retain safe filtered/ambiguous selection without admission.

Discovery lists only current enabled standalone inputs or exact current groups;
group members are never scheduled independently, including obsolete groups. A
bounded page cursor visits the next target/Project on later passes, while the
existing durable scan claim prevents duplicate owners. Batch/concurrency settings
bound each pass. Retries retain safe per-target class, attempt count, earliest
retry and exhaustion in the existing scan record. Source uses the explicitly
installed `FactorySourceWorkerSettings.max_attempts` as its positive per-target
failure budget, independently of the supervisor's consecutive-failure circuit
threshold; success resets it and an
immutable definition replacement resets exhausted state. Provider retry lower
bounds are never shortened to the local exponential backoff ceiling. Classified
invalid/protocol/conflict results require definition repair. Cancellation retains
the budget rather than spending an attempt. Ordinary Factory WIP backpressure
also retains the prepared item and retry budget; capacity does not disable a
Source or republish its immutable inputs. Expired preparations wait for the
independent retention sweep without spending attempts; cleaned/rejected originals
are settled as retained dispositions, never remapped or renewed from a new poll.

Pending original preparations are settled in bounded pages before new discovery.
Recovery and checkpointing consider the target's current enabled input identities.
Disabled or removed members cannot block the remaining members; their original
preparations stay private until ordinary retention. Joining or leaving a group
does not change preparation identity or the original scan provenance. Recovery
requires membership under the current claim and the same installed resource;
an old owner or an unrelated group cannot resume those preparations.
A frozen intent is reused across replacement/restart; an unfinished mapping needs
its original exact installed profile. Each admission/replay checks current input,
Project/group fence, verified executable/version/profile and current private read
grant through Source `authorize`; pending unmapped snapshots receive that check
before byte publication too. Ordinary admission independently verifies management
permissions, exact target/repository policy, WIP and duplicate identity. Whole
slice deadlines reserve cancellation/settlement time inside the claim window;
provider operations retain their own original process deadlines and tree cleanup.

Trusted deployments build a Source with `FactoryRuntimeResources::source_worker`
and attach it using `FactoryProfileRuntimeInstaller::with_work_sources`, or the
existing `FactoryRuntimeInstallation.work_sources` extension. Absence is the
normal default. The supervisor runs Source independently; provider availability
is excluded from Factory/manual admission readiness. Provider-specific installer
formats and Console activation are subsequent tasks. Manual Factory input and
ordinary CI/CD require no Source installation, provider token or provider call.

Private input/page retention is always scheduled, even without Source polling or
with Factory disabled. It uses a bounded wrapping sweep and the existing retention
cadence for late-write checks; cleanup resolves no tracker grant or provider.

## Configured status projection

`set_status` carries an arbitrary bounded configured value. Existing contracts
exercise `needs-human-eyes`, `cannot_reproduce`, and `Ожидает уточнения`; their
spelling selects no execution behavior. A GitHub adapter could map the value
to a label, a GitLab adapter to a configured label/state operation, and a Jira
adapter to a permitted native transition/resolution. Each adapter must validate
its own closed parameter and capability contract before any provider write.

Only a configured trusted action requests that projection. Research output,
comments, provider state changes, and reporter success do not select a Flow
edge. Optional reporting accepts a durable intent in the same transaction as
the authoritative node result. Its independent worker uses the original
operation identity, frozen parameters, permission ceiling, deadline and retry
budget. A lost acknowledgement is observed or replayed idempotently; retry,
failure, or exhaustion cannot roll back or reopen the accepted node.

`HostedFactoryWorkStatusProjector` binds this port to the verified Work Reporter
host. `FactoryReporterProfile::new` content-pins the complete operator selection:
Project, installation/capability/resource, executable digest, provider configuration,
mapping, separate write handle/profile and required network hosts. Its exact
binding is published as the input's optional Reporter. Token bytes never enter
this profile, Work or the application. Retain original installed profile versions
for pending projections; a replacement cannot silently substitute one.

`source_preparation_for_work` reads the original settled Project/Work association
from the existing source journal in memory and PostgreSQL. It returns no association
for manual Work and never reads the current input version. The retained normalized
snapshot supplies the original display key; missing private data fails closed
rather than refetching mutable ticket content. Exact native resource/item identity
and admitted Work context are checked before the mapping or grant reaches a process.

`validate_accepted_factory_status_request` reconstructs only the accepted configured
`set_status` request from the current claimed outbox row and immutable Flow record.
The request carries its current outbox claim independently of the stable operation
id. Pending/terminal rows, another claim, plugin, status, subject, permission set,
budget or deadline cannot authorize a dispatch. The hosted projector requires the
verified executable, network and secret-profile ceilings and rechecks accepted
authority before projection. It always observes before requesting missing effects;
the original operation, native item, provider profile and mapping are unchanged.

Metadata resolution and every inspect/apply step share one finite operation window,
narrowed by the original action/claim deadline and budget. Provider retry hints are
lower bounds on the existing outbox retry policy, never a new attempt or deadline
authority. Reporting failures remain local; they cannot reopen accepted Flow data
or require a provider for manual admission. Synchronous action cancellation is
not installed by this outbox-only projector; the verified host owns operation
timeout/cancellation and process-tree cleanup.

The trusted installer can explicitly construct the dispatcher with
`FactoryRuntimeResources::work_reporter_dispatcher` and supply it through
`FactoryRuntimeInstallation.outbox_dispatchers`. Construction performs no database,
credential or provider I/O. An absent installation starts no Reporter worker.
Controlled recovery tests cover missing/malformed Project responses, native HTTPS
response loss after partial effects, and invalid final read-back. These uncertain
results remain retryable within the original policy; a retry first observes the
same intent. Invalid observations grant no write authority. Claim takeover during
observation blocks the old owner's projection; takeover after apply or worker loss
recovers by observation alone. Even exhaustion leaves accepted Work/Run/Flow
records unchanged and never compensates external effects by rolling them back.
Reporter v2 exposes opaque field digests and conditional single-write steps. The
host never interprets native labels or states and refuses a version-1 downgrade.
Each step applies a commuting subset of the frozen effects; nonconvergent native
workflow transitions cannot use this contract. Current authority and the field
order are checked before each mutation, rather than once before a bulk write.

Task 4.4 adds a per-native-item sequence assigned atomically with accepted status
publication in memory and PostgreSQL (migration 0063). Its technical bucket is
Project plus a digest of opaque native container/item ids, shared across input
bindings and installations. Admission and source correlation retain their
installation-scoped identities. Per-operation immutable field metadata,
latest per-field heads/generations and unresolved write permits remain technical
ordering facts; accepted Flow records and the existing outbox retain authority.
Unknown later scopes conservatively block older writes, including coincident
opaque ids from different provider instances. Once their adapter-owned metadata
is known, disjoint fields remain independent. Bounded journals retain at most 4096 heads and 128 unresolved
permits per item. Hitting a bound reports conflict and grants no further write.

A native observation is fenced on both sides by per-field revisions. Success
requires stable matching effects, current ownership and known completion of
older overlapping writes. Late old effects trigger newest-state reconciliation
within the original deadline/output/retry bounds. An expired client deadline
cannot prove remote completion: unresolved foreign writes remain conflicts,
while known late completion may release only its original permit. Valid typed
Conflict, Permanent, InvalidRequest and Unsupported responses release that
permit; missing, invalid, cancelled or transient completion retains the hazard.
Continuous
human edits exhaust bounded reporting without changing Work/Run/Flow or replacing
unrelated labels. GitHub uses native label-id and name-alias digests so renames
and same-name recreation preserve overlap. Digests include the canonical API
instance, native container and item, avoiding overlap between unrelated forges
with coincident numeric ids while sharing fields across installations. Other
providers supply their own
opaque overlapping field identities. Native multi-request writes remain non-atomic.

Migration 0063 bootstraps retained legacy accepted intents deterministically by
their retained timestamps/digests; historical commit order cannot be recovered.
Only the exact accepted source Reporter capability participates; unrelated
configured status adapters are excluded. The application and stores share one
accepted-action provenance check, including nested immutable definitions.
Every new acceptance uses the locked counter. No Work or Flow history is rewritten.
Tasks 4.2–4.5 are implemented/qualified. Stage-local reporting uses ordinary graph
routes: a declared success, escalation or typed failure can select a trusted
`set_status` action before the enclosing Flow finishes. Escalation is an arbitrary
outcome key, not another primitive. Stage or terminal completion without a selected
action queues nothing, including outcomes named `approved`.

The outbox's `input_digest` identifies its exact accepted action record. That
record retains the immutable definition, Flow Run, cycle and attempt. Explicit
predecessor projections retain the source record digest/attempt and its accepted
outcome; nested Flow parent-call identities disambiguate reused node keys. Reporting
retry changes its independent claim, while preserving that accepted action and
original operation. Neither successful delivery nor reporting failure advances or
rewrites Flow. Qualification covers an update while a child Flow has not returned,
then ordinary parent completion after reporting retry and owner reconstruction.
Native labels/state remain behind the pinned mapping, and the shared graph/action
contracts remain provider-neutral. Integrations administration, pipeline authoring
and operator-facing activation retain their pending section-5 tasks.

The GitHub implementation should extend the single `octacity-github` provider
plugin with separately advertised Source, repository-read, Reporter and Delivery
capabilities. Share installation and provider transport where appropriate;
retain separate process contracts, credentials, grants and conformance tests.
Do not create a plugin per Flow phase or make Delivery credentials available
to ticket reads or status projection by default.

## An Octa-local JEV task

The installed server-side JEV adapter implements the provider-neutral Decision
Signal port. It is separate from a future Octa task plugin. A local plugin may
produce only the bounded outputs of its declared task inside one Job; ordinary
Octa DAG dependencies can consume those outputs within that Job. It cannot
hold a Factory claim, publish a Node Attempt, alter an admitted Flow, add another
Job, skip a mandatory gate or declare a candidate accepted.

Any later Factory consumption still passes through the exact persisted input,
evidence, policy and fenced owner boundaries. Never give a local task the
server's provider credential merely to reuse transport code. Its credential
class, explicit network grant, data handling, deadlines, cancellation and
output limits require separate qualification. The existing server-side
pre-execution tool-risk broker remains a blocking authorization boundary, not
an advisory call that a local plugin may bypass.

## Acceptance gates for each follow-up

A concrete source/reporter change must supply provider-wire conformance,
visibility and scope tests, bounded polling/webhook verification, duplicate and
lost-response recovery, restart/fencing, arbitrary configured status mapping,
permission/credential separation, private-data redaction, and ordinary CI/CD
regressions. Retain a released provider-boundary pilot before enabling it.

A later Decision Signal or Decisions API adapter must additionally pass the
same exact-provider/model capability, finite-question, calibration, privacy,
replay, fail-closed and negative-security gates as JEV. Structurally similar
probabilities do not inherit another provider's calibration or rollout approval.
An Octa-local plugin also needs a released task/Job isolation pilot proving it
cannot become Factory lifecycle authority. None of these follow-ups is
qualified merely by publishing this boundary document.

The existing application contracts in `factory_admission_tests`,
`factory_flow_action_tests`, and `factory_decision_signal_tests` establish the
shared admission, arbitrary status, reporting recovery and non-authoritative
signal seams. See [server architecture](server.md),
[management REST](../reference/management-rest-v1.md#optional-factory-management),
and [JobSpec v3](../reference/protocols/signed-job-spec-v3.md) for the current
released-product interfaces.
