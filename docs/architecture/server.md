# Server vocabulary, ownership, and seams

This document maps the canonical [OctaCity coordination language](../../CONTEXT.md)
to crate ownership and public interfaces. It describes the modular-monolith
shape; it is not a database schema or a REST contract.

## Coordination flow

```text
Trigger Occurrence -> Trigger Engine -> Build -> Attempt -> materialized Jobs
                                                        |
persisted Job outcomes -> Orchestrator -> newly Ready Jobs
                                              |
                         Placement Scheduler -> fenced Lease -> Agent Executor
```

The roles are deliberately separate:

- the Trigger Engine decides whether one normalized occurrence creates a Build;
- the Orchestrator advances a persisted DAG and derives Attempt and Build state;
- the Placement Scheduler sees only already Ready Jobs and compatible Agents;
- the Executor exists on the Agent and runs one verified `JobSpec`;
- a Pipeline is an immutable definition, while an Attempt is its materialized execution graph.

The server never executes repository-controlled build code. Scheduling a
Trigger, progressing a DAG, placing a Ready Job, and executing a Job are four
different responsibilities.

## Dependency direction

```text
API adapters -> application commands and queries -> core modules and ports
composition root -> API + application + selected infrastructure adapters
infrastructure adapters -> core ports and provider-neutral protocols
agent and CLI -> shared cross-product contracts only
```

Only `octacity-server` selects concrete adapters. Core modules do not depend on
Axum, SQLx, S3 SDKs, Git implementations, provider SDKs, or process
configuration. Architecture checks enforce these directions from Cargo
metadata for normal, build, and development dependencies. Test-only integration
crates belong under `server/tests`; a layer-local dev-dependency cannot bypass
the production dependency rules. API crates may use HTTP libraries, but they
may not select persistence, object-store, VCS, or provider implementations.
External dependencies in API, application, core, shared, and protocol crates
use explicit per-layer allowlists. An unknown SDK is rejected by default, so
enforcement does not depend on recognizing every database, broker, VCS, or
provider crate name.

The Factory core has an additional guard: provider-specific types, persistence
rows, HTTP DTOs, UI state, and Codex protocol contracts cannot enter it. The
existing Build, Attempt, Job, Lease, Pipeline, orchestration, and placement
cores remain independent of Factory, while Factory lifecycle and decision
invariants stay together in the single `octacity-server-factory` module. That
module also owns the narrow provider-neutral Decision Signal port and its pure
routing/tool-risk consumption policy; provider discovery, credentials, wire
formats, retries, and concrete adapters remain outside the core.

### Configured node contracts and graph ownership

`FlowDefinitionTemplate` is the editable representation for supplied examples and
profile-driven instantiation. Management REST authoring uses
`FactoryFlowDefinitionDraft`: its `FactoryFlowRootDraft` carries the same typed
node, transition, terminal, context and data grammar, while the server owns root
identity/version/digest publication and resolves exact published subflows.
Deserialization preserves node constructor invariants, including unique finite
outcomes and the correspondence between `subflow_call` and its exact child.

`FactoryFlowConfiguration` holds the reachable immutable closure, execution
limits, exact data schema catalogue and arbitrary named pool settings.
Publication checks technical references against their selectable catalogue
categories. Plugin/tool permission grants require `node_capability`; external
gate policies and verification data-handling policies require `execution_policy`;
Build model selections require `node_capability` or `decision_signal_model`.
Credential references retain their own categories, and Build bindings require
an exact visible Project-owned Build Configuration. Arbitrary extension parameter
objects remain data owned by their exact plugin/policy contract: their JSON field
names never become reference or permission authority. The selected adapter
validates its parameter contract before effects. Replacing configuration affects
future admissions; admitted Work retains its complete closure.

Configuration validation returns the resolved document plus safe field diagnostics,
required node/Agent/backend contracts, and the intersection of Flow, definition
and node permissions. Backend enforcement includes denied categories. Agent-local
grants are resolved at dispatch and can narrow those ceilings again; previews
report that boundary explicitly. Rejected publication exposes only field pointers
and stable error codes, without hidden choice identities or provider messages.

`FlowDataSchema` describes closed objects, bounded strings, integers, booleans,
arrays and scalar enums. Unknown fields, deep or oversized values and schema
substitution fail closed. `FlowNodeInput` binds the complete Work identity,
subject, configuration, definition, Flow Run, Workflow Cycle, projected payload,
explicit Context Manifest, retrieval receipts and source-record digests.
Sources can be immutable Work, schema-bound incoming data, an exact accepted
record or the latest declared predecessor's payload or verified metadata.
Control edges grant no implicit context or data access. A newer pending or
incorrectly routed producer blocks preparation from an older result.

`FlowNodeRecord` retains the exact frozen input, finite outcome, schema-checked
payload, trusted producer and observation time. Providers cannot deserialize
these authority-bearing records. The journal restores historical inputs
against their original source snapshot; appending a new input checks current
history. Schema or execution metadata cannot replace independently verified
facts. Node names have no runtime semantics, and architecture checks reject
phase-specific execution types and phase-name dispatch.

`FactoryNodeRunner` performs one fenced append per pass. It commits input and
execution intent before dispatch, observes only that persisted operation on
restart, charges attempts once and atomically retains result, completion,
usage and provenance. `FactoryFlowRunner` selects the next configured node or
terminal through the pure interpreter. Each pass starts, observes or publishes
readiness for one node. Bounded repeats create new attempts and preserve prior
records. Each node generation freezes a distinct input and acquires its own
pool selection and capacity reservation, even for identical payloads.

A configured `starts_cycle` edge records a bounded append-only cycle boundary
with the exact triggering attempt. The interpreter derives its entry from that
admitted edge; the journal retains the previous cycle, results and contracts for
audit. Only the newest cycle and descendants of current parent calls retain
execution authority. A new entry can explicitly select its triggering predecessor
result, while other record selections stay within the new cycle. Restart and
pool readiness use this same interpreter rather than assuming a fixed entry.

The pure interpreter also validates nested definition closures and explicit
subflow return contracts. A call attempt and one child Flow/cycle are committed
atomically; the call suspends while the child owns executable WIP. Child nodes
explicitly select frozen caller data, and the parent returns only the exact
retained child terminal result. The journal validates both caller-input and
child-result provenance after restart. The optional server owner uses this same
runtime and the existing durable worker supervisor. Deployment installs the
required technical capabilities explicitly; the supplied manual-intake contract
exercises ordinary Builds, built-in gates and named pools.

Factory reconciliation reads bounded execution state and only the current
ownership proof. Complete claim and secret-safe audit histories use the paged
Factory diagnostic port. Control reads validate their own canonical audit proofs
under the Run lock; unrelated archived claims or audits cannot exhaust the
working snapshot ceiling or disable cancellation.

### Optional Factory runtime installation

Absence of `[factory]` keeps ordinary CI/CD independent of Factory capabilities.
Enabling the common owner requires four explicit worker settings:

```toml
[factory]
poll_interval_milliseconds = 1000
claim_lifetime_milliseconds = 30000
batch_size = 16
concurrency = 4

[factory.retry]
max_backoff_milliseconds = 5000
consecutive_failure_limit = 3
circuit_open_milliseconds = 30000
```

Intervals must be positive and at most five minutes, the batch must fit the
store's reconciliation bound, and concurrency must be positive and no greater
than the batch. These are technical resource bounds, not logical flow defaults.
The optional retry policy bounds exponential delays and opens a circuit after
the configured consecutive failures. One recovery probe runs after the cooldown;
success resets the ordinary poll interval. Delay and cooldown must be at least
the poll interval and at most five minutes; the failure limit is 1–100. Omitting
the retry section uses a three-failure circuit with the larger of the claim
window and poll interval as its maximum delay and cooldown. Each installed
outbox dispatcher has independent retry state. Cancellation interrupts a
cooldown immediately and drains an already running owner pass.

`ServerRuntime::start_with_factory` accepts a trusted `FactoryRuntimeInstaller`.
The installer receives runtime-owned PostgreSQL journals, ordinary Build storage,
retained object storage and the approved toolchain snapshot. It supplies exact
immutable executor bindings, Project-scoped capability availability, a trusted
input preparer and dependency readiness. Duplicate executor bindings fail startup;
logical node names never select an executor. Installation is bounded by the
configured readiness timeout. Provider-specific context selection, protected task
compilation and output verification belong to the installed adapters.

`FactoryProfileRuntimeInstaller` composes native retained-Artifact input preparation,
qualified Codex compilers/output readers, routing Signal journals and other exact
installed technical executors. It groups Codex profiles by executable identity
and rejects duplicate executor selections before exposing a capability catalog.
Choices are Project-scoped: foreign Build Configuration references and profiles
missing their exact installed choice are rejected. Unknown Projects report Factory
disabled. Profile construction performs no provider or storage I/O; dependency
availability remains a separately supervised readiness check.
Build-backed nodes without selected Artifact context receive an explicit empty
manifest. This grants no additional data and makes data-only execution compatible
with the same frozen input contract. Manifest identities are derived from exact
input, subject, construction policy and selected entries; repeated preparation
therefore produces identical bytes, and restart uses the retained input.

`ServerRuntime::start`, including the default binary, can load an explicitly
configured top-level `factory_profiles_file`. The strict version-1 JSON catalog
contains Project-owned `projects` choices and exact `codex` profiles. It is limited
to 4 MiB and must be a stable owner-only regular file; parse and filesystem errors
produce fixed diagnostics without private instructions or file paths. Offline
configuration rebasing also relocates this path. Codex entries distinguish
read-only evaluator profiles from coding profiles with an explicit trusted capture
policy; their credential purposes and source authority cannot be exchanged.
Native readiness checks installed Projects and retained object storage
independently of ordinary CI/CD readiness.

`FileProfileInstaller` preserves separately typed deployment bindings until native
resources are available. `review_planning` selects exact immutable review policies;
`protected_validation_evidence` reconstructs the original retained inputs and
publishes verified evidence through the existing native adapter. `delivery`
installs a verified process registry and private Artifact materializer, with exact
content-derived target policies. Neither installation nor discovery performs a
publication. Provider configuration and credentials remain outside Agent tasks.
The public metadata-only `FactoryRuntimeProfileCatalog::parse_json` refuses a
catalog containing deferred resource bindings, so callers cannot silently drop
an installed capability. Binary startup uses the complete file installer.

Installed command programs use the ordinary managed Job path independently of
model execution. Their exact Octafile bytes, finite task selection, protected
input destination, output contracts, permission ceilings and candidate capture
pin participate in the task digest. The compiler publishes canonical frozen
input JSON and the installed program as separate protected documents. Flow
data cannot replace executable instructions. A command profile's `executor`
identifies the installed native capability; it is not a claim that a provider
external executable exists on an Agent. Built-in plugin programs carry an
opaque `FactoryManagedCommandProof` binding the exact intent and published
program. The common assembler retains the external-executable requirement
when that proof is absent. This changes neither Octa nor the Agent protocol.

Native command qualification requires the approved plugin digest, readonly
source/protected roots, explicit scratch/output authority and no model,
network, secret, external-tool or workload-identity grants. The same immutable
profile drives retained-result verification. JSON schemas come from the
installed output contract and admitted schema catalog, never from a report's
own provenance fields. Ordinary reports may use the Agent's binary transport
media type; their verified bytes must still satisfy the installed JSON schema.
This does not permit binary Artifact results to bypass the JSON contract.
Command consumption uses a conservative authoritative
server wall-time bound, including queueing and cleanup, summed across retries;
it does not treat a periodic Agent metric as a final sample or invent model
tokens/cost. Existing promotion and protected-validation adapters apply their
additional exact-candidate, retained-test and observation contracts around the
ordinary Build ports.
Retained candidate results reconstruct the original opaque source proof and
require the exact complete signed input manifest. Compiler-supplied candidate
inputs are reused only when every field matches the independently verified
selection; conflicting metadata still fails validation. Cancellation prevents
semantic completion while preserving consumption already recorded by ordinary
Job timestamps. Recovering that consumption grants no new Artifact access.
Protected validation uses the same original input source for compilation and
observation. A third protected document supplies the exact candidate identity,
test inventory, report input digest and ordered class keys. Those facts cannot
be reconstructed from arbitrary Flow payloads or inferred from private test
contents. It carries no test bodies. The compiler and observer both use the
typed materialization to verify the complete candidate, private inputs and
compiler document set; neither removes unknown inputs to make a manifest fit.
The ordinary Build service also compares action data against the installed
compiler's deterministic `action_input` projection before dispatch or replay.
The default projection is absent. Request JSON cannot introduce an action
capability or replace a trusted plan; a mismatch fails before Build recovery
or new materialization. This port performs no provider I/O and resolves no
credentials.
Promotion command bindings explicitly select a separate compiler/observer
role. It publishes the independently frozen plan and digest in a third
protected document and verifies that document together with the retained
candidate inputs. A plain command observer rejects those extra inputs. This
role is never inferred from a generic trusted-action node and cannot be
combined with private test validation. The same frozen plan supplies the
compiler's action projection; the common Build service contains no promotion
or provider-specific request logic.

Enabled configuration without this file or an explicit installer is rejected
before loading resources or opening listeners. No providers or business Flow are
implicitly supplied. A deployment must install the capabilities required by its
published definitions. Provider API shapes remain inside their adapters behind
`DecisionSignalProvider`; a file catalog does not select a provider-specific API
inside the common Flow interpreter.
`parse_json_with_signal_providers` can bind routing metadata to already installed
connectors with exactly matching immutable capabilities. Missing or ambiguous
connectors fail closed. This parser performs no provider I/O and accepts no
provider API URL, key, HTTP payload or SDK configuration.
An unavailable installation or installation timeout keeps ordinary server startup
working and reports Factory unavailable, without starting a partially installed
owner. Invalid installation/configuration still fails startup. Installing missing
material in this initial-degradation state currently requires a server restart.
Disabled mode neither invokes installation nor probes its dependency readiness.
Enabled mode tracks both the supervised owner's health and installed capability
readiness, including every installed outbox dispatcher's health. The
`/health/factory` endpoint reports `disabled` (200), `ready` (200), or
`unavailable` (503). Factory dependency failures leave ordinary `/health/ready`
independent; failures of shared server dependencies still make Factory
unavailable. Critical conditions for the two readiness scopes have disjoint
identities, even when a dependency name is shared.

Owner transition audit facts are committed with the journal, budget and outbox
under the current fence. Their request digest binds the complete accepted
append, frozen contexts, execution provenance and budget usage. Structured
transition events are emitted only after successful persistence and correlate
to that audit fact. They contain UUIDs, immutable digests, versions and numeric
usage. Operator-defined node names are hashed; context bodies, prompts, findings,
commands, paths and provider diagnostics are excluded. Worker failures record
only a closed classification, retry delay and circuit state. Native pool selection
events are emitted after commit and bind the exact pool policy, selected input,
claim fence and same-transaction audit identity. Replay emits no new selection
event; pool names, node names and worker aliases are excluded. Node-start events
correlate the committed attempt UUID, number, primitive, original deadline, fence
and hard budget limits. Committed node completions include hashed outcome/output
identities, latency from the persisted start, and measured usage; they never log
the attempt, input or provider result as a debug object.

### Ordinary Build nodes and extensible actions

`FlowBuildIntent` freezes a command or reasoning node's selected prompt/task,
model or executable, plugin, Build Configuration, result schema/output,
evidence requirements, effective permissions, budget, deadline, complete input,
context and immutable Work priority. Its stable operation identity includes
the Node Attempt; a restart cannot rediscover or replace those choices.
`FactoryOrdinaryBuildService` loads that persisted intent and invokes an explicitly
installed `FactoryBuildMaterializer`. The Codex compilation boundary
`compile_codex_node_task` verifies the envelope against the intent, including the
exact task Artifact selected in its frozen Context, before producing protected
documents. `compile_codex_flow_task` separately compiles read-only observations
from the admitted node's exact configured schema, frozen input and installed
instruction digest, without a business-stage enum. It selects one authorized
review credential and rejects writable source, arbitrary tools, capture authority
and substituted instructions. JSON Schema text lengths conservatively approximate
UTF-8 byte bounds; authoritative Flow payload validation still enforces the exact
byte contract. `compile_codex_flow_task_with_context` additionally requires the
complete explicit Artifact selection, verifies content identities and renders
bounded UTF-8 context as JSON data. Missing, substituted, non-text or undeclared
bytes fail closed. `FactoryArtifactReader` establishes Project ownership and
retention through the original execution deadline before supplying those bytes.
Neither compiler creates a Build. `CompiledCodexTask::managed_program` binds its generated documents
to the exact published manifest before passing an ordinary managed program to
DAG assembly. Document debug output contains only kind, size and digest.
Ordinary acceptance independently checks managed v3 Job causality,
source revision, permissions, allowed pools and the original execution deadline.
Admission and observation use separate fenced owner passes. After a successful
creation, the adapter validates and retains the exact acceptance receipt and
returns `Waiting`. The next pass reads the Build with its fresh observation time;
compiler and persistence I/O cannot make the accepted timestamp appear to come
from the future. Existing Build observation still rejects future timestamps,
foreign origins and substituted configuration or permission pins.
The atomic store transaction rechecks current ownership, cancellation and time
after compilation. Creation replays retain the original Build without renewed
authority or a fabricated Trigger.
Acceptance records `factory.build.accepted` in the same transaction as the Build,
first Attempt, complete Job DAG and signed root queue entries, before later
owner linkage. The fact binds the immutable intent operation, worker identity
digest and original acceptance time. Restart and lost-response replay retain
that fact without duplicating it or renewing dispatch authority.

`StoreBackedBuildExecutionContext` loads exact Configuration, Repository and
Pipeline versions plus inherited Project policy through the existing ordinary
read ports. Manual Triggers share this loader. Installed managed task compilers
use `materialize_factory_managed_build` to assemble the complete published DAG
from explicit generated programs. It shares runtime policy and placement
derivation with ordinary CI/CD, derives stable Build/Attempt/Job identities from
the persisted operation, preserves dependency policies and freezes retention,
permissions and the node deadline. Missing programs, ungranted tool/plugin
identities and substituted plugin digests fail before signing. Managed programs
own their generated execution variables and credential selection; ordinary Build
parameters and legacy secret files are not implicitly forwarded. Capture and
candidate materialization require an explicit verified source plan.
`materialize_factory_managed_build_with_source` accepts an opaque candidate proof
bound to the frozen input and rechecks retained bundle/manifest identities at
acceptance. Every signed Job preserves the Work base revision and receives the
exact candidate inputs. Capture validates the frozen write scope and deliverable
contract before signing; it grants no broader paths. The original readonly
wrapper preserves its input manifest and deterministic identities. These pure
assembly capabilities do not qualify a writable backend or install a tool broker.

`FactoryCodexBuildMaterializer` composes exact configuration resolution, the
trusted context reader, read-only compilation and fenced input publication with
ordinary Build assembly. Its installed profile contains only exact executable,
plugin, model and Build Configuration references, instructions, credential
purpose, parameters and independent Project/backend permission ceilings. Flow
identifiers and logical node names do not select installed instructions. The
current qualification supports one explicitly configured Pipeline Job; other
DAGs and writable/capture profiles fail closed. Permission narrowing and the
instruction digest are checked before reading private context, and fresh owner
time is checked before reads and again before returning materialization. The
ordinary acceptance service supplies the persisted admitted closure and still
performs the final atomic fence/cancellation/deadline check.
The materializer accepts a bounded catalog selected by exact tool/plugin/model,
Build Configuration and instruction digest. It does not select a role by Flow or
node name. `FactoryRuntimeResources::codex_executor` qualifies the package and
credential/permission contract before wiring native context resolution, S3
publication, ordinary Build creation/query/cancellation and the native retained
output reader into one registry binding for the exact executable. This
binding constructor performs no dependency I/O; readiness remains separately
qualified. Writable/capture compilers and deployment
profile loading are separate required installation components.

`FactoryCodexBuildOutputSource` reconstructs the exact persisted common intent
from ordinary Build origin. It matches installed identities and narrowed grants,
then compares the generated program and complete protected manifest with the
executed Job. Output lookup filters the exact Attempt before the byte/read ceiling.
The requested terminal Attempt must still be the latest ordinary Build Attempt;
retained predecessor results cannot complete a node after a retry becomes current.
The adapter verifies immutable bytes, trace structure, invocation provenance,
schema and reported consumption; only declared structured data enters the common
result. Private harness records stay in ordinary Build Artifacts. A technical
harness or ordinary execution failure retains verified consumption without a
semantic node outcome. Missing measurement remains unavailable, never zero.


Installed output verifiers may return `FlowVerifiedBuildData` for a schema-checked
projection while retaining the original Artifact bytes and identity. The common
Build adapter consumes this neutral contract and never guesses a provider's wire
format. The Codex adapter alone decodes its versioned result wrapper; harness
identifiers do not enter the projected node payload. The owner checks the declared
schema and retains the raw Artifact digest plus the validated facts in provenance;
the primary projected facts must equal the accepted node payload.

`FactoryExecutionInputPublisher` publishes only explicit trusted compiler
documents. It validates the complete protected manifest, derives stable opaque
identities from the node operation and exact document content, and reserves
cleanup metadata before writing bytes. Reservation and publication both verify
the persisted intent, current owner, cancellation and original deadline.
Interrupted writes remain discoverable for cleanup; verified publication replays
create no new objects. These private inputs have node provenance and never appear
as Agent-produced Build outputs. `ArtifactBytePublisher` and `ArtifactByteReader`
verify exact size and SHA-256, enforce a 64 MiB memory ceiling and apply storage
operation deadlines; the S3 reader also binds GET to the inspected generation.

Protected transfer accepts either ordinary published Artifacts or these private
execution inputs and independently verifies the object before issuing a download.
The capability cannot outlive its Lease or input expiration. The independently
supervised input retention worker deletes expired objects before acknowledging
cleanup, including reservations left by failed compilation. This maintenance
continues with Factory disabled using only ordinary database and object-store
credentials; it does not enable Factory or affect ordinary readiness. Run metadata
retention waits for physical cleanup before removing the input journal references.
Reservation, publication and deletion audit facts share their metadata transaction.

`FactoryArtifactFlowInputs` prepares configured input mappings and reads only
explicitly selected, retained Project-owned Artifact references. Candidate context
requires a verified publication in selected result ancestry, including explicit
intermediate results; control ordering alone grants no context. Repository range
or fragment retrieval requires a separately installed retrieval capability.

Decision Signal nodes use the same exact immutable operation binding as trusted
actions. Their installed adapter owns the parameter contract and provider policy;
logical node names do not choose a provider or confer transition authority.
Installed routing bindings pin their complete preparation/consumption policy,
output mappings and independent authority into the published action parameters.
A changed plan cannot retain the action selected by an admitted node or turn an
earlier pending reservation into a zero-usage cancellation. Older policy bindings
can remain installed under their original exact action alongside a replacement.
`FactoryDecisionSignalReceipts` connects the provider coordinator to private
PostgreSQL dispatch intents and receipts. `DecisionSignalDispatchAuthorization`
checks current ownership, cancellation and the original deadline before external
I/O. A same-transaction audit fact identifies the frozen request. Unresolved calls
hold their full resource ceiling; verified receipts replace it with measured usage.
Reservations and receipts share the node budget under the Run lock, so changing
request UUIDs cannot bypass the ceiling. Exact receipt recovery retains the
original provider/model, consumption and usage after ownership expires; it grants
no renewed dispatch authority. Reads verify retained document digests. Private
journals are scanned one document at a time under bounded count/byte ceilings.
Bounded Run retention deletes both journals before their node/input references.

Provider failures distinguish measured consumption from an unobserved external
outcome. JEV transport failures preserve absent usage, and the common consumption
policy cannot turn them into a free terminal receipt. The coordinator rechecks time
after asynchronous dispatch authorization. Expired reservations or unavailable
capabilities still awaiting an earlier result remain pending instead of discarding
unknown consumption. Installed execution adapters must recover these outcomes
without creating new effects after the dispatch deadline. The management projection
uses typed discovery facts rather than private request documents.

`BuildOrigin::Factory` retains the stable operation, Run, Node Attempt and input
digest; ordinary trigger-origin Builds retain their existing origin. Discovery
returns nullable trigger/cause fields and an optional `factory_origin` for a
Factory Build. Terminal internal events carry their actual source origin and can
start a normalized internal-event root without inventing a Trigger occurrence.
Cancellation uses the current observer's claim and the original operation through
the ordinary Job/queue transaction. A cancellation replay returns its original
receipt after restart. Signed managed Jobs retain the Node's absolute deadline
when observing ordinary retry history; Agent staging and execution obey that same ceiling.

`FactoryNodeBuildAdapter` implements the same execution port for both kinds.
It dispatches through `OrdinaryBuildApplication`, queries existing Build and
Attempt state and requires retained published outputs from
`FactoryNodeBuildOutputSource`. The observer can recover ordinary retry history
inside the original Node Attempt; provenance uses the actual latest Attempt and
Jobs. Management Build retry is rejected for Factory Builds because it carries
no current owner fence. Operator retry uses `RetryNode` and creates a fresh
configured Node Attempt. An owner-authorized ordinary retry producer remains
part of the deferred worker integration.

The adapter rechecks actual execution choices, exact schemas, tool/plugin
identities, output digests, publication state, retention, measured usage and
the original deadline. Failed or cancelled Builds retain execution diagnostics
and do not become semantic outcomes. Ownership takeover cannot extend the
execution deadline. After expiry, recovery uses a side-effect-free lookup of the
already created operation; it can retain outputs published before the original
deadline without dispatching or retrying execution. Retained evidence may outlive its semantic freshness; a
configured gate decides freshness using the trusted observation clock.
`FlowBuildExecution` retains actual Build/Attempt/Job identities even while an
execution is pending. PostgreSQL retention protects these Builds and removes
execution links before cleaning referenced ordinary metadata.

Read-only Codex usage settlement checks retention at the current observation
time even after the original execution deadline; it cannot renew dispatch
authority. Provider timing must fit the persisted Job creation and terminal
window and its signed deadline. Each installed Codex profile explicitly selects
reported cost or token rates for its exact immutable model, with separate cached
input accounting and checked arithmetic. The protected invocation envelope freezes
that policy before Build admission, so a changed rate policy cannot verify an
existing execution. Missing counters remain unavailable; no built-in price table
or implicit zero cost is used.
The generic Codex task-definition digest binds instructions, the consumed
credential purpose/profile and the cost policy. Different policy versions can
remain installed together for old and new admitted calls; selection uses that
exact digest rather than a mutable tariff or logical node name. Unused credential
purposes do not couple unrelated calls.

`FlowActionBinding` selects an exact installed plugin, its action key and
bounded parameters. `FactoryFlowActionAdapter` implements the common node
executor; plugins receive the frozen projected input, exact grants, ceilings,
deadline and stable operation identity. They must observe ambiguous responses
before repeating effects and return a schema-valid payload, declared outcome
and measured usage. The owner commits their record and completion through the
same journal as Build results. New technical actions require a plugin; new
logical stages using installed actions need only configuration.

The native profile installer binds review planning to the same PostgreSQL Run
journal and trusted execution clock. Each installed planner identity includes
the complete review-profile digest; criterion packs must belong to an installed
Project. `FactoryReviewPlanningAction` consumes the exact selected, verified
Evidence record and freezes policy, evaluator branches, quorum, budget and
freshness into one retained plan. Missing required evaluator capabilities produce
the configured escalation outcome. Replay returns the accepted plan through the
common Node journal rather than creating another planning store.

`FactoryProtectedValidationEvidenceAction` publishes a complete Evidence Manifest
from one explicitly projected report and qualification-facts Artifact pair. Both
references must match one retained ordinary Build completion, its original intent,
execution and independently verified output contracts. The report bytes are checked
again; qualification facts come from the schema-validated common journal. The
manifest retains the candidate and all protected input identities, bounded
freshness and typed discovery facts. The common Node owner persists the result;
replaying a completed node after restart does not reread the report.

`FactoryRuntimeResources::protected_validation_evidence_executor` composes native
PostgreSQL/S3 readers with the installed authoring policy, exact retained candidate,
private test inventory and original validation materialization. Inventory recovery
reads only the selected inventory bytes and each test file's publication metadata;
private test bodies remain signed Agent inputs. Mandatory public deliverables are
requalified from exact retained capture bytes, including the protected-copy check.
Preparation of a live validation requires retention through its original deadline;
read-only recovery of a completed validation neither renews that deadline nor
requires a fresh execution claim. Binding construction validates immutable profile
identity and output contracts without dependency I/O. Loading additional deployment
JSON roles is a separate configuration concern.

`FactoryWorkStatusAction` accepts an arbitrary configured status. A connector
projects that value onto its own representation: GitHub uses a label and Jira
uses a native status. A research observation never automatically changes an
external status. Operators insert a status action, notification or another
plugin action on the desired outcome path. Connector credentials and SDK
objects stay outside Flow contracts; exact host and credential grants are
checked before effects. Inline projection failure cannot produce a successful
node completion. Optional reporting instead declares `outbox` retry parameters:
its receipt means a durable intent accepted atomically with the common node
result. The independent `FactoryWorkStatusOutboxWorker` claims only its effect
kind and resolves the original immutable record, parameters and exact installed
projector. Retry budgets cannot extend the original deadline. Reporter success,
failure and restart change only outbox state; external status cannot authorize a
Factory transition or reconstruct authoritative state. Runtime installation can
register independent dispatchers without changing the graph owner.
`FactoryRuntimeResources::work_status_dispatcher` binds the installed projector
registry to that native journal and the trusted clock without probing or writing
to a provider during installation.

`FactoryCandidatePublicationAction` selects a protected publisher through the
same plugin execution boundary. Its frozen request includes the complete captured
candidate, exact manifest/bundle, Work and cycle, operator target, permissions,
budget, original start, current observer window and deadline. The complete selected
input/provenance digest binds its control proof. The target is an immutable policy
reference: mutable provider mappings cannot redirect the request. The connector
verifies bytes and ancestry, deduplicates the
whole request and observes the original operation before repeating an ambiguous
write. Another operation or candidate cannot provide its receipt. Receipt-schema
validation proves structural compatibility with the complete producer contract
before effects; probing a few sample payloads cannot establish it. Review validity
narrows the attempt deadline. A reusable target declares the base and candidate
reference prefix; the stable operation digest materializes the exact branch.
Different operations do not share a candidate branch. The Delivery request carries
an exact network ceiling checked against the provider's actual API/Git hosts before
credential resolution. Observing an owned PR requiring metadata repair returns a
separate typed outcome; observing an external merge does not require its current
base to equal the original accepted base.
Configured gates independently check exact receipt and review freshness before
freezing readiness; publication alone is not acceptance. Provider SDKs and merge
authority remain outside this port.

`octacity-delivery-protocol` and `octacity-server-delivery` reuse common verified
process mechanics and keep writes outside VCS and repository Jobs. The single
[`octacity-github` provider plugin](../../server/infrastructure/octacity-github/README.md)
uses Octocrab behind this boundary; future GitHub intake/read/reporting extend the
same plugin with separate contracts and grant classes. Executable/configuration
digests and exact Project/Repository, network and secret-profile ceilings are
checked before protected handles. Tokens remain inside trusted provider processes.
Common append-only results retain branch/review identities and verified remote
heads; existing promotion records retain exact deployment observations.
`FactoryRuntimeResources::promotion_executor` composes the installed deployment
materializer with native ordinary Build acceptance, observation and cancellation.
The promotion adapter checks frozen readiness and exact candidate identity;
deployment credentials and provider mechanics stay in the installed compiler.
Binding construction performs no dependency I/O.
`FactoryRuntimeResources::delivery_materializer` uses native Project-owned
Artifact publication records, ordinary Build ownership and bounded S3 reads.
The installed target policy selects an existing owner-private connector input
directory. Both exact bundle and manifest are verified before their opaque
handles become available; files are published atomically with private permissions.
Replay accepts identical bytes and refuses to overwrite another input. These
directories remain outside repository Jobs and model contexts. This filesystem
handoff is qualified on Unix; other platforms fail installation closed until
their private-directory access boundary is qualified.

`FactoryDecisionSignalRequestPlan` freezes an installed provider/capability,
questions, calibration, explicit schema-checked payload projection and call
ceiling. Preparation contains no I/O and adds no context or transcript implicitly.
The request identity binds the common attempt, frozen input and complete plan;
restarting the worker prepares the same request. The call budget must fit inside
the common Node budget, and its deadline is the earlier of the Node deadline and
original start plus call ceiling. Preparation grants no dispatch authority;
durable fenced reservation is still required before contacting a provider.

`FactoryDecisionSignalNodeExecutor` maps a consumed routing disposition to an
explicit declared outcome and schema-checked projection of safe Signal facts.
Its bounded catalog selects an exact action and Project; logical node names
carry no dispatch meaning. `FactoryRuntimeResources::decision_signal_executor`
binds it to the native journal after verifying independent Project and local
authority, without contacting dependencies during binding construction.
Shadow and advisory execution preserve the installed baseline. Private request
documents and provider payloads never enter the common result. Ambiguous
disposition mappings are rejected during installation, and four-layer authority
is checked before provider I/O. Cancellation observes receipts without dispatch:
an undispatched operation can stop, while a reserved operation with unknown usage
remains pending. PostgreSQL can settle an exact previously reserved call after
cancellation under current ownership; this records measured consumption without
renewing dispatch authority or completing the cancelled node. Tool-risk execution
requires its separate deterministic authorization and is not installed by this
routing executor.

The composition root bounds every installed Signal connector by both its call
ceiling and the remaining immutable request deadline. A timeout after dispatch
retains unknown usage and its durable reservation; an already expired request
never contacts the connector and records known zero consumption. The application
and common Factory contracts contain no HTTP payload, SDK, endpoint or bearer
format for JEV or another provider.

The JEV adapter accepts its own bounded versioned deployment JSON through
`JevDecisionSignalProvider::from_deployment_json`. The host resolves its API key
separately; inline credentials and unknown fields are rejected. Installation
does not contact the provider. Endpoint, billing conversion and HTTP timeout are
adapter configuration, while the exact immutable capability is matched by the
common installed-provider catalog. Configuration errors and adapter diagnostics
omit private configuration values.

`load_factory_signal_connectors` reads a version-1 catalog of `kind`, `credential_profile`,
`configuration_file` and `credential_file` references. The catalog and referenced
files use the same stable owner-only regular-file gate as other server secrets;
configuration is bounded to 4 MiB and the connector count is finite. Duplicate
exact provider/adapter/model identities within one credential profile are rejected.
The resulting neutral registrations bind each transport to its independently
resolved credential and can be passed to `FactoryRuntimeProfileCatalog::load_with_signal_providers`
without embedding provider wire configuration or credentials into Flow metadata.
Routing and tool-risk resolve both the exact capability and credential profile;
a declared policy alias cannot silently use another installed transport's key.
Native routing and tool-risk installations require the connector's neutral
`network_destinations` declaration. Each declared destination must match the
separately granted host; an undeclared footprint fails qualification. An explicit
empty declaration is reserved for transports that perform no network I/O.
JEV derives its destination from its validated endpoint, and the deadline wrapper
preserves that declaration. Logical provider identity never substitutes for
actual transport authority.

Each installed registration retains a shared, secret-free observation of failed,
interrupted or invalid evaluations and recovery lookups. Native Factory readiness
combines that state with its Project and Artifact dependencies; a domain fallback does not hide a
degraded connector behind a healthy worker. An exact valid evaluation or recovered
result clears the observation. An absent recovery observation preserves the
previous state; an invalid result, failure or interrupted lookup cannot establish
health. Recovery does not redispatch the evaluation. Readiness performs no
additional provider evaluation or billable probe. Installation alone establishes
no remote availability, and the observation
is local to the current server process. Ordinary CI/CD readiness remains separate.

The default server startup resolves an explicitly configured
`factory_signal_connectors_file` before loading the Factory profile catalog and
before opening shared resources. This optional setting requires enabled Factory
workers and `factory_profiles_file`; ordinary mode needs no connector files or
credentials. Offline validation rebases the catalog path with the other runtime
files. Referenced connector configuration and credential paths are absolute
operator paths and must be provisioned separately in that runtime filesystem.
The optional `tool_risk` profile entries resolve an exact installed connector
independently of routing entries. Each pins the complete provider plan, finite
choice mapping, call allocation, credential and network grants, base task digest
and Project-owned Build Configuration. Parsing never contacts a provider or
creates credentials. Local executor qualification remains required before that
policy can authorize an actual operation.

`FactoryRuntimeResources::tool_risk_authorizer` composes exact installed workload
and policy pairs over native Run, ordinary Build acceptance, current signed Job,
lease and Signal journals. It uses the same bounded connector wrapper. The
server-owned provider grants stay separate from the Job's permissions. Current
authority is checked before reservation and after the provider response;
cancelled Builds and lost leases cannot obtain an executable action. The Agent
still needs a qualified local broker and protected operation route before a
tool-using Job can be admitted. An enabled installation with no tool policy has a
digest-bound fail-closed authorization service at the authenticated Agent API.

The common Flow owner emits structured transition events only after its journal,
budget and immutable audit fact commit together. They retain exact Configuration,
Definition, input, Context construction-policy, permission and execution-profile
digests, Context entry count, input generation, claim/fence and hard budgets.
Run states and root Flow lifecycle changes come from the committed state and
validated checkpoint; appended cycles retain predecessor and source-attempt UUIDs.
Node start, completion and technical stop events carry finite states, original
attempt identity, measured usage and latency; retries name the exact prior attempt.
Terminal and logical node names are hashed. Replay and rejected stale authority
emit no new committed events. Context entries, task bodies, protected test source,
prompts, commands, provider payloads, finding messages, paths, credentials,
private URLs and hidden reasoning are excluded from these events.

Signal reservation and measured settlement emit an allowlisted structured event
only after the immutable journal and audit commit. Events correlate Run, attempt,
request, fence, audit, receipt, exact provider/model/policy/calibration digests,
rollout mode, hard ceilings and measured usage. Exact replay emits no new commit
event. Private request state, questions and logical provider aliases are excluded.

### Supplied base Flow and Accepted Work

[`base-flow.json`](../../config/factory/flows/base-flow.json) supplies the
baseline eligibility, classification, optional defect/feature investigation,
requirements-needed decision and downstream path. All logical names, result
shapes, policy thresholds and transitions are configuration. The template
compiler contains no special path for this example. The manifest composes explicit
relative parts through a caller-supplied resolver before immutable publication;
per-contract schemas, per-node definitions and transitions can be edited separately.
Imports never participate in execution or admission authority. Eligibility and routing
gates consume retained verifier facts bound to the actual producing input;
model prose cannot grant eligibility, prove reproduction or choose small-Work
bypass. Missing, contradictory, stale or mismatched evidence selects the
configured fallback. A feature proposal remains evidence until the declared
gates accept it.

`FlowGateProgram` is a bounded declarative evaluator with ordered finite rules,
explicit fallback, existence and typed scalar comparisons, boolean operations,
bounded array quantifiers and explicit output mappings. A shared default output
mapping avoids repeated projections; an exact outcome mapping overrides it. It has hard nesting,
rule, schema, byte and evaluation-step ceilings. Missing observations remain
unknown, including under negation. Owner observation time is a dedicated
operand; providers cannot supply that clock. The journal recomputes configured
gate records before accepting them.

A deterministic gate can declare `accepted_work_outcomes`. For such a retained
completion, `AcceptedWorkContract` is a canonical frozen projection of the full
immutable Work, accepting gate policy including parameters, exact configuration
and definition, and complete accepted input ancestry with incoming envelopes.
The journal validates its size before committing the accepting completion.
Restoration recomputes the same bytes and digest against the authoritative
journal. There is one retained authority: the atomic gate record and source
records, rather than a parallel requirements journal or mutable snapshot row.
Acceptance criteria and the exact base remain the original Work artifacts and
subject. A policy replacement cannot change an admitted contract.

The supplied bypass route still traverses `protected_tests`, `implementation`,
`review` and `verification`. The current slice verifies this configured control
path through the common owner and PostgreSQL restart. Configured specification
authoring, independent review, exact candidate publication and readiness are
implemented through generic node contracts and installed capabilities. Protected
test authoring, isolated implementation inputs and exact candidate/test validation
are implemented as technical capabilities. Bounded validation classification,
rework/correction routes and independent code-review gates are configuration.
Immutable applicable deliverable policies are verified before dispatch, by trusted
capture and against exact protected validation inputs.
Policies can be selected by explicit local template references and are resolved
into immutable parameters before admission. Current capture changes enforce the
node's write scope; a separately qualified cumulative Work index preserves required
updates across rework and detects removed deliverables. Test correction receives
bounded author-only remediation, excluded from implementation input. Full production wiring
is supplied by the native runtime resources; live provider and controlled-tool
qualification remains 16.4. These integrations bind
capabilities and profiles without introducing stage-name dispatch.

### Durable phase-ready pools

`PhasePoolPolicy` fixes the logical phase, a permutation of severity, Project
priority and age, concurrent pool and Project WIP ceilings, and a resource
reservation budget. Highest severity and Project priority sort first; age uses
the original Work admission time. Equal priorities use Work, Factory Run,
Flow Run, cycle, node, and content identities as stable tie breakers. A refresh
cannot replace the priority inputs of the same Run version and ready target.

`FactoryPhasePoolStore` publishes, reads and selects bounded ready projections.
Publication reconstructs the ready node from the pinned closure and current
Flow history rather than accepting a provider's readiness claim. Entries bind
the exact Run version, Flow Run, latest cycle, node, Work and Project, phase
input digest, accepted severity, frozen Project priority, exact capabilities,
bounded dependencies and full per-node resource reservation. Each queued node
selects an arbitrary named pool through its immutable `phase_pool` binding.
Publication reconstructs severity and dependencies from the frozen input using
the configured mapping, and rechecks admitted Project priority and capabilities.
It rejects undersized reservations, substituted inputs and undeclared pools.
The selected key resolves to frozen policy independently of the node name or
kind. Dispatch requires the current authoritative Run claim and the durable
selection's exact Flow, cycle, node, generation and complete input digest. Dependencies require the exact
retained Work digest and declared completed root-Flow resolution in the same
Project. Pool membership conveys no execution permission.

The enabled native owner discovers a bounded page of persisted ready-policy
identities before claiming Runs. It supplies the exact installed execution
catalogue, bounded by the same 256-identity ceiling as executor installation;
node requirements do not establish available capabilities. If the Work batch
fills before every discovered policy is processed, the next pass resumes after
the last processed policy. Policy discovery acquires no claims or reservations.
The existing pool operation remains responsible for admission and ranking.

Each selection pass reconstructs at most 100 candidate histories in policy order and selects
at most 100 Work items. It rechecks current Flow history, cancellation, Run and
candidate identity, current claim, dependency resolution, exact capabilities,
remaining Run budget, Flow WIP, pool WIP, Project WIP and aggregate reservations.
Both adapters use the same ordering and decision functions. A full window that
selects nothing while pool WIP remains available retains its last inspected
entry as a scan continuation. It also fingerprints the exact capabilities,
the skipped prefix and lightweight current metadata for its Runs, their exact
dependencies and reserved capacity, including live claims and admission-time
readiness. New lower-ranked entries do not invalidate continuation. The next fresh
pass resumes after that entry only while this eligibility scope is unchanged;
otherwise it restarts at the highest-ranked entry. PostgreSQL locks inspected
Runs and dependencies, then rechecks the skipped prefix guard before committing;
a concurrent change to that prefix rejects the pass for a fresh retry. The metadata scan
covers this prefix without loading all retained histories;
exact replay never advances the scan. A successful selection, a full pool, or
the end of the ordered scan resets continuation to the highest-ranked entries.
This keeps each pass bounded without permanently hiding eligible Work behind
100 dependency or capability blockers. Continuation is mutable scheduling
progress, separate from immutable policy bytes and retained selection facts.
PostgreSQL stores
immutable policies, ready entries, passes and selections in schema 50, uses
an indexed composite ordering, and serializes pool reservations before taking
Run locks in identity order. This lock belongs only to Factory pool operations.

The selection, ordinary fenced Run claim and secret-safe audit fact commit in
one transaction. Selections retain the policy, snapshot and complete selection
input digests. Lost responses replay the original batch, including empty
batches, without reordering or reclaiming Work. A target can be selected only
once across pools and Run-version refreshes; a new workflow cycle has a new
target identity. Claim expiry preserves the logical capacity reservation;
completion, cancellation or superseding the cycle releases it. Recovery takes
over the ordinary Run claim and preserves the original logical selection. Dispatch still requires the current Run version and claim fence.
Retention removes selections and entries before their referenced claims and
Flow history. Manual-intake wiring and worker supervision use these operations
through the application boundary.

### Factory management discovery

Run collections expose bounded safe diagnostics: exact base/candidate, up to
`MAX_FACTORY_RUN_SUMMARY_NODES` active attempts plus the full active count, current
budget consumption/limits, and candidate-bound Decision and delivery summaries.
Escalation exposes its immutable identity without reason text. Null means no
verified fact is retained, rather than inferring a disposition from a logical node
name, advisory signal, payload field or prose.

Trusted action adapters can attach `FlowDiscoveryFacts` to the common result.
The owner checks the exact Work subject before committing those facts under the
same result digest, completion fence and immutable journal. Candidate publication
already supplies its exact verified subject. Decision/delivery/escalation adapters
project verified domain records through the same extension seam. These read facts
never select transitions or grant execution authority. The latest explicit candidate
publication takes precedence over frozen branch Context, which is a fallback only
when no publication exists. Both precede receipt subjects; late results from an
older branch cannot replace the current candidate. Decision/delivery summaries
are matched to the current exact subject.

PostgreSQL schema 53 indexes safe result metadata with checks that tie every
indexed value to the retained document. `factory_run_discovery` derives current
facts and supports decision/delivery filters before pagination. It stores no
parallel semantic history. Counts and composite cursors use only visible matching
Runs. Phase-ready collection reads use a read-only repeatable-read transaction;
an entry was validated under the publication lock, and its exact Run version,
unselected target and generation are checked before paging. Discovery therefore
does not reconstruct full histories or acquire worker Run locks.

### Browser console boundary

The standalone application under `ui/` is a separately built and deployed
REST client, not another server layer. It depends only on the generated
OpenAPI contract and relative `/api/v1` paths. It does not link server crates,
connect to PostgreSQL or object storage, own domain decisions, or require the
server to serve static files. A same-origin reverse proxy serves its immutable
assets and forwards API and health routes to the private management listener.

The console has no authenticated browser identity in v1. Requests enter the
same canonical anonymous trusted-network management context as any other REST
client, with browser credentials omitted and no CORS or forwarded-identity
contract. Removing the console and its static route therefore leaves every
management workflow available to headless REST clients.

## Crate ownership

| Crate | Owns | Explicitly does not own |
| -------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------- |
| `octacity-server` | Process composition and concrete adapter selection | Domain decisions or reusable transport contracts |
| `octacity-server-api-rest` | Management REST DTOs, decoding, routing, OpenAPI, HTTP error mapping | Transactions, domain state, database rows |
| `octacity-server-api-webhook` | Exact-body public webhook routing, bounded decoding, and mapping to typed application delivery commands | Provider SDKs, adapter process supervision, Trigger persistence |
| `octacity-server-api-agent` | HTTP adaptation of the shared server-Agent protocol | Agent execution or placement decisions |
| `octacity-server-api-cache` | Exact bounded adaptation of Octa HTTP cache v1, including protocol headers and media types | Action-key calculation, plugin contracts, quota decisions, or physical object keys |
| `octacity-server-application` | Transport-independent commands, queries, handlers, projections, transaction coordination, and cross-module Project-policy resolution | HTTP DTOs and concrete infrastructure |
| `octacity-server-domain` | Server-only identities, versions, bounded values, timestamps, and typed errors | Aggregates, transport DTOs, persistence rows |
| `octacity-server-factory` | Bounded Factory values, immutable records, permission intersection, lifecycle and candidate decisions, and the provider-neutral Decision Signal contract and consumption policy | Build/Job execution lifecycle, provider wire/process protocols or SDKs, adapter discovery and dispatch, persistence rows, HTTP DTOs, and UI state |
| `octacity-server-trigger` | Trigger normalization, occurrence deduplication, and Trigger evaluation state | Ready-Job placement or provider payloads |
| `octacity-server-pipeline` | Immutable Pipeline versions, DAG validation, dependency policy, Attempt materialization rules | Agent execution and queue leasing |
| `octacity-server-job` | Server Job state, requirements, and signed-intent construction inputs | Agent-side Job execution lifecycle |
| `octacity-server-orchestrator` | Build, Attempt, and reconciliation transitions; DAG progression decisions | Agent selection or repository code execution |
| `octacity-server-scheduler` | Ready-Job placement, Lease lifecycle, and Pool drain decisions | Trigger evaluation, DAG traversal, execution |
| `octacity-server-secrets` | Logical secret references, policy, and short-lived grant interfaces | Provider credentials and durable raw secret values |
| `octacity-server-cache` | Cache namespace authority, fenced session lifecycle, immutable byte-store port, and protocol-level blob integrity | Octa action-key calculation or plugin task contracts |
| `octacity-server-artifacts` | Logical upload, verification, publication, visibility, and retention state | Bucket names, object keys, provider credentials |
| `octacity-server-store` | Backend-neutral atomic persistence and log-search ports | SQL rows, SQL transactions, PostgreSQL syntax |
| `octacity-webhook-provider-protocol` | Versioned provider-neutral webhook adapter messages | Provider payload types and process supervision |
| `octacity-vcs-protocol` | Versioned provider-neutral VCS adapter messages | Git implementation and build workspaces |
| `octacity-agent-provisioning-protocol` | Versioned future provision/observe/terminate contract | vSphere, Proxmox, or other production adapters |
| `octacity-server-store-postgres` | PostgreSQL schema, declarative constraints, rows, locking, transactions, and store-port implementation | Application commands, stored business routines, and domain policy |
| `octacity-server-adapter-host` | Shared registry filesystem checks, executable digest verification, bounded framing, cancellation, timeout, and process cleanup | Provider manifests, wire-message semantics, capability policy, and adapter selection |
| `octacity-server-webhook` | Operator-installed webhook-adapter discovery, capability gating, and webhook protocol policy over the shared host | Public webhook HTTP routing, provider SDKs, Trigger evaluation, and durable delivery retry |
| `octacity-server-vcs` | Operator-installed VCS-adapter discovery, capability gating, credential-handle isolation, and VCS protocol policy over the shared host | Git implementation, repository execution, build workspaces, and provider SDKs |
| `octacity-server-audit` | Stable actor vocabulary, secret-safe bounded metadata, immutable fact projections, and deterministic read-query bounds | Independently callable audit writes, transport DTOs, and SQL rows |
| `octacity-vcs-git` | Read-only Git refs, immutable commit selection, tree browsing, and bounded content reads through an ephemeral bare object database | Build workspaces, checkout, repository hooks, Octafile evaluation, and source execution |
| `octacity-artifact-store` | Backend-neutral immutable-byte interface | Logical Artifact lifecycle and storage-provider details |
| `octacity-artifact-s3` | S3-compatible implementation of Artifact and cache immutable-byte ports with separate private layouts | Logical policy and public S3 details |
| `octacity-observability` | Shared stable metric names, units, closed low-cardinality labels, series budgets, safe facade emission, trace fields, and redaction rules | Exporters, subscribers, transport queues, raw diagnostics, or correctness state |
| `octacity-protocol` | Shared signed JobSpec, server-Agent, and Artifact-transfer wire contracts | Server domain entities and HTTP routes |

Planned ownership names are not compiled as empty packages. Tasks 6.3 and 6.4
have introduced `octacity-server-webhook` over the shared verified process host and
`octacity-server-api-webhook` with its exact-body authenticated ingress.
Tasks 6.6 and 6.7 have introduced `octacity-server-vcs` with its verified
process host and `octacity-vcs-git` as the first read-only implementation.
Task 8.1 has introduced `octacity-server-audit` with transactional mutation
consumers and read-only management queries. Task 8.2 has introduced
`octacity-observability` with closed metric labels, explicit per-instrument
series budgets, stable trace fields, and both server and Agent consumers.

The Artifact Store port lives in the core layer. Its S3-compatible adapter is
an infrastructure crate selected only by a composition root; neither the core
nor the server-Agent wire contract depends on the AWS SDK.

`octacity-server-job` owns negotiated JobSpec derivation and the private-key
signer. It accepts only immutable Build facts, a strict Pipeline execution
template, and validated server policy; it never accepts a caller-created
`JobSpecV1`, `JobSpecV2`, or signed envelope. Agent registration selects and
persists the newest mutually supported execution-contract revision. Placement
uses that persisted selection: legacy registrations can receive only v1
Native/OCI intent, while v2 registrations may receive provider-neutral
host/isolation/virtualization intent allowed by the exact Project execution
target policy. Materialization persists a stable `JobSpecTemplate` without
Job identity, Attempt number, issue time, or signature. The authoritative store
adds those volatile facts and persists `jobs.signed_job_spec` in the same
transaction that changes a root or dependent Job to `Ready`, before inserting
its queue entry. Blocked Jobs therefore cannot retain an expired envelope, and
key rotation applies naturally when they become ready. Lease fences, transfer
capabilities, and secret grants are created later and remain outside the signed
execution intent.

The architecture guard grants only `octacity-server-job` access to the pure
`ed25519-dalek` and `base64` libraries. This package-specific capability keeps
private-key handling out of shared wire contracts and prevents unrelated core
crates from acquiring crypto implementation dependencies implicitly.

Process readiness is composed only in `octacity-server`. The REST adapter sees
a constant-time boolean callback and therefore has no dependency on SQLx, S3,
signing keys, secret providers, or worker implementations. A supervised
monitor refreshes that snapshot under an aggregate deadline. Migrations,
PostgreSQL, object storage, signing capability, and the lease-expiry worker are
unconditional checks. Future configured mandatory secret providers and workers
must be added by the composition root through the same additional-required-check
seam. The expiry worker claims bounded batches in PostgreSQL with an owner and
deadline, fences the expired Lease, and only then requeues or terminally fails
the Job. Process timers merely wake the worker; the durable Lease and claim rows
remain authoritative across restart and replica races. The first successful object-storage check verifies the
actual bounded PUT, GET, COPY, and DELETE capability set with process-unique
probe objects. Healthy recurring checks use `HeadObject` on the process-owned
marker, so readiness adds no bucket-list permission. The configured capability
recheck interval periodically re-arms the full probe; an availability failure
or failed artifact operation does so immediately. Qualification and
invalidation are serialized so a concurrent older successful probe cannot hide
an object-operation failure. Versioned buckets require lifecycle cleanup for
noncurrent probe versions and delete markers below the health prefix. Recurring
migration readiness does not replace disaster-recovery reconciliation.
PostgreSQL and the object prefix are restored as one offline consistency unit;
the composition root exposes a listener-free `reconcile-restore` command that
invalidates restored execution and retention claims and requeues outbox work.
Those changes append digest-bound Factory audit facts in the same recovery
transaction; repeated recovery preserves the original facts. The command
checks every visible immutable-object reference before an operator starts the
normal runtime. Artifact reads independently repeat published-object integrity
verification. Optional search rebuild preserves the committed watermark while
resetting the indexed watermark, so freshness remains false until replay.
Recurring
migration readiness performs a read-only compatibility check
and takes the migration lock only while the schema is an exact older prefix.
A failed, newer, gapped, reordered, or checksum-mismatched history remains
unready. Failed migration sessions are discarded so their PostgreSQL advisory
locks cannot poison the connection pool. Rollback to the previous binary uses
the declared pre-upgrade database snapshot boundary; it never runs that binary
against an unknown newer schema. The monitor logs
only initial state and transitions, with a stable non-secret dependency name
and unavailable/timeout reason. Liveness remains process-local and independent
of this snapshot.

Ingress overload control is also composed only in `octacity-server`. Each
replica keeps bounded, disposable fixed-window counters for management clients,
expensive management reads, Agent credentials, Agent heartbeats, and webhook
senders. Peer identity comes from the accepted socket; forwarded-address
headers are not trusted. Credential material is reduced to a one-way digest
before it can become an in-memory key. Health and metrics probes bypass the
management quota, while Agent heartbeats use a dedicated quota that ordinary
Agent traffic cannot consume. Rejections use HTTP `429` plus `Retry-After` (and
the Agent protocol's `retry_after_ms`). These counters are deliberately neither
durable nor shared between replicas: resetting or evicting one can only change
best-effort admission, never authorize a mutation or alter Build, Job, Lease,
Trigger, audit, idempotency, or outbox state. Operators can tune the bounded
policy through the `[admission]` server configuration table.

```toml
[admission]
window_milliseconds = 1000
tracked_identities = 4096
management_requests = 200
expensive_management_requests = 20
agent_requests = 200
agent_heartbeats = 20
webhook_requests = 50
```

The same composition root constructs the production management application
from typed Project, Pipeline, Repository, Build Configuration, Pool, Agent,
enrollment, Trigger, Build-run and Job-event handlers. It separately constructs
the authenticated Agent application for registration, placement, heartbeat,
event append and terminal completion. Both share the PostgreSQL adapters, but
only operations that can create a Ready Job receive the active JobSpec signer.
Health-only routers remain test components and are not the production wiring.

Several entries above currently contain only ownership documentation because
task 1.2 established the dependency boundaries before their implementation
tasks begin. These phase scaffolds are not evidence that a separate crate has
earned a permanent seam: they remain dependency-free until implemented, and
each must pass the deletion test described in the design (own real invariants
or an enforceable boundary) or be merged into its caller.

The shared observability package contains vocabulary and safety invariants, not
an exporter implementation. Its thin metrics facade validates the declared
instrument and closed labels before handing a point to the process recorder;
without a recorder it is a bounded no-op. High-cardinality request, Build, Attempt, Job,
Agent, Lease, Trigger-occurrence, and integration identities are trace-only
correlation fields. Metric labels are sealed enums with finite value sets, so
raw paths, identifiers, user input, and protected wrappers cannot create new
time series. Export failure is diagnostic loss and never correctness state.
The application layer owns the transport-independent Agent telemetry exporter
port, the Agent HTTP adapter owns its bounded concurrency and timeout gate, and
the composition root selects the concrete exporter adapter.

Task 8.4 instruments management and webhook HTTP boundaries, provider hosts,
Trigger and orchestration decisions, placement and Lease operations, the
PostgreSQL port boundary, cache and Artifact services, transactional-outbox
delivery, authoritative ready-Job queue snapshots, and supervised worker passes. Request,
adapter, store, and worker spans carry trace-only correlation while metric
series retain only the closed classifications above.

The server composition root installs a process-local Prometheus recorder,
supervises its maintenance task, and exposes snapshots on the management
listener at `GET /metrics`. Recorder initialization and scrape failures degrade
observability only: they cannot reject domain work or stop protocol listeners.

## Public protocol seams

A protocol seam crosses a process or product boundary and therefore requires
version negotiation, bounded decoding, classified failures, and independent
documentation. A core port is a Rust interface inside the server and is not a
wire compatibility promise.

| Seam | Contract owner | Host or consumers | Trust and translation rule |
| ------------------------------- | ------------------------------------------------------------ | ------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Management commands and queries | `octacity-server-application` | REST now; future GraphQL | Each transport owns its DTOs and maps them to application types |
| Public webhook delivery | `octacity-server-api-webhook` | Provider callbacks | The dedicated listener durably admits exact bounded bytes and allowlisted headers, returns a receipt identity, and never waits for adapter execution |
| Server-Agent coordination | `octacity-protocol` | `octacity-server-api-agent`, `octacity-coordinator` | Enrollment, registration epoch, Lease fence, and signed JobSpec remain mandatory |
| Artifact transfer | `octacity-protocol::artifact` | Server Artifact domain and Agent output flow | Only logical identifiers and short-lived opaque capabilities cross the seam |
| Webhook provider process | `octacity-webhook-provider-protocol` | `octacity-server-webhook` | Exact raw delivery is authenticated before normalization; optional managed create, observe, rotate, and delete use stable idempotency identities, protected credential handles, and durable bounded retry state |
| VCS provider process | `octacity-vcs-protocol` | `octacity-server-vcs` | Configured integration identities select an operator-pinned adapter and protected credential handle; mutable refs resolve once to immutable revisions, while repository content remains bounded data only |
| Agent provisioning process | `octacity-agent-provisioning-protocol` | Future infrastructure host | No production adapter or dynamic-provisioning readiness dependency exists in v1 |
| Authoritative store ports | `octacity-server-store` | PostgreSQL adapter; deterministic in-memory test adapter | Initial ports expose implemented atomic coordination, credential, and log-index watermark use cases and grow with feature implementations; SQL types never cross an interface |
| Build-log search ports | `octacity-server-store::{LogIndexWorkStore, LogSearchIndex}` | Authoritative store plus PostgreSQL projection; deterministic in-memory test adapters | The application reads the committed watermark from the authoritative port and combines it with contiguous projection progress; PostgreSQL query syntax and physical log locations do not cross either interface |
| Immutable byte-store ports | `octacity-artifact-store::{ArtifactStore, LogChunkStore}` | `octacity-artifact-s3` | Physical bucket, key, credential, and ETag remain adapter-private; log writes are idempotent and independently verified before their manifest can commit |
| Secret-provider port | `octacity-server-secrets` | Future configured provider adapters | Logical references and scoped grants cross the port; raw values do not enter durable domain state |
| Octa remote-cache data plane | Published Octa HTTP cache protocol | Cache HTTP adapter and Octa | Server authorizes namespaces and sessions but does not reinterpret Octa cache keys |
| Cache immutable bytes | `octacity-server-cache::CacheBlobStore` | Configured S3 adapter | Scope and validated descriptor cross the port; bucket, physical key, ETag, and credentials remain private |

Language-neutral wire documents and golden fixtures are indexed in
[the protocol catalogue](../reference/protocols/README.md). The server-side adapter protocol
crates are additionally mapped in
[the server protocol README](../../server/protocols/README.md). The ownership,
type-isolation, activation, and crate-creation rules for integrations that are
deliberately absent from v1 are defined in
[deferred server extension seams](extension-seams.md).

## Agent credential lifecycle

`AgentCredentialStore` is a core interface rather than a wire protocol. The
application layer supplies independently generated 256-bit secrets; adapters
persist only domain-separated SHA-256 verifiers. Secret and verifier formatting
is always redacted, and audit and outbox payloads contain only logical
identities, Pool binding, epoch, and expiry metadata.

An enrollment credential is bound to an exact Agent Pool version, an optional
exact platform policy, and an exclusive expiry. Its first successful
registration consumes it atomically. A process restart proves possession of
the current registration credential and creates the next positive epoch in the
same transaction that revokes the old registration. Exact retries derive the
proposed registration identity from the presented credential and
shared-protocol `request_id`, so a lost response recovers the original outcome
even when the server observes a later time. Using a consumed enrollment
credential or a superseded registration under a new identity is rejected. The
Agent API validates the complete shared `AgentInventory`, maps the request
through the application service, and never exposes store rows.

Credential files contain an opaque dot-separated token. Enrollment tokens use
`enrollment.<credential-uuid>.<base64url-secret>`. After registration the Agent
keeps the same 256-bit secret in memory and promotes the token to
`registration.<registration-uuid>.<base64url-secret>`; the raw secret is never
logged or persisted by the server. Every poll, heartbeat, event append, upload,
cache, and completion use case authenticates this current registration before
performing work. A new registration revokes the old epoch, and fenced lease
mutations also verify that their registration has not been revoked.

## Store transaction and projection invariants

Atomic store requests are bounded before an adapter opens a transaction. The
core interface publishes limits for encoded documents, Trigger materialization,
dependency edges, Pool allowlists, Agent inventory, and event batches. Concrete
adapters revalidate those limits and use bounded bulk statements for collection
writes. A durable Job event computes its own canonical digest from event kind,
source time, and payload; callers cannot supply payload and digest separately.

Application use cases depend on operation-shaped ports rather than the complete
adapter surface: Trigger acceptance, Job execution, and Build run control have
separate interfaces. `AuthoritativeStore` is only their composite adapter
contract. Manual Trigger context loading also uses a dedicated Trigger-definition
query and verifies the exact enabled manual Trigger version and target before
any mutable VCS reference is resolved. The REST command reserves its stable
intent and canonical payload in `trigger_evaluation_work` before that external
read. The request and recovery worker share one fenced claim; transient VCS
failures use a bounded policy, while permanent or exhausted failures remain as
secret-free dead letters. The first successful mutable-reference result is
checkpointed as an immutable revision under that claim before Build creation,
so a later store retry cannot observe a moved branch. A completed replay reaches
the authoritative Trigger acceptance record before it could invoke VCS again.

The PostgreSQL package exposes a plain `PostgresStore` for configuration,
Project, Pipeline, credential, and indexing ports. Only
`PostgresAuthoritativeStore` accepts the active JobSpec signer and implements
coordination mutations that can move Jobs through a signed `Ready` boundary.
This keeps unrelated persistence capabilities usable without signing material.
Platform requirements use the shared closed OS and architecture types at the
configuration boundary, so an unsupported spelling cannot be published and
fail later during JobSpec derivation.

PostgreSQL describes storage shape with keys, foreign keys, unique indexes,
nullability, and row-local checks. It does not own lifecycle transitions,
hierarchy validity, sequence allocation, or append-only policy through stored
routines or triggers. Cross-row decisions stay in Rust store operations and
are serialized with an explicit transaction isolation level, row lock, or
transaction-scoped advisory lock. This keeps the same core behavior available
to another store adapter while PostgreSQL remains responsible for durable
atomicity and structural integrity.

Pipeline dependency policy is a typed immutable Pipeline value. A terminal
Job mutation locks its Build and Attempt and loads the complete persisted Job
graph. The Orchestrator validates that snapshot and computes a deterministic
fixed point:
newly satisfied Jobs become `Ready`, dependency failures cascade `Skipped`
through descendants, and the post-transition graph derives the Attempt and
Build state. PostgreSQL then updates Jobs, the global ready queue, Attempt,
Build, completion replay data, audit, and outbox in the same transaction.
Job read projections call this durable Pool/Agent pair an assignment rather
than current ownership and retain it for executed terminal Jobs. Current Lease
ownership and its secret fence remain coordination-only data; queued or skipped
Jobs cannot claim an assignment, while cancellation may be terminal with or
without one depending on whether placement occurred.
The first newly persisted Agent event advances a leased Job through the core
`ExecutionStarted` transition to `Running`; terminal completion uses the same
state machine and may infer that transition only for a zero-event Job.

The decision is pure and stably ordered by Job identity. Re-evaluating the
same graph produces no transitions, while an exact completion replay returns
the originally committed transition set and aggregate states. A process crash
can therefore expose either the graph before the transaction or the complete
post-orchestration graph, never a persisted outcome with partially advanced
dependencies. The in-memory and PostgreSQL adapters run the same decision and
behavioral contract. This keeps failure policy out of infrastructure while
preventing concurrent predecessor completion from leaving a satisfied child
blocked.

Build cancellation and retry use separate complete store operations. A
cancellation locks the Build and its current Attempt, records one durable
Build-level intent, and asks the Orchestrator to transition the complete Job
set. Blocked and ready Jobs become terminal `Cancelled`, their queue entries
are removed, leased or running Jobs become `Cancelling`, and current Leases
become `cancellation_requested`; terminal Jobs never change. The Attempt and
Build remain running only while an Agent must acknowledge cancellation, then
normal fenced completion derives their terminal `Cancelled` state. Exact
replay returns the original transition set.

Management Build Retry applies to Trigger-origin Builds. A Factory-origin Build
must use the exact Factory RetryNode control: the ordinary command carries no
current Factory claim or Node budget authority and is rejected atomically before
materializing another Attempt. The ordinary read path still retains all prior
Attempt history.

A retry is allowed only for the latest failed Attempt. The PostgreSQL adapter
locks the Build, calculates its next positive Attempt number in Rust, and
rejects a stale proposed number, so concurrent requests cannot create sibling
retries. Candidate Jobs carry fresh identities, while one backend-neutral core
decision compares every Pipeline node, dependency, root and non-root policy,
placement requirement, Job snapshot, and stable JobSpec template with the
preceding immutable Attempt. Root JobSpecs are signed only after that check.
The new Attempt records `retry_of_attempt_id`; the prior Attempt, events,
logs, and artifacts remain append-only and addressable. Attempt creation, DAG
materialization, root queue insertion, Build reactivation, idempotency, audit,
and outbox commit atomically.

### Application projection boundary

`octacity-server-application` owns transport-independent projections for
Projects, immutable Pipeline and Build Configuration versions, Builds,
Attempts, Jobs, diagnostic DAG causality, and Trigger history. They are not
REST DTOs or database rows: later query handlers return these models, while a
REST or future GraphQL adapter maps them into its own versioned contract.

Projection construction is deliberately lossy at security boundaries. A
Pipeline node is decoded through the strict repository-controlled
`JobExecutionTemplate`; a Build projection decodes the server-owned input and
effective-policy snapshots but omits its JobSpec toolchain policy; a Job
projection omits the signed JobSpec, Lease fence, and internal node snapshot;
and Trigger history omits opaque provider metadata. Configuration policy
exposes only validated logical cache, secret-profile, and workload-identity
references. Output
references contain logical Artifact identity, name, content digest, size, and
publication time, never a bucket, object key, credential, or transfer URL.

The diagnostic DAG projection validates that every dependency stays within
one Attempt and that the resulting graph is acyclic. It exposes stable Job and
Pipeline-node identities, current states, dependency policy, and safe failure
classification so skipped and failed causal paths remain explainable without
returning execution credentials or provider configuration.

Project definition pages, Project Builds, Pool-filtered Agents, global
resource search, and operator attention are application/store projections as
well. Their typed inputs bind ownership, exact filters, search or attention
scope, cursor, and page bounds before an adapter query executes. Both the
in-memory and PostgreSQL stores apply authorization visibility before
classification or matching, deterministic ordering or ranking, pagination,
and cursor production. REST only maps those already-safe projections into
versioned DTOs; it never fetches a broader page and filters it afterward.

Search returns only Project, Build, Agent, and Agent Pool identities, safe
labels, and bounded context. Operator attention returns only scoped resource
transitions and explicitly requested server-classified critical conditions;
it owns neither recipients nor personal read state and is not reconstructed
from audit or raw Job events. Agent current-execution detail is a bounded safe
projection rather than exposure of coordinator Lease or fencing state.

### Immutable Pipeline publication

`octacity-server-pipeline` accepts a publication draft only through
`PublishablePipelineDag::new`. The resulting proof type is serializable but
not deserializable, so a restored historical `PipelineDag` must explicitly
cross `for_publication` with the current capability catalog before a store
create or publish operation can accept it. Construction validates node and
edge identity, missing references, cycles, fan-in and fan-out bounds,
process-safe JSON depth, per-node and aggregate encoded size, and requested
execution capabilities. It then canonicalizes nodes, edges, capabilities, and
nested JSON objects. Topological ordering uses node identity as the stable tie
breaker, and the serialized snapshot carries an explicit schema version.

Capabilities are checked against the publication-time `CapabilityCatalog`.
Reading an existing snapshot validates its structure and schema but does not
re-evaluate it against the current catalog: removing a capability from a later
server release must not make historical Pipeline versions unreadable.
Dependency policies return explicit `Blocked`, `Ready`, or `Skipped` decisions,
so failure propagation is owned by Pipeline and Orchestrator code rather than
being inferred by a storage adapter.

`PipelineStore` exposes three use-case-shaped operations: create a Pipeline
with version one, append exactly the next version under an optimistic version
precondition, and read an exact version. It intentionally exposes no update or
delete operation for a published snapshot. Each accepted mutation commits its
idempotency result, audit fact, and outbox event in the same transaction.
PostgreSQL locks the Pipeline identity to serialize competing publications;
the reusable store contract and a PostgreSQL concurrency test verify that one
next version wins and every earlier snapshot remains unchanged.

### Immutable Repository and Build Configuration publication

A Repository has a stable Project-owned identity and sibling-unique name, but
its provider-neutral VCS integration, locator, and revision-selection policy
live in append-only Repository versions. A Build Configuration follows the
same identity/version split. The same `RepositoryLocator` value crosses
configuration and JobSpec derivation:
it accepts provider-owned opaque remote identifiers as well as URLs while
rejecting local paths, traversal, embedded credentials, and query fragments.
Each Build Configuration version captures its enabled state, an exact
Repository version, an exact immutable Pipeline version owned by the same
Project, parameter schema and defaults, accepted Trigger kinds, Agent
requirements, allowed Pools, runtime and network policy, cache authority, an
optional logical Octa secret profile, artifact and report ceilings, and retry
policy. The profile is checked against the immutable effective Project-policy
snapshot before source resolution or persistence side effects.

### Logical secrets and delegated grants

`octacity-server-secrets` owns bounded provider identities, logical secret
references, secret and workload-identity profile names, validated public
provider configuration, and the asynchronous short-lived grant port. A grant
request binds one allowed profile and one provider's bounded reference set to
the Project, Build, Job, current application-authenticated Lease, issue time,
and expiry. The application caller must verify the fence before constructing
this request; the raw fence never crosses the provider seam. The grant lifetime
cannot exceed the provider ceiling, and a returned grant cannot change provider
or extend that authority.

Provider credentials and provider-specific mappings remain inside concrete
adapters. Delegated grant bytes are transient zeroizing values: they implement
neither serialization nor display, and debug output is always redacted.
Provider errors cross the core seam only as stable classifications without raw
messages. Consequently durable Build state, signed JobSpec data, management
REST resources, audit facts, logs, and metric labels can contain logical
profile/reference metadata but not raw secret values, provider credentials, or
delegated grant bytes. Repository-controlled Pipeline templates still cannot
supply a secret profile; the server inserts the policy-authorized logical
profile while deriving the signed JobSpec.

Mutable VCS references, exact network hosts, runtime classes, and artifact
ceilings are server-domain value objects shared by Trigger, policy, and
configuration modules. VCS references therefore have one byte bound, network
allowlists cannot contain URLs, ports, or implicit wildcards, and artifact
shape validation cannot drift between inherited policy and configuration.

`ConfigurationStore` exposes only create, append-next-version, and exact-version
read operations for these aggregates. The core types validate bounded typed
snapshots at every adapter seam; PostgreSQL supplies keys, foreign keys, row
locks, atomicity, and JSON persistence but does not decide configuration
policy. Stable identities and version rows are separate tables, and published
versions have no update or delete operation. Competing publications use the
caller's expected-current-version precondition, while exact idempotent replays
return the original outcome without adding audit or outbox facts.

A Build records exact Build Configuration and Pipeline version references.
Publishing later Repository, Pipeline, or Build Configuration versions can
therefore affect only subsequent Builds. Existing Builds and retries continue
to resolve the snapshots they originally captured. The reusable in-memory and
PostgreSQL store contract verifies version immutability, reference validation,
typed policy rejection, replay behavior, and atomic mutation evidence; a
PostgreSQL concurrency test additionally verifies that only one competing next
configuration version wins.

### Normalized Trigger occurrences

`octacity-server-trigger` owns the normalized occurrence interface. Manual
commands, persisted schedule times, authenticated external repository events,
and server-generated internal events become one strict
`NormalizedTriggerOccurrence` before they reach authoritative persistence.
Every occurrence names an exact Trigger definition and Build Configuration
version, a source-observed time, a source-scoped deduplication identity, a
typed provider-neutral cause, and causal lineage. Only authenticated external
events may retain bounded opaque provider metadata.

Deduplication and causality are deliberately separate. The deduplication key
is the exact Trigger version plus the stable source identity and prevents
retries with different occurrence IDs from creating another Build. Causal
lineage records the root occurrence, optional direct parent, and derivation
depth so later internal-trigger cycle policy can reason about ancestry without
using a provider delivery identifier as a graph edge. Root manual, scheduled,
and external occurrences point to themselves at depth zero; internal
occurrences require distinct root and parent identities and a positive depth.

The authoritative store revalidates this core model and the exact
configuration target before mutation. PostgreSQL persists typed cause,
lineage, origin kind, and bounded provider metadata as data, applies only
structural checks and foreign keys, and keeps the unique source-scoped
deduplication key. The existing atomic Trigger-acceptance operation and its
in-memory/PostgreSQL contract verify that duplicate occurrences expose at most
one Build.

Public webhook ingress first commits a raw receipt with a server-owned identity
and returns `202 Accepted`. A durable worker claims verification work with an
owner and expiry, invokes the pinned adapter, and atomically records the
provider-neutral event under the integration-scoped provider delivery identity.
If the integration was disabled before processing, the worker moves the receipt
to the terminal `suppressed` state without invoking the adapter or evaluating a
Trigger. This also closes the race where a receipt was admitted immediately
before the integration was disabled.
An exact duplicate completes without new Trigger work; conflicting normalized
data under the same provider identity is dead-lettered. The canonical receipt
then evaluates the Trigger with a stable composite identity. A crash before
completion therefore replays the same occurrence and Build. Only transient
adapter failures receive bounded exponential retries; exhausted and permanent
failures retain attempt counts and secret-free diagnostics, while raw bodies
and header values are cleared from terminal records.
Delivery verification and managed-registration work intentionally share one
operator-level polling, claim-lifetime, and retry policy because both invoke
the same bounded webhook adapter host. Their batch sizes remain independent so
one workload cannot silently enlarge the other workload's sequential claim
budget.

Terminal Build transitions publish `build.succeeded`, `build.failed`, or
`build.cancelled` facts through the same transactional outbox commit as the
authoritative state change. The internal-Trigger worker claims bounded batches
with a process owner and expiry, reloads authoritative Build and occurrence
state, and evaluates every enabled immutable Trigger definition that existed
when the source event committed. A crash before claim completion causes
at-least-once redelivery; each downstream occurrence uses the outbox entry as
its stable source identity, so replay reaches the existing Build instead of
creating another one.

Internal causality has two independent guards. An exact Trigger definition may
appear only once from the root occurrence through the candidate, which rejects
direct and indirect cycles such as `A -> B -> A`. A non-repeating chain is also
limited to 32 internal derivations. Protected candidates create no Build, but
the worker completes their source event after processing other candidates, so
a causal loop cannot become an unbounded retry loop. PostgreSQL stores claims
and lineage; the cycle and depth decisions remain in `octacity-server-trigger`
and are shared by every adapter.

`ManualTriggerService` completes the manual path without exposing persistence
or VCS protocol details to a transport. It loads the exact immutable Build
Configuration, Repository, Pipeline, and effective Project policy through
application ports; rejects invalid parameters, source selection, and policy
before external work; then resolves or verifies the selected source through a
provider-neutral `RevisionResolver`. Occurrence, Build, first Attempt, and Job
identities are derived deterministically from the Trigger version and
deduplication identity, so a lost-response replay cannot create a second graph.
The service materializes every Pipeline node and dependency, intersects the
configuration and Project Pool allowlists, and submits one `AcceptTrigger`
operation. That transaction persists the occurrence, immutable Build snapshot,
Attempt, Jobs, dependency edges, audit, outbox, and only root ready-queue
entries together. The first manual observation time is retained, but a replay
with the same source identity is not rejected merely because the retry arrived
later.

`LogIndexWorkStore` reads the committed project-local watermark from the
authoritative store. The application, rather than an external query DTO,
combines it with the greatest contiguous position reported by
`LogSearchIndex`. Applying a later position therefore cannot conceal an earlier
gap. Build deletion records a durable tombstone; delayed indexing and rebuild
work has a projection-specific `Superseded` outcome and cannot restore
searchable documents.

Agent stdout and stderr take a stricter acknowledgement path. The application
decodes runner output frames, masks configured in-memory credentials and URL
query credentials, rewrites the durable event payload with the redacted bytes,
and coalesces adjacent events of one stream into bounded chunks. It writes each
deterministically identified chunk through `LogChunkStore` and verifies length
and SHA-256 before calling the authoritative append. That single append
transaction locks the Project counter and commits Job events, the contiguous
cursor, immutable manifests, and one indexing-work position per chunk. A lost
acknowledgement repeats the same object and transaction identities. An object
store outage cannot advance the cursor; a database rollback can leave only an
invisible object, which orphan cleanup deletes after checking that no committed
visible manifest owns the identity.

## State-machine interfaces

The initial pure transition interfaces live with the modules that own their
invariants:

- [Build, Attempt, and orchestration](../../server/core/octacity-server-orchestrator/src/lib.rs);
- [DAG Job](../../server/core/octacity-server-job/src/lib.rs);
- [Lease and Pool drain](../../server/core/octacity-server-scheduler/src/lib.rs);
- [Artifact publication](../../server/core/octacity-server-artifacts/src/lib.rs);
- [cache session](../../server/core/octacity-server-cache/src/lib.rs);
- [Trigger Occurrence](../../server/core/octacity-server-trigger/src/lib.rs).

Each transition is a pure `state + event -> new state` decision. Persistence,
audit, outbox publication, and wakeups are coordinated outside these modules so
the same interface is used by production callers and table-driven tests.
