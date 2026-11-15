# Task 10.2: PostgreSQL selected-field persistence

## Scope and implementation

The existing Issue revision/history/idempotency transaction persists selected
scalar values and their provenance. No migration, new store or old-format reader
was introduced. Pipeline field origins now require `form`: the SHA-256 identity
of the canonical reviewed `IssueFieldForm` selection. It includes the Project,
immutable definition version, field identities and required/optional flags;
definitions and values are not copied into another schema or value store.

Current development metadata fixtures include that required pin. Ordinary manual
and native-source origins keep their existing owners. Original operation receipts
retain their exact saved revision, field/form/definition pins and accepted source.

## TDD evidence

Boundary: public `PostgresStore` Issue/property/classification ports and exact
history/replay reads, using the existing disposable `TestDatabase`. The existing
evidence-count probe checks mutation/audit/outbox cardinality; assertions do not
inspect private classification tables or reconstruct application logic.

Baseline:

```sh
cargo test --locked -p octacity-server-store-postgres --test issue_execution \
  pipeline_priority_choice_preserves_unselected_planning_and_records_its_catalog -- --ignored
```

Observed: 1 passed before the change.

Red/Green:

```sh
cargo test --locked -p octacity-server-store-postgres --test issue_execution \
  selected_field_form_round_trip -- --ignored
```

- **Red:** scalar values saved, but the public field origin returned no form pin:
  `origin.form` was `null` instead of the reviewed selection's content digest.
- **Green:** after retaining the required form identity through the common Issue
  writer, the same test passed. Closing the original pool and opening a new
  store/pool preserved current values, original history, exact definitions and
  the original replay outcome.

Additional PostgreSQL regression cases qualify existing transaction guarantees:

- All five custom scalar kinds and four core choice fields round-trip; optional
  omissions and unselected values remain unchanged.
- Six fresh concurrent saves create one revision, receipt, audit and outbox event;
  five callers receive the original outcome as replays.
- After manual edits, catalog retirement and a pool restart, an exact retry with
  expired runtime authority returns its original receipt. Reusing the operation
  with changed form requiredness conflicts without another mutation.
- A missing required result rejects the entire field set and creates only its
  durable rejected receipt. That original conflict survives restart; corrected
  content cannot reuse the rejected operation but can use a fresh operation.
- A concurrent manual edit of a selected property and the form save have exactly
  one winner, with no mixed values/origins. An intervening property edit remains
  a conflict even when the value is subsequently restored.

These existing invariants passed without modifying the PostgreSQL transaction or
adding parallel validation logic. No artificial failing implementation was used
for already-supported behavior.

## Final checks

All database commands used `OCTACITY_POSTGRES_URL` pointing to an isolated local
PostgreSQL 18.6 service. The ordinary stand's database was not used.

| Command | Observed result |
| --- | --- |
| `cargo test --locked -p octacity-server-store-postgres --test issue_execution selected_field_form_ -- --ignored` | 5 passed |
| `cargo test --locked -p octacity-server-store-postgres --test issue_execution -- --ignored --test-threads=2` | 35 passed; 1 non-ignored test filtered out |
| `cargo test --locked -p octacity-server-store-postgres --test issues classification_metadata -- --ignored --test-threads=2` | 2 passed |
| `cargo test --locked -p octacity-server-store --lib issue_classification` | 25 passed |
| `cargo clippy --locked -p octacity-server-store-postgres --test issue_execution --test issues -- -D warnings` | Passed |
| `cargo fmt --all -- --check` | Passed |
| `python3 tools/check_architecture.py` | Passed: 64 packages, 264 dependency edges |
| `openspec validate add-issue-classification-snapshots --strict` | Passed |
| `git diff --check` | Passed |

Automatic agent-result application, generated REST/client/UI contracts and the
ordinary-stand journey remain separate tasks 10.3–10.10. No provider call, stand
restart, external effect, PR or commit was made. The disposable PostgreSQL service
was stopped after qualification.

## Review follow-up: independent forms and shared scalar shapes

The PostgreSQL ordering guard previously compared a form with the latest save
anywhere in the Issue. A regression test now supplies only `summary` from an
earlier Run after a newer Run saves `passed`. It also selects `passed` as optional
but omits it, checks that its newer attribution survives, and rejects a later
attempt by the older Run to overwrite that same field.

```sh
cargo test --locked -p octacity-server-store-postgres --test issue_execution \
  independent_field_forms_can_finish -- --ignored
```

**Red:** the disjoint `summary` save returned `Conflict { entity: Issue }`.
**Green:** checking order against each supplied field's last receipt allows both
disjoint saves while still rejecting the overlapping write. Priority, selected
properties and managed-label effects retain their own ordering/baseline checks.
The private receipt lookup now requires a field; it cannot request an Issue-wide
latest save.

Scalar schema derivation was consolidated into
`IssuePropertyKind::value_schema`, reused by forms and snapshot readers.
Read-side optional empty-choice handling stays with the snapshot reader. This
was a behavior-preserving refactor: before editing, the form shape test, six
ordinary-property snapshot tests and the empty-choice snapshot test passed.

Follow-up verification records the final checks below; the earlier table remains
the initial task-10.2 qualification history:

| Command | Observed result |
| --- | --- |
| `cargo test --locked -p octacity-server-store-postgres --test issue_execution independent_field_forms_can_finish -- --ignored` | Red, then Green |
| `cargo test --locked -p octacity-server-store-postgres --test issue_execution -- --ignored --test-threads=2` | 36 passed; 1 non-ignored test filtered out |
| `cargo test --locked -p octacity-server-store --lib issue_classification` | 25 passed |
| `cargo test --locked -p octacity-server-application --lib issues` | 214 passed, including ordinary scalar and optional empty-choice checks |
| `cargo clippy --locked -p octacity-server-store -p octacity-server-application --lib -- -D warnings` | Passed |
| `cargo clippy --locked -p octacity-server-store-postgres --test issue_execution -- -D warnings` | Passed |
| `cargo fmt --all -- --check` | Passed |
| `python3 tools/check_architecture.py` | Passed |
| `openspec validate add-issue-classification-snapshots --strict` | Passed |
| `git diff --check` | Passed |
