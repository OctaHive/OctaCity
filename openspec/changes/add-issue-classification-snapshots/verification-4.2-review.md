# Tasks 4.1–4.2 review follow-up

## Scope

The review found two behavioral defects and one small duplication:

- Canonical Issue input still authorized an unused provider completion Artifact.
- A snapshot frozen before admission could never advance after an Issue edit.
- Ordinary preparation resolved the same target separately for snapshot probing
  and predecessor/provider selection.

These fixes preserve the existing Source identity, snapshot builder, Work/Run
owner, original input pins and retention. Tasks 4.3 and 4.4 remain unchecked.

## Public-boundary Red/Green evidence

```sh
# OCTACITY_POSTGRES_URL selects the owned disposable PostgreSQL service.
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server --lib issue_snapshot_ignores_unused_completion \
  -- --ignored --nocapture
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server --lib issue_snapshot_pending_restart_refreshes \
  -- --ignored --nocapture
```

The first test observes **Red**: canonical Issue intake reports `(completed=0,
deferred=1)` instead of `(1,0)` when an unused completion Artifact is hidden.
Its **Green** additionally retains independent permission checks for actual Work
inputs on replay and for the provider-completion mode.

The pending-input test pauses either after a physical byte write but before its
acknowledgement, or during VCS resolution after intent publication. Requirements
are changed and classification is applied to the new requirements. The shared
public reader independently proves that this new snapshot is valid before the
restarted Source worker is invoked. Against the original pending resolver,
**Red** is zero Runs instead of one, despite that valid canonical input.

The initial fixture changed classification before requirements, making it
correctly inapplicable; that observation is not evidence of this recovery bug.
The fixture order was corrected and the new reader/worker pending-resolution
branches temporarily restored to their original behavior for the valid Red.
The corrected test retains its expected literal payload through the fix.

The corrected recovery scenario passes at both interruption boundaries in
9.56 seconds. A further public store test exposed an unsafe late reservation:
after refresh, a writer could reserve old task bytes under the same still-live
claim. Its **Red** is acceptance of those superseded objects. Reservation now
reuses the admission document-byte validator to require the current snapshot's
exact role/digest/size before metadata mutation. The test also checks stale-seal
rejection and cleanup ownership. A test-only call to private `to_store` was
replaced with the fixture's explicit trusted store scope before observing that
behavioral Red; its compile failure is not counted as Red.

## Pending input ownership

An explicit compare-and-swap replaces only an unadmitted Issue snapshot under a
live current scan owner. The old seal, same Issue/input contract and exact target
must match, and the store independently excludes an existing automatic admission.
The preparation and its existing materialization are updated atomically. Work
and preparation identities do not include the new classification or timestamp.

Replaced objects remain in the same private cleanup manifest; no table or byte
store is added. Cleanup metadata uses the existing Source input byte ceiling.
New objects retain the original materialization deadline and reservation time;
refresh is neither a retention renewal nor another automatic cycle. Admitted
preparations keep their immutable snapshot and original receipt.

## Authorization and target selection

Authorization receives the actual canonical input selection during preparation
and reads its retained mode on admission/replay. Only a path consuming provider
completion requires its Artifact permission; actual Work inputs keep their
independent permission checks. The installed settings remain pinned, including
older pending policy bytes. The initial normalization-to-Body implementation
passed the permission test, then was replaced by this mode-aware authorization
to avoid changing the serialized policy merely to skip an unused permission.

One target resolution selects a closed provider/predecessor/Issue input mode.
The predecessor helper consumes that resolved target; it no longer resolves the
same target immediately after a negative snapshot probe. Existing predecessor
proof and observed-text validation is shared between both entry paths. The
existing 153 admission tests passed before this refactoring.

## Regression commands and outcomes

```sh
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server-application --lib factory::admission
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server-application --lib issues
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server-application --lib management::tests::project_text_intake_tests
CARGO_INCREMENTAL=0 cargo test --offline --locked -p octacity-server-store --lib
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server --lib runtime::sources::tests::worker

# Disposable PostgreSQL is selected with OCTACITY_POSTGRES_URL.
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server --lib runtime::sources::tests::worker::issue_snapshots \
  -- --ignored --test-threads=2
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server --lib runtime::sources::tests::worker \
  -- --ignored --test-threads=2
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server-store-postgres --test factory_discovery \
  source_publication_is_atomic_owned -- --ignored --nocapture

CARGO_INCREMENTAL=0 cargo clippy --offline --locked \
  -p octacity-server-application -p octacity-server-store \
  -p octacity-server-store-postgres -p octacity-server \
  --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
python3 tools/check_architecture.py
openspec validate add-issue-classification-snapshots --strict
git diff --check
```

Final suites pass **153 admission**, **211 Issue**, **27 shared intake**,
**128 store**, **97 ordinary worker**, **54 native worker** and **one native
Source publication contract**: **671 disjoint tests**. The six focused snapshot
tests pass in 18.75 seconds and are included in the native total, not counted
again. All three new behavioral tests pass, including late-writer rejection;
the full native worker suite completes in 241.13 seconds.

Clippy with warnings denied, formatting, architecture (64 packages, 263 edges),
strict OpenSpec validation, 39 local documentation links and diff whitespace
checks pass. Linking retains the existing macOS compact-unwind warning. Only the
owned disposable PostgreSQL container is stopped after qualification. No commit
is created, and prior task 4.1–4.2 changes remain present. Documentation-only
updates have no executable behavior and are qualified by links/spec/whitespace
checks rather than a separate TDD cycle.

## Second review: publication acknowledgements

Reservation was already fenced to current snapshot bytes, but publication still
accepted only the original policy digest. An Issue edit can change task bytes
without changing that policy. Two new public PostgreSQL/preparer tests retain an
old acknowledgement under the same live claim, refresh canonical requirements
and attempt the old publication either before replacement reservation or after
reservation while a byte publisher is paused before its first write.

```sh
# OCTACITY_POSTGRES_URL selects the owned disposable PostgreSQL service.
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server --lib issue_snapshot_rejects_late_publication \
  -- --ignored --nocapture
```

**Red:** both tests accepted the old publication instead of rejecting it (3.09
seconds). This is an executable reproduction of the review's same-claim lifecycle
concern; it does not assume that normal polling processes one item concurrently.

**Green:** publication carries the exact reserved objects whose bytes this writer
verified. The store requires that complete active set, including each Artifact,
upload, digest, size, role and media type, to match before setting `published_at`.
The writer supplies its original reservation, never a post-I/O manifest reread.
The two guarded timing windows now reject without metadata mutation. Genuine
replacement publication still writes readable fresh task bytes and admits one
Run under the same preparation/input identity and original expiry.

The existing six snapshot tests passed before fixture refactoring (18.00
seconds). All eight snapshot tests then passed after the behavior fix (22.25
seconds). The shared public preparation fixture replaces duplicated test setup,
and the old byte-acknowledgement recovery test now uses its existing
`interrupted_worker` helper instead of repeating the constructor. No additional
generation counter, journal, migration or byte store is introduced. Stored
materialization decoding and original receipt data are unchanged; the existing
both-store publication contract also checks identical acknowledgement replay.

The regression commands above were rerun after the publication fix and helper
reuse. They pass **153 admission**, **211 Issue**, **27 shared intake**, **128
store**, **97 ordinary worker**, **56 native worker** and **one native both-store
publication contract**: **673 disjoint tests**. The focused eight-test run is
included in that native total. The full native worker run completed in 252.56
seconds. Clippy for the four affected packages with all targets/features and
warnings denied, formatting, architecture, strict OpenSpec validation, 39 local
documentation links and whitespace checks pass.

The first parallel run of the single-connection publication contract failed at
fixture pool creation with `PoolTimedOut`, before its behavioral assertions.
PostgreSQL readiness was verified and the unchanged test rerun separately after
the other native tests finished; it passed in 6.11 seconds. No timeout or fixture
limit was changed. This setup failure is not a behavioral Red. Only the owned
disposable PostgreSQL service is stopped afterward; no commit is created.
