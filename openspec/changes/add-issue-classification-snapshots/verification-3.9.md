# Task 3.9 verification

## Scope and public boundaries

This slice qualifies bounded material preparation and final admission through
`RequestIssueExecutionCommand`, `PublishIssueExecutionCommand`, the independently
authorized body queries and `ProjectTextInputStore::admit_project_text`.
PostgreSQL tests pause actual Work byte publication, then change source lifetime
or approved selection before invoking the public final admission boundary. They
assert no Run or Issue association on rejection.

Preparation distinguishes unavailable optional content from a count/UTF-8 limit
violation. The latter blocks both required and optional aliases with
`issue_snapshot_material_limit_exceeded`; unavailable optional text remains an
explicit retained omission. Existing authoring validation still bounds all
material declarations together to 32 entries and 1 MiB. Existing source, Incoming
and encoded snapshot bounds remain additional limits.

Before first admission, managed intake reauthorizes the frozen exact references
and verifies their original text/metadata. It never rebuilds selection, changes
owners or replaces an included optional value with an omission. PostgreSQL also
checks the current approved selections and source availability inside its
admission transaction. The selection and admission paths share the existing
owner/publication/digest/size/expiry validator, rather than duplicating its SQL.
Project/Issue locks exclude selection edits and discussion cleanup; document
publication/object rows remain share-locked until commit. Expiry uses the current
trusted admission guard time, not the preparation's original admission timestamp.

## Observed Red/Green cycles

```sh
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server-application --lib issue_material_snapshot_blocks_oversized

# OCTACITY_POSTGRES_URL explicitly selects a disposable PostgreSQL service.
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server --test project_text_intake material_admission_rejects_retired \
  -- --ignored --nocapture

CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server-application --lib issue_material_snapshot_recovery_rechecks
```

1. **Red:** an optional selection containing three approved entries with a
   two-entry consumer limit reserved an execution request instead of blocking.
   **Green:** count/byte violations have a separate failure category and a safe
   alias-specific blocker, with no Issue/Factory mutation or Run.
2. **Red:** after both Work input objects were written, deleting the selected
   comment body through ordinary retention still allowed the final PostgreSQL
   admission to publish a Work/Run. **Green:** the final transaction reuses the
   source validator under its canonical Issue fences and rejects that admission.
3. **Red:** recovery from a lost byte acknowledgement admitted a pending request
   even when its discussion-body reader now denied access. **Green:** the shared
   managed intake checks the frozen sources immediately before first admission.
   Restored access resumes the same preparation; once admitted, replay returns
   its original receipt without reopening sources.

The UTF-8 boundary, document lifetime/ownership matrix, exact publication budgets,
optional omissions and source-substitution cases additionally qualify existing
reader/validator behavior and the new gates. No independent missing-behavior Red
is claimed for those characterization cases. A missing test-only `ArtifactId`
import during the final expiry-fixture extraction was corrected and is not a
behavioral Red.

Task 3.8's recovery qualification now expects one read of each original document
for the final availability check. That check compares against the retained
snapshot; it does not resolve newer sources or rewrite any frozen text. Its
post-admission no-source-read assertion remains intact. The 3.8 report records
that slice's original observations; this task adds the pending-admission check.

## Qualified scenarios

| Scenario | Independently specified result |
| --- | --- |
| Optional count overflow | Three selected sources cannot fit a two-entry contract; review blocks rather than silently omitting them. |
| Aggregate UTF-8 bytes | The three original bodies total 78 characters and 91 UTF-8 bytes. Both required and optional contracts block at 90 bytes and admit their exact text at the inclusive 91-byte limit. |
| Combined declaration budgets | Publication accepts two 16-entry/512 KiB aliases, exactly 32 entries and 1 MiB total. Existing cases reject a 33-entry or over-1-MiB combined budget, duplicate exact references and owner-kind disguises. |
| Denied bodies | Execution rights cannot replace discussion or Artifact body permissions. Required preparation fails without source I/O/mutation; an unavailable optional alias is retained as an explicit omission. |
| Exact pin substitutions | Wrong comment digest, document digest/size and Issue/Project owner-kind claims block before document body reads or Run creation. |
| Cross-Issue substitution | A live document on another Issue in the same Project cannot be approved with a forged local parent or consumed through an exact local selector. Project visibility supplies no owner promotion. |
| Corrupt document bytes | An external byte reader returning different bytes for either owner kind causes required preparation to fail despite unchanged publication references. |
| Expired exact documents | Each document owner kind is tested independently for required/optional handling. A newer same-name publication is live and independently readable, but is never read or substituted for the expired selected publication. |
| Pending recovery and revoked access | Included required and optional material both block on a newly denied reader, preserving the original private snapshot. Restoring access admits that same input; a later denied-reader replay reopens no source. |
| Comment retirement after byte publication | Native retention removes the selected body; the final store admission conflicts and creates no Run/association. |
| Document expiry and cleanup after byte publication | Both Issue and Project publications block final admission at their exact deadline, with and without physical body cleanup. Reservation occurs one millisecond before source expiry so neither the Work lease nor its input retention supplies the rejection. |
| Selection removal before commit | Removing the reviewed reference after Work byte publication conflicts without dispatch; no mixed snapshot is admitted. |
| Unselected discussion | Appending unrelated discussion while selected text is prepared still admits exactly the original selected body. |
| Admitted recovery/retention | Existing native tests retain the original Work/Run after later edits, restarts and source/private-body cleanup, without resurrecting content. |

No whole-thread, all-document or latest-publication lookup, new byte store,
implicit selection, owner transfer, migration or public API is added. Automatic
Source intake remains section 4; HTTP/Console exposure remains section 6.

## Regression commands and outcomes

```sh
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server-application --lib issues
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server-application --lib factory::authoring
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server-application --lib management::tests::project_text_intake_tests
CARGO_INCREMENTAL=0 cargo test --offline --locked -p octacity-server-store --lib

# Disposable PostgreSQL and MinIO are selected through OCTACITY_POSTGRES_URL
# and OCTACITY_MINIO_ENDPOINT/ACCESS_KEY/SECRET_KEY.
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server --test project_text_intake -- --ignored --test-threads=1
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server --test project_text_intake
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server-store-postgres --test issues issue_materials_ \
  -- --ignored --test-threads=2

CARGO_INCREMENTAL=0 cargo clippy --offline --locked \
  -p octacity-server-application -p octacity-server-store \
  -p octacity-server-store-postgres -p octacity-server \
  --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
python3 tools/check_architecture.py
openspec validate add-issue-classification-snapshots --strict
git diff --check
```

Passed disjoint suites: **208 Issue**, **80 authoring**, **27 shared intake** and
**128 store** tests; **34 PostgreSQL/MinIO intake**, **five checkpoint** and
**four PostgreSQL material migration/history/concurrency** tests: **486 total**.
Focused material/admission runs are not counted twice. Thirteen new tests cover
this task: eight Issue/application, one authoring and four native admission cases;
individual matrix combinations are not counted as separate tests.

Clippy with warnings denied, formatting, architecture (64 workspace packages,
263 dependency edges), strict OpenSpec validation, documentation links and diff
checks pass. Native linking retains the existing macOS compact-unwind size warning.
Only the owned disposable test containers are stopped afterward. No commit is
created; the earlier 3.7–3.8 working changes are preserved.

## Review follow-up: encoded-size compatibility

Review found one P2 ordering bug in `issue_execution_snapshot::select`: it
validated newly available material encoding before attempting the exact retained
optional-omission candidate. A previously reviewed, small omitted snapshot could
therefore fail publication after readers were installed.

Permanent public-boundary tests publish and explicitly select a valid Project
text document, review an optional material contract without readers, and publish
that same request with ordinary authorized readers. They require the original
title-only Incoming, explicit omission, empty material lineage, unchanged sealed
digest and identical admitted replay.

```sh
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server-application --lib compatibility::encoded_size
```

**Red:** both new compatibility tests failed on the original implementation:

- `x` followed by 600,000 newlines fits the raw consumer limit, but JSON escaping
  exceeds Incoming's encoded limit; publication returned `invalid_contract`.
- 1,048,000 ordinary ASCII bytes fit Incoming, but source metadata makes the
  private snapshot exceed its encoded limit; publication returned
  `InvalidProjectTextInput`.

**Green:** both cases now preserve the original omission and digest. A third test
checks the same two input shapes with both required and optional material on a
new review that has readers. All four combinations still reject oversized actual
selected input, with no execution request mutation or Run. The focused suite has
**three passing tests**. A test-fixture partial-move compilation error was fixed
before observing behavioral Red and is not counted as Red.

The selector first matches a bounded candidate against the sealed digest across
its existing current/headless/full-catalog formats, restoring optional omissions
only for an exact match. An oversized nonmatching candidate does not stop checking
older candidates. It then validates Incoming and the private snapshot encoding
for the final selected projection. Required-source, consumer count/byte bounds,
readiness and policy checks still precede compatibility selection. No size limit
is increased and no new persistence or permission mechanism is introduced.

Follow-up regression commands:

```sh
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server-application --lib issues
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server-application --lib factory::authoring
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server-application --lib management::tests::project_text_intake_tests
CARGO_INCREMENTAL=0 cargo test --offline --locked -p octacity-server-store --lib
CARGO_INCREMENTAL=0 cargo clippy --offline --locked \
  -p octacity-server-application --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
python3 tools/check_architecture.py
openspec validate add-issue-classification-snapshots --strict
git diff --check
```

These pass **211 Issue**, **80 authoring**, **27 shared intake** and **128 store**
tests: **446 disjoint cases**. Focused runs are not counted twice. Existing old
full-catalog receipt and golden wire/digest cases remain passing. Clippy with
warnings denied, formatting, architecture, strict OpenSpec, documentation links
and whitespace checks pass. PostgreSQL/MinIO were not started for this follow-up:
the fix changes application candidate ordering and its tests only; the native
qualification above was performed for the preceding 3.9 implementation. No commit
is created.
