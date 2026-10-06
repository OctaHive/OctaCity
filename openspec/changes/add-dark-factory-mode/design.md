## Context

See `proposal.md` for motivation and `docs/planning/proposals/dark-factory-architecture.md` for the broader target architecture. OctaCity is already a layered Rust modular monolith with immutable Project/Pipeline/Build Configuration versions, durable Trigger -> Build -> Attempt -> Job orchestration, a global ready queue, fenced Agent leases, signed JobSpec v1/v2, generic Octa runner events and outputs, PostgreSQL authority, S3-compatible immutable bytes, management authorization, REST/OpenAPI, and a separately deployed Operator Console.

The OctaCity release path pins Octa `v0.5.0` at source revision `8b4269eff298dccadf38bc7011b759464fcdc1e1`. That release contains the official `octa_plugin_codex`, machine-readable plugin and Codex CLI `0.130.0` compatibility metadata, explicit secret/public environment mappings, sanitized bounded JSONL, process-tree cleanup, schema-validated result records, and generic Artifact/report publication. OctaCity verifies the release checksum inventory, exact source and platform identity, Codex metadata, plugin lock, manifest, executable digest, license, and provenance contract before publishing a local installation.

Existing JobSpec execution names repository Octa tasks and passes explicit variables and arguments. It does not carry protected server-owned task inputs, a generated Octafile, a Factory Permission Set, or a candidate ChangeSet. Existing VCS integration is intentionally read-only. No Factory Run, Stage Attempt, ChangeSet, Evidence Manifest, Evaluation Plan, Assessment, Decision, or delivery tables and application operations exist.

The design must preserve the current dependency direction:

```text
REST / workers -> application commands and queries -> core decisions and ports
composition root -> selected infrastructure and process adapters
Agent / Octa      -> shared versioned execution contracts only
```

Provider SDK types, model messages, forge payloads, SQL rows, HTTP DTOs, and UI state must not enter factory domain values. New crates are created only with a working vertical slice; empty seam packages are prohibited.

## Goals / Non-Goals

**Goals:**

- Keep ordinary CI/CD runnable, releasable, and regression-tested without Factory mode, model credentials, or factory workers.
- Add one durable code-owned lifecycle above Builds with exact state, bounded work, idempotent side effects, fencing, restart recovery, and human escalation.
- Treat LLM execution as bounded typed calls inside a program-owned call DAG, following the LLM-as-Code separation: code owns flow; models own only reasoning, generation, or assessment within a call.
- Make inter-stage memory explicit through immutable typed Stage Handoffs and reproducible Context Manifests so no provider session is correctness state.
- Allow optional probability-backed routing and tool-risk judgments through a provider-neutral Decision Signal contract while keeping every executable transition and permission decision in deterministic policy.
- Reuse existing Build, Job, scheduling, Agent, Octa, Artifact, audit, retention, authorization, REST, and UI boundaries rather than creating a second execution platform.
- Integrate the concrete official Codex task first and defer a shared harness abstraction until a second production implementation proves one.
- Establish enforceable permissions, exact ChangeSet/evidence identity, independent evaluation, deterministic decisions, and idempotent delivery-for-review before permitting unattended backlog processing.

**Non-Goals:**

- Replacing ordinary Build orchestration, teaching Octa about backlog or delivery, or teaching the Agent about Codex semantics.
- Automatic merge, arbitrary repository-driven factory policy, dynamic Agent provisioning, public-network console access, or browser authentication in this change.
- Preserving a provider session as correctness state, replaying hidden chain-of-thought, or feeding a complete accumulated transcript into each later model call.
- Binding Factory correctness to JEV, OpenAI Decisions API, another provider, a provider-specific response shape, or a mutable `latest` model alias.
- Building a production Repository Knowledge/RAG service, cross-Project knowledge base, or semantic-memory UI in this change; a later change may produce bounded repository fragments for the same Context Manifest contract.
- Supporting every model, forge, Work Source, operating system, and execution backend in the first vertical slice.
- Introducing universal `coding` or `evaluator` plugin facades before multiple real implementations need them.

## Decisions

### 1. Keep CI/CD and Dark Factory as separate application lifecycles over one execution substrate

The existing Trigger and manual Build paths remain unchanged. Factory mode adds admission and reconciliation above the Build application. A Factory Stage Attempt creates one ordinary Build, records its Build identity, and observes its authoritative terminal state. Job materialization, DAG progression, placement, leases, cancellation, retry, events, logs, outputs, and Build Result retention remain owned by existing modules.

Factory availability is independently reported. With no enabled Factory Configuration, model/evaluator/delivery adapters and factory workers are not readiness dependencies. A Factory provider outage may block a Factory Run but cannot make an otherwise healthy ordinary CI/CD deployment unavailable.

Embedding Factory states into `BuildState` was rejected because implementation, validation, evaluation, rework, and delivery span multiple exact revisions and Builds. Representing ordinary Builds as single-stage Factory Runs was rejected because it would make all CI/CD depend on a larger lifecycle without adding value.

### 2. Add one deep factory core, not a set of placeholder crates

The first vertical slice adds `octacity-server-factory` as the core owner of bounded factory identities and values, pure lifecycle decisions, state-machine invariants, permission intersection, budget accounting, call-DAG rules, candidate/evidence binding, and deterministic Decision evaluation. Transport-independent handlers and projections remain in `octacity-server-application`; operation-shaped atomic ports remain in `octacity-server-store`; SQL remains in `octacity-server-store-postgres`; REST DTOs remain in `octacity-server-api-rest`.

Work Source, evaluator, and delivery abstractions begin as narrow application/core ports. A versioned process-protocol crate is added only together with a real out-of-process implementation and conformance fixtures. The initial manual Work Source uses the management application directly. The initial Codex implementation and evaluator use the released Octa plugin. The first write-capable forge adapter earns a delivery protocol and verified process host in the same slice.

Splitting Work, Factory, Evaluation, and Delivery into empty crates immediately was rejected. These concepts remain explicit internal modules and types inside the deep factory core until their dependency or release boundaries differ in deployed code.

### 3. Persist immutable definitions and append-only execution history

PostgreSQL will store stable Factory Configuration identities plus immutable version rows. A Work Envelope is immutable after admission. A Factory Run has a monotonic aggregate version and current-state projection, while Stage Attempts, Stage Handoffs, Context Manifests, call nodes, Decision Signal Requests and Receipts, ChangeSets, Evidence Manifests, Evaluation Plans, Assessments, Decisions, escalations, delivery attempts, reporter attempts, audit facts, and outbox records are append-only.

Current pointers are projections backed by foreign keys to immutable rows; they are not replacements for history. Every transition transaction locks the Factory Run, verifies its expected version and current claim fence, applies one pure decision, appends its facts and outbox work, updates the current projection, and increments the version. Stable identities for external and Build side effects are derived from Factory Run, stage, attempt, operation kind, and immutable input digest so a replay observes the original result.

Large task bodies, prompts, traces, ChangeSet bundles, reports, and evidence bytes remain in the Artifact Store. PostgreSQL stores bounded metadata, digests, logical object references, ownership, ordering, and retention state. Raw provider messages and credentials are never persisted.

An event-sourced aggregate with no current projection was rejected because most operator and worker decisions need bounded indexed current-state reads. Mutable stage rows were rejected because they obscure retry and rework history.

### 4. Use a fenced reconciler and pure next-action decisions

One supervised Factory reconciler claims bounded eligible runs with owner, expiry, and fence. It loads the complete bounded factory snapshot and calls a pure decision function that returns one of: no action, request an optional Decision Signal, create a Stage Attempt, create a Build, wait for linked Builds, capture a candidate, construct evidence, plan evaluations, decide, request rework, escalate, request delivery, report, cancel, or complete. The store commits the transition and durable outbox work before a worker performs an external side effect. A recorded signal is only another typed input to that function; it never commits its own transition.

External observations are checkpointed once under the claim before later transitions consume them. Process timers only wake work. Lost claims expire; stale completions fail their fence. WIP, stage concurrency, provider concurrency, attempt, elapsed-time, token, cost, and output budgets are authoritative inputs to the pure decision.

Using one long-running async task per Factory Run was rejected because process memory, timers, and provider sessions would become hidden correctness state. Driving transitions from LLM text was rejected because mandatory validation, joins, budgets, and completion would become probabilistic.

### 5. Represent reasoning as a durable macro call DAG

OctaCity records a macro call node for every bounded implementation, evaluation, or rework model invocation. A node binds its parent call or stage, immutable Context Manifest identity and digest, selected plugin/executable/model/prompt/schema identities, budget, terminal typed result, bounded summary, usage, and full trace Artifact reference. Parallel evaluators are sibling nodes; deterministic program logic owns fan-out, joins, retries, substitution, and abandonment.

Each completed model-backed Stage Attempt publishes one immutable, schema-validated Stage Handoff. The handoff contains only bounded typed information required by later code-owned stages: outcome, summary, decisions and assumptions that affect the candidate, unresolved items, changed-component references, validation observations, prior findings, and exact Artifact, ChangeSet, Evidence, policy, and provenance references. It contains neither credentials nor hidden chain-of-thought, and later stages never have to rediscover required prior state through text search.

Before dispatching a reasoning call, deterministic application code constructs one immutable Context Manifest. The manifest gives every selected input a stable order, source kind, logical identity, revision or subject binding, Artifact or repository range reference, content digest, encoded size, inclusion reason, and provenance. Construction walks only declared stage/call dependencies and includes explicit typed outputs or bounded summaries selected by the parent function. The same immutable inputs and policy produce the same manifest bytes and digest; replay consumes the recorded manifest rather than repeating discovery. Full child transcripts remain audit/replay evidence and are not automatically copied into a sibling or parent model context.

Repository Knowledge is a separate non-authoritative discovery subsystem, not Factory memory. A future change may build revision-bound hybrid symbol, lexical, graph, and vector indexes and return bounded repository fragments plus a Retrieval Receipt containing the exact repository revision, index and embedding identities, normalized query, retrieval policy, stable ranking, ranges, and content digests. A Factory call may consume only results frozen into its Context Manifest. Retrieval cannot omit required Stage Handoffs or deterministic evidence, grant authority, select a lifecycle transition, or silently change a dispatched or replayed call. The first implementation uses exact references, typed handoffs, and the coding harness's bounded workspace tools; it does not add an empty retrieval port before a concrete Repository Knowledge implementation exists.

The first implementation does not attempt to expose Codex's internal tool-call graph as factory control flow; that provider trace remains opaque evidence below one macro call.

A flat conversation transcript was rejected because context and cost grow with total steps and the model must reconstruct workflow position. Persisting hidden chain-of-thought was rejected for security, portability, and provider-compatibility reasons. Treating mutable RAG results as implicit call input was rejected because index drift, ranking drift, authorization mistakes, and repository prompt injection would make replay and evaluation isolation unverifiable.

### 6. Introduce negotiated JobSpec v3 for protected managed execution

JobSpec v1/v2 remain byte- and behavior-compatible for ordinary CI/CD. Factory-owned Builds require a negotiated v3 execution contract. V3 retains provider-neutral source, Octa, runtime, cache, and output sections and adds:

- a generic optional factory-causality block containing only logical identities and digests;
- a bounded protected-input manifest with logical Artifact identities, sizes, SHA-256 digests, media types, and reserved portable destinations;
- a managed Octa execution descriptor identifying the protected generated Octafile and exact task names;
- the typed Factory Permission Set and required enforcement capability vocabulary.

Lease acquisition supplies short-lived protected-input transfer capabilities outside the signed stable template, as output upload already separates logical intent from credentials. The Agent verifies bytes and materializes them into a reserved read-only subtree inside the execution root. A server-generated Octafile, prompt, result schema, Task Envelope, Context Manifest, selected Stage Handoffs, criterion packs, and previous typed findings live there. The writable source tree and separate scratch/output roots are explicit mounts. Factory Jobs are ineligible for Host or another backend that cannot protect this layout and all required permissions.

Overloading v2 with optional fields was rejected because strict decoding and compatibility are security properties. Letting the repository own the factory Octafile was rejected because it would allow untrusted source to alter the selected model task, prompt mapping, result schema, environment selection, or required deliverables.

### 7. Compile the Task Envelope to a concrete Codex task at the trusted boundary

The Factory Codex adapter consumes a validated immutable Task Envelope and creates a minimal server-owned Octafile containing exactly one `codex` task for the selected stage. It uses an inline or protected prompt, a strict result schema, explicit public and secret Octa variable mappings, exact source revision, fixed run-record directory, and required generic deliverables. It cannot add permissions; it only projects already-intersected intent.

The OctaCity release pin, local-stand immutable input manifest, Agent installation verification, plugin lock, release evidence, and backend contracts are updated together to a released Octa revision containing `octa_plugin_codex`. The plugin's supported Codex CLI and absolute executable identity remain operator-installed, pinned, verified, and outside the repository workload's write authority.

The server and Agent continue to see generic runner events, reports, Artifacts, usage, and terminal status. Codex-specific configuration is confined to the adapter and generated Octafile. A universal coding plugin was rejected until a second production harness provides evidence for a common interface.

### 8. Intersect permissions once and enforce them at every boundary

The canonical Factory Permission Set is a typed domain value. The application computes it as the intersection of Factory Configuration, Task Envelope, immutable effective Project/Build policy, and the locally advertised Agent/backend ceiling. It covers plugin and executable identities, tool identities, commands plus argument patterns, descendants, workspace/scratch/input roots, mount modes, network hosts, secret/workload-identity profiles, CPU, memory, disk, processes, time, Artifact/report counts and bytes, and permitted output kinds.

Signed intent carries the result and the semantic enforcement capabilities required. Placement matches those capabilities; the Agent revalidates local grants, resolved paths, executable identity, profiles, and backend immediately before source or process activity; the backend enforces mounts, network, processes, and resources; the output and secret paths independently enforce their subsets. Each layer can narrow but never widen. Unknown or unenforceable fields fail closed without weaker fallback.

The initial factory deployment profile requires a qualified isolation or virtualization backend. Codex's own sandbox is defense in depth, not the product boundary. Prompt-only permission instructions and a free-form permission map were rejected because neither is complete or enforceable.

### 9. Capture one trusted Git bundle ChangeSet per successful writable attempt

After the runner exits and before workspace cleanup, the Agent's trusted lifecycle invokes a ChangeSet capture component outside the harness. It re-resolves the owned source root, rejects unsafe symlinks, submodule boundary escapes, forbidden paths, excessive file count or bytes, untracked secret patterns, and repository control-file modifications forbidden by policy. It stages allowed changes without hooks, creates one commit with server-selected author/committer identity and Stage Attempt time, verifies ancestry against the exact base, and emits:

- a Git bundle containing the exact candidate commit and required base relationship;
- a canonical changed-path and mode manifest;
- a bounded textual patch for operator evidence when allowed;
- digests, sizes, capture-tool identity, and Stage Attempt provenance.

The Agent uploads these as generic outputs and completes the Job only after their verified publication. The server accepts the ChangeSet only when output provenance, expected names, bundle and manifest digests, base revision, and Stage Attempt fence agree. Later Builds materialize the original Repository at the exact base and apply the verified bundle without hooks to reproduce the exact candidate. Evaluation uses a read-only candidate mount plus separate scratch/output roots; deterministic validation may use a disposable writable worktree but cannot redefine candidate identity.

Publishing directly from the coding harness was rejected because it grants forge authority and makes the external branch the only copy of unfinished work. A plain patch without candidate commit identity was rejected because binary files, modes, renames, and reproducible exact revision materialization are weaker. Server-side workspace capture was rejected because the server never materializes build worktrees.

### 10. Treat decision models as replaceable signal providers, not authorities

The factory core defines a small provider-neutral `DecisionSignalProvider` port over canonical typed requests and results. A request contains one bounded redacted state document, one or more versioned questions, finite answer or score domains, purpose (`routing` or `tool_risk`), subject and policy digests, deadline, and budget. A result contains only schema-valid typed answers, normalized probability or confidence data when the selected capability provides it, usage, and provider/model provenance. Provider discovery advertises supported input media, question kinds, probability semantics, limits, data-handling class, latency/cost metadata, and exact model/version identity. Factory Configuration selects a compatible logical profile; provider SDK and wire types remain in infrastructure adapters.

JEV is the first concrete adapter. A future OpenAI Decisions API or another decision-model service is a separate adapter behind the same port once its public contract and probability semantics are stable. The common contract does not assume identical calibration: thresholds, margin rules, allowed question kinds, and evaluation corpora are versioned per purpose and provider/model identity. Mutable aliases such as `latest` may be resolved at configuration publication, but an admitted Factory Run freezes an exact adapter, model, question set, policy, and input digest.

A routing assessment may rank or choose only the finite outgoing edges declared by the immutable Factory Configuration. Required validation, evaluation, budget, escalation, delivery, and human-review gates are not candidates and cannot be skipped. Deterministic routing policy consumes the signal plus current authoritative state and either follows one declared edge, selects a configured fallback, or escalates. It never accepts a provider-supplied stage or transition name.

A tool-risk assessment occurs only after the proposed action has been normalized and proven to fit the intersection of Factory Permission Set, Task Envelope, Project policy, local Agent grants, and backend capabilities. Deterministic hard-deny rules run first; deterministic hard-allow rules may avoid a provider call. Only an ambiguous action already inside that envelope may be assessed. The result can narrow execution to deny or escalate, but it cannot add a tool, command, argument, path, host, credential, descendant, resource, or output capability. The Agent and backend remain the final enforcement boundary immediately before the protected operation.

Routing calls originate in the server. Tool calls require a blocking pre-execution hook in the selected pinned harness adapter: the trusted Octa plugin forwards a canonical proposal to an Agent-local authorization broker bound to the current Job, lease and fence; the broker applies signed deterministic policy locally and sends only an ambiguous redacted in-envelope assessment request to the server-side Decision Signal adapter. Provider credentials never reach the Agent workload or plugin. The action remains blocked until the broker receives the recorded deterministic disposition, then the Agent and backend revalidate the unchanged action. Cancellation, timeout, stale fence, broker loss, or receipt mismatch denies the action. A harness or plugin without the advertised blocking-hook capability is ineligible for `tool_risk` bounded control rather than running with a best-effort gate.

Every call uses a stable logical request identity and produces one immutable Decision Signal Receipt containing canonical input, question-set and policy digests, exact provider/adapter/model identities, typed answers, probabilities or confidence semantics, usage, latency, terminal classification, and the deterministic disposition that consumed it. Unknown-response recovery observes or reuses the recorded receipt; retry never silently queries a different model for the same logical decision. Timeouts, transport failures, invalid results, unsupported capabilities, or confidence below the configured floor follow an explicit fail-closed fallback (`deny` or `escalate`). Raw provider payloads, credentials, prompts, repository secrets, and unbounded command text are not persisted or exposed.

Rollout is independently versioned per purpose as `shadow`, `advisory`, or `bounded_control`. Shadow mode records comparison evidence but cannot affect execution. Advisory mode presents a recommendation while deterministic baseline policy acts. Bounded control permits the signal only in the two constrained seams above after an evaluation corpus, calibration thresholds, drift limits, circuit breakers, and an audited promotion have passed. Ordinary CI/CD and Factory configurations without a Decision Signal profile remain fully functional.

Calling JEV directly from the Factory Controller was rejected because it couples domain behavior to one provider. Letting a decision model authorize an otherwise forbidden action, invent an edge, skip a mandatory gate, or produce the final Evaluation Decision was rejected because probabilistic output is not authority. Re-querying on retry was rejected because model and calibration drift would make one durable transition non-reproducible. One global confidence threshold was rejected because providers and routing/tool-risk purposes have different calibration.

### 11. Separate facts, assessments, and decisions

Validation Builds publish generic reports and Artifacts. A trusted application projection constructs the Evidence Manifest only from verified retained outputs bound to the exact candidate, producer Build/Job, plugin/tool identity, schema, digest, and freshness. Deterministic failed gates are direct Decision inputs and are never paraphrased by a model.

Criterion Packs are operator-owned immutable artifacts with stable schema and digest. An Evaluation Plan binds exact candidate, Evidence Manifest, packs, evaluator selections, budgets, deadlines, data handling, and quorum before any evaluator starts. The initial evaluator is an independent read-only Codex invocation executed as an ordinary Factory evaluation Build through the same concrete Octa plugin but a distinct Task Envelope, prompt/schema, secret profile, call node, and workspace policy. Its output is parsed into a provider-neutral Assessment; transport or protocol failure is not an `indeterminate` assessment.

The Decision Engine is pure code. It combines required deterministic gates, immutable Assessments, quorum, severity thresholds, and `indeterminate` policy into a typed Decision. It cannot average arbitrary scores or accept prose. Required missing, failed, or indeterminate input never silently passes. Rework creates a new Stage Attempt, ChangeSet, Evidence Manifest, Evaluation Plan, and Decision.

Allowing an evaluator to run arbitrary evidence tools was rejected because it conflates facts and judgment. Letting the implementation call review itself in the same session was rejected because it shares writable state and context. A dedicated universal evaluator plugin is deferred until another real evaluation transport requires it.

### 12. Deliver through a separate verified GitHub adapter and stop at review

The initial concrete delivery slice adds a provider-neutral versioned delivery process protocol, a verified adapter host reusing registry/digest/framing/timeout/process-cleanup patterns, and an `octacity-delivery-github` adapter. The adapter receives a protected credential handle and bounded request containing repository identity, exact base/candidate/ChangeSet/Decision digests, stable delivery identity, target branch policy, and safe review metadata. It can observe, publish the exact candidate branch, and create or update one pull request. It cannot merge in this release.

Observe-before-retry uses the stable delivery identity in branch/review metadata so a lost response does not create a duplicate. The server checkpoints the first accepted external identities and later verifies that the remote head still equals the accepted candidate. Delivery credentials are available only to the adapter process and never to coding or evaluation Jobs.

Adding GitHub-specific cases to the Factory Controller or VCS read protocol was rejected. Reusing the coding Job to push was rejected because it crosses the strongest trust boundary. Generic automatic merge was rejected until risk classification, independent/quorum review, calibration, rollback, and audited enablement are separately implemented.

### 13. Keep REST authoritative and make the console a complete optional management surface

REST adds versioned Factory Configuration resources, authorization-safe bounded choice collections needed to author them, manual Work admission, Factory Run collections and details, bounded child collections, cancellation, eligible retry, escalation disposition, and delivery-for-review. Every command uses existing management authorization, idempotency, request correlation, audit, and strong-precondition patterns. Visibility is applied in store queries before filters, ordering, counts, evidence projection, and cursor construction. OpenAPI remains the source for generated UI types.

The Operator Console is a complete management client rather than a view-only dashboard. It adds a Factory activity to the existing rail, a bounded `Project -> Factory Configuration -> Factory Run` explorer, guided Factory Configuration creation and replacement, manual Work admission, URL-backed filters, deep-linked run/stage/evaluation diagnostics, and confirmed commands through the shared mutation helper. Configuration authoring uses typed sections for admission, stages and Build Configurations, WIP and hard budgets, permissions, criterion packs, evaluators, rework, delivery, and enabled state. All references come from authorization-filtered REST choices; operators never need to enter opaque resource identifiers or provider payloads.

The configuration editor begins from an empty supported template or the current immutable version, keeps unfinished form data only in component memory, validates each section locally from generated types, and requests authoritative server validation before confirmation. A review step shows the exact normalized request, effective permission narrowing, required Agent/backend capabilities, unavailable selections, and the current-version precondition. Publication creates one immutable replacement through the shared confirmed-intent helper; it never edits the current version in place. Server conflict or capability drift preserves the unsent form long enough to reconcile and retry explicitly, but secrets, task bodies, and configuration drafts are not written to browser persistence.

The console does not infer next state, Decision, compatibility, severity, effective authority, or delivery success. It renders server-published validation and compatibility results and remains optional: the same operations are available headlessly through REST, and existing Projects, Builds, Agents, and Audit activities remain available when Factory mode is absent or degraded.

Embedding factory-only business logic in the console, exposing a raw JSON configuration editor, or requiring WebSockets was rejected. Raw JSON would leak provider shape, make invalid cross-resource references easy, and bypass the console's guided safety review. The existing typed REST, cursor, and bounded-wait patterns are sufficient for the first release and preserve headless parity.

### 14. Make retention, audit, and reporting follow exact factory identity

Factory records reference immutable Build Results and Artifact objects rather than copying bytes. A visible non-terminal, escalated, or held Factory Run retains the Task Envelope, current and accepted ChangeSets, Evidence Manifests, Assessments, Decisions, delivery provenance, and required Build Results. Cleanup first removes logical visibility according to policy and only then deletes unreferenced projection, manifest, and byte data idempotently.

Every accepted state mutation appends an audit fact in the authoritative transaction. High-cardinality task text, paths, prompts, raw findings, provider diagnostics, and external issue identifiers remain out of metric labels. Optional Work Reporter delivery uses outbox/claim/retry state but is never authoritative.

Making an external issue or pull request the Factory source of truth was rejected because provider loss, manual edits, and retries could invent or erase lifecycle state.

### 15. Verify vertical slices before broadening autonomy

Implementation proceeds through thin released-product slices. Each slice must include the core decision, in-memory and PostgreSQL parity, migrations and prior-schema upgrade, REST/OpenAPI where applicable, audit/outbox behavior, failure/replay/fencing tests, documentation, and a released-product contract. Architecture checks must keep provider SDKs in adapter packages and prohibit factory dependencies from entering existing core modules in the wrong direction.

The first end-to-end pilot is manual Work -> Codex implementation -> trusted ChangeSet -> deterministic validation -> one read-only assessment -> deterministic Decision -> at most one rework -> GitHub pull request -> human merge. Unattended polling and auto-merge remain disabled until all negative permission contracts and recovery slices pass on a qualified real backend.

Building all providers and autonomous policies before one complete released slice was rejected because it would create many unproven abstractions and multiply failure modes.

## Risks / Trade-offs

- **[The change is large and cross-cutting]** → Land independently releasable slices with ordinary CI/CD regression gates after every section; do not create unused crates or schemas ahead of the slice that consumes them.
- **[Factory reconciliation duplicates Build orchestration]** → Permit the reconciler to create and observe Builds only; all Job DAG transitions and leases remain in existing modules and are referenced by identity.
- **[JobSpec v3 rollout strands older Agents]** → Negotiate v3 explicitly, require it only for Factory Jobs, retain v1/v2 unchanged, surface incompatible capacity, and update released Agent contracts before enabling Factory Configurations.
- **[Protected generated inputs can be replaced inside a writable workspace]** → Use a reserved verified read-only mount, exclude it from writable roots, revalidate digests before runner start, and reject backends that cannot enforce the separation.
- **[A coding model exfiltrates source or secrets]** → Default to no network, minimal explicit model credential mapping, qualified outer isolation, exact host allowlists, separate credential profiles, redaction, bounded outputs, and real-backend negative contracts.
- **[Permission vocabulary becomes backend-specific or incomplete]** → Define semantic capabilities in shared contracts, keep backend names diagnostic-only, add deny-by-default unknown-field behavior, and require positive and negative conformance for each permission category.
- **[Agent failure loses an unpublished workspace]** → Treat the workspace as disposable; only a verified published ChangeSet completes implementation. A lost pre-capture workspace causes a bounded new Stage Attempt from the exact base.
- **[Git bundles add source-materialization complexity]** → Keep one canonical bundle/manifest format, verify ancestry and digests with hooks disabled, and test round trips for modes, renames, binaries, empty changes, malicious paths, and base mismatch.
- **[The same model family implements and evaluates]** → Use separate read-only calls, prompts, credentials, contexts, and immutable evidence in the pilot; retain human merge and add independent providers/quorum before higher autonomy.
- **[Factory context still grows over long rework loops]** → Persist the macro call DAG, pass only declared ancestor context and typed summaries, bound summary and call depth, and retain full traces only as artifacts.
- **[GitHub delivery couples the product to one forge]** → Keep GitHub behind the first provider-neutral delivery protocol and require Factory domain, REST, and UI types to expose only generic delivery identities and states.
- **[Provider rate limits or outages stall runs]** → Use provider-specific bounded concurrency, durable claims, classified idempotent retry, circuit breakers, budget deadlines, and escalation; do not affect ordinary CI/CD readiness.
- **[Decision-model providers expose superficially similar but differently calibrated probabilities]** → Normalize only structural result shape, retain provider/model-specific probability semantics and calibration policy, require per-purpose corpora and thresholds, and promote each exact model independently from shadow to bounded control.
- **[A probabilistic tool gate is mistaken for an authorization boundary]** → Run immutable permission intersection and hard deterministic deny rules first, permit the signal only to narrow an in-envelope ambiguous action, and keep Agent/backend enforcement authoritative.
- **[A provider upgrade changes a replayed route]** → Freeze exact adapter, model, questions, policy, canonical input digest, and receipt per logical request; configuration replacement affects only later admissions.
- **[Factory retention becomes unbounded]** → Account artifacts and traces against immutable policy, preserve only referenced/held evidence, expose usage, and test safe incremental cleanup.
- **[Self-evolution bypasses review]** → Treat workflow and policy edits as ordinary candidates and prohibit a running model call from mutating its own controlling definitions.

## Migration Plan

1. Release Octa with `octa_plugin_codex`, update the OctaCity source/release pins and immutable local-stand inputs, and prove ordinary Build execution plus opt-in Codex task execution without enabling Factory mode.
2. Ship additive Factory Configuration, Work Envelope, Factory Run, Stage Attempt, call-DAG, budget, claim, and audit schema with workers disabled by default. Verify migration reentrancy, prior-schema upgrade, backup/restore, and ordinary CI/CD parity.
3. Release JobSpec v3, Agent protected-input staging, permission enforcement, generated Octafile support, and qualified backend contracts. Older Agents continue ordinary v1/v2 work and are ineligible for Factory Jobs.
4. Enable manual Work admission and one implementation Build path, then add trusted Git bundle ChangeSet capture and exact candidate rematerialization. Keep evaluation and delivery commands unavailable until their migrations and adapters are present.
5. Add the provider-neutral Decision Signal contract and JEV adapter in shadow mode, persist immutable receipts, and calibrate routing and tool-risk policies independently without changing execution.
6. Add deterministic validation/evidence, read-only assessment, Decision policy, one bounded rework, and escalation. Promote only proven signal policies to advisory or bounded control and run the released PostgreSQL-backed isolated pilot with delivery still disabled.
7. Add the verified GitHub delivery adapter and delivery-for-review command, then add Factory REST discovery and Operator Console workflows. Human merge remains the only merge path.
8. Enable Factory Configuration per Project only after the deployment proves required Agent v3 capacity, backend enforcement, model/evaluator/decision-signal secrets, Artifact retention, and delivery adapter readiness. Default remains disabled.

Rollback disables Factory admission and workers, allows active Jobs to be cancelled through existing Build control, preserves all immutable factory history and outputs for diagnosis, and leaves ordinary CI/CD paths operational. Schema rollback is not required for service rollback; older binaries must reject a database schema newer than they support rather than partially interpreting factory rows.

## Open Questions

- Which second production coding harness will be used in a later change to test whether a shared `CodingHarness` abstraction is justified?
- Which independent evaluator provider and quorum policy will be required before moving beyond human-reviewed pull requests?
- Which future Decisions API provider will be the second Decision Signal adapter, and what exact public probability/calibration contract must it expose before bounded control is permitted?
- Which low-risk classes, rollback guarantees, and emergency controls would be required by a separate future auto-merge change?
