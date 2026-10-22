# Task 7.1 — exact operator-action descriptions

## Boundary and scope

Public application descriptors compose the existing immutable Factory snapshot
and exact Issue/Work association. Current HumanGate eligibility is extracted from
the existing atomic response validator, with the original invalid/conflict
classification and response-schema validation preserved. Response and retry
validation share the same exact node-contract lookup. Retry descriptions reuse
the existing admitted-policy/budget validator rather than reproduce its rules.

No queue storage, migration, worker, provider client, HTTP endpoint or Console
control is introduced. Those read-model and transport slices remain 7.2–7.3.
Descriptions grant no authority; consumers must apply current visibility and
freshness, and mutation still uses the existing independently authorized,
idempotent Run-version control boundary. Issue version/status, comments and node
names cannot select a control.

## Baseline and characterization before extraction

```sh
cargo test --locked -p octacity-server-application --lib factory_flow_control_tests
cargo test --locked -p octacity-server-application --lib human_control_eligibility
cargo test --locked -p octacity-server-application --lib factory_node_failure_tests
```

The existing four HumanGate/operation/control cases and five technical
failure/retry cases passed. Before implementation, the added characterization
passed through the public response validator and real retained owner snapshots:
the admitted response deadline is inclusive, earlier-than-claim and later-than-
deadline responses fail, and recorded intent is no longer unanswered even before
the owner consumes it.

## Red → Green slices

1. **Original Issue-associated gate description.**

   ```sh
   cargo test --locked -p octacity-server-application --lib issue_actions_describe
   ```

   Red: the required public description API was absent. Green: a manually created
   Issue's retained association yields exact Work/Run/version, Flow/cycle/attempt,
   definition, frozen input, response contract, actionable time and deadline.
   Missing candidate provenance remains absent. Describing the action changes
   neither the Issue nor the Run and rejects a foreign Run association.

2. **Only admitted technical retries.**

   ```sh
   cargo test --locked -p octacity-server-application --lib action_descriptions_offer
   ```

   Red: the permitted retained technical failure yielded no action. Green:
   allowance zero remains unavailable, while allowance one yields the exact
   failed attempt, original typed candidate and retained failure time through
   the existing retry validator. Recording that retry retires the unanswered
   action. The existing exhausted-retry journey additionally verifies that a
   second failure cannot create another description beyond the pinned allowance.

3. **Closed bound-operation choices.**

   ```sh
   cargo test --locked -p octacity-server-application --lib bound_action_descriptions
   ```

   Red: a declared outcome outside the operation binding appeared as a second
   allowed response (2 instead of 1). Green: delivery, promotion and escalation
   retain their exact typed binding and only its mapped response contracts.
   Inclusive deadline and expiry checks remain shared with ordinary controls.

## Further public behavioral qualification

```sh
cargo test --locked -p octacity-server-application --lib factory_flow_control_tests
```

Ten cases passed. Two real simultaneous execution snapshots under one Issue
identity retain distinct Work/Run/attempt targets even when node names match.
One contains a nested HumanGate: its child Flow and exact child definition are
retained, and the enclosing call is not an action. Cross-Run response and
association substitution are rejected without changing either snapshot.

Real owner/control journeys also verify recorded intent before consumption,
superseded cycles, cancellation, completed Runs, and rejection of an observation
time earlier than the supplied snapshot. The existing bounded-retry and completed
HumanGate journeys assert description retirement in addition to command behavior.

Documentation-only changes describe these application contracts and their limits;
they have no executable behavior requiring a separate Red/Green cycle.

## Regression and resources

```sh
cargo test --locked -p octacity-server-application -p octacity-server-store --lib
cargo clippy --locked -p octacity-server-application -p octacity-server-store --all-targets -- -D warnings
cargo fmt --all -- --check
python3 tools/check_architecture.py
openspec validate add-factory-work-card --strict
git diff --check
```

All 652 application tests and 81 store tests passed. The full application suite
took 354.61 seconds; the store suite took 3.79 seconds. Strict Clippy passed
without warnings. Formatting and diff checks passed;
architecture passed with 63 packages and 256 edges, and strict OpenSpec validation
passed. No manifest, dependency lock, SQL, transport or UI contract changed, so no
new dependency resolution, PostgreSQL instance, API generation or Console build
was needed for this pure application/core slice.

Rust checks use one task-owned temporary Cargo target, two build jobs, disabled
incremental compilation and zero dev/test debug info. Free space was 46 GiB
initially and at least 38 GiB during the checks. Pre-existing targets and shared
caches are preserved. No commit or push is part of this implementation request.

After checks, the task-owned 792 MiB target and its temporary launcher/marker were
removed. Free space after cleanup was 39 GiB. No shared cache or pre-existing
target was deleted.

Task 7.1 is complete; OpenSpec progress is 48/56. Tasks 7.2–7.3 remain unchecked.
