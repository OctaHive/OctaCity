# Task 1.7 verification

Qualified on a disposable PostgreSQL **18.6-bookworm** container. An existing
manual-Issue persistence test passed before implementation, establishing a working
database environment. All assertions use public Issue/document/discussion ports;
database snapshots and an outbox constraint are test setup/fault injection.

## Red / Green

Commands below assume `OCTACITY_POSTGRES_URL` identifies the disposable service.
All Cargo commands used `CARGO_INCREMENTAL=0`.

```sh
cargo test --locked -p octacity-server-store-postgres --test issues issue_materials_upgrade_restore -- --ignored --nocapture
```

**Red:** PostgreSQL did not implement `edit_issue_materials` or
`issue_material_edit`. **Green:** the new adapter persisted a comment, an
Issue-owned document and a linked Project document in one immutable revision.
The test covers schema-120 databases and a physical restored copy, current/exact
reads after connecting a new pool, legacy-empty selections, original creation and
selection receipts after later requirements/selection edits, and removal without
deleting source content or transferring ownership.

```sh
cargo test --locked -p octacity-server-store-postgres --test issues issue_materials_validate_source_pins -- --ignored --nocapture
```

**Red:** a forged comment digest was accepted (`invalid source 0`). **Green:**
fresh mutations validate actual source pins in their transaction. Qualified cases
include another Issue's comment/document, wrong owner kind, digest/size mismatch,
an unpublished document, expired comment bodies and both document deadlines before
deletion. Rejected operations publish neither a revision nor a receipt/outbox fact.

After discussion/requirements expiry and document cleanup, exact replay preserves
the original references while requirements remain redacted and source bodies stay
unavailable. The read-only receipt also remains redacted. Removing expired sources
works without reopening them and preserves historical references.

## Persistence and migration choice

No new DDL is necessary: existing bounded JSONB Issue revisions already retain
the optional typed selections, and the existing scoped idempotency journal can
retain the original reference-only command. Its ordinary flat Issue outcome is
kept so requirements-retention SQL redacts selection receipts through the same
path as other Issue mutations. No reference backfill, owner reassignment, parallel
body table or competing receipt authority is introduced.

The writer reuses Project/Issue fencing and the common revision/audit/outbox
transaction. Discussion cleanup takes the same Issue fence. Document journal and
object rows are share-locked through publication of the selection revision. Only
desired references are resolved; exact replay is checked first.

## Concurrency and transaction qualification

```sh
cargo test --locked -p octacity-server-store-postgres --test issues issue_materials_ -- --ignored --nocapture --test-threads=2
```

**Four tests passed.** The two additional tests qualify the reused transaction
and idempotency behavior without adding a second transaction mechanism:

- Concurrent edits of one reviewed revision have one winner; the rejected key is
  not consumed and can be used after reviewing the new version.
- Concurrent identical scoped keys produce one Applied result and one exact
  Replayed result; conflicting intents have one winner.
- Equal caller keys in Alice/Bob scopes remain independent and retain actual
  selection attribution. Later edits leave the original receipt unchanged.
- An injected outbox failure rolls back head/history/receipt together; retry
  through a new pool applies once. A configured provider-push policy produces no
  push intent for this local selection change; source bodies remain unchanged.

## Regression and static checks

```sh
CARGO_INCREMENTAL=0 cargo test --locked -p octacity-server-store-postgres --test issues --test issue_execution --test migrations -- --ignored --test-threads=2
CARGO_INCREMENTAL=0 cargo test --locked -p octacity-server-store-postgres --test issue_execution execution_watermark_is_issue_local_in_memory -- --nocapture
CARGO_INCREMENTAL=0 cargo clippy --locked -p octacity-server-store-postgres --lib --test issues --test issue_execution --test migrations -- -D warnings
cargo fmt --all -- --check
python3 tools/check_architecture.py
git diff --check
openspec validate add-issue-classification-snapshots --strict
```

**112 tests passed:** 80 Issue, 23 PostgreSQL execution, one in-memory execution
contract and eight migration tests. Clippy, formatting, diff and architecture
checks passed (64 packages, 263 edges). Strict OpenSpec validation passed with the
existing informational archive notice about syncing the predecessor delivery
preset spec before archiving this change.

HTTP/Console material operations remain tasks 6.7–6.9; snapshot consumption and
its combined 1 MiB selected-text ceiling remain section 3. This task does not
start execution or alter admitted Work/Run inputs.
