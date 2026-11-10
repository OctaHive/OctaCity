# Review corrections after tasks 1.4–1.7

## P1: isolate Issue documents from generic execution-input lookup

The command materializer resolves original Work task/acceptance references through
the public `ArtifactRecordStore::published_execution_input` port. Its Project-text
fallback previously returned Issue documents without requiring their owner.

Added the public-port regression before implementation:

```sh
CARGO_INCREMENTAL=0 cargo test --locked -p octacity-server-store-postgres --test issues issue_documents_cannot_be_substituted -- --ignored --nocapture
```

**Red:** a live Issue-owned document resolved successfully; the assertion failed
with `Issue owner must be checked before generic command input resolution`.

**Green:** common publication lookup now requires an exact optional Issue owner.
Project and generic command/Job callers select no Issue owner; the dedicated Issue
reader supplies the exact Project/Issue. The owner check runs in SQL before
returning input metadata and uses retained ownership rather than a mapping-name
denylist. Existing legacy Project inputs remain compatible.

The test verifies that a raw Issue document is NotFound through generic command
lookup, shared Project documents still resolve, another Issue cannot resolve the
attachment, and the exact owner's reader still returns its original publication.
Both Issues remain unchanged. A caller-supplied Work Artifact reference does not
grant the command materializer access to the attachment.

## P3: separate document query and publication ports

This correction changes interface dependencies, with no intended behavior change.
Before refactoring, the existing Issue and management application tests passed.

- `ProjectDocumentQueryStore` owns read-only metadata/discovery and inherits the
  existing Project input query port.
- `IssueDocumentInputStore` contains only its owner-aware read methods and
  `Send + Sync`; it no longer inherits Project publication/recovery methods.
- `ProjectDocumentInputStore` explicitly adds publication and pending-body
  recovery operations for the shared intake workflow.
- Material validation, document query handlers and outcome navigation depend on
  query ports; publication handlers require the write ports explicitly.

The in-memory and PostgreSQL implementations retain the same method bodies;
no parallel storage or alternate publication implementation is added.

## Regression checks

PostgreSQL commands used an explicitly configured disposable PostgreSQL 18.6
service through `OCTACITY_POSTGRES_URL`. All Cargo commands disabled incremental
caches with `CARGO_INCREMENTAL=0`.

```sh
cargo test --locked -p octacity-server-application --lib issues::tests --quiet
cargo test --locked -p octacity-server-application --lib management --quiet
cargo test --locked -p octacity-server-application --lib factory::commands --quiet
cargo test --locked -p octacity-server-store --lib --quiet
cargo test --locked -p octacity-server-store-postgres --test issues --test issue_execution --test migrations -- --include-ignored --test-threads=2
cargo test --locked -p octacity-server-store-postgres --test artifact_records -- --include-ignored --nocapture
```

**516 tests passed:** 169 Issue application, 61 management, 56 Factory command,
116 core store, 81 PostgreSQL Issue, 24 execution, eight migration and one Artifact
contract. The application test build retains its 11 existing `dead_code` warnings
in `factory_test_support`.

The full server composition was also checked with Clippy, including the new
query-port dependencies used by the runtime. API payloads, document ownership,
retained publications and execution snapshot planning remain unchanged.

```sh
CARGO_INCREMENTAL=0 cargo clippy --locked -p octacity-server-store -p octacity-server-application -p octacity-server-store-postgres -p octacity-server --lib -- -D warnings
CARGO_INCREMENTAL=0 cargo clippy --locked -p octacity-server-store-postgres --lib --test issues --test issue_execution --test migrations --test artifact_records -- -D warnings
cargo fmt --all -- --check
python3 tools/check_architecture.py
git diff --check
openspec validate add-issue-classification-snapshots --strict
```

All checks passed. Architecture remains 64 packages and 263 dependency edges.
OpenSpec retains its existing informational notice about synchronizing the
predecessor delivery preset spec before archiving this change. The disposable
PostgreSQL container was removed after qualification.
