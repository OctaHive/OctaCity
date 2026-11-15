# Task 10.1: selected Issue field contract

## Scope

`IssueFieldForm` replaces the fixed-slot selection in the shared Issue writer.
One Project/catalog pin and a bounded field list derive both the closed result
shape and allowed writes. Core choices and ordinary Project scalar properties
use the same contract; no classification role, source-path mapper, duplicate
schema, import or clear operation is added.

The existing owner retains association, source-ownership, requirements and
selected-value baseline checks, per-field provenance and immutable revisions.
Manual overrides of ordinary properties retain their actual human attribution.
Default catalogs serialize explicitly and selected size definitions include the
configured default scale. Development fixtures use the current encoding.

PostgreSQL runtime qualification belongs to 10.2. Automatic save on ordinary
agent steps, consumer/template changes and generated REST/Console contracts
remain tracked by 10.3–10.8. This report does not claim those tasks are complete.

## Observed Red/Green

Public boundaries: `IssueFieldForm::form_schema` / `complete`,
`IssueRevision::classified`, and ordinary planning/manual attribution.

- `cargo test --locked -p octacity-server-store --lib selected_issue_field_form_derives`
  first failed because the form API was missing, then passed with Project-derived
  text/choice/boolean/integer/date shapes and required/optional handling.
- `cargo test --locked -p octacity-server-store --lib selected_issue_field_form_applies`
  first failed because form completion was missing, then passed applying all
  supported kinds and preserving an omitted optional value and original revision.
- `cargo test --locked -p octacity-server-store --lib selected_issue_field_form_priority_only`
  first failed because priority-only application changed unselected planning,
  then passed after restricting planning updates to supplied planning fields.
- `cargo test --locked -p octacity-server-store --lib selected_issue_field_form_retains_configured_default_catalogs`
  first failed because default sizes serialized as absent, then passed with
  explicit catalogs and retained selected default sizes.
- `cargo test --locked -p octacity-server-store --lib selected_issue_field_form_manual_override`
  first failed because an ordinary property's manual override lost attribution,
  then passed without renewing unrelated fields.
- `cargo test --locked -p octacity-server-store --lib selected_issue_field_form_debug`
  first exposed agent text through `Debug`, then passed with redacted formatting.

Additional behavioral cases reject unknown/unselected/duplicate targets,
foreign/stale catalogs, retired fields/choices, invalid scalar values, missing
required values, nulls and protected targets. They exercise unchanged
source-ownership, requirements, Issue-identity and selected-field conflict guards.

## Regression checks

| Command | Observed result |
| --- | --- |
| `cargo test --locked -p octacity-server-store --lib` | 138 passed |
| `cargo test --locked -p octacity-server-application --lib issues` | 214 passed |
| `cargo test --locked -p octacity-server-application --lib issue_classification` | 7 passed |
| `cargo check --locked -p octacity-server-store-postgres --tests` | Passed; compilation, not database qualification |
| `cargo clippy --locked -p octacity-server-store -p octacity-server-application --lib -- -D warnings` | Passed |
| `cargo clippy --locked -p octacity-server-store --all-targets -- -D warnings` | Passed |
| `cargo fmt --all -- --check` | Passed |
| `python3 tools/check_architecture.py` | Passed: 64 packages, 264 dependency edges |
| `openspec validate add-issue-classification-snapshots --strict` | Passed |
| `git diff --check` | Passed |

The PostgreSQL integration tests were updated to the current Rust/form contract
and compiled; they were not executed against a disposable database in this slice.
No provider calls, stand restart, native delivery run, PR or commit was made.
