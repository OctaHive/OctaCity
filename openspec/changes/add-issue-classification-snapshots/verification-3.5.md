# Task 3.5 verification

## Scope and boundaries

This task qualifies consumption of an already frozen Issue snapshot through the
ordinary Flow input, Codex compilation and model Artifact Context boundaries.
The fixture starts with trusted admitted `FlowIncomingIssueSnapshot` data. Actual
comment/document lookup, owner authorization and byte freezing during manual
preparation remain task 3.8; material availability/admission races remain 3.9.
These tests do not claim those future readers are implemented.

Two nodes in the same Flow consume the same frozen Incoming channel through
different declared mappings. Implementation selects title, priority and bounded
comment/document text arrays; review selects only title. Both use real
`FlowNodeInput::prepare`, `FlowBuildIntent` and Codex compilation. The fixture
qualifies its declared mappings against the public Incoming schema contract and
uses explicit credential consumers independently of node display names.

No production code, public API, persistence format, permission profile or
authorization policy changed. The new tests characterize existing projection,
compilation and private Artifact access enforcement.

## Test-first results

```sh
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server-application --lib issue_snapshot_context -- --nocapture
```

The initial positive projection/compilation test passed on the existing
implementation. The three additional negative/access tests also passed before
any production modification. The final four-test run passed after adding an
explicit authority request to the selected comment and removing inference of
credential purpose from fixture node names. A final validation run additionally
qualifies live trusted metadata access before each rejected model read.

There was no missing-behavior Red or production fix in this slice, and none is
claimed. Fixture compilation/setup failures are not Red. These are
characterization tests for the generic Context machinery reused by the snapshot
feature, whose introduction and admission fences have separate Red/Green evidence
in [3.1](verification-3.1.md), [3.2](verification-3.2.md) and
[3.3](verification-3.3.md).

| Test | Observed outcome |
| --- | --- |
| Per-step field/text projection without a producer | Implementation input/prompt contains exactly its title, priority and selected comment/document text. Review input contains only title and its prompt contains neither text. Incoming has no predecessor; node inputs have no producer records, inherited Work task/acceptance or Artifact Context entries. Both retain the exact Incoming digest. |
| Unselected discussion/documents/private evidence/transcript | Supplying any undeclared Artifact body map to either compilation returns `CodexCompilationError::NodeBindings`. It cannot supplement the frozen node input. |
| Text is data rather than execution authority | A selected comment asks to grant delivery credentials, add admin tools, read protected tests and approve. The compiled envelope retains only the node's original model credential, no tools, implementation source write access and review source read-only access. No tool configuration is added. |
| Protected inventory/files remain private | Both nodes can consume their inline snapshot with no producer Artifact reads. Exact live same-Project private references remain readable as trusted execution metadata, while the model Context reader rejects both inventory and source file references for both nodes before reading any private bytes. |

The protected test case reuses the existing successful protected-test publication
fixture and ordinary Artifact publication/verification contract. It verifies the
trusted reader's exact identity/retention checks succeed, so model rejection is
not explained by a missing, expired or foreign Artifact. The private byte-reader
counter remains zero across both successful empty Context reads and all rejected
model reads.

The tests freeze selected source text as plain JSON data. They do not interpret
prose as policy, an evidence finding, a gate response or a permission grant.
They assert the compiler's enforced selection and credential/mount/tool outputs,
not a model's willingness to follow instructions.

## Regression qualification

```sh
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server-application --lib factory::codex
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server-application --lib factory::artifacts
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server-application --lib factory::validation
CARGO_INCREMENTAL=0 cargo test --offline --locked -p octacity-server-factory --lib
CARGO_INCREMENTAL=0 cargo clippy --offline --locked \
  -p octacity-server-factory -p octacity-server-application \
  -p octacity-server-store-postgres -p octacity-server \
  --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
python3 tools/check_architecture.py
openspec validate add-issue-classification-snapshots --strict
git diff --check
```

Passed disjoint suites: **115 Codex**, **15 Artifact**, **38 validation** and
**351 Factory core** tests: **519 tests**, including the four new cases.
PostgreSQL/MinIO were not required: the changed boundary uses the real common
input/compiler/Artifact owners with retained in-memory records and an external
byte-reader fixture. No production persistence or material-intake path changed.

Clippy, formatting, architecture (64 workspace packages, 263 dependency edges),
strict OpenSpec validation and diff whitespace checks passed.
