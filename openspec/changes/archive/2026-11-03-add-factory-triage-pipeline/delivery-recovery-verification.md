# Task 6.3: retained result and ordered delivery recovery

## Boundary and implementation decision

The existing ordered Reporter/outbox owners already implement the behavior
required by 6.3. This task extends their tests before considering another binding;
no additional production code, API, engine, provider wrapper, journal or retry loop
was needed. The reporting draft and exact subflow binding from 6.1 are reused
unchanged, together with the current writer/producer checks from 6.2.

The composed test uses public Source configuration publication, Source
preparation/admission, `FactoryResultReportingDraft::compile`/`bind`, complete
ordinary Flow/schema closure, `FactoryFlowRunner`, accepted `set_status` action,
durable PostgreSQL outbox and the content-verified Reporter process. Its constant
accepted-result gate represents the reporting seam; it does not substitute for or
claim two real AI-stage invocations.

The ordinary Factory configuration is installed through the public domain/store
fixture contract. Source publication and dispatch use normal application
boundaries with explicit managed connection, installed editing policy and current
Project read/update grants. The fixture does not introduce a test-only authority
bypass or retarget an accepted input.

## Test-first observations

All Cargo commands used this environment:

```sh
export CARGO_PROFILE_DEV_DEBUG=0
export CARGO_PROFILE_TEST_DEBUG=0
export CARGO_INCREMENTAL=0
```

The initial existing recovery baseline was run before extending tests:

```sh
cargo test --locked -j2 -p octacity-server --lib reporting::recovery:: --quiet
```

**5 passed**. Task 6.2's preceding full Reporter qualification was also retained
as an unchanged baseline. New tests were written and exercised before deciding
whether implementation was missing:

```sh
cargo test --locked -j2 -p octacity-github --test issue_reporting \
  independent_reviewed_effects -- --nocapture
cargo test --locked -j2 -p octacity-server --lib \
  reviewed_result_delivery_keeps_original -- --ignored
```

The first new native test passed against the existing production adapter. The
composed test passed all four cases after correcting fixture setup to existing
contracts. There was no missing production behavior and therefore no fabricated
behavioral Red or unnecessary implementation change. Setup corrections are not
claimed as TDD Reds: the initial managed fixture omitted its advertised finite
mapping; its original output ceiling of 1,000 bytes was insufficient for the real ordered
protocol exchange; and malformed Inspect was incorrectly expected to mean retry
exhaustion. The final fixture explicitly selects a ceiling of 10,000 bytes within
its original parent limits, and a real transient provider failure for exhaustion.
Malformed Inspect retains the existing fail-closed protocol semantics.

### Composed durable reporting

`reviewed_result_delivery_keeps_original_decision_and_operation_through_recovery`
qualifies these independently specified outcomes:

- Publication has no Run or Reporter request, and an ordinary Reporter worker
  claims no event before admission.
- One accepted bounded decision (including advisory priority 75) binds the
  reporting call to its exact original record digest. The child action retains
  the exact caller input digest, returns the unchanged result and queues one
  configured effect before the child returns.
- Root Flow completion reaches `RunState::Completed` with exactly one configured
  Reporter operation. The original pending row, operation ID and accepted input
  digest remain present through every delivery state.
- Loss of the reply after Apply leaves an actually applied effect and a pending
  retry. Recreating the PostgreSQL store adapter, projector and worker over the
  same retained database confirms through Inspect of the identical original
  intent/projection ID/native item, without a second Apply or Run.
- Removing only the current writer grant after the unknown reply records
  `Forbidden` without another provider call. Temporary provider unavailability on
  the final attempt records `Exhausted`. Both preserve the completed Run, Work,
  original scheduling metadata, accepted Flow/results and operation identity.
- Explicit preservation (`None`) completes the Run without an outbox effect or
  provider call even though the writer is installed. Completion cannot introduce
  a fallback producer.

The runtime peer qualifies the public ordered process/owner boundary. PostgreSQL
remains running while owners are recreated; this is durable owner recovery, not a
claimed PostgreSQL process or full server restart.

### Native GitHub effects

`independent_reviewed_effects_keep_disjoint_scopes_and_recover_without_replaying_a_label_write`
uses the actual GitHub executable with a stateful local HTTPS peer. Three finite
reviewed effects have separate installed label-group/state scopes. A POST changes
the original Issue and deliberately loses its response. Fresh native processes
inspect and confirm that same effect before separate removal and state changes.
The only writes are the expected POST, DELETE and PATCH; every confirmation is
read-only, and the human-owned label remains after every step.

Native installed groups remain ownership units; this test deliberately declares
independent groups rather than changing or narrowing the adapter's group
semantics. No cache, label creation, broad label replacement or hidden mapping
rewrite is introduced.

### Existing contracts reused

The full Reporter suite supplies ordered supersession, original deadlines,
current grant checks, intermediate/terminal diagnostics, field independence,
uncertain-write hazards and exhausted/forbidden delivery without accepted-result
rollback. Application reporting tests supply unconfigured completion and exact
nested provenance. Source preview tests verify read-only selection with no
admission, progress mutation or configured native write.

## Review and qualification limits

Separate Standards and Spec reviews found no missing production behavior or need
for another owner. Their test-strengthening feedback was applied: the composed
case now explicitly retains the original outbox row, compares operation/input
identities and provider intent, and requires post-checkpoint Inspect. The retained
row assertion makes identity preservation non-vacuous.

These tests qualify reporting contracts and ordinary owners, not packaged
end-to-end triage. Real AI stages, installed Runtime/MCP, Console and full service
restart journeys remain tasks 7.x/8.x. No paid model or live GitHub call was used.
Documentation has no executable behavior and is validated through formatting and
OpenSpec checks.

## Final verification

With the Cargo environment above, the following checks passed:

- `cargo test --locked -j2 -p octacity-server --lib reporting:: -- --include-ignored --test-threads=2`:
  **78 passed** (163.96 seconds), including disposable PostgreSQL cases and the
  composed accepted-result/delivery scenario. No ignored case was counted as
  executed without enabling it.
- `cargo test --locked -j2 -p octacity-github --test issue_reporting --quiet`:
  **19 passed**, including native effect preservation and uncertain-response
  recovery. The new native test also passed individually before the full suite.
- `cargo test --locked -j2 -p octacity-server-application --lib reporting --quiet`:
  **21 passed**, including existing stage/nested reporting and result composition.
- `cargo test --locked -j2 -p octacity-server-application --lib source_preview --quiet`:
  **7 passed**. Together these four suites contain **125 distinct tests**.
- After adding the explicit retained-row assertion,
  `cargo test --locked -j2 -p octacity-server --lib reviewed_result_delivery_keeps_original -- --ignored`:
  **1 passed**, exercising all four delivery cases again.
- `cargo clippy --locked -j2 -p octacity-server -p octacity-github --tests -- -D clippy::all`:
  passed. Server Clippy also passed after the final test assertion. The existing
  macOS compact-unwind linker warning and unused `pending_factory_profile_fixture`
  compiler warning do not affect the successful test results.
- `python3 tools/check_architecture.py`: **64 packages / 263 edges**, passed.
- `cargo fmt --all -- --check`, scoped `pnpm exec prettier --check`,
  `openspec validate add-factory-triage-pipeline --strict` and `git diff --check`:
  passed.

Final separate Standards and Spec reviews reported no remaining actionable
findings. Disk was checked throughout with `df -h .`: **50 GiB at the start**,
**48–49 GiB during validation**, **48 GiB after cleanup**. Compilation used two
jobs, disabled debug information/incremental builds and reused existing caches.
The owned PostgreSQL 18.6 container and disposable storage were removed after
validation; no user data was deleted. No commit was created and 7.1 was not started.
