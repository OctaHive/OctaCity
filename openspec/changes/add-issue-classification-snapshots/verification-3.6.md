# Task 3.6 verification

## Scope and ownership

The optional `FlowIssueSnapshotInput.admission` declaration is pinned inside the
existing immutable Flow/Configuration. It has explicit choice-to-Work-priority
and choice-to-risk mappings plus conjunctive classification choice allowlists.
It introduces no scheduler, mutable policy registry, model scoring pass or
implicit launch rule. Project catalogs still own field types and choice IDs;
Project launch/access policy remains independent.

The priority source must be a required built-in Issue priority selection. Other
policy inputs must be required classification selections with compatible Project
choices. Missing aliases, empty mappings/allowlists, unsupported choices, optional
fields and ordinary prose/property sources fail publication. There are no
built-in numeric assignments or fallback values. A qualified but unmapped or
disallowed current value returns a bounded field-specific
`issue_snapshot_policy_ineligible` diagnostic before reservation.

Review computes the result after current selection/applicability/evidence checks
and seals it in the private snapshot. The original execution request retains
`admission_classification` so receipt recovery needs no private bodies or current
Issue lookup. The publisher uses this result rather than a caller rescore and
does not promote caller metadata into the Work. Final admission checks the
policy result against the snapshot, preparation, Work and reviewed request in
the existing admission transaction. The pure mapping is checked for consistency;
an agent never reruns classification or changes admitted scheduling.

Earlier contracts omit the new optional fields, retaining original serialization,
snapshot digests, caller-reviewed classification and replay fingerprints. A new
policy requires an explicit new publication. Automatic Source integration,
delivery example authoring and HTTP/Console review remain their subsequent tasks.
See [the application contract](../../../docs/reference/issues.md#pinned-issue-admission-mapping).

## Test-first evidence

All test commands below use `CARGO_INCREMENTAL=0`, `--offline` and `--locked`.

1. `cargo test -p octacity-server-application --lib issue_snapshot_policy_owns -- --nocapture`
   - **Red:** the existing Flow wire contract rejected the newly required
     `admission` declaration as an unknown field.
   - **Green:** the ordinary authorized request/publication path produces
     priority **80**, risk **low** and empty metadata from Issue priority `high`
     under the explicit pinned example policy. Caller request priority **1** /
     risk **high** / `accepted=true` and publisher priority **20** cannot replace
     it. The Incoming payload retains the canonical Issue choice `high`.
2. `cargo test -p octacity-server-application --lib issue_snapshot_policy_publication -- --nocapture`
   - **Red:** publication accepted a priority mapping referring to the nonexistent
     alias `missing`.
   - **Green:** a valid policy round-trips through authorized Flow publication;
     eight invalid mapping/selection cases block publication. Fixture schema
     compatibility was corrected so prose/ordinary-property rejection reaches
     policy validation rather than failing an unrelated field-shape check.
3. `cargo test -p octacity-server-application --lib issue_snapshot_policy_rejects_a_rescored -- --nocapture`
   - **Red:** the public final-snapshot admission contract accepted a consistently
     changed preparation and Work with priority **1** / risk **high**, while the
     frozen policy result was **80** / **low**.
   - **Green:** the same otherwise-consistent envelope is rejected as an Issue
     conflict. The ordinary publisher remains the positive control.

Final focused command:

```sh
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server-application --lib issue_snapshot_policy -- --nocapture
```

Passed **4 tests**. Additional readiness qualification covers missing size,
unmapped priority, policy-disallowed size and stale requirements with arbitrary
`accepted`, `ready` and `admission_risk` metadata. Each returns its specific
blocker without an execution reservation or Factory mutation. The existing
applicability checks remain responsible for stale data and unsupported evidence.
The stale-data expectation was corrected to the first blocked field (`priority`);
that fixture correction is not claimed as a production defect or Red.

## Durable recovery qualification

The new native PostgreSQL case uses public authorized Issue execution and ordinary
managed publication. After losing a byte acknowledgement it recreates the store
and publisher, recovers the original code subject/input, then changes Issue
priority, retires private input bodies and recreates the store again. Publishing
with yet another caller classification returns the exact original receipt and
unchanged Work/Run, priority **80**, risk **low**, metadata and Incoming choice.
The original decision survives JSON persistence without a migration or new replay
identity. Only two byte writes occur because recovery reads the already-published
first object; an initial test expectation of three was corrected without a
production change and is not a missing-behavior Red.

Disposable PostgreSQL 16 and MinIO provide the existing native regression harness.
The full Project text intake suite also qualifies the prior races, substituted
snapshots, restart/crash boundaries, legacy explicit classifications, HTTP resume
and retention behavior.

## Regression finding: consistent crash checkpoints

The first full native run passed 28 cases but exposed a race in the existing
shared crash driver. `snapshot_process_restart_keeps_original_input_at_each_publication_boundary`
failed because its retained baseline had `frozen_revision: None`, whereas the
recovered preparation correctly retained `original-base`. The child had already
crossed the byte barrier between the driver's metadata read and marker check.
This is an observed behavioral Red of the existing recovery test, not a policy
regression or a timeout.

The driver now observes external byte/association barriers before reading the
preparation used as its baseline. Publication/admission barriers remain checked
on that preparation itself. No assertion or production fence was relaxed.
The focused snapshot crash test then passed all five stages (first byte, both
bytes, published, admitted, associated). The final full native run qualifies
both snapshot and legacy users of the shared driver.

## Regression commands

```sh
CARGO_INCREMENTAL=0 cargo test --offline --locked --all-features \
  -p octacity-server-application -p octacity-server-store \
  -p octacity-server-factory --lib
CARGO_INCREMENTAL=0 cargo test --offline --locked -p octacity-server \
  --test project_text_intake -- --ignored --test-threads=1
CARGO_INCREMENTAL=0 cargo clippy --offline --locked \
  -p octacity-server-factory -p octacity-server-application \
  -p octacity-server-store-postgres -p octacity-server \
  --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
python3 tools/check_architecture.py
openspec validate add-issue-classification-snapshots --strict
git diff --check
```

Passed disjoint suites: **1118 application**, **351 Factory core**, **128 store**
and **29 native Project text intake** cases: **1626 tests**, including the five
new policy cases. The final complete native run passed after the crash-driver
fix; the earlier failure is documented above. The focused policy and crash
commands also passed and are not counted twice.

Clippy with warnings denied, formatting, architecture (64 workspace packages,
263 dependency edges), strict OpenSpec validation and diff whitespace checks
passed. The two task-owned disposable PostgreSQL/MinIO containers were stopped
and automatically removed after qualification. No commit was created.

## Post-review correction: process exit versus association observation

Review identified another interleaving in the crash driver: the association read
can begin before the child's commit and return no checkpoint, while `try_wait`
already observes a successful exit. Treating every observed exit as premature
made a completed publication fail its recovery test.

The original snapshot and legacy five-stage tests passed before refactoring.
Only the existing test-harness polling loop was extracted into
`publication_checkpoint.rs`; the snapshot five-stage case stayed green before
changing exit handling. The observer remains the owner of the durable barrier,
and the waiter supervises the real child process without depending on Issue or
PostgreSQL types.

The deterministic regression uses an actual exited child and a controlled stale
first observation. Before the fix, the focused suite had **3 passed / 2 failed**:
successful associated exit was incorrectly rejected after a stale read, and an
already visible checkpoint hid a failed child exit. After the fix, **5 passed**.
Successful exit is accepted only at the `associated` boundary, with a confirmed
checkpoint; an initially absent checkpoint is re-read after exit. Nonzero exit
and missing durable association still fail. The driver's final wait also checks
successful completion at `associated`, covering termination after the polling
loop returns. Other stages retain forced termination and their original bounds.
Negative tests require the intended failure diagnostic rather than any setup
panic. No production admission, replay, authorization or persistence code changed.

```sh
CARGO_INCREMENTAL=0 cargo test --offline --locked -p octacity-server \
  --test project_text_intake publication_crash::checkpoint::tests -- --nocapture
# Native commands additionally use the disposable OCTACITY_POSTGRES_URL.
CARGO_INCREMENTAL=0 cargo test --offline --locked -p octacity-server \
  --test project_text_intake snapshot_process_restart_keeps -- --ignored --nocapture
CARGO_INCREMENTAL=0 cargo test --offline --locked -p octacity-server \
  --test project_text_intake killed_publishers_resume -- --ignored --nocapture
CARGO_INCREMENTAL=0 cargo test --offline --locked -p octacity-server \
  --test project_text_intake snapshot_admitted_restart_recovers -- --ignored --nocapture
CARGO_INCREMENTAL=0 cargo clippy --offline --locked -p octacity-server \
  --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
python3 tools/check_architecture.py
openspec validate add-issue-classification-snapshots --strict
git diff --check
```

Passed **8 distinct tests**: five deterministic waiter cases and all three native
consumers, covering **12 process boundaries** (five snapshot, five legacy, two
retention/recovery). The earlier 1626-test result records the pre-review full
suite; this correction reruns the affected test boundary rather than claiming a
new full-suite run. The task-owned disposable PostgreSQL service is removed after
qualification. No commit is created.
