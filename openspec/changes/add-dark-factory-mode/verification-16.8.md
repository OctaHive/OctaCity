# Task 16.8: offline preparation and deferred live qualification

Status: complete for the operator-approved offline scope, revised on
2026-10-16. The operator explicitly chose to close this change without live
provider evaluation. Task 16.8 covers synthetic corpora, offline comparison,
independent adapters, conformance/negative tests and the deferred experiment
plan. No real-model qualification or bounded-control promotion is claimed.

## OpenAI shadow adapter preparation

At the operator's request, added an independent OpenAI Decisions adapter and
deferred all live calls because API billing is not funded. No API key was read,
no paid request was sent and no existing deployment was reconfigured. This is
offline preparation. The live qualification work listed below is explicitly
outside this change's closure criteria; its runtime safety gates remain intact.

The common contract stays in `octacity-server-factory`. JEV is now implemented
by `octacity-server-decision-signal-jev`, OpenAI by
`octacity-server-decision-signal-openai`. Neither implementation depends on the
other or on the registry. `octacity-server-decision-signal` retains only exact
neutral discovery, with no HTTP/TLS or provider parser dependencies. Only the
server composition root selects a concrete implementation. The release
security runner includes both adapter packages.

The OpenAI implementation supports canonical JSON and finite choices for
routing/tool-risk shadow evaluation. It checks exact request bindings, question
identity/domain, complete normalized probabilities, model echo, token totals,
measured latency/size and configured input-token charges. Credentials and raw
provider diagnostics do not enter results. TLS verification and explicit-root
bounds stay enabled; redirects, automatic POST retries and guessed read-only
recovery are disallowed. Unknown outcomes retain unknown usage.

The documented beta model alias `gpt-6-luna` is explicitly bound as
`unversioned-shadow-v1`, an experiment sentinel, not an immutable model snapshot.
The adapter denies advisory/bounded-control before dispatch. No real model
approval or artificial snapshot identity is inferred from an HTTPS fixture.
The [operator guide](../../../docs/operations/openai-decisions-shadow.md)
records private-file installation, the reserved key path, the experiment plan
and the remaining live collection/promotion work. The offline comparison CLI
remains unchanged; this increment does not add a live corpus collector.

TDD used the installed-provider/public evaluate boundaries and actual local
HTTPS requests. Observed Reds before implementation were: unsupported OpenAI
connector; explicit private CA/typed choice unavailable; non-shadow dispatch
incorrectly succeeding; duplicate/incomplete/unnormalized distributions being
accepted; forged configuration being installed; inconsistent token totals being
accepted. Each corresponding Green passed after its minimal correction.
The crate-boundary test first failed because the independent packages were
absent, then passed after extraction. The security runner first omitted the
two packages, then included them after its own Red/Green cycle. Existing
behavioral tests passed before extraction (19 tests); the extracted JEV
transport retains its original behavior and TLS/recovery contracts.

Task-local logs are under `/tmp/octacity-openai-decisions-evidence/`, with
`{install,choice,shadow,probabilities,configuration,usage}-red.log` and matching
Green logs, `crate-boundary-{red,green}.log`, and
`security-runner-{red,green}.log`. Additional failure/TLS/redaction tests
characterize existing guarded behavior without external calls. Documentation
and formatting have no executable TDD behavior.

Before extraction, each adapter Red and Green used
`cargo test --locked -j 2 -p octacity-server-decision-signal FILTER -- --nocapture`,
with `FILTER` set in turn to `installed_openai_connector`, `openai_https_choice`,
`openai_unversioned_model`, `openai_refuses_incomplete`,
`openai_installation_rejects`, and `openai_refuses_inconsistent_usage`. Each Red
had one failed behavioral assertion; its Green had one passed test. Tests now
live in the independent OpenAI package. Architecture Red/Green used
`python3 -m unittest tools.tests.test_check_architecture.ArchitecturePolicyTests.test_decision_signal_adapters_are_independent_workspace_packages -v`;
security-runner Red/Green used `python3 -m unittest tools.tests.test_security_matrix -v`.

Final local regressions:

```shell
cargo test --locked -j 2 -p octacity-server-decision-signal \
  -p octacity-server-decision-signal-jev -p octacity-server-decision-signal-openai
# Registry 1, JEV 12, OpenAI 9: 22 passed, no failures or ignored tests.
cargo test --locked -j 2 -p octacity-server --lib
# 87 passed, no failures/ignored; includes private OpenAI catalog composition.
cargo clippy --locked -j 2 -p octacity-server --all-targets --all-features -- -D warnings
cargo clippy --locked -j 2 -p octacity-server-decision-signal \
  -p octacity-server-decision-signal-jev -p octacity-server-decision-signal-openai \
  --all-targets --all-features -- -D warnings
RUSTDOCFLAGS='-D missing_docs' cargo doc --locked -j 2 --no-deps \
  -p octacity-server-decision-signal -p octacity-server-decision-signal-jev \
  -p octacity-server-decision-signal-openai
python3 tools/check_architecture.py
# 61 packages, 245 workspace dependency edges; passed.
python3 -m unittest discover -s tools/tests -v
# 431 run, no failures, 8 environment-dependent skips (tools-final.log).
cargo fmt --all -- --check
openspec validate add-dark-factory-mode --strict
openspec validate --specs
git diff --check
```

Rust commands used task-local `CARGO_PROFILE_DEV_DEBUG=0`,
`CARGO_PROFILE_TEST_DEBUG=0`, and `CARGO_INCREMENTAL=0`. JEV's transport source
was moved byte-for-byte unchanged (SHA-256
`6e8f4bc11c60edfbd6a2653274618fe779fe836e0ef1bff367d24a7a414ffbe8`).
No external dependency version changed. These are local results for the new
adapter increment, not a reassignment of task 16.6/16.7's earlier exact-source
GitHub qualification or evidence of a new live backend/model experiment.

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

## Deferred live qualification: separate follow-up

The session has no configured exact JEV deployment/model and credential for
these experiments. The earlier released Factory pilot uses a hermetic provider
fixture, which cannot qualify a real model. The following work is deferred by
the operator's explicit scope decision and must pass before real-model
qualification or bounded control, independently of this change's completion:

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
