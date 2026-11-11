# Task 3.8 verification

## Scope and public boundaries

This slice resolves explicitly selected same-Issue comments, exact Issue-owned
text publications and separately linked exact Project publications during manual
Issue execution. Tests use `RequestIssueExecutionCommand`,
`PublishIssueExecutionCommand`, the ordinary authorized source readers and the
existing managed input preparation/Work/Run query boundaries. Setup appends,
publishes and selects through the existing authorized application commands.

`IssueSnapshotMaterialReaders` composes independent authorized discussion and
owner-aware document readers. Execution rights supply no body-read shortcut.
Source owner/reference/digest/byte-size checks precede projection. Exact Flow
references must already be approved on the Issue; a bounded approved-set contract
preserves its reviewed order. No source discovery, latest-publication fallback,
new content store, Project catalog entry or ownership transfer is introduced.

Text occurs once in snapshot `values`, as an ordered array under the declared
Incoming alias. Separate `materials` lineage retains selection attribution and
original source metadata. Empty lineage is omitted from serialization so existing
snapshots retain their original encoding/digests. Shared Project publications
retain their available publication metadata; no nonexistent publisher actor is
invented. Source prose does not become evidence, policy or a gate response.

## Test-first observations

The successive focused commands were:

```sh
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server-application --lib \
  issue_material_snapshot_prepares_selected_comment_text -- --nocapture
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server-application --lib \
  issue_material_snapshot_prepares_owned_and_linked_document -- --nocapture
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server-application --lib snapshots::materials -- --nocapture
```

1. **Red:** an explicitly selected, readable required comment failed review with
   `unavailable_reference` at `/issue_snapshot/context`. **Green:** installing the
   ordinary authorized discussion reader admitted exactly its original Unicode,
   CRLF and trailing newline, excluding an unselected comment.
2. **Red:** approved exact Issue/Project publications remained unavailable under
   the comment-only implementation, with `issue_snapshot_material_unavailable`
   at `/issue_snapshot/materials/context`. **Green:** independent owner-aware
   document readers supplied the exact ordered text and original metadata for
   both exact and approved-set contracts.
3. **Red:** a request reviewed before readers were installed retained an optional
   omission, but publication after installation returned `Conflict { entity:
   Issue }`. **Green:** the compatibility path restores the original omission
   only if the complete candidate digest matches the already reviewed digest.
   It cannot downgrade required material or accept a different snapshot.

The final focused suite passes **five tests**. Independent permission denial and
lost-ack replay cases passed on the reader/publication implementation and are
additional qualification, not fabricated Red observations. Fixture compile
failures (an unavailable in-memory retention trait and misplaced schema setup in
an extracted native fixture) were corrected; neither is counted as behavioral
Red. Real retention qualification uses PostgreSQL rather than adding another
in-memory retention implementation.

## Qualified scenarios

| Scenario | Observed outcome |
| --- | --- |
| Explicit comment | The selected body is exact; unselected discussion is absent. Appending alone leaves material selection empty. |
| Exact and approved-set documents | Issue documents retain their Issue parent/publication/author, and separately linked Project documents retain their Project owner/publication. Exact selection excludes an approved comment that the consumer did not request. |
| Original provenance | Frozen entries equal original approved references and actor/time; source metadata equals the original comment/document publication, including digests and byte sizes. |
| Independent access | Live source reads establish the positive control. Denied discussion or Artifact access then blocks required preparation before private document byte I/O, Issue/Factory mutation or Run creation despite granted execution rights. |
| No implicit execution | Comment append and both document publication kinds leave approved selection unchanged and create no Run. Review itself creates no Run. |
| Lost byte acknowledgement | Managed preparation retains the original snapshot. Retry reuses it and does not reread either source document or resolve a newer code base. |
| Later source/selection changes | New comments and Issue/Project document versions followed by explicit selection replacement do not change the original admitted Work/Run, Incoming, private snapshot or receipt. |
| Legacy optional omission | Reader installation preserves a previously reviewed omission and digest; it does not silently add newly available text. |
| PostgreSQL recovery and retention | A new store instance recovers exact text/lineage after a lost acknowledgement. Existing retention deletes the selected comment body, both source document bodies and both Work input bodies. Replay through another store instance returns the same admitted association/Run without restoring private preparation or source content. |

The native recovery case reads the retained snapshot back through the PostgreSQL
public input store, compares all three source metadata records and selections,
and checks actual Work Incoming. It uses the existing file-backed byte adapter
and a resolver that rejects repeated resolution. The two source publications and
two Work inputs require exactly four byte writes, including the lost-ack recovery.

## Regression and repository checks

```sh
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server-application --lib issues
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server-application --lib factory::authoring
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server-application --lib management::tests::project_text_intake_tests
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server-store --lib

# OCTACITY_POSTGRES_URL and OCTACITY_MINIO_* select disposable local services.
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server --test project_text_intake -- --ignored --test-threads=1
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server --test project_text_intake material_snapshot_recovers \
  -- --ignored --nocapture
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server --test project_text_intake

CARGO_INCREMENTAL=0 cargo clippy --offline --locked \
  -p octacity-server-application -p octacity-server-store \
  -p octacity-server-store-postgres -p octacity-server \
  --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
python3 tools/check_architecture.py
openspec validate add-issue-classification-snapshots --strict
git diff --check
```

The disjoint application/store suites pass **200 Issue**, **79 authoring**,
**27 shared intake** and **128 store** tests: **434 total**. Native qualification
passes **30 PostgreSQL/MinIO intake tests** and **five checkpoint tests**, for
**469 disjoint tests** overall. Focused runs and the strengthened final native
lineage check are not counted twice. The previously qualified unselected-comment
admission case also passed separately before adding the native material case.

Clippy with warnings denied, formatting, architecture (64 workspace packages,
263 dependency edges), strict OpenSpec validation and diff checks pass. Native
linking emits the existing macOS compact-unwind size warning; it does not fail
these tests. Disposable PostgreSQL and MinIO services are stopped after checks.
No commit is created.

Aggregate UTF-8/count qualification, unavailable/mismatched/forged-source matrices
and final admission content/selection races remain task 3.9. Source intake,
example versions and HTTP/Console authoring/review remain their later planned
slices. Existing explicit producer-result execution and per-step Context contracts
are unchanged and remain covered by the regression suites.
