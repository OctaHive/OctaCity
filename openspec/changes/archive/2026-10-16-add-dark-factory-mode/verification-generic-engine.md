# Generic engine migration (9.21–9.26)

Status: completed for tasks 9.21–9.26 and the common-model migration of 10.1.
The common lifecycle, configuration, execution journal, Build origins and operator
controls no longer depend on fixed business stages. The optional bounded owner is
wired through the server supervisor and explicit trusted capability installation.
No staging, commit or push was performed.

The increment records below are chronological; statements about remaining work
refer to that increment. The final verification section records the completed
scope and supersedes intermediate test counts. Concrete provider deployment,
requirements authoring/review and later product pilots retain their separately
planned tasks; completion here does not claim an installed Codex/JEV/forge pilot.

## Public test seams

- Immutable Factory Configuration publication and restoration.
- Configured Flow and Node owners, exact installed executors and retained outputs.
- Strongly preconditioned operator commands and the authoritative store ports.
- PostgreSQL forward migration, recovery, fenced append and retained provenance.

## Verified increments

Baseline: 396 library tests passed before changes.

| Behavior | Observed Red | Observed Green |
| --- | --- | --- |
| Simple Flow needs no candidate/review/delivery settings | Draft deserialization rejected missing `stages`; later regressions detected retained global policies | Common configuration has no stage/evaluation/rework/delivery/credential/signal policy fields; selected capability profiles validate their own contracts |
| Exact executor selection for an unknown action node | Registry API was missing (`E0433`) | Exact reporter binding executes and unrelated binding receives no effect |
| Ordinary node progress has technical Run state | `Running` API was missing (`E0599`) | Common owner persists `running` before ordinary Build dispatch |
| Technical Run state survives PostgreSQL recovery | Store rejected `running` under the prior DB constraint | Configured manual intake/unknown node/queue journey recovers successfully |
| Logical stages cannot be Run states | Parsing `implementing` succeeded | Only admitted/running/waiting/cancelled/completed parse |
| Unreleased schema accepts only technical execution states | Migration 51 rejected `running` with SQL 23514 | Directly revised unreleased migration 47 accepts all five technical states and rejects old stage/outcome names; no compatibility archive is required because the user confirmed there are no deployed Factory Runs |
| Every runtime admission requires its exact data catalogue | A graph without its pinned schemas passed runtime validation | Missing catalogue is rejected; schema-less journal and budget-selection modes are removed from memory and PostgreSQL paths |
| No node can opt into a legacy stage projection | A serialized projection contract was accepted | The field and runtime/journal/queue/Build exceptions are removed; only the rejection test retains the old field name |
| Operator response addresses an exact configured HumanGate | ResolveFactoryNodeCommand was missing (E0422); PostgreSQL lacked FactoryRunControlStore (E0277) | Intent is retained without manufacturing a completion; the owner resumes the same frozen input/attempt and follows its arbitrary declared terminal in both backends |
| HumanGate cancellation has technical execution meaning | Owner rejected accepted cancellation with InvalidInput | Cancellation fences the prior worker and survives restart; no semantic result, reporting or delivery action is invented. Effectful cancellation now uses exact executor adapters and durable technical receipts |
| Valid operator payload is retained at its admitted bound | A 300 KB schema-valid response failed the PostgreSQL control-document constraint | The unreleased constraint permits the bounded payload plus envelope; the same result survives restart and owner completion |
| Evaluation exposes operation-local progress | Public progress remained FactoryLifecycleProgress instead of EvaluationState (E0308) | Evaluation returns only its local state; the historical test driver owns its legacy checkpoint mapping |
| WIP addresses arbitrary nodes | Config serialized max_active_stages; both backends returned zero WIP for a pending ordinary HumanGate | Config exposes max_active_nodes; common runtime and store counts derive unfinished executable attempts, excluding suspended subflow call frames |
| Candidate binds the producing node's frozen input, independent of stage roles | The constructor required StageAttempt instead of input/NodeAttempt and lacked the exact node identity API | An arbitrary node's candidate is retained with the common journal and restored in memory/PostgreSQL with no Stage record; the unreleased FK addresses the Node Attempt and Run |
| Candidate input binding is checked before an atomic append | PostgreSQL accepted a candidate whose retained repository was substituted | Both stores reject the foreign subject before commit, leaving input, completion, candidate and snapshot unchanged |
| Generic Build cancellation observes its existing operation | The Node Build adapter lacked the cancellation API (E0599/E0433) | Cancellation never creates an absent operation, rejects an unrelated overlapping owner, and retains the actual terminal Build/Attempt/Jobs on restart without selecting a business outcome |
| Signed execution provenance binds arbitrary nodes | Strict v3 decoding rejected flow_run_id because it required stage identity and role | Exact Flow/cycle/Node/input provenance replaces fixed stages and predecessor categories; source access follows signed permissions, capture requires write access, and materialization verifies its selected protected inputs |
| Trusted capture records the producing node | Manifest decoding rejected node_attempt_id as an unknown field | Capture, canonical manifest, Agent and server verification share the exact Node Attempt identity; existing artifact and protected-path checks remain |

Scoped executor request refactoring was preceded by the existing behavioral suite
and verified by the Factory application tests; adapters no longer receive the
aggregate snapshot or its legacy capability history.

Command forms: `cargo test -p octacity-server-factory <filter> --lib`,
`cargo test -p octacity-server-application <filter> --lib`,
`cargo test -p octacity-server-factory -p octacity-server-store -p octacity-server-application --lib`,
and, with an explicitly disposable PostgreSQL 18 service,
`cargo test -p octacity-server --test factory_configured_flow <filter> -- --ignored`.
Cargo runs use `CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0`.

The latest full library regression passed 409 tests (181 factory, 159 application,
69 store), with no exclusions. Command:
`cargo test -p octacity-server-factory -p octacity-server-store -p octacity-server-application --lib`.
Logs: `/tmp/octacity-build-cancel-library-regression.log`. This remains an intermediate
regression, not completion of all architectural migration tasks.

Candidate Red/Green logs: `/tmp/octacity-candidate-common-node-red.log`,
`/tmp/octacity-candidate-common-node-green.log`,
`/tmp/octacity-candidate-binding-fence-red.log` and
`/tmp/octacity-candidate-binding-fence-green.log`. Commands used the domain
`evidence_tests` filter and PostgreSQL
`configured_unknown_node_journal_matches_memory_and_survives_restart` contract.
The capture adapter's old lifecycle/Build-link dependency remains to migrate;
the common candidate record no longer depends on it.

Build cancellation API Red/Green logs: `/tmp/octacity-node-build-cancel-red.log`
and `/tmp/octacity-node-build-cancel-green.log`, using application filter
`cancelling_a_configured_build`. The adapter checks ordinary terminal facts;
owner receipts, measured cancellation consumption, and exact plugin cancellation are now implemented (see the verified increment below).
The removed candidate stage field was also observed accepted (Red) and then
rejected (Green): `/tmp/octacity-candidate-legacy-field-{red,green}.log`.

Wire provenance and capture Red logs: `/tmp/octacity-node-wire-provenance-red.log`
and `/tmp/octacity-capture-node-wire-red.log`. The combined Green regression
`cargo test -p octacity-protocol -p octacity-changeset -p octacity-job -p octacity-server-application --lib`
passed 266 tests (60 protocol, 13 capture/materialization, 34 Agent Job and
159 application): `/tmp/octacity-capture-node-wire-green.log`. The released
v1/v2 golden bytes are unchanged. The unreleased Factory v3 fixture is revised
directly, with no fixed-stage compatibility fallback.

PostgreSQL regression passed 20 tests: 13 discovery/journal/pool contracts,
4 retention contracts and 3 schema migration contracts. Command:
`cargo test -p octacity-server-store-postgres --test factory_discovery --test factory_retention --test migrations -- --ignored`.
These checks used the disposable PostgreSQL service; they cover no production
data. Full final validation remains required after the remaining migration.

The configured PostgreSQL owner regression now passed all seven tests, including
technical cancellation, payload-bound and WIP recovery
(`/tmp/octacity-build-cancel-postgres-owner-regression.log`). Full discovery/journal/pool regression
must be repeated after the remaining lifecycle migration; its previous remaining
transaction rollback failure passed after the fixture used a neutral graph checkpoint.

Previous library/test targets passed Clippy with `-D warnings`; the architecture
checker passed (56 packages, 209 dependency edges). Formatting, Clippy and architecture
checks must be repeated after the remaining migration.

Current four Factory library/test packages pass Clippy with all targets/features
and `-D warnings` (`/tmp/octacity-build-cancel-clippy.log`). The architecture
checker again passed 56 packages and 209 edges after the candidate/cancellation
increments. These are intermediate checks, not completion of runtime migration.

After the wire/capture migration, all seven touched library/test packages passed
Clippy with all targets/features and `-D warnings`
(`/tmp/octacity-node-wire-clippy.log`), formatting and `git diff --check` passed,
and the architecture checker again passed 56 packages/209 edges. The common
Build creation facade still exposes the old Stage causality variant; its migration
and the root lifecycle cleanup remain required before 9.23/9.24 can be closed.

User clarification: persisted Factory data is development/test data only. The
unreleased schema is revised directly; the temporary archive migration and its
cleanup were removed. The new schema-boundary test passed after an observed
constraint failure, and all seven migration/retention tests passed.

## Common cancellation receipts and actual stop verification

The owner now freezes the original accepted cancellation time, dispatches to the
exact installed executor, and retains a canonical technical stop receipt before
root finalization. No business outcome or implicit reporting effect is produced.
Receipts, actual execution IDs and measured resources survive PostgreSQL recovery.
A stopped node releases WIP; its receipt contributes consumption to the owning
Flow's budget. Worker transitions cannot clear an accepted cancellation or append
new effects, inputs or semantic results after it.

Observed Red/Green:

- Missing common executor cancellation API: `/tmp/octacity-effectful-stop-red.log`;
  memory owner Green: `/tmp/octacity-effectful-stop-green.log`.
- Accepted worker clearing an operator cancellation:
  `/tmp/octacity-stop-control-fence-red.log`; common memory/PostgreSQL Green:
  `/tmp/octacity-effectful-stop-postgres-green.log`.
- Accepted arbitrary external outbox effect after cancellation:
  `/tmp/octacity-stop-effect-fence-red.log`; the same common Green covers rejection
  without changing the retained aggregate.
- Lost stop receipt after PostgreSQL restart:
  `/tmp/octacity-effectful-stop-postgres-red.log`; receipt persistence Green:
  `/tmp/octacity-effectful-stop-postgres-green.log`.
- Nested unknown ownership fields accepted on receipt restoration:
  `/tmp/octacity-stop-restoration-red.log`; full domain Green below.
- Terminal Build and Attempt with still-cancelling Jobs were prematurely treated
  as stopped: `/tmp/octacity-job-stop-fence-red.log`; actual Jobs checked:
  `/tmp/octacity-job-stop-fence-green.log`.
- Still-cancelling Jobs from a previous ordinary retry were ignored:
  `/tmp/octacity-retry-job-stop-red.log`; bounded complete ordinary retry lineage
  checked: `/tmp/octacity-retry-job-stop-green.log`.
- Confirmed stopped node still occupied WIP (1 instead of 0):
  `/tmp/octacity-stop-wip-red.log`; common runtime/history and both backend counts
  corrected: `/tmp/octacity-stop-wip-green.log`.

Characterization tests also verify that unavailable measured consumption cannot
produce a receipt, result parsing is never performed during cancellation, and a
trusted action with an unknown response is observed through its original operation
identity after its deadline. Logs: `/tmp/octacity-stop-measurement.log` and
`/tmp/octacity-action-stop-observation.log`.

Latest full Green: 415 library tests (182 factory, 164 application, 69 store):
`cargo test -p octacity-server-factory -p octacity-server-store -p octacity-server-application --lib`
with debug profiles disabled, `/tmp/octacity-stop-wip-green.log`.
All eight configured PostgreSQL owner tests passed with the disposable database:
`cargo test -p octacity-server --test factory_configured_flow -- --ignored`,
`/tmp/octacity-stop-wip-postgres-green.log`.
These checks do not close the remaining capability/lifecycle/runtime migration.

## Generic capture, evidence and recovery

Capture now verifies exact ordinary execution and published files against the
frozen node input, including an explicitly selected predecessor candidate. Its
installed executor returns the manifest through the common result contract and
retains the bundle/manifest provenance. Recovery derives the same ChangeSet from
the retained record and intent without reading a stage pointer or projection.
The common Build binding explicitly selects Report or Artifact publication for
its JSON result. Evidence construction consumes a verified common Build completion
instead of legacy Build links and stage observations.

Observed Red/Green:

- Generic capture API missing: `/tmp/octacity-generic-capture-red.log`;
  `/tmp/octacity-generic-capture-green.log`.
- Artifact result selection missing: `/tmp/octacity-build-artifact-result-red.log`;
  `/tmp/octacity-build-artifact-result-green.log`.
- Evidence constructor required legacy stage links:
  `/tmp/octacity-generic-evidence-red.log`; `/tmp/octacity-generic-evidence-green.log`.
- Installed capture executor missing: `/tmp/octacity-capture-executor-red.log`;
  `/tmp/octacity-capture-executor-green.log`.
- Common candidate recovery API missing: `/tmp/octacity-capture-restoration-red.log`;
  `/tmp/octacity-capture-restoration-green.log`, including rejection of a substituted
  actual Build or capture tool after canonical record recovery.

The full three-library regression passed 419 tests after capture recovery and
the pool correction below: `/tmp/octacity-capture-restoration-regression.log`.
Four touched packages passed Clippy (all targets/features, `-D warnings`) after
capture recovery: `/tmp/octacity-generic-capture-clippy.log`.
The effectful legacy capture acceptance facade still exists; this capability
migration is not yet complete.

## Pool reservations and fresh observer ownership

An accepted cancellation was prematurely releasing a memory pool reservation.
The public pool contract now retains it while effects are stopping and releases
it on the executor's retained receipt, matching PostgreSQL selection behavior.
Red: `/tmp/octacity-stop-pool-reservation-red.log`; Green:
`/tmp/octacity-stop-pool-reservation-green.log`.

The common owner now reads trusted time after an asynchronous executor call and
before committing. An expired observer cannot publish a result, stop receipt or
acknowledge progress through the caller's cached timestamp. The injected clock
is required by owner construction and is shared with bounded reconciliation;
provider timestamps cannot extend the claim. Public owner test Red:
`/tmp/octacity-generic-owner-clock-red.log`; Green:
`/tmp/octacity-generic-owner-clock-green.log`. Application regression passed 169
tests: `/tmp/octacity-generic-owner-clock-regression.log`. The clock consolidation
kept all five reconciler contracts passing:
`/tmp/octacity-shared-owner-clock-refactor.log`.
All eight configured PostgreSQL owner tests passed:
`/tmp/octacity-generic-owner-clock-postgres.log`.

Node execution now retains its actual start and a deadline bounded by the
declared node budget, independently of its creator's worker lease. Successor
observation preserves both the original intent and execution window; it cannot
renew them. The store verifies creation ownership and the original start, while
domain/history/intent validation shares the execution-window invariant.
Red: `/tmp/octacity-node-execution-window-red.log` (100ms worker lease incorrectly
replaced the configured deadline of 1011ms); Green:
`/tmp/octacity-node-execution-window-green.log`. Characterization through the
actual ordinary Build adapter after owner takeover passed:
`/tmp/octacity-node-window-successor.log`.

A result predating the frozen start was accepted even though its creator's claim
was already active. Red: `/tmp/octacity-result-before-start-red.log`; Green:
`/tmp/octacity-result-before-start-green.log`. Historical fixtures now use valid
declared execution windows and advance their trusted Build timestamps with owner
time; validation was not weakened to retain impossible fixture chronology.

Generic Build retention previously scheduled cleanup from legacy Build links
alone, ignoring the actual generic execution's retained deadline. The public
PostgreSQL owner/retention contract observed premature cleanup at 20ms instead of
the retained 100ms bound: `/tmp/octacity-generic-build-retention-red.log`.
Scheduling now uses the same execution association as retention claims and holds.
Green: `/tmp/octacity-generic-build-retention-green.log`, including bounded cleanup
of stop receipts before their referenced execution and node.
All eight PostgreSQL owner contracts passed after execution-window separation:
`/tmp/octacity-node-window-postgres-regression.log`.

No task checkbox is closed by these intermediate checks. Legacy capability
facades/checkpoints, remaining generic materialization/retry work and production
composition still prevent completion of 9.21 and 9.23–9.26.


## Explicit candidate materialization and technical stops

`factory_node_candidate_materialization` verifies an explicitly selected candidate
Context and exact retained bundle/manifest Artifact identities without a stage
pointer or implicit predecessor. Pending or unselected artifacts fail closed.
Red: `/tmp/octacity-generic-materialization-red.log`; Green:
`/tmp/octacity-generic-materialization-green.log`. Application regression passed
170 tests and five-package Clippy passed before the stop work.

The common stop journal now represents independently confirmed cancellation or
execution failure in one receipt type/table (`FlowNodeStop`,
`factory_node_stops`). A failure has no semantic result, cannot advance a graph,
releases executable WIP only once actual effects stop, and charges total measured
operation consumption exactly once. Recovery observes the same Build/Attempt and
receipt without dispatching again. Missing terminal usage remains unavailable.
The ordinary adapter observes the accepted operation before creating a Build,
including recovery from a lost creation response.

Observed Red/Green public contracts:

- Missing common failure receipt: `/tmp/octacity-common-stop-red.log`;
  `/tmp/octacity-common-stop-green.log`.
- Missing retained owner failure/usage API:
  `/tmp/octacity-failed-node-owner-red.log`;
  `/tmp/octacity-failed-node-owner-green.log`.
- Failure consumption and observe-before-creation:
  `/tmp/octacity-failed-build-usage-red.log`;
  `/tmp/octacity-failed-build-usage-green.log`.
- Terminal Build was accepted while an actual Job was still running:
  `/tmp/octacity-failed-build-jobs-red.log`;
  `/tmp/octacity-failed-build-jobs-green.log`.

The shared failure owner characterization passed with actual execution identities
in memory and after PostgreSQL store replacement:
`/tmp/octacity-failed-node-shared-memory.log`,
`/tmp/octacity-failed-node-postgres.log`. Three-library regression passed 426 tests
(application 173, factory 184, store 69):
`/tmp/octacity-common-stopped-owner-regression.log`. Eight configured PostgreSQL
owner contracts passed: `/tmp/octacity-common-stop-postgres-regression.log`.
Five-package all-target/all-feature Clippy passed:
`/tmp/octacity-common-stop-clippy.log`.

The schema table-list expectations were corrected after observing the missing
stop-table mismatch. Three migration contracts passed:
`/tmp/octacity-generic-migrations-green.log` (before the receipt table rename).
The 13 discovery and four retention contracts passed before that rename:
`/tmp/octacity-generic-owner-postgres-store-final.log`.

Technical retry work is in progress. Its explicit node bound changes the
immutable definition and conservative admission expansion; the editable template
publishes the same bound. Red/Green:
`/tmp/octacity-node-retry-policy-{red,green}.log`,
`/tmp/octacity-template-retry-{red,green}.log`. Retried inputs preserve frozen
payload and Context, and runtime/journal validation requires a confirmed failed
predecessor and declared retry bound:
`/tmp/octacity-node-retry-lineage-{red,green}.log`.
The common operator command now addresses the exact confirmed failed attempt.
The owner consumes the accepted intent, preserves the frozen input and starts a
new bounded execution with its own stable operation identity. Workers cannot
create retry permission by appending a retry lineage themselves. Exact command
replay does not create another attempt. The shared contract passed in memory and
PostgreSQL: `/tmp/octacity-confirmed-node-retry-green.log`,
`/tmp/octacity-confirmed-node-retry-postgres.log`. Red was observed for the missing
command API, a worker-created retry and recovery incorrectly replaying the old
stop: `/tmp/octacity-confirmed-node-retry-red.log`,
`/tmp/octacity-confirmed-node-retry-precondition.log`,
`/tmp/octacity-confirmed-node-retry-owner-red.log`.

Confirmed failed ancestors do not occupy executable WIP after a successful later
retry. The interpreter accepts the complete runtime history, and pool selection
uses the same active-attempt definition, excluding stopped executions and
suspended subflow call frames. Public terminal regression Red/Green:
`/tmp/octacity-node-retry-terminal-{red,green}.log`,
`/tmp/octacity-node-retry-capacity-{red,green}.log`.

A queued technical retry freezes the next input generation from the original
memento without collecting Context again, publishes a new ready entry, and waits
for a fresh capacity reservation before creating its attempt. Old stopped
reservations are released. Red/Green:
`/tmp/octacity-pooled-node-retry-{red,green}.log`; the same contract passed after
PostgreSQL store replacement: `/tmp/octacity-pooled-node-retry-postgres.log`.

The three-library regression before the queued retry slice passed 431 tests
(application 174, factory 188, store 69):
`/tmp/octacity-node-retry-capacity-regression.log`. After the stop table rename,
13 discovery, four retention and three migration contracts passed:
`/tmp/octacity-node-stop-retry-postgres-store.log`.

Configuration-wide capacity is now pinned explicitly in `AdmittedFlow` from the
published configuration; direct test admissions provide it explicitly. There is
no default on restoration. Per-Flow concurrency and the configuration-wide
ceiling retain distinct meanings. Both backends count actual executable attempts
under their capacity lock and validate the append's active-attempt delta; a
confirmed stop releases capacity and rejected allocation leaves no journal or
projection changes. PostgreSQL locks the exact immutable configuration version
before a Run lock, preventing two independently claimed Runs from overbooking.

The public capacity contract observed Red for the missing admission ceiling API
and for two independently claimed Runs acquiring a single slot:
`/tmp/octacity-configured-node-capacity-red.log`,
`/tmp/octacity-configured-node-capacity-precondition.log`. Green in memory and
PostgreSQL: `/tmp/octacity-configured-node-capacity-green.log`,
`/tmp/octacity-configured-node-capacity-postgres.log`. An additional concurrent
PostgreSQL characterization passed with two owner calls in flight:
`/tmp/octacity-concurrent-node-capacity-postgres.log`. Common history construction
remained Green: `/tmp/octacity-configured-node-capacity-shared-history.log`.
The admission fixture was updated to publish the exact pinned capacity after the
storage regression exposed its old mismatching configuration:
`/tmp/octacity-configured-capacity-postgres-storage.log`,
`/tmp/octacity-configured-wip-admission-fixture-green.log`.

Restoration also requires every retained technical retry to name its accepted
operator intent, with a request time no later than its start. Common receipt
validation checks the original cancellation time in both backends. Removing the
retry authorization row from an otherwise intact PostgreSQL journal produced
Red; restoration now rejects it:
`/tmp/octacity-retry-control-restoration-{red,green}.log`.

The interpreter and runtime validator share measured resource aggregation by
Flow. Failed executions remain charged when a later retry succeeds. A configured
100-token Flow budget rejects 100 failure tokens plus one successful retry token:
`/tmp/octacity-failed-execution-budget-{red,green}.log`.

Before the final resource aggregation slice, 433 library tests passed
(application 176, factory 188, store 69):
`/tmp/octacity-retry-control-restoration-regression.log`. Thirteen ignored
configured-owner PostgreSQL contracts and five-package all-target/all-feature
Clippy passed before the authorization-restoration slice:
`/tmp/octacity-configured-node-capacity-owner-regression.log`,
`/tmp/octacity-configured-node-capacity-clippy.log`. Architecture validation
passed 56 packages and 209 edges:
`/tmp/octacity-configured-node-capacity-architecture.log`.

These checks do not close 9.21 or 9.23–9.26; legacy lifecycle/capability facades
and production composition remain open.

## Retired fixed-stage management controls

The public management surface now contains only Run cancellation, exact Node
Attempt retry, and exact configured HumanGate response. The old stage retry,
escalation disposition, and delivery approval commands and intents were removed,
as was the separate memory-only lifecycle mutation path. Both stores prepare
controls through the same graph-neutral contract. Logical approvals and delivery
are selected by configured gates and operations.

Before removal, generic public contracts characterized partial capture failure,
retry from the exact frozen base, restoration of the accepted candidate from the
common result journal, and rejection of a second retry beyond a pinned allowance.
They replace the retired tests that depended on the fixed-stage management API;
candidate materialization and independent evidence checks remain covered by the
common capability contracts. The generic capture fixture explicitly budgets
output bytes for its configured retry allowance.

- Characterization: `/tmp/octacity-generic-capture-retry-characterization.log`
  and `/tmp/octacity-generic-retry-bound-characterization.log`.
- Public API removal Red: `/tmp/octacity-legacy-control-api-red.log` (the removed
  commands were still importable, so the compile-fail contract failed).
- Green: `/tmp/octacity-legacy-control-api-green.log`.
- Regression: `/tmp/octacity-legacy-control-removal-regression.log`: 432 library
  tests passed (176 application, 189 domain, 67 store).
- PostgreSQL configured-owner regression:
  `/tmp/octacity-legacy-control-removal-postgres.log`: 13 passed.
- Five-package all-target/all-feature Clippy:
  `/tmp/octacity-legacy-control-removal-clippy.log`: passed with warnings denied.

The preceding resource-accounting regression also passed 434 library tests,
20 PostgreSQL storage tests and 13 PostgreSQL owner tests, retained in the
`/tmp/octacity-common-execution-budget-*` logs. Its formatting-only Clippy
remark was corrected before the final successful control-removal Clippy run.
These changes do not yet retire the old Build/capture facades or lifecycle
projections, and do not close the production composition task.


## Root authority, Build causality and capability preparation

The root lifecycle now contains only admission, execution of an exact root,
confirmed cancellation and a terminal key proved by the admitted graph. Fixed
business lifecycle variants, the test-only legacy lifecycle interpreter and the
store's fallback stage routing are removed. Operator commands address exact
configured node attempts; there is no retry/escalation/delivery stage command.
Root checkpoints no longer contain a global Decision Signal phase. Signals and
source statuses belong to configured operations, never root transition logic.
Both the root progress and its PostgreSQL document reject unknown authority fields.

Observed Red/Green:

- `cargo test -p octacity-server-factory technical_lifecycle_rejects_retired_business_phase_documents --lib`:
  the legacy Stage document was accepted, then rejected after retirement
  (`/tmp/octacity-technical-lifecycle-format-{red,green}.log`).
- `cargo test -p octacity-server-factory technical_progress_rejects_unknown_authority_fields --lib`:
  a business-stage authority field was silently ignored, then rejected
  (`/tmp/octacity-technical-progress-fields-{red,green}.log`).
- `cargo test -p octacity-server --test factory_configured_flow root_progress_rejects_a_retired_global_signal_phase_in_postgres -- --ignored`:
  PostgreSQL restored an obsolete signal field, then rejected it; the complete
  configured-owner suite passed 14 tests
  (`/tmp/octacity-root-technical-owner-green.log`).
- Reconciliation resumed an admitted graph without its configuration catalogue:
  first failed with `NotFound FactoryConfiguration`, then succeeded using the
  already pinned closure (`/tmp/octacity-pinned-owner-configuration-{red,green}.log`).

The Stage Build bridge, Stage causal request variant and legacy effectful capture
facade are deleted. Common Build requests carry Node Attempt causality directly.
A trusted terminal Build observation can be reconstructed from the exact common
execution and actual Build/Attempt/Job facts; foreign identities, active Jobs,
foreign output producers and predating observations are rejected. Its missing
public API established Red before implementation
(`/tmp/octacity-common-build-observation-{red,green}.log`).
Worker restart, competition and shutdown tests use the actual configured owner.
Partial capture retry, frozen base recovery, measured usage and retry exhaustion
are exercised through the common Node owner rather than an obsolete stage driver.
The full three-library lifecycle regression passed 407 tests before further
capability contract migration (`/tmp/octacity-technical-lifecycle-regression.log`).
PostgreSQL discovery, atomic journal, retention and migration regression passed
20 tests after lifecycle/signal retirement
(`/tmp/octacity-root-technical-postgres-green.log`).

Capability observation returns verified bounded data, immutable Artifact references
and measured usage; it no longer creates Stage Handoffs, MacroCall completions or
operation-local evaluation progress in the root store. Existing parsing, integrity,
provenance and failure classification tests are retained. The new data-only
observation API failed before implementation and passed afterward
(`/tmp/octacity-capability-data-{red,green}.log`). Retirement of the parallel
`FactoryCodexEvaluationCompletion` interface was observed through its public
compile-fail contract (`/tmp/octacity-evaluation-projection-api-{red,green}.log`).

Task Envelopes and transient credential authorizations now bind a common
Node Attempt. Task restoration rejects the old stage owner field. Exact Build
intent verification checks Node identity, frozen Context, subject, selected
toolchain and permission/budget ceilings. Public missing-API failures established
Red (`/tmp/octacity-node-task-envelope-red.log`,
`/tmp/octacity-node-credential-red.log`, `/tmp/octacity-node-task-binding-red.log`);
subsequent domain/application regressions passed 171/172 tests before the extra
Build-binding test, which also passed
(`/tmp/octacity-node-task-credential-green.log`,
`/tmp/octacity-node-task-binding-green.log`). The generic Build composition must
use this verification; production wiring remains open, not implied by pure tests.


## Common root journal and capability owners

The root snapshot, current projection and transition append now contain only
technical claim/budget/checkpoint facts and the common Flow execution/data journal.
The 18 fixed capability collections and their pointer/validation exceptions are
removed. PostgreSQL no longer creates the 22 stage/call/candidate/evaluation/delivery
projection tables. Candidate and capture restoration use the verified common
Build record and its exact input/output provenance. `factory_run_builds` derives
retention ownership solely from retained common Build executions.

Public diagnostics page Node Attempts and Node completions through exclusive
immutable cursors. An observed query-plan Red showed the missing `(run_id,id)`
index; the subsequent Green uses the two justified diagnostics indexes.

Verification:
- Public retired append API Red/Green: `/tmp/octacity-common-root-journal-api-{red,green}.log`.
- Schema retirement Red/Green: `/tmp/octacity-common-root-schema-{red,green}.log`.
- Query-plan Red: `/tmp/octacity-common-node-diagnostic-index-red.log`.
- PostgreSQL common journal/discovery/pool tests: 13 passed; retention: 4 passed,
  `/tmp/octacity-common-root-postgres-green.log`.
- Migration regression: 4 passed after updating the expected unreleased schema
  inventory and diagnostics indexes, `/tmp/octacity-common-root-migration-green.log`.

Evaluation results and Decision Signal requests now bind exact common Node
Attempts and frozen input digests. Their data-only capability contracts never
append Stage or MacroCall projections. Decisions reject a receipt from another
Run/input even when a restored result substitutes its node identifier.
Evaluation progress requires an exact evaluator and terminal result; the legacy
unbound-branch deserialization fallback is removed.

Observed Red/Green:
- Common frozen input owner API: `/tmp/octacity-capability-input-binding-{red,green}.log`.
- Node-bound signal request: `/tmp/octacity-common-signal-owner-red.log`;
  Green is covered by the capability regression below.
- Evaluation branch API: `/tmp/octacity-common-evaluation-owner-red.log`;
  intermediate full Green: `/tmp/octacity-common-evaluation-owner-green.log`.
- Substituted node with unrelated frozen input:
  `/tmp/octacity-common-evaluation-signal-binding-{red,green}.log`.
- Unbound evaluation restoration: `/tmp/octacity-evaluation-bound-only-{red,green}.log`.
- Retired StageAttempt/MacroCall/StageHandoff public API:
  `/tmp/octacity-retired-stage-call-api-{red,green}.log`.
- Common Context Artifact retention role:
  `/tmp/octacity-common-context-retention-role-{red,green}.log`.

Latest capability library regression passed 407 tests (172 factory, 163 application,
67 store, 5 Decision Signal adapter), with no exclusions:
`cargo test -p octacity-server-factory -p octacity-server-application -p octacity-server-store -p octacity-server-decision-signal --lib`,
`/tmp/octacity-evaluation-bound-only-green.log`.
Six Factory/server/provider packages passed Clippy with all targets/features and
`-D warnings`, `/tmp/octacity-retired-stage-call-clippy.log`.
Architecture check passed 56 packages/210 dependency edges. These checks precede
the subsequent Context retention-role adjustment and remaining production wiring;
final validation must follow those changes.

The ordinary Build creation path currently requires a real Trigger origin.
Factory production acceptance must use its own retained generic node operation
origin and the shared ordinary execution transaction, without fabricating a
manual Trigger. Exact adapter installation, trusted Context construction, and
optional-mode worker/readiness wiring remain required before migration is complete.


## Final common-engine verification (2026-10-11)

### Completed boundaries

- Capability tasks, credentials, evaluation and Decision Signal data bind exact
  common Node Attempts and frozen inputs. Candidate/capture recovery verifies
  ordinary execution and retained provenance. No Stage/MacroCall projections,
  parallel journals, fixed-role dispatch or legacy lifecycle fallback remain.
- Ordinary Factory Builds use the shared atomic Build/Attempt/Job transaction
  with their actual typed origin. Managed Jobs must retain source, causality,
  permission ceilings, allowed pools and the original absolute node deadline.
  Ordinary v1/v2 wire contracts remain unchanged.
- Ordinary cancellation verifies the current observer and original creation
  operation, changes actual Jobs/queues and retains an idempotent receipt.
  Terminal internal events retain their source origin without inventing a Trigger.
- The common server owner installs exact immutable executor bindings and a
  trusted input preparer, uses configured worker bounds and shares the existing
  supervisor. Disabled mode requires no Factory dependencies; enabled mode
  without an installer fails before resource loading. Installed dependency
  degradation affects readiness, and the same arbitrary node/input/intent is
  observed again after graceful shutdown and a fresh runtime start.
- Supplied business flows and Accepted Work are configuration over these common
  interfaces. New logical names require no Rust stage type. New technical
  capabilities are installed adapters with the same input/result/operation
  contract. Source status, reporting and delivery are explicit configured actions;
  concrete connector deployment remains in its later tasks.

`FactoryBuildMaterializer`, `FactoryFlowInputPreparer` and
`FactoryNodeBuildOutputSource` are trusted installation ports, not implicit
built-in providers. `compile_codex_node_task` verifies the actual common intent
before protected compilation, including the exact selected task Artifact.
The production runtime contract installs a test-only waiting executor to prove
supervision, readiness and recovery. Separate real PostgreSQL contracts prove
ordinary protected Build creation, cancellation and operation replay. These
checks do not claim a live model-provider execution or forge delivery pilot.
The default executable uses ordinary startup; Factory deployments explicitly
provide `FactoryRuntimeInstaller` via `ServerRuntime::start_with_factory`.

### Final TDD increments

Commands used the public boundaries below, with
`CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0`. Each Red was observed
before the implementation correction; environment setup failures are excluded.

| Public command/filter | Observed Red | Observed Green |
| --- | --- | --- |
| `cargo test -p octacity-server --test factory_configured_flow factory_build_admission_rejects_jobs_without_the_frozen_execution_authority -- --ignored` | Legacy Jobs were admitted without the frozen managed authority | The request is rejected before ordinary execution is created |
| Same integration target, `factory_build_admission_rejects_an_existing_pool_outside_its_configuration` | An existing foreign pool was accepted | The configuration's exact pool allowlist is enforced |
| Same target, `factory_build_cancellation_uses_ordinary_job_and_queue_state_and_replays_after_restart` | The ordinary cancellation API was missing; then terminal event claiming returned `NotFound Build` | Actual Jobs and queues stop, replacement-service cancellation replays, and the Factory-origin terminal event is consumable |
| `cargo test -p octacity-server-trigger internal --lib` | A valid normalized internal-event root was rejected as invalid causality | Valid roots and existing derived-event ancestry pass; unrelated roots remain rejected |
| `cargo test -p octacity-server factory_requires_explicit_installation_before_loading_resources_or_binding --lib` | Explicit enabled configuration was rejected as an unknown field | Configuration is accepted and missing installation fails before resource loading |
| Configured-flow integration target, `production_factory_owner_uses_installed_bindings_and_recovers_the_same_arbitrary_node` | The explicit installed-runtime API was missing | Actual server supervision, enabled readiness degradation and fresh-process observation retain the same node, input and intent |
| `cargo test -p octacity-server-application factory_codex_tests --lib` | Common-intent compilation API was missing | Cross-node task compilation fails before protected documents are produced; existing deterministic compiler contracts pass |
| `cargo test -p octacity-server-factory task_authority_is_verified_against_the_common_build_intent_and_frozen_context --lib` | A substituted task body Artifact passed against the frozen Context | Only the exact selected task Artifact is accepted |
| From `ui/`: `node node_modules/vitest/vitest.mjs run src/features/projects/ProjectBuilds.test.tsx` | A Factory-origin Build caused `TypeError` when reading its null Trigger cause | Factory Builds render and retain ordinary Build navigation; all five component tests pass |

Detailed Red/Green logs are retained under `/tmp/octacity-factory-*`,
`/tmp/octacity-codex-common-compilation-binding-*`,
`/tmp/octacity-frozen-task-artifact-binding-*` and
`/tmp/octacity-internal-root-event-*`. Pure refactors of ordinary cancellation,
runtime fixtures and owner composition were preceded by behavioral baselines.
Documentation and formatting have no executable behavior and are checked by
formatting, diff and OpenSpec validation.

### Final regression

| Scope | Result | Evidence |
| --- | --- | --- |
| Eleven Rust library packages | 562 passed | `/tmp/octacity-generic-engine-final-library-tests.log` |
| REST unit and integration contracts | 62 passed | `/tmp/octacity-generic-engine-final-rest-tests-v2.log` |
| PostgreSQL discovery, journal, retention, migrations, internal events and ordinary store | 43 passed | `/tmp/octacity-generic-engine-final-postgres-tests.log` |
| Configured PostgreSQL owner and installed server runtime | 20 passed | `/tmp/octacity-factory-runtime-configured-owner-regression.log` |
| Configured-flow memory/public-port contracts | 15 passed; the 20 service-dependent tests passed separately above | `/tmp/octacity-generic-engine-final-configured-tests.log` |
| Ordinary HTTP Build/Agent pipeline with Factory disabled | 1 passed | `/tmp/octacity-generic-engine-final-ordinary-runtime.log` |
| Application and Factory public documentation contracts | 4 passed | `/tmp/octacity-generic-engine-final-doc-tests.log` |
| Clippy, twelve packages, all targets/features, warnings denied | Passed | `/tmp/octacity-generic-engine-final-clippy-v5.log` |
| Cargo architecture and policy tests | 56 packages / 210 edges; 24 tests passed | `/tmp/octacity-generic-engine-final-architecture.log` |
| UI unit tests | 292 passed in 35 files | `/tmp/octacity-generic-engine-ui-unit.log` |
| UI architecture tests | 20 passed | `/tmp/octacity-generic-engine-ui-architecture-tests.log` |
| Generated API, UI typecheck, lint, build, bundle bounds and changed-file formatting | Passed | `/tmp/octacity-generic-engine-{api-generate,ui-typecheck,ui-lint,ui-build,ui-bundle-check,ui-format}.log` |

Reproduction commands:

```sh
export CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
cargo test -p octacity-server -p octacity-server-application -p octacity-server-factory -p octacity-server-store -p octacity-server-job -p octacity-job -p octacity-changeset -p octacity-protocol -p octacity-observability -p octacity-server-decision-signal -p octacity-server-trigger --lib
cargo test -p octacity-server-api-rest --tests
cargo test -p octacity-server --test factory_configured_flow
cargo test -p octacity-server-application -p octacity-server-factory --doc
# With explicitly disposable PostgreSQL 18 and S3-compatible object storage:
cargo test -p octacity-server-store-postgres --test factory_discovery --test factory_retention --test migrations --test build_discovery --test internal_triggers --test store_contract -- --ignored
cargo test -p octacity-server --test factory_configured_flow -- --ignored
cargo test -p octacity-server --test postgres_runtime -- --ignored
cargo clippy -p octacity-server -p octacity-server-application -p octacity-server-factory -p octacity-server-store -p octacity-server-job -p octacity-job -p octacity-changeset -p octacity-protocol -p octacity-observability -p octacity-server-decision-signal -p octacity-server-api-rest -p octacity-server-store-postgres --all-targets --all-features -- -D warnings
python3 -m unittest tools.tests.test_check_architecture
python3 tools/check_architecture.py
openspec validate add-dark-factory-mode --strict
cargo fmt --all -- --check
git diff --check
```

UI checks were run from `ui/` using the installed Node CLIs: API generation,
`node node_modules/typescript/bin/tsc --noEmit`, ESLint, Vitest,
`node --test scripts/verify-architecture.test.mjs`, Vite build and
`node scripts/verify-architecture.mjs --dist dist`. Browser/visual suites were
not run for the changed Build-origin display; no new browser interaction was added.

Final `cargo fmt --all -- --check`, `git diff --check` and strict OpenSpec
validation passed after the documentation and task updates
(`/tmp/octacity-generic-engine-final-openspec.log`).

Only disposable development/test services were used; both temporary PostgreSQL
and MinIO containers were removed after the passing integration tests. The unreleased Factory
migrations were revised directly as authorized; no persisted production Factory
data or legacy compatibility path is assumed. Free disk was monitored throughout;
the final compilation left approximately 8.8 GiB free, above the 5 GiB cleanup
threshold. No staging, commit or push was performed.
