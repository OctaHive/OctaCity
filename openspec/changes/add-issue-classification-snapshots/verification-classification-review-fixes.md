# Classification review fixes

## Scope

Addressed the two findings from the review of tasks 2.3–2.5:

- Retained property assessments now derive reproduction semantics from the exact
  catalog version in their attribution. The authorized query loads each required
  publication once per request. Current definitions discover unassessed fields;
  they do not change the meaning of historical values or evidence claims.
- `IssueReproductionClaim` owns the supported choice IDs and the distinction
  between unresolved values and outcomes requiring verified evidence. Manual
  validation, pipeline classification, applicability review and independent
  evidence qualification share it. Canonical property storage remains strings.

The pure review also reports `definition_unavailable` if a retained property's
original catalog is missing or belongs to another Project/version. The authorized
query propagates an unavailable catalog read rather than substituting current
semantics. Existing Artifact authorization, body qualification and receipt
formats are unchanged. No migration, extra persisted value or new dependency
was introduced.

## TDD observations

All Cargo commands used `CARGO_INCREMENTAL=0`.

```sh
cargo test --locked -p octacity-server-application --lib \
  issue_classification_review_preserves_historical_reproduction_after_catalog_rebinding
```

Red: before the fix, an exact historical Issue with a `reproduced` claim and an
independently qualified `NotMatched` observation reported a conflict initially,
but returned an empty problem list after the Project moved its reproduction
binding. Green: that same historical revision retains `EvidenceClaimConflict`;
the read leaves its Issue data and mutation evidence counts unchanged.

```sh
cargo test --locked -p octacity-server-store --lib \
  classification_review_reports_a_missing_original_definition
```

Red: absent original definitions produced an empty problem list. Green: the
serialized diagnostic contains `definition_unavailable`, without treating the
current catalog as the original definition.

The vocabulary extraction changes no behavior. Before it, the existing evidence
reader tests (2), authorized evidence query (1), manual classification tests (6)
and core classification tests (14) passed. The retained-Issue fixture was shared
between the existing and new authorized query tests with its existing test kept
green. Before extracting the vocabulary, the core suite was extended to
characterize all four supported unresolved routes and passed (15 tests).

The new unavailable-definition diagnostic exposed an older evidence test fixture
that referenced catalog 1 while supplying only the default catalog. That test now
uses an evidence-dependent built-in size to isolate its evidence checks from
definition availability. A missing planning wire field during that fixture update
was corrected; fixture setup failures are not counted as behavioral Red.

## Final qualification

```sh
cargo test --locked -p octacity-server-store --lib
cargo test --locked -p octacity-server-application --lib issues
cargo test --locked -p octacity-server-application --lib factory_command_output_source_tests
cargo test --locked -p octacity-server-store-postgres \
  --test issues --test issue_execution -- --ignored --test-threads=2
cargo clippy --locked -p octacity-server-store -p octacity-server-application \
  -p octacity-server-store-postgres --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
python3 tools/check_architecture.py
openspec validate add-issue-classification-snapshots --strict
git diff --check
```

All **442 tests passed**: 127 core, 175 Issue application, 28 command output,
30 PostgreSQL execution and 82 PostgreSQL Issue tests. The new historical-binding
regression is included in the 28 command-output cases. PostgreSQL tests selected
a disposable PostgreSQL 18.6 service explicitly through `OCTACITY_POSTGRES_URL`;
its container was stopped after testing.

Clippy passed with warnings denied for all targets/features of the three affected
packages. Formatting, architecture (64 packages, 263 edges), strict OpenSpec
validation and diff checks passed. No REST/client schema or UI behavior changed,
so their separate integration tasks remain pending. No commit was requested.
