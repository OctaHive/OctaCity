# Task 16.8: calibration preparation and remaining gates

Status: partial. Task 16.8 remains unchecked; no real-model qualification or
bounded-control promotion is claimed.

## Implemented preparation

Added `tools/fixtures/factory-signal-calibration/corpus-v1.json`: eight routing
and eight tool-risk synthetic engineering seeds with independently specified
labels and conservative deterministic baselines. Inputs contain only bounded
public enums/booleans and require an already verified permission envelope.
These seeds exercise the evaluation pipeline; they are not representative
production holdouts. The adjacent README defines labels, provenance, metrics,
input/output schemas and remaining operational gates.

Added a standard-library-only offline CLI, `tools/factory_signal_calibration.py`.
It validates the corpus and compares normalized observations separately for
routing and tool risk, retaining exact purpose-specific provider, adapter,
model, probability-semantics, question-set and policy references. Reports
include baseline/model accuracy, disagreement, mean confidence, top-label
Brier score, calibration gap, ten-bin expected calibration error, failure rate,
nearest-rank p95 latency and drift from the exact previous report. Reports
retain corpus and observation digests; absent completed answers produce null
model metrics rather than invented observations.

Shadow/advisory observations must preserve the independently supplied baseline
disposition. Advisory requires a previous comparison of the same corpus,
purpose bindings and source kind. Closed, bounded input rejects private fields,
mutable version aliases, out-of-envelope cases, duplicate identities, invalid
numbers and ambiguous evidence. Reports cannot overwrite their own inputs.
The CLI makes no network calls or Factory state changes and always emits
`comparison_only: true`. Declared live provenance and receipt hashes are not
cryptographic attestations; their authoritative verification remains external.

## TDD evidence

The public boundary is the CLI subprocess and its JSON report. Each behavior
was introduced by a failing test, observed before implementation, then made
green. The initial labeled example expected independently calculated literal
metrics; it did not reproduce the classifier or metric implementation.

```shell
python3 -m unittest tools.tests.test_factory_signal_calibration -v
# Final focused result: 12 passed.
python3 -m unittest discover -s tools/tests -v
# Final regression result is recorded in verification-16.7.md.
python3 tools/factory_signal_calibration.py validate \
  --corpus tools/fixtures/factory-signal-calibration/corpus-v1.json
# routing: 8; tool_risk: 8; seed-v1, exact file digest.
```

Task-local receipts are retained under `/tmp/octacity-166-evidence/`, with each
command, exit code and duration in JSON and its output in the matching log.
`calibration-comparison-red` failed for the missing comparison CLI;
`calibration-comparison-green` passed. Subsequent Red/Green pairs cover
`authority`, `schema`, `bindings`, `drift`, `sequence`/`subject`, `depth`,
`validate`, `corpus`, `ece`, `provenance`, `evidence`, `permissions` and
`null-metrics`. Red commands exited 1 for their new assertions; corresponding
Green commands exited 0. The final module includes positive comparison/drift
and negative privacy, authority, binding, numeric and evidence-preservation
cases. Environment failures are not counted as Red.

## Required before completion

The session has no configured exact JEV deployment/model and credential for
these experiments. The earlier released Factory pilot uses a hermetic provider
fixture, which cannot qualify a real model. Required work remains:

- Review representative labeled holdouts and purpose-specific acceptance and
  drift policies, with their immutable provenance and exact question semantics.
- Capture real JEV shadow runs, then advisory runs, retaining authoritative
  request/receipt identity and actual latency/failure observations.
- Integrate and test trusted, audited promotion for the exact provider/model,
  question, semantics and policy bindings before bounded control is enabled.
- Demonstrate immediate live demotion and deterministic fallback after
  revocation, drift or failure; replay and permission gates remain mandatory.
- Apply the same conformance, calibration, privacy, replay and negative-security
  qualification to every future Decisions API adapter. Neither probability
  shape nor another provider's approval transfers this authority.

Offline comparison success does not satisfy any runtime promotion gate.
