# Task 3.7 verification

## Scope and public boundaries

This slice qualifies ordinary Project text, boolean, bounded-integer, date and
choice properties through `RequestIssueExecutionCommand` and
`PublishIssueExecutionCommand`, the existing managed input publication and exact
retained Work/Run reads. Setup publishes Project definitions and edits Issue
planning through public store contracts. The final fixture has no built-in
priority/size, classification attribution or semantic property binding and needs
no producer Run. Actual comment/document readers remain task 3.8.

Only tests, shared test setup and documentation change. No production behavior,
persistence model, permission, API schema, scheduler or scalar converter is
added. The common fixture builder now accepts an explicit scalar consumer schema
rather than duplicating its graph/configuration construction. Existing string
callers retain the same schemas and contract bytes.

## Test-first observations

```sh
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server-application --lib execution::snapshots -- --nocapture
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server-application --lib snapshots::properties -- --nocapture
```

Before changes, **14 existing snapshot tests passed**. The first new five-type
snapshot test passed on the existing production implementation, together with all
14 existing cases after extracting the reusable fixture constructor. All six new
cases subsequently passed without a production change. The final fixture was
strengthened to start with a completely unclassified Issue; all **20 snapshot
tests passed**. The final focused six-case run also passes after removing a
fixture-order dependency from the required-field selection.

There was **no missing-behavior Red**, and none is claimed. These are
characterization/qualification tests for generic selection already implemented
under the original [3.1](verification-3.1.md) and [3.2](verification-3.2.md)
Red/Green cycles. Fixture assembly/compilation failures are not behavioral Red.
No behavior was removed or artificially broken to manufacture one.

## Qualified scenarios

| Case | Independently specified outcome |
| --- | --- |
| Native scalar input without classification | UTF-8 text `ёж`, boolean `false`, integer `-2`, date `2028-02-29` and choice ID `test` appear in Incoming with their native scalar encoding. No predecessor, semantic slot or fabricated assessment attribution is present. |
| Exact definitions | The snapshot retains original text byte/integer bounds, date/boolean types, complete choice IDs/names/retirement flags and the exact full publication digest. Selected optional declarations remain available, while an unselected property's value/declaration is excluded. |
| Missing required scalar | Each of the five scalar types independently reports `issue_snapshot_missing_value` at its selected alias, before reserving an execution request or mutating Factory state. |
| Missing optional scalars | An all-optional contract admits `{}` with six explicit `Missing` omissions and no invented empty text, false/zero, date or choice default. |
| Present empty values | Required empty text and integer zero remain values alongside boolean false; the independent Work acceptance text remains nonempty. |
| Pinned bounds despite broader current data | A later valid Project publication permits a five-byte Unicode string, integers just outside either original bound, or a new choice ID. The old consumer pin rejects each supplied value with `issue_snapshot_incompatible_value`, including optional selections, despite its broader scalar schema and otherwise valid current Issue. |
| Later definition/value publications | After rename, wider bounds, choice retirement/addition and changes to every selected value plus a newly supplied optional value, exact request/publication replay retains the original Work/Run, Incoming, private snapshot, definitions, omissions and digest while the Issue retains its newer values. |

The required/optional bound matrix has eight cases; required absence checks each
of the five native types. Expected outcomes use explicit payload literals,
original catalog declarations and public failure diagnostics, not private
selector methods. No regression test reaches into SQL or reconstructs the
production selection algorithm.

## Regression commands

```sh
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server-application --lib issues
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server-application --lib factory::authoring
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server-application --lib management::tests::project_text_intake_tests
CARGO_INCREMENTAL=0 cargo clippy --offline --locked \
  -p octacity-server-application --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
python3 tools/check_architecture.py
openspec validate add-issue-classification-snapshots --strict
git diff --check
```

Passed disjoint suites: **195 Issue/application**, **79 authoring** and
**27 shared text intake** cases: **301 tests**, including the six new tests.
Focused snapshot/property runs are not counted twice. PostgreSQL/MinIO are not
needed for this slice: its unchanged persistence/admission owners retain their
previous native qualification, while the changed boundary consists entirely of
application tests and documentation. No commit is created.

Clippy with warnings denied, formatting, architecture (64 workspace packages,
263 dependency edges), strict OpenSpec validation, verification references and
diff whitespace checks passed. The final focused six-test run and final Clippy
check include the fixture-order cleanup. No services or external credentials
were needed, and no production code or dependency lockfile changed.
