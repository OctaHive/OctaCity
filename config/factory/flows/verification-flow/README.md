# Configurable verification

`../verification-flow.json` is a nested-flow template assembled through the same
`FlowDefinitionTemplate::from_manifest` and `instantiate` boundaries as the base
flow. Its seven check names, acceptance classes, profile choices, ordering,
thresholds and terminal outcomes are configuration. Renaming or inserting a check
requires a declared schema/profile, policy entry and transitions, without changing
the engine. The template imports the common candidate and Artifact schemas.

## Frozen plan and inputs

The caller supplies a closed `verification.input` payload containing `target`:
the complete candidate subject, immutable deployment/environment receipt, public
contract Artifact reference and exact HTTP(S) endpoint. A trusted predecessor
must qualify that receipt against the actual deployment. A mutable environment
name or URL alone does not establish candidate identity. Every supplied check
explicitly selects the same frozen caller payload; Context is separately selected.

The optional `execution.verification` policy selects arbitrary node keys, target
projections, source exposure, network host/credential/identity ceilings, required
checks, quorum and an immutable data-handling reference. `data-handling.json` is
the example policy document whose exact bytes match the declared digest. Editing
it requires publishing a new policy reference and definition.

`VerificationPlan` binds the admitted closure, actual Flow Run and workflow cycle,
exact candidate, immutable policy (including target projection recipes) and
enclosing budget. The definition binds every node's profile, schema and narrower
budget. `VerificationCheckInput` resolves only the current selected check's target
from its frozen input. Its input digest binds the exact deployment, endpoint,
contract, Context, provenance and execution generation. Checks may use different
input schemas and consume earlier own evidence while retaining the same plan.
Unselected helper Builds use the ordinary Build contract.

Admission and Build-intent restoration retain the original identities when current
configuration changes. Earlier cycles, another deployment or a retried input cannot
supply a matching report. `freeze`/`restore` cover initial-cycle policy metadata;
execution restoration uses `FlowBuildIntent`.

## Authority and source visibility

Example permissions default to deny-all. Publication must select qualified
command/reasoning profiles and explicit node and enclosing authority. A check's
declared grants must stay inside its verification policy; ordinary admission and
Agent signing still apply their existing ceilings. Host grants use canonical
host strings, including unbracketed IPv6; the selected URL may contain no user
information, query or fragment. Network access requires an exact selected endpoint.
Credential and workload identity profile names are explicit capabilities, never
secret values or an implicit deployment credential grant.

For `black_box`, Context may contain only the selected public contract as a task
or specification Artifact and explicitly selected own evidence Artifacts.
Repository ranges, implementation summaries and typed sibling results are denied.
The input schema and projection must also exclude implementation data. Context
collection still verifies exact Artifact bytes and subject through the common
broker. The operator owns the trusted contract selection; these controls do not
classify arbitrary prose automatically.

A source-free evaluation grants only read-only `/octacity/protected`, writable
`/octacity/outputs` and writable `/octacity/scratch`. The Agent retains its private
host checkout for normal lifecycle management but projects no host source into
the execution sandbox; the guest working directory is scratch. Source capture,
materialization and writable overlays require an explicit source grant and are
rejected without it. `read_only_source` may additionally grant read-only source.
Microsandbox and OCI plans apply this same technical visibility flag; execution
does not select behavior from a logical check name.

## Ordinary checks and qualified evidence

The six deterministic example profiles use parameter documents in
`../../verification-probes/`. Install the repository-owned
`tools/factory_verification_probe.py` as an exact pinned executable/tool outside
candidate source. A trusted profile adapter provides its bounded protected JSON
on stdin and retains stdout as the declared typed `VerificationObservation`
Artifact/Report. Merge profile assertions with the frozen `plan_digest`,
`input_digest`, `check`, `target`, exact `base_url = target.endpoint`, `allowed_hosts` and finite `limits`:
`max_requests`, `timeout_ms`, `max_response_bytes`, `max_elapsed_ms`. The trusted
adapter must select these fields from the Build causality and admitted profile,
never from provider prose. Normal Job resource and deadline ceilings remain in
force. The script is a reusable tool contract; production composition/pinning is
qualified in task 16.1, rather than implicitly installed by this template.

The profiles demonstrate a contract sequence, latency bound, public HTML/accessibility
smoke assertion, security headers and denied access, exact staged deployment and
bounded repeated post-deployment observation. HTTP methods are read-only; redirects
and ambient proxies are disabled. Response bytes, requests, intervals and observation
time are bounded. Chunked reads check the observation window between reads; socket
timeouts and the ordinary Job deadline bound blocking operations. JSON assertions
compare JSON types recursively, so booleans cannot satisfy numeric expectations.
JSON Pointer rejects malformed escapes and noncanonical array indices. Malformed
HTTP responses produce an indeterminate observation without a traceback or raw
protocol diagnostics; an earlier failed assertion remains failed. Reports retain
counts and outcomes, without response bodies, credentials or endpoint diagnostics.
Deployment assertions select candidate revision, ChangeSet digest and the complete
deployment receipt from the frozen target; a changed receipt digest fails even
when its logical identity is unchanged.

The UI example checks a public HTTP/HTML contract; browser interaction and deeper
security/performance suites can be separate installed profiles. These observations
do not create a deployment or grant promotion authority. Rollout and trusted
promotion belong to tasks 12.4–12.6. Long-running monitoring remains external.

The independent reasoning profile uses the same typed observation contract with
its own pinned model/task/tool identity. Build completion rejects observations for
another plan, node or exact frozen input before journaling. `verify_factory_verification_check`
qualifies actual retained bytes, schema, Artifact publication, Build/Attempt/Job
provenance, profile, producer and freshness. Its Evidence Manifest records the
exact public contract and selected evidence inputs. The same canonical inventory
validator runs before dispatch and at evidence retention: at most 256 Artifacts,
including the contract, sorted by Artifact identity, with duplicate identities
rejected even when their digests differ. Existing payload/schema limits can be
stricter. Provider JSON alone cannot construct `VerifiedFactoryVerificationCheck`.

`evaluate_factory_verification` applies the immutable quorum and required-check
policy to these proofs. A fresh required failure dominates all passing checks,
including reasoning; missing, stale or indeterminate required evidence cannot
pass. Duplicate checks and foreign plans/candidates are rejected. Proofs are
reconstructed from existing retained records after restart, without a second
persistence model.

The supplied gates stop at a required failure or unavailable observation. Their
final `observed` terminal is a bounded collection result, not acceptance authority:
the caller must qualify evidence and apply the plan policy. The application test
composes all seven ordinary Build completions and checks both complete success
and a deterministic failure followed by a passing reasoning observation. It also
checks a distinct input schema consuming earlier own evidence without replacing
the common plan. Full journey/caller wiring remains task 12.7; live provider and released-backend
qualification remain tasks 16.1/16.4.
