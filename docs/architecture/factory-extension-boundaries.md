# Factory integration follow-ups

The current Factory slice admits Work through the management application and
executes an immutable Flow. External ticket discovery, concrete GitHub/GitLab/
Jira Work Source and Work Reporter adapters, and an Octa-local JEV task plugin
remain separate implementation changes. The GitHub Delivery capability does
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
reserve admission identities nor change running Work. The future store/worker
integration must resolve the current group and commit selection/admission under
the same transaction and scan fence; an in-memory revision check alone does not
provide durable ownership or lock external tracker edits.

The current manual command demonstrates those common admission invariants; it
uses the management security scope and the `manual` source kind. A concrete
connector needs its own installed ingress/identity contract. Do not impersonate
another management scope or describe the manual endpoint as an installed
ticket poller. Reusing the admission authority may require an additive adapter
seam, but must not change the Flow interpreter or weaken its checks.

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
