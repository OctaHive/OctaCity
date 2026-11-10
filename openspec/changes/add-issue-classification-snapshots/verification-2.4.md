# Task 2.4 verification

## Scope

The existing Issue owner now provides `ReviewIssueClassificationQuery` and a
shared immutable-revision review. Field diagnostics distinguish missing values,
legacy unattributed values, changed requirements, changed/unavailable code subjects,
changed declared material dependencies and unsupported reproduction claims.
Review uses the canonical values and origins; it never renews an assessment,
publishes another Issue revision, changes Work scheduling or starts execution.

Attributions can retain optional exact code and material dependencies in bounded
`IssueClassificationAssessmentInputs`. Materials reuse the existing owner-aware
comment/Issue-document/Project-document references and shared 32-entry/duplicate
validation. The classification writer checks declared material pins against both
the reviewed and current Issue selections. An explicitly projected
`assessment_inputs` value must equal the accepted producer result, and a declared
code subject must match that producer's actual input. Earlier publications omit
these pins and preserve their original encoding. No receipt envelope or database
schema migration was introduced.

Evidence diagnostics are separate from the field's own applicability problems.
`reproduced` and `not_reproduced` require independent proof; honest unresolved
values remain usable even when referenced evidence is unavailable or inconclusive.
Their evidence diagnostics remain visible rather than being converted into a
negative finding. Manual edits retain task 2.3's prohibition on writing verified
claims; this read path grants no new write or admission authority.

`FactoryIssueClassificationEvidenceReader` reuses the retained Factory journal,
exact installed command catalog, ordinary command observation verifier and shared
Artifact reader. It checks original Work/Run/record, subject, environment,
criterion, observation/freshness window, profile, report schema and exact current
published bytes. A missing profile, denied/expired/unavailable body, wrong pin or
technical condition cannot establish a verified reproduction outcome. The reader
returns an opaque qualified finding without report bytes. It requires independent
Artifact download permission, not producer transcript access. Proofs are shared
only within one review request and are requalified on later reads.

REST/OpenAPI exposure, Console presentation, snapshot admission and versioned
triage/example wiring remain their separately planned tasks. This slice adds no
poller, duplicate content store, implicit material ingestion or execution trigger.

## Observed Red and Green

All Cargo commands used `CARGO_INCREMENTAL=0`. PostgreSQL tests explicitly selected
a disposable PostgreSQL 18.6 service through `OCTACITY_POSTGRES_URL` and ran their
normally ignored native cases. Fixture/setup failures were corrected and are not
counted as behavioral Red. Missing newly required public APIs established Red
before their implementations, as permitted by the repository TDD contract.

```sh
cargo test --locked -p octacity-server-application --lib \
  classification_review_marks_changed_acceptance_stale_without_mutating_the_assessment \
  -- --nocapture
```

Red: the public Issue classification review/diagnostic contract was absent.
Green: an assessed size is current before an acceptance edit, reports
`requirements_changed` afterward and retains its original attribution without a
mutation from the read.

The following core tests were each added and run before their corresponding
implementation:

```sh
cargo test --locked -p octacity-server-store --lib \
  classification_review_compares_the_declared_exact_code_subject -- --nocapture
cargo test --locked -p octacity-server-store --lib \
  classification_review_compares_only_declared_exact_material_dependencies -- --nocapture
cargo test --locked -p octacity-server-store --lib \
  classification_application_retains_declared_inputs_and_rejects_substituted_materials -- --nocapture
cargo test --locked -p octacity-server-store --lib \
  classification_review_rejects_unproved_reproduction_without_blocking_unresolved_values -- --nocapture
cargo test --locked -p octacity-server-store --lib \
  classification_review_requires_current_independently_readable_evidence -- --nocapture
cargo test --locked -p octacity-server-store --lib \
  classification_review_preserves_unresolved_routes_when_the_selected_evidence_is_unavailable -- --nocapture
cargo test --locked -p octacity-server-store --lib \
  classification_review_distinguishes_missing_values_from_legacy_unattributed_values -- --nocapture
```

- Exact subject/material pin and diagnostic APIs were initially absent. Green
  retains exact identities/digests, distinguishes missing versus changed subjects
  and reports a removed declared material without changing requirements or the
  original assessment. Final characterization covers comments, Issue documents,
  linked Project documents and unrelated selection additions.
- The classification update initially lacked assessment inputs. Green commits
  those pins with only the selected fields and rejects a substituted material
  digest without changing the baseline.
- Reproduction initially lacked missing-proof diagnostics. Green rejects claims
  without independent evidence while allowing explicit unknown/needs-input data.
  A later Red extended this test with `probably`; Green reports an unsupported
  reproduction value instead of treating it as current.
- Selected evidence initially lacked independent availability/expiry/subject
  diagnostics. Green reports metadata-only evidence as unavailable, respects the
  exclusive freshness deadline and identifies another or unresolved code subject.
- Behavioral Red: an honest `unknown` field was blocked merely because its
  referenced evidence was unavailable. Green separates evidence diagnostics from
  claims that actually require proof and preserves unresolved routes.
- Missing-value/unattributed-value diagnostics were initially absent. Green
  distinguishes an unset field from legacy data without inventing a producer.

```sh
cargo test --locked -p octacity-server-application --lib \
  issue_classification_evidence_uses_exact_retained_command_bytes_and_criterion -- --nocapture
cargo test --locked -p octacity-server-application --lib \
  issue_classification_review_qualifies_evidence_without_private_producer_access_or_mutations \
  -- --nocapture
```

Red: the independent qualified reader and its Issue-query installation API were
absent. Green uses a real ordinary command/output/Flow journal fixture and exact
retained Artifact bytes. Positive and negative findings follow the installed
criterion; unavailable environments remain indeterminate. Contrary claims,
unsupported claims and a substituted criterion cannot manufacture verification.
Missing bytes are not reconstructed from the journal. A policy granting only
Artifact downloads proves that producer transcript permission is unnecessary;
revoking that permission leaves field metadata readable and evidence unavailable.
The query tests also preserve an unresolved value with unavailable evidence and
assert unchanged Issue mutation evidence.

Before extending the native fixture, the existing successful application test
passed. The new native behavior then followed Red/Green:

```sh
cargo test --locked -p octacity-server --test factory_configured_flow \
  classification_action_retains_declared_code_inputs_and_reviews_changed_subject_after_restart \
  -- --ignored --nocapture
```

Behavioral Red: the accepted result explicitly declared its code input, but all
five applied field origins still had empty pins. Green retains the exact input
through the real action, authorized Issue command, PostgreSQL revision and restart.
The same subject is applicable; another base reports changed-subject diagnostics
without rewriting Issue data. An additional native negative test rejects a subject
not supplied to the accepted producer and preserves the accepted result, Issue,
Work and outbox. Legacy publications explicitly retain empty/omitted input pins.

The assignee/date/unselected-comment scenario passed as characterization: those
existing mutations preserve assessment origin and requirements applicability,
do not select discussion and leave the size current under the new review.

## Final regression checks

```sh
cargo test --locked -p octacity-server-store --lib
cargo test --locked -p octacity-server-application --lib issues
cargo test --locked -p octacity-server-application --lib factory_command_output_source_tests \
  -- --test-threads=2
cargo test --locked -p octacity-server-store-postgres \
  --test issues --test issue_execution -- --ignored --test-threads=2
cargo test --locked -p octacity-server --test factory_configured_flow \
  issue_outcomes -- --ignored --test-threads=2
cargo clippy --locked -p octacity-server-store -p octacity-server-store-postgres \
  -p octacity-server-application -p octacity-server --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
python3 tools/check_architecture.py
openspec validate add-issue-classification-snapshots --strict
git diff --check
```

Results: 126 core store, 175 application Issue, 27 ordinary command output, 82
PostgreSQL Issue, 28 PostgreSQL execution and 18 native configured Flow Issue
outcome tests passed (456 total). The supplementary `--lib classification` filter
also passed 19 tests, overlapping these suites; its existing requirements-cycle
test completed successfully after approximately 146 seconds. Clippy, formatting,
architecture (64 packages, 263 dependency edges), strict OpenSpec validation and
diff checks passed. Matching field identities replaced positional coupling in
the query; the affected query/output tests pass after that behavior-preserving
cleanup.

An in-memory fixture helper restores historical successor revisions solely for
public query qualification, without a production mutation endpoint or receipt.
The ordinary byte-store fixture now returns `NotFound` for unavailable bytes
instead of panicking; the full affected output suite passed.

No UI/browser or full-workspace execution was claimed for this backend slice.
Documentation/task tracking have no executable behavior requiring TDD. The
disposable PostgreSQL container was removed after verification. Existing task 2.3
work was retained; no commit was created.
