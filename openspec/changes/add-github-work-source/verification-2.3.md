# Task 2.3 verification: durable source state and fenced preparation

Status: complete for task 2.3; change progress is 7/29.
Task 2.4 and later remain outside this slice.

## Boundary and retained authority

`FactorySourceStore` is a trusted provider-neutral persistence port implemented
by the existing memory and PostgreSQL adapters. Its immutable publication batch
contains at most 64 input changes and one optional exclusive group. Both adapters
validate exact stored Configuration/root Flow and Repository content, Project
ownership, next versions, and the group's exact current member definitions before
publishing the complete batch. A conflict rolls back every member publication.
Restoration validates closed input/group documents without making deserialization
an installation authorization grant.

Migration 0059 adds four tables: input versions, group versions, scans, and private
preparations/correlations. It does not add another Work deduplication table or
modify the existing admission identity. Source namespace, native item identity
and normalized intent digest are shared with application ingress. The application
continues reexporting the original intent and Repository source selection types;
manual wire representation and digest field order remain unchanged.

A scan pins an exact current input/group version and receives a bounded lease
with a monotonically increasing fencing token. Every fresh preparation, intent,
admission and checkpoint checks current definitions, enabled state, complete
ownership and trusted time under the same memory lock/PostgreSQL transaction.
Replacing a group or member invalidates old owners; new definition versions reset
scan progress, while fences remain monotonic. PostgreSQL uses short Project-row
locks and performs dependency/replay reads using the transaction's connection.
No provider or VCS call occurs while these locks are held.

A preparation retains the first bounded private snapshot and stable operation
identity before artifact/VCS work. Normalization freezes the complete original
admission intent, including source expression, artifacts and classification.
Takeover and changed observations cannot replace it. A current owner can resume
original intent after definition replacement only for the same stable selected
input and installed resource; original target/repository/input provenance stays
pinned. Exclusive group selection must be reevaluated under its current revision
by the future trusted worker before choosing a member.

Prepared admission validates that Work and probe repeat the original normalized
intent. A fresh commit publishes Work, Run, normal idempotency/audit/outbox facts
and source correlation atomically through the existing admission implementation.
If ordinary admission was already committed before source correlation, recovery
attaches the existing strict receipt under a current fence. It discards any
provisional transport reservation rather than creating another outcome or audit.
Safe paged associations return original input versions and existing Work/Run ids
without selecting private snapshot bytes. Pending pages support restart; an
unfinished preparation prevents a checkpoint covering its scan.

## Red / Green evidence

The public persistence contract is in
`server/infrastructure/octacity-server-store-postgres/tests/factory_discovery/source_contract.rs`.
The same behavioral assertions run unchanged against memory and PostgreSQL for
`github`, `gitlab`, `jira`, and `custom-tracker` installation identities.

1. Retained input/group restoration: Red was the missing `Deserialize` contract;
   Green restored a replaced group and rejected foreign/duplicate/empty members,
   incompatible Reporter scope and unknown credential fields.
2. Atomic publication: Red was the missing publication/read API; Green published
   exact versions, rejected foreign reads/ownership and rolled back a fresh input
   when its group publication conflicted.
3. Scan ownership/progress: Red was the missing claim/checkpoint API; Green proved
   competing owners, forged ownership, expiry, monotonic takeover and cursor CAS.
4. Pending reservation: Red was the missing preparation API; Green retained the
   original snapshot across takeover and edits, rejected foreign/oversized work,
   and prevented checkpoint publication over unfinished preparation.
5. Normalized intent/restart paging: Red was the missing intent/public pending
   API; Green retained the full typed command, rejected conflicting artifacts,
   and enforced bounded exclusive pending cursors.
6. Atomic prepared admission: Red was the missing fenced admission/probe API;
   Green rejected the stale owner before Work publication, committed original
   correlation with admission and replayed the same authoritative receipt.
7. Safe Issue associations: Red was the missing paged correlation API; Green
   returned exact original input versions and Work/Run identities, isolated
   foreign Projects and enforced exclusive continuation.
8. Replacement during pending preparation: Red was an actual conflict when the
   new current owner resumed the original intent. Green allowed that owner to
   resume the same stable input/resource while rejecting the old input/group
   versions and owners. Disabled current inputs/groups acquired no scan.
9. Bounded connection pool: Red was an actual source-publication failure with an
   otherwise working one-connection PostgreSQL pool because the transaction
   attempted a nested pool acquisition. Green reused the transaction connection
   for exact dependencies and admission replay. The entire source contract now
   runs with one connection and a bounded acquisition timeout.

The initial memory fixture needed its existing Factory choices enabled; this was
fixture setup, not counted as behavioral Red. Compiler repairs during Green were
not counted as additional TDD cycles.

Red/Green commands:

```sh
cargo test --locked --jobs 2 -p octacity-server-factory retained_source_versions -- --nocapture
cargo test --locked --jobs 2 -p octacity-server-factory source_input_tests -- --nocapture
# Explicit disposable service; the URL is supplied only for this owned test container.
OCTACITY_POSTGRES_URL=<disposable-url> cargo test --locked --jobs 2 \
  -p octacity-server-store-postgres --test factory_discovery source_publication -- --ignored
```

The expanded contract additionally verifies concurrent scan contention, restart
between snapshot/normalization/admission, current group revision replacement,
manual admission committed before correlation under a different transport key,
and exact original receipt recovery. Four providers produce exactly 12 Runs,
13 total idempotency outcomes and 13 total management audit facts (including the
one target Configuration publication); memory also reports exactly 13 outbox
facts. Replays introduce no extra admission evidence.

## Regression results

- Application admission baseline before common-intent extraction: 23 passed.
- Application admission after extraction and shared source identity: 23 passed.
- Factory/store all-feature suites: 293 + 71 passed, with two compile-fail doctests.
- PostgreSQL Factory discovery/contracts: 23 passed on the disposable service.
- PostgreSQL migrations: 5 passed, including empty/reentrant migration, incompatible
  histories and previous-binary snapshot rollback rehearsal.
- The new 58-to-59 compatibility contract preserves existing manual Work, Run,
  audit/idempotency counts and original replay receipt across migration/restart.
- Manual-trigger integration regression: 34 passed.
- Clippy for application/store/PostgreSQL, all targets and features: passed with warnings denied.
- Final source-only rerun: 2 passed, including input/group replacement checks.
- Factory Clippy, all targets/features: passed with warnings denied.
- Formatting, architecture (61 packages / 245 edges), strict OpenSpec validation
  and diff checks: passed.
- Total: 449 distinct behavioral tests and two compile-fail doctests passed.

Commands:

```sh
cargo test --locked --jobs 2 -p octacity-server-application --lib factory_admission_tests -- --test-threads=2
cargo test --locked --jobs 2 -p octacity-server-application --test manual_trigger -- --test-threads=2
cargo test --locked --jobs 2 -p octacity-server-factory -p octacity-server-store --all-features -- --test-threads=2
OCTACITY_POSTGRES_URL=<disposable-url> cargo test --locked --jobs 2 \
  -p octacity-server-store-postgres --test factory_discovery --test migrations -- --ignored --test-threads=2
OCTACITY_POSTGRES_URL=<disposable-url> cargo test --locked --jobs 2 \
  -p octacity-server-store-postgres --test factory_discovery sources:: -- --ignored --test-threads=2
cargo clippy --locked --jobs 2 -p octacity-server-application -p octacity-server-store \
  -p octacity-server-store-postgres --all-targets --all-features -- -D warnings
cargo clippy --locked --jobs 2 -p octacity-server-factory --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
python3 tools/check_architecture.py
openspec validate add-github-work-source --strict
git diff --check
```

## Scope and operations

No Source/Reporter process, credentials, provider network call or running user
stand was activated. Concrete artifact mapping, private byte retention/cleanup,
classified preparation failure and source worker/polling composition belong to
subsequent tasks. Unfinished preparation deliberately prevents claiming its scan
position as covered; the store never fabricates an admission/disposition to move
a cursor. The source port is internal: future management/diagnostic use must
apply authorization and visibility before exposing associations.

Rollback disables current inputs and retains pending intent, progress, original
Work/Run and reporting provenance. The previous server binary cannot open a
schema version it does not know; the existing snapshot rollback policy remains
unchanged. Documentation changes have no executable behavior and are checked by
OpenSpec validation, architecture validation and diff checks.

The owned disposable PostgreSQL container was stopped and removed after testing.
No commit was created. Changes from tasks 2.1 and 2.2 are retained.
