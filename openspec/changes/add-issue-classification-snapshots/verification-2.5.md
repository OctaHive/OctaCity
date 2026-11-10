# Task 2.5 verification

## Scope

Two PostgreSQL characterization tests exercise the public
`ApplyIssueClassification` store boundary with existing Issue and Factory
fixtures. Both close the original connection pool and open a fresh pool before
retrying, so recovery reads durable receipts rather than process-local state.

The successful receipt test applies definition-backed priority and size with an
exact code-subject pin, then changes acceptance, priority, size, type, due date and
a custom property. It also publishes a newer catalog retiring the original
priority. An exact retry returns the original Issue outcome, including its values,
origins and input pins. Four concurrent retries return that same result. A retry
with a later operational timestamp and an expired runtime claim also returns the
stored outcome; it does not authorize a fresh write. Reusing the operation ID
with changed selected values or input pins returns a conflict.

The rejected receipt test first records a conflict against a manually changed
size. After another requirements edit and pool restart, retries still return
that original conflict even when the runtime claim has expired. Recovery does
not replace a recorded rejection with a new authority failure.

Both cases assert that retries leave the current Issue, revision history and
idempotency/audit/outbox evidence counts unchanged. Full Factory Run snapshot
equality also covers Work and scheduling state. The successful test compares
against the snapshot captured before either classification publication and
checks that the original historical Issue remains readable.

No production code, receipt format, database schema or authorization boundary
needed changing. Existing mutation fixtures and evidence probes were reused.

## Observed test outcomes

These are characterization tests for already implemented behavior. The first
test passed on its initial run; the expanded pair also passed before any
production change. There was no behavioral Red and no Red/Green implementation
cycle to perform. A failure was not manufactured by changing correct code.

All Cargo commands used `CARGO_INCREMENTAL=0`. Native tests explicitly selected
a disposable PostgreSQL 18.6 service with `OCTACITY_POSTGRES_URL`.

```sh
cargo test --locked -p octacity-server-store-postgres --test issue_execution \
  typed_classification_replay_after_later_edits_and_pool_restart_returns_the_original_outcome \
  -- --ignored --nocapture
```

Initial characterization: **1 passed**.

```sh
cargo test --locked -p octacity-server-store-postgres --test issue_execution \
  mixed_classification::replay -- --ignored --nocapture
```

Expanded characterization: **2 passed**.

```sh
cargo test --locked -p octacity-server-store-postgres \
  --test issues --test issue_execution -- --ignored --test-threads=2
cargo clippy --locked -p octacity-server-store-postgres \
  --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
python3 tools/check_architecture.py
openspec validate add-issue-classification-snapshots --strict
git diff --check
```

Final PostgreSQL regressions: **112 passed** (30 execution, 82 Issue), including
legacy classification replay, ordering and conflict coverage. Clippy, formatting,
architecture checks (64 packages, 263 edges), strict OpenSpec validation and diff
checks passed. The disposable service was stopped after qualification.

This test-only slice does not change UI or API schemas. Earlier task 2.3/2.4
work remains in the working tree; no commit was requested for task 2.5.
