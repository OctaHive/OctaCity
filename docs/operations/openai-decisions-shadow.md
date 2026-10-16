# OpenAI Decisions: deferred shadow experiment

The OpenAI adapter is implemented and tested against a local HTTPS fixture.
The operator deferred live evaluation because API billing is not funded.
ChatGPT/Codex subscription usage is separate from API-key billing. An API key
is not currently needed, and no existing stand or Project is reconfigured.

## Independent implementations

- Contract: `octacity-server-factory::DecisionSignalProvider`.
- Exact neutral registry: `octacity-server-decision-signal`.
- JEV implementation: `octacity-server-decision-signal-jev`.
- OpenAI implementation: `octacity-server-decision-signal-openai`.
- Connector selection and private credential loading: server composition root.

Neither implementation depends on the other or on the registry. The registry
has no HTTP/TLS dependencies or provider credential/wire parsers. The release
security matrix runs the contract, registry and both concrete implementations.

## Supported experiment boundary

The official [Decisions guide](https://developers.openai.com/api/docs/guides/decisions)
and [API reference](https://developers.openai.com/api/reference/resources/decisions/methods/create)
were checked on 2026-10-16. The API is a public beta at
`https://api.openai.com/v1/decisions`, using `gpt-6-luna`.

This adapter accepts canonical JSON state and finite string-choice questions
for routing and tool risk. Predicates, images and score questions are not
advertised. Inputs contain only the already selected state, question
instructions and declared choice descriptions; labels and baseline answers
are evaluation data and must not be included in model evidence.

The beta response echoes the model name but does not establish an immutable
snapshot identity. The local model reference therefore uses the explicit
sentinel version `unversioned-shadow-v1`. Its digest binds the operator's
experiment manifest, not undisclosed model weights. This sentinel is not
evidence of an exact immutable remote model. The adapter refuses both advisory
and bounded-control requests before network activity; no alias is silently
treated as production-qualified.

Answers must match question names, order and declared choices. Probability
distributions must contain every choice exactly once, be finite/in range and
sum to one within one millionth. The common selected/runner-up probabilities
come from that distribution; OpenAI's separate `confidence` is validated but
is not substituted for selected-label probability. Usage includes measured
elapsed time and response size, internally consistent token totals and the
upward-rounded configured input-token charge. Rates are operator-owned values,
not an authorization to spend or a guarantee of a future price.

HTTPS hostname verification stays enabled. An operator may explicitly add a
bounded private CA list for a trusted endpoint; this grants no redirect
authority. Redirects and automatic POST retries are disabled. A lost response,
rate limit, timeout, malformed answer or refusal cannot become a success or
invented zero usage. Read-only recovery returns no observation; there is no
guessed external request ID, status lookup or repeat evaluation. Durable
Factory request/reservation and replay semantics remain with the existing
application, not an adapter-local cache.

## Private files for a later funded run

Reserve these operator-owned paths outside the repository:

```text
/Users/roman/.config/octacity/secrets/openai-decisions.key
/Users/roman/.config/octacity/secrets/openai-decisions.json
/Users/roman/.config/octacity/secrets/signal-connectors.json
```

The directory must have mode `0700`; each file must be a regular owner-only
file with mode `0600`, not a symlink. The key file contains one API key without
`Bearer`, quotes or an inline configuration field. Never put a key in Flow
metadata, an Agent workload, source control, command arguments or a chat.
See [OpenAI's quickstart](https://developers.openai.com/api/docs/quickstart)
for key creation. Do not reuse a ChatGPT/Codex login token as an API key.

The connector catalog selects the implementation independently of Flow state:

```json
{
  "version": 1,
  "connectors": [{
    "kind": "openai-decisions",
    "credential_profile": "openai-shadow",
    "configuration_file": "/Users/roman/.config/octacity/secrets/openai-decisions.json",
    "credential_file": "/Users/roman/.config/octacity/secrets/openai-decisions.key"
  }]
}
```

The adapter configuration has `version: 1`, the HTTPS `endpoint`, a complete
`capability`, `timeout_milliseconds` between 1 and 300,000, and a positive
`input_cost_micro_units_per_million_tokens`. The latter is in the operator's
chosen micro-currency unit per million input tokens. For example, 100,000
micro-USD represents USD 0.10 per million; review the applicable rate before
any live call. Optional `root_certificates_pem` follows the bounded explicit
trust rules. Unknown fields and inline credentials fail.

The capability must use provider `openai`, adapter `openai-decisions-http`,
model `gpt-6-luna` / `unversioned-shadow-v1`, and probability semantics
`openai-choice-probabilities` / `v1`. Supported media/question kinds are
`CanonicalJson` / `FiniteChoice`. All references, purposes, limits, budget
ceiling and data-handling policy must be authored and bound to the reviewed
experiment. Reuse neither JEV semantics/calibration nor fictitious snapshot
identities. Installation performs no billable probe.

## Prepared plan and remaining live work

1. Review separate routing and tool-risk labels and acceptance policies. The
   [seed corpus](../../tools/fixtures/factory-signal-calibration/README.md)
   contains eight examples for each purpose; it is suitable for a smoke
   experiment, not representative calibration or a production holdout.
2. Freeze corpus, question, policy, data-handling and capability manifests.
   Keep routing/tool-risk bindings distinct and configure only `Shadow`.
3. After funding and explicit live-run authorization, install the private
   connector and purpose profiles in a disposable experiment deployment.
   Capture observations through the actual provider/runtime port; do not
   substitute fixture answers or label a local test as live evidence.
4. Retain canonical requests/results, measured usage/latency, failures, hashes
   and the independent baseline disposition. Unknown outcomes stay unknown;
   creating another billable evaluation does not recover a missing receipt.
5. Export the bounded normalized observations described in the corpus README
   and run `tools/factory_signal_calibration.py evaluate`. The existing CLI is
   offline; it does not collect live observations or authorize promotion.

This change prepares the adapter, installation contract and experiment plan;
it does not add a live corpus collector or complete a Factory calibration run.
The operator approved closing `add-dark-factory-mode` on offline evidence.
Exact-model qualification, representative holdouts, advisory, audited promotion
and live demotion/fallback are a separate follow-up, required before any live
model gains bounded-control authority. An OpenAI result does not qualify JEV.
Closing the implementation change neither qualifies a model nor relaxes these
runtime gates.

Local validation, requiring no API key:

```shell
cargo test --locked -p octacity-server-decision-signal \
  -p octacity-server-decision-signal-jev \
  -p octacity-server-decision-signal-openai
```
