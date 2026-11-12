# Task 4.4 review fixes

## Finding and correction

The review found one P2: an approved document can fit the declared raw UTF-8
limit but exceed encoded Incoming or private snapshot limits. Previously the
shared builder returned `invalid_contract` or a generic store input error. The
Source did not recognize either as a candidate-local wait, so it aborted the
scan before an eligible later Issue could progress.

The snapshot owner now reports its existing JSON size limit through
`StoreInputError::JsonDocumentTooLarge`. The application checks the private
snapshot before constructing Incoming: that encoding encloses the same values
and schema pin plus provenance, within the existing one-MiB bound. Only that
specific size error from snapshot encoding becomes
`issue_snapshot_encoding_limit_exceeded` at `/issue_snapshot/encoding`.
Serialization, schema, storage and unrelated input errors retain their existing
handling. No wire format is duplicated and no ceiling is raised.

Source maps this explicit diagnostic to `Incompatible`, using its existing
preparation/wait owner. Optional oversized text remains a blocker. Manual review
gets the same actionable diagnostic. Legacy reviewed optional omissions are
restored before the encoding check and retain their original digest/receipt.

## Observed Red/Green

The new native test uses the actual Source worker, migrated PostgreSQL and
authorized document/comment/material commands. Each case places the oversized
Issue before an eligible Issue in the provider page. Four cases cover required
and optional contracts with:

- `x` followed by 600,000 newline bytes: raw UTF-8 fits, JSON escaping overflows.
- 1,048,000 plain bytes: Incoming fits, retained source metadata overflows the
  private snapshot ceiling.

```sh
# OCTACITY_POSTGRES_URL selects the owned disposable PostgreSQL service.
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server --lib source_encoded_material_overflow_waits \
  -- --ignored --nocapture
```

**Red:** Source returned `completed=0, deferred=1` instead of completing the scan
with one waiting candidate (3.54 seconds). **Green:** all four cases passed in
one test (17.88 seconds): the oversized Issue retains an `Incompatible` wait,
no prepared input or Run; the later eligible Issue admits its literal approved
text. Only the named document and eligible Work inputs are written.

The existing public manual-review test was extended before implementation to
require the new diagnostic for both encoding failures:

```sh
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server-application --lib \
  issue_material_snapshot_rejects_new_oversized_text_without_a_reviewed_omission \
  -- --nocapture
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server-application --lib compatibility::encoded_size \
  -- --nocapture
```

**Red:** manual review reported `/issue_snapshot/fields` instead of the explicit
encoding diagnostic (0.17 seconds). **Green:** all three encoded-size tests
passed (0.55 seconds), including both retained-omission compatibility cases.
Compiler/fixture corrections are excluded from Red observations.

An ordinary public-classifier regression additionally verifies that generic
`invalid_contract` and store input errors, including an unscoped
`JsonDocumentTooLarge`, do not become Source input waits. There is no broad
catch-and-downgrade of store failures.

## Regression commands

```sh
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server-store --lib
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server-application --lib factory::admission
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server-application --lib issues
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server-application --lib management::tests::project_text_intake_tests
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server --lib runtime::sources::tests::worker
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server --lib runtime::sources::tests::worker \
  -- --ignored --test-threads=2
CARGO_INCREMENTAL=0 cargo clippy --offline --locked \
  -p octacity-server-store -p octacity-server-application -p octacity-server \
  --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
python3 tools/check_architecture.py
openspec validate add-issue-classification-snapshots --strict
git diff --check
```

Observed final results (all passed):

| Suite | Passed |
| --- | ---: |
| Core store | 128 |
| Application admission | 153 |
| Application Issues | 211 |
| Shared manual text intake | 27 |
| Ordinary Source workers | 98 |
| Native PostgreSQL Source workers | 66 |
| **Unique regression tests** | **683** |

The native suite passed in 284.46 seconds. It includes the new four-case fairness
test; ordinary workers include the unrelated-error classification guard. Focused
reruns are not counted again. Clippy passed for all targets/features of the three
affected packages with warnings denied. Formatting, strict OpenSpec validation,
architecture (64 packages, 263 edges), 47 local documentation links and whitespace
checks passed. The owned disposable PostgreSQL container was removed after
qualification. No user credentials or live provider were used, and no commit was
created.
