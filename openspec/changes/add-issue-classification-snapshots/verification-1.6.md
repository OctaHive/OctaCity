# Task 1.6 verification

Implemented and qualified through `AuthorizedCommandHandler` and the existing
`GetIssueQuery`, using the public Issue/discussion/document ports and in-memory
adapters. The new narrow `IssueMaterialStore` reuses immutable Issue revisions and
their atomic audit/outbox/replay publisher. No execution or provider port is
installed in these fixtures.

## Red / Green

Each test preceded its implementation. Commands used `CARGO_INCREMENTAL=0` to
avoid growing incremental caches on this workstation.

| Command suffix after `cargo test --locked -p octacity-server-application --lib` | Observed Red | Observed Green |
| --- | --- | --- |
| `issue_materials_select_remove -- --nocapture` | Missing selection command, handler and revision field. | Same-Issue comment selection/removal, exact historical read and original replay; source body unchanged and absent from selection metadata. |
| `issue_materials_pin_both -- --nocapture` | Missing Issue-/Project-document reference variants. | Both exact owner kinds selected; later publications do not retarget; removing links does not delete or transfer sources. |
| `issue_materials_reject_duplicate -- --nocapture` | Duplicate publication accepted (`invalid selection 0 was accepted`). | Duplicate comments/documents and 33 mixed entries reject without mutation; exactly 32 succeed; retained metadata rejects duplicate/oversized lists and invalid selection actors. Legacy absent selections decode empty. |
| `issue_materials_validate_sources -- --nocapture` | Document expired during review was accepted (`expired during review`). | Final deadline check rejects without publishing a revision/receipt/audit fact. Foreign owners, unavailable identities, forged digests/sizes, forged attribution and independent source/parent permissions are also qualified. |

`issue_materials_` now runs five passing tests. The additional shared-transaction
characterization test qualified the existing publisher before removing redundant
application owner checks in favor of the shared validator. It checks all five
transaction failure points, same-key isolation between Alice/Bob, actual selection
actors, retained attribution for unchanged entries, original replay after later
edits, rejection of conflicting retry intent and current Issue write authority.
Removing a hidden source selection needs no source body access; replay returns
safe original metadata without reopening the body.

## Regression checks

```sh
CARGO_INCREMENTAL=0 cargo test --locked -p octacity-server-application --lib issues::tests --quiet
CARGO_INCREMENTAL=0 cargo test --locked -p octacity-server-application --lib management --quiet
CARGO_INCREMENTAL=0 cargo test --locked -p octacity-server-store --lib --quiet
```

Results: **169 Issue tests, 61 management tests and 116 store tests passed**.
The application test build retains its 11 existing `dead_code` warnings in
`factory_test_support`; none originate from material selection.

```sh
CARGO_INCREMENTAL=0 cargo clippy --locked -p octacity-server-store -p octacity-server-application -p octacity-server-store-postgres --lib -- -D warnings
cargo fmt --all -- --check
python3 tools/check_architecture.py
git diff --check
openspec validate add-issue-classification-snapshots --strict
```

These checks pass. Architecture retains 64 packages and 263 dependency edges.
OpenSpec retains the existing informational archive notice: the main delivery
preset spec must first be synced from its predecessor change before archiving
this delta. It is unrelated to task 1.6.

## Scope

This task adds safe reference metadata, selection editing, current-version review,
history and scoped receipts. Source bodies and owners remain in their original
stores; publication alone selects nothing. The shared count limit is 32 entries;
the combined 1 MiB text ceiling is enforced by the later snapshot-consumer work.

PostgreSQL material mutation/receipt transactions, migration and recovery
qualification remain **1.7**. Its Issue constructor only initializes the new
legacy-empty field here. HTTP and Console operations remain **6.7–6.9**.
