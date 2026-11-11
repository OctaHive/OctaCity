# Task 3.3 verification

## Behavior and public boundaries

Manual Issue snapshot execution is fenced at the PostgreSQL transaction that
publishes its ordinary Work/Run. Input byte publication is recoverable preparation,
not permission to admit a snapshot that has changed since review.

The admission owner checks the retained private snapshot, its complete Incoming
payload and resolved subject, the pinned consumer input contract, the original
Issue execution request and its security/replay identities. It reuses the existing
Issue association validator for the Work's requirements, target and classification.
The Project catalog and Issue head remain locked until the admission transaction
commits. A conflict rolls back ordinary admission without a Run or implementation
dispatch; already published immutable input bytes remain recoverable input data.

New snapshots retain an optional `definition_head` alongside the immutable
consumer `input.definition_version`. Any catalog publication while review or
preparation is pending requires fresh review, including compatible publications.
This concurrency fence does not replace the consumer's pinned definitions.
Earlier compact/full snapshot encodings remain readable with their original
digests; an unadmitted snapshot without the new fence must still match its pinned
catalog at admission. Already admitted receipts keep the existing recovery path.
No migration, new execution journal, endpoint or scheduler was introduced.

Tests use authorized request/publication commands, ordinary Issue/catalog/comment
mutations, and the public prepared-admission store boundary. A controlled external
Artifact byte port pauses after the two input objects have been written. The test
then commits the concurrent edit and resumes publication; no sleep or scheduling
assumption determines which data wins. Integrity tests substitute input through
the admission contract, without editing persisted rows.

## Observed Red/Green cycles

All commands below ran with `CARGO_INCREMENTAL=0` and an explicitly configured
disposable `OCTACITY_POSTGRES_URL`.

```sh
cargo test --locked -p octacity-server --test project_text_intake \
  snapshot_admission_rejects_requirements_changed_after_byte_publication -- --ignored --nocapture
cargo test --locked -p octacity-server --test project_text_intake \
  snapshot_admission_rejects_definitions_changed_after_byte_publication -- --ignored --nocapture
cargo test --locked -p octacity-server --test project_text_intake \
  snapshot_admission_rejects_substituted_payload -- --ignored --nocapture
cargo test --locked -p octacity-server --test project_text_intake \
  snapshot_admission_rejects_a_snapshot_substituted -- --ignored --nocapture
cargo test --locked -p octacity-server --test project_text_intake \
  snapshot_review_requires_fresh_review -- --ignored --nocapture
```

| Case | Observed Red | Observed Green |
| --- | --- | --- |
| Acceptance changes after input byte publication | The stale request incorrectly created and associated a Run. | Conflict; no admission, Issue association or discoverable Run. |
| Definitions change after input byte publication | The request incorrectly admitted under its earlier review. | Conflict while holding the catalog/Issue fences. |
| Payload changes while the selection digest is retained | Prepared admission incorrectly accepted the different payload. | Conflict after comparing the complete retained Incoming value. |
| Another Issue request supplies its processing identity | An identical-text Issue could admit the first Issue's snapshot. | Conflict after binding the preparation to its exact scoped request. |
| Definitions change before preparation begins | The mismatch returned generic `Invalid` instead of a review conflict. | `Conflict` before any byte write or preparation reservation. |

Additional coverage exercises a concurrent classification edit through the same
exact Issue-version fence and a positive case: appending unselected discussion
admits the original selected values and snapshot digest. These characterize the
shared fence without additional implementation branches.

After all corrections, the seven snapshot tests passed together. After splitting
their fixtures from race/integrity assertions and removing SQL copies of the
domain identity encoding, they passed again in the complete native suite below.

## Regression qualification

```sh
CARGO_INCREMENTAL=0 cargo test --locked -p octacity-server-application --lib issues
CARGO_INCREMENTAL=0 cargo test --locked -p octacity-server-store --lib
CARGO_INCREMENTAL=0 cargo test --locked -p octacity-server-application --lib \
  management::tests::project_text_intake_tests
CARGO_INCREMENTAL=0 cargo test --locked -p octacity-server-application --lib factory::authoring
```

Passed: **186 Issue/application**, **128 store**, **27 shared intake**, and
**78 authoring tests**: **419 tests** in disjoint selections. This includes the
legacy full-catalog wire/digest fixture, publication of an unprepared legacy
request, and the bounded compact snapshot regression.

```sh
CARGO_INCREMENTAL=0 cargo test --locked -p octacity-server --test project_text_intake \
  -- --ignored --test-threads=1
```

Passed: **24 native tests** against disposable PostgreSQL 16 and MinIO, with
`OCTACITY_POSTGRES_URL`, `OCTACITY_MINIO_ENDPOINT`, `OCTACITY_MINIO_ACCESS_KEY` and
`OCTACITY_MINIO_SECRET_KEY` configured for those services. The suite includes
the seven new snapshot cases, byte/admission/association recovery, process death,
HTTP restart, transaction failure, body cleanup and existing exact-producer
classification. Total qualified behavioral tests: **443**.

```sh
CARGO_INCREMENTAL=0 cargo clippy --locked \
  -p octacity-server-factory -p octacity-server-application \
  -p octacity-server-store-postgres -p octacity-server \
  --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
python3 tools/check_architecture.py
openspec validate add-issue-classification-snapshots --strict
git diff --check
```

All checks passed. The architecture check reported 64 workspace packages and
263 dependency edges. Disposable services were stopped after qualification.

The native race assertions qualify this task's admission fence. Snapshot-specific
replay/restart qualification, per-step access, scheduling policy, and material
body availability/selection fences remain the separately planned tasks 3.4–3.9.
