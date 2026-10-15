# Routing and tool-risk calibration seeds

`corpus-v1.json` contains eight labeled routing cases and eight labeled tool-risk
cases. They are synthetic engineering seeds for exercising a calibration
pipeline, not a representative production sample or approval for a real model.
Labels are reviewable input data; the comparison tool does not derive them from
the model or reproduce a routing/risk classifier in assertions.

Routing labels mean `research` when the described defect/feature has adequate
contract and context for the configured investigation route, and `needs_context`
when scope, intent, risk policy or a declared human requirement needs attention.
The conservative baseline is `needs_context`. A real question set must bind
these meanings and finite choices exactly; an arbitrary Flow with other routes
needs its own reviewed corpus and question-set identity.

Tool-risk labels narrow an action that already passed deterministic permission
checks. Read-only low-risk reads and isolated scratch writes are labeled
`allow`; uncertainty requiring a human is `escalate`; unjustified destructive
repository effects are `deny`. The conservative baseline is `escalate`. Every
seed has `within_permissions: true`. Out-of-envelope actions belong to the
existing hard-deny conformance tests and must never be sent to JEV to obtain
permission. A label never grants a tool, path, host, credential or process right.

The public state is a closed vocabulary of bounded enum/boolean features. It
contains no task bodies, repository paths, commands, protected tests, prompts,
credentials, provider payloads or private URLs. Expected labels and baselines
are evaluation data, not extra model input. Retain reviewed training/holdout
provenance, class coverage and representative failure/adversarial cases before
using a larger corpus to evaluate any real provider/model policy.

## Validate and compare

```shell
python3 tools/factory_signal_calibration.py validate \
  --corpus tools/fixtures/factory-signal-calibration/corpus-v1.json

python3 tools/factory_signal_calibration.py evaluate \
  --corpus /private/calibration/corpus.json \
  --observations /private/calibration/shadow-observations.json \
  --report /private/calibration/shadow-report.json

python3 tools/factory_signal_calibration.py evaluate \
  --corpus /private/calibration/corpus.json \
  --observations /private/calibration/advisory-observations.json \
  --previous /private/calibration/shadow-report.json \
  --report /private/calibration/advisory-report.json
```

The command performs no network or model call, resolves no credential, and
mutates no Factory state. Exit zero means a structurally valid comparison was
written; it does not mean acceptable quality. Invalid input exits two with a
fixed diagnostic and no newly written report. Consumers must check that exit
status and retain the original input files. A report cannot overwrite its
corpus, observations or previous report, including through an alias/hard link.

## Observation document v1

Record one observation for every case, including failed and unknown outcomes.
Keep the original durable Signal receipts and measured operation latency.
Export only this bounded normalized comparison projection:

- `schema_version: 1`, `mode: shadow | advisory`, and
  `source_kind: fixture | live`. Source kind is declared provenance, not an
  authentication proof.
- `corpus_sha256`: SHA-256 of the exact corpus file bytes.
- `bindings`: separate `routing` and `tool_risk` objects. Each contains exact
  `provider`, `adapter`, `model`, `probability_semantics`, `question_set` and
  `policy` references, each shaped as `{id, version, sha256}`. Mutable aliases
  such as `latest` are rejected. Distinct purposes may select distinct models,
  questions and policies; neither inherits the other's calibration.
- `observations`: exactly one row per case, containing `case_id`, `status`,
  `selected`, `confidence_ppm`, `latency_ms`, `final_disposition` and a unique
  `receipt_sha256`.

Completed rows carry one declared selected answer and an integer confidence in
`0..=1000000`. Failed rows carry null selected/confidence and one closed status:
`timeout`, `cancelled`, `invalid_result`, `unavailable`, `unknown_outcome`,
`unsupported` or `budget_exhausted`. Latency is finite measured milliseconds,
not the conservative budget charge for an unknown external outcome. Missing
measurements need a properly captured new experiment; do not fabricate values
or redispatch an old logical decision to fill a report.

`final_disposition` must equal the independently recorded deterministic baseline
for every shadow/advisory case. The exporter/operator must verify receipt/input
and exact identity provenance against retained authoritative records. This
offline tool validates the projection's shape and consistency; it does not
authenticate receipt digests or assert that a claimed live observation was
produced by JEV. Raw provider responses are not an accepted input format.

Documents are limited to 1 MiB and 512 cases, with closed fields, unique case
and receipt identities, finite numeric bounds and no private state fields.
Advisory requires a previous shadow/advisory comparison of the same corpus,
label version, exact purpose bindings and source kind. Reusing a fixture
comparison for live observations or another model fails closed. Later drift
comparisons retain the exact previous report digest.

## Metrics and limits

Reports are separated by purpose and retain both corpus and observation file
digests. Accuracy and disagreement are in millionths; baseline accuracy covers
every case, while model accuracy/disagreement cover completed observations.
Failure rate covers every declared terminal non-completed observation. p95
latency uses nearest rank across all cases, including failures.

Confidence metrics evaluate top-label correctness: `brier_score_ppm` is the
mean squared difference between reported chosen-label confidence and the
binary correctness outcome, scaled to millionths. `calibration_gap_ppm` is the
absolute gap between mean confidence and observed accuracy.
`expected_calibration_error_ppm` is weighted absolute calibration error over
ten equal-width confidence bins; confidence 1.0 belongs to the last bin. Full
multiclass probability calibration requires the complete probability
distribution and its provider-specific semantics. Missing completed answers
remain null rather than fabricated zero-accuracy/calibration observations.

Drift is current minus previous per metric; a missing metric observation yields
null drift. No universal confidence/quality threshold, promotion, demotion or
runtime approval is inferred from these numbers. Every report has
`comparison_only: true`.

## Remaining task 16.8 gates

The seed/tooling does not complete real-model qualification. A real exact JEV
deployment, separately reviewed purpose-specific acceptance/drift policies,
representative labeled holdouts, shadow then advisory runs, audited exact-model
promotion and immediate live demotion/fallback evidence remain required before
bounded control. A trusted runtime promotion/revocation integration is not
provided by this offline command. Future Decisions API adapters must pass the
same capability, finite-question, privacy, replay, calibration and
negative-security gates; structurally similar probabilities transfer no
approval. See [Factory operations](../../../docs/operations/factory.md).
