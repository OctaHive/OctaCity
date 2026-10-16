## Context

See [proposal.md](proposal.md) for motivation and the [GitHub source contract](specs/server/github-work-source/spec.md) for observable behavior.

Observed implementation boundaries:

- `server/application/src/factory_admission.rs` currently accepts `AdmitManualFactoryWorkCommand`, probes replay before repository resolution and publishes through `FactoryAdmissionStore`. It hardcodes the `manual` source, so an installed connector cannot safely masquerade as a manual caller.
- `server/core/octacity-server-store/src/factory_admission_model.rs` already scopes external identities by source and accepted security scope and retains immutable admission intent. Extend that authority; do not add a second deduplication database or reimplement the pipeline's business duplicate analysis.
- `server/application/src/factory_work_status_outbox.rs` already dispatches accepted `set_status` effects under independent outbox fences and retry policy through `FactoryWorkStatusProjector`.
- `server/infrastructure/octacity-github` currently implements the Delivery protocol, private installed configuration and credentials, pinned Octocrab transport, and bounded verified process execution. It does not read Issues or implement a Work Reporter.
- `ui/src/features/factory/ConfigurationEditor.tsx` already selects visible Factory dependencies through generated REST types. Add an input section there or in its adjacent configuration detail rather than create a second pipeline designer.
- `ui/src/features/factory/FlowGraphEditor.tsx` and `FlowBindings.tsx` already author ordinary trusted-action bindings; use those surfaces for stage-local source effects.

The existing [extension boundaries](../../../docs/architecture/factory-extension-boundaries.md) deliberately reserved this change. The approved generic Flow and non-authoritative reporting contracts remain controlling.

## Goals / Non-Goals

**Goals:**

- Attach an operator-configured Issue source to a pipeline input and make its selectors, exact target and reporting effects inspectable before activation.
- Support the same Issue across independently configured assessment and implementation inputs while reusing existing admission, eligibility, duplicate handling, Flow execution and reporting.
- Keep GitHub payloads and network access in the GitHub provider; retain authorization and durable orchestration in the server.

**Non-Goals:**

- No hardcoded labels, universal approval vocabulary, defect/feature dispatcher, automatic model decision, or new Flow primitive.
- No webhook ingress, GitHub Projects board/status-field API, Issue comment/attachment crawling, automatic label creation, arbitrary search-query language, or cross-forge implementation in this slice. Polling plus Issue labels and optional open/closed state supplies the requested behavior.
- No new business duplicate detector, browser token entry, implicit re-run on an Issue edit, live paid-model qualification, automatic merge, redesign of the whole console, or manipulation of the user's running stand.

## Decisions

### 1. A stable input binding selects an exact Factory target

Introduce a Project-owned Work Source input aggregate with immutable versions and one current enabled version. Its stable identity survives selector and target-version edits. A version binds installed Source capability/configuration, repository, exact target Factory Configuration, source/base policy, bounded input mapping, selector, poll policy and optional Reporter mapping. The selected Factory Configuration already pins its root Flow closure. In this document, “pipeline input” means this entry into existing Factory execution, not a replacement for the ordinary CI/CD Pipeline aggregate.

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

### 3. Reuse replay with an input-scoped stable Issue identity

The trusted connector installation resolves its allowed Project and source security scope. Refactor manual admission only after existing public behavior passes; extract or add a source-neutral application entry that preserves visibility, immutable definitions, input validation, replay-before-resolution, audit and atomic publication. Do not accept arbitrary claimed management identities from the provider process.

Construct a canonical admission identity from source installation, repository immutable identity, Issue stable item identity and stable input-binding identity. Issue number is display/lookup context and is verified against those identities. Binding version, current labels, observed timestamp, token and current target version are deliberately absent from this key. Retain them with the original intake snapshot/intent instead.

For repeated observations, recover the retained intent before preparing a new candidate: a changed Issue body must not cause an exact replay request with fresh artifacts or a new mutable base resolution. New candidates freeze title/body, selected labels/state, update identity and explicit task/acceptance mapping into bounded Artifacts; they then use existing admission. Durable pending preparation and input-owner fencing preserve the first accepted intent across crashes before or after admission. Artifact staging uses stable operation identity and bounded retention/cleanup. Missing acceptance data follows the configured mapping's declared failure, never fabricated criteria.

An Issue admitted once to the assessment input may subsequently be admitted once to a distinct implementation input. Repeated observations, relabeling and input replacement cannot re-run either. Existing pipeline business duplicate checks still run when that pipeline first processes Work. The source identity distinguishes intended stages, not a mechanism to bypass those checks. Diagnostics correlate both admissions to the same Issue. An intentional fresh processing cycle requires an explicit new input identity; ordinary retry stays the existing Run/attempt mechanism.

Alternative: use only Issue ID, blocking every later pipeline; or use Issue update/version/labels as dedup keys, creating loops after our own writes. Both fail the requested staged flow.

### 4. Exclusive groups make ambiguous downstream selection visible

Independent inputs may legitimately select the same Issue. Operators can explicitly place alternative implementation inputs in an exclusive group. One bounded selection pass evaluates the group's pinned member selectors against the same candidate snapshot. More than one match produces an ambiguous disposition and no admission; zero is filtered; one is submitted to its selected target. The group's current-version precondition and fenced scan prevent members being evaluated against different group revisions.

This avoids silently choosing a defect or feature pipeline when both label sets are present. No global label-based exclusivity is inferred. A later operator correction can make a previously ambiguous, unadmitted Issue eligible. Already admitted identities remain unchanged; editing labels does not cancel, migrate or replace running Work. GitHub edits cannot be atomically locked with database admission: selection is bound to the retained observation, not a promise that external labels stay unchanged.

### 5. Polling first, with retained progress and reconciliation

Extend the existing GitHub plugin with separate Source and Reporter protocol entry points, independently advertised capability versions and credential classes; reuse its common transport and verified host process mechanisms. Do not create a separate GitHub plugin per stage or allow the Delivery grant to authorize Issue operations. Source requires repository-scoped Issue-read authority; Reporter requires separately granted Issue-write authority. Secrets remain private installed handles, never Work metadata.

Use repository Issue listing rather than unrestricted Search. GitHub's [Issues API](https://docs.github.com/en/rest/issues/issues#list-repository-issues) returns pull requests as well as Issues; reject records carrying `pull_request`. Apply local selector validation after decoding bounded responses. Pagination links are untrusted and must stay within the installed host/repository policy. Honor [GitHub retry guidance](https://docs.github.com/en/rest/using-the-rest-api/best-practices-for-using-the-rest-api), original deadlines and finite retries.

Retain per-input/group progress, scan windows, pending candidate intents, owner/fence and safe dispositions. Drain pages within configured per-pass limits; checkpoint only covered work. Use overlapping update windows plus periodic fair bounded full reconciliation, stable observation identities and existing admission replay. Do not assume timestamps or ETags alone provide exactly-once intake or a provider snapshot. Bound concurrency, bytes, pages and retries; expose backlog/cursor age rather than scan without a ceiling. Disabled inputs start no new preparation/admission; already committed Work and pending reporting keep their original policies.

### 6. Report configured desired effects through the existing outbox

Adapt the existing `FactoryWorkStatusProjector` to the verified Reporter process contract. Resolve only a pinned status mapping and the original source Issue association. A mapping declares logical status value, managed label groups, labels to add/remove and optional Issue state; `approved`, `rejected` or any other spelling is ordinary configuration. No action is inferred from model prose or a terminal outcome unless the Flow explicitly selects that action.

Every stage's declared outcome can transition through an ordinary `trusted_action` node that queues a source update. This includes intermediate/nested stages and configured failure/escalation branches; the Run need not be terminal. In the existing Flow editor the operator selects “update source labels”, selects labels to add/remove, and reviews the resulting exact mapping/action binding. Authoring can resolve a named installed effect preset or publish the reviewed versioned mapping with the selected labels; the admitted action always pins the resolved identity, not a mutable preset. This is presentation over the existing `set_status` and outbox contracts, not an automatic callback that bypasses the graph. Separate outcome routes can select different effects, and a stage with no selected action writes nothing. Retain stage/outcome/action provenance with each projection so reporting progress can be inspected before the pipeline ends.

Use the [GitHub label endpoints](https://docs.github.com/en/rest/issues/labels) to add/remove only managed labels, not replace the entire Issue label collection. Validate mapped labels using bounded repository choices and report missing/renamed labels; do not auto-create them. Multi-request projection is not atomic: retain original desired effects, inspect after unknown responses and converge partial progress. A durable per-Issue managed-group projection order shared by relevant reporters prevents a late older operation from restoring a superseded value. Recheck the current fence and latest authorized intent before each write; if an in-flight stale request lands late, reconciliation restores the newest desired state. Report unreconcilable external edits as conflicts rather than claim atomic compare-and-swap support from GitHub.

Share the existing reporting retry/outbox authority; add only the correlation/order state needed for external effects. Freeze adapter/mapping versions per intent. Neither projection completion nor GitHub labels reconstruct accepted Factory evidence. A matching downstream input still creates ordinary Work under its own gates; importing an upstream accepted artifact would require an explicit trusted provenance mapping, not interpreting the word `approved` as evidence.

### 7. Guided management configuration and safe diagnostics

Expose versioned bindings/groups through ordinary management commands, query and choice collections, validation and confirmed publication. Select installed resources by server-filtered aliases/references, not browser-entered tokens or raw adapter configuration. Expose typed label predicates, bounded repository label choices with explicit unavailable/missing states, input mapping and status effects. A bounded read-only preview performs real provider reads but cannot change production progress, reserve Work, write labels or create a Run.

Add a pipeline input section and source/reporting detail using existing Factory UI patterns. Show exact target, effective predicates, exclusive group, enabled state, safe last progress/retry, matching/disposition counts and visible Work/Run correlations. Preview exposes safe summaries rather than raw Issue bodies; errors distinguish no matches from unavailable/rate-limited or hidden data. Keep drafts in component memory, use current-version/idempotency helpers and complete English/Russian text. No redesign dependency on the separate Work-card or quickstart proposals.

Extend the existing Flow action editor with typed per-outcome “labels to add / labels to remove” controls and a review of the pinned mapping and affected source. The UI must expose this at every stage, not only in pipeline input setup or final status configuration. Published action bindings remain ordinary graph nodes with the same permissions and finite outcomes as other trusted actions. Show queued/applied/exhausted reporting beside its originating stage, without delaying unrelated pipeline execution merely for external projection availability.

## Risks / Trade-offs

- [Own label writes trigger new observations] → Stable binding-scoped admission keys and replay prevent loops; downstream bindings are intentional separate consumers.
- [Filters change while discovery is paginated] → Pinned group/binding revisions, overlap and fair reconciliation; never treat an incomplete scan as complete.
- [Multiple pipelines write the same status group] → Explicit managed groups, durable Issue-level ordering and conflict/convergence evidence; preserve unrelated human labels.
- [External labels change between read and admission/write] → Retained snapshot semantics, explicit conflicts and reconciliation; no claim of distributed atomicity with GitHub.
- [Task text is private or adversarial] → Bounded private Artifacts, explicit mapping and permissions, no implicit resource fetching, safe diagnostic projections and secret-redaction tests.
- [Cross-pipeline identity accidentally disables existing duplicate policy] → Assert that staged inputs reuse original admission/lifecycle contracts and configured eligibility checks rather than accept classification from labels.
- [Provider outage blocks Factory] → Input-scoped deferred progress and independent reporting; ordinary Builds and manual intake remain available.

## Migration Plan

1. Add new input/progress/correlation state and capability contracts without altering existing manual admission identities, retained Runs or Delivery installation semantics. Test migrations and retained-row compatibility through existing store contracts.
2. Deploy compatible server/plugin versions with source inputs disabled. Install separately scoped read/write grants and explicit repository/label mappings; expose only supported choices.
3. Validate and preview sample inputs, then enable a controlled disposable-repository pilot with fixture assessment outputs. Verify first admission, status projection, downstream selection, restart/lost responses and ordinary CI/CD without paid model calls. Live GitHub writes require an explicitly selected test repository and credentials; no user's repository is used implicitly. Hermetic HTTPS/process conformance is required independently and is not called live account qualification.
4. Enable real inputs only after their provider-boundary qualification and operator review. Retain exact source/plugin/configuration evidence and ordinary CI/CD regression results.
5. Roll back by disabling inputs to stop new admissions and pausing the affected Reporter dispatch if required; retain pending intents, cursors, Work and audit. Existing Runs continue under their pinned definitions. Do not drop state or reverse externally applied labels automatically; reconcile later using the retained mapping and newest intent.
