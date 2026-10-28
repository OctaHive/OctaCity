# Task 1.5: retained native MCP evidence

## Implementation and dependency status

Published [Octa 0.8.0](https://github.com/OctaHive/octa/releases/tag/v0.8.0)
is now pinned in OctaCity. The tag resolves to
`590ecfe4a8de4c2e7d54a9a60b7075781ab0cac4`; the implementation commit is
`fbf9a094d16302f3af014e50e6d1af8efeb1662d`. The operator approved the upstream
work after the real cancellation test demonstrated trace deletion in 0.7.3,
then published the qualified release.

[Tests](https://github.com/OctaHive/octa/actions/runs/40675351955),
[Coverage](https://github.com/OctaHive/octa/actions/runs/40675352220) and
[Build](https://github.com/OctaHive/octa/actions/runs/40676966553) passed.
All five platforms have archives/checksum sidecars covered by the verified
GitHub build attestation for the exact source/tag/workflow. Line coverage is
96.41% on Linux, 96.42% on macOS and 95.76% on Windows; thresholds were unchanged.
The initial 95.66% Windows result led to direct public ExecutionEngine tests for
failure/timeout, `ignore_error`, retained Artifacts and absent semantic outputs.

| Immutable input | Verified value |
| --- | --- |
| Linux ARM64 archive SHA-256 | `7fd1fcc88c9d3cac16c9d2e9ac9abd67a104507fdb110dc9840ea9d3784cfa0d` |
| Source archive SHA-256 | `51c1d18ca85eb1cbc480e8bad12d420dc8d95225076e92c6c3d04b385835dae5` |
| macOS ARM64 qualification archive SHA-256 | `bd917f92a3bd34f1da1a398c74ed0fd0ffbedd58ed3a751cbdece845720bdd2a` |

Cargo dependencies/lock, source revision, stand release/source input hashes,
staging allowlist and runner defaults use 0.8.0. The existing release and native
stand installers now explicitly require `codex.native-mcp-evidence.v1` and reject
otherwise valid bundles without it. Ordinary legacy records remain readable,
but cannot satisfy the capture-only verification contract. No unpublished or
mutable dependency was installed.

The version bump passed Red/Green through documented release versus Cargo
metadata: 20 workspace packages initially reported 0.7.3, then package/path
versions and lock entries matched 0.8.0. The plugin's 143 tests passed against
0.8.0. All four actual native tests passed again against the downloaded, verified
release plugin, not only a local candidate build. Qualification covers Codex CLI
0.161.0 and this plugin; it certifies no other AI plugin.

## Existing owners and record contract

Octa's existing command-scoped, sanitized, atomic run-record writer owns capture.
Trace/provenance format 2 adds observation timestamps, capture lifecycle, native
terminal presence, exact record count, coverage and a BLAKE3 trace digest. Result
format 1, plugin protocol 2 and generic runner event schema 4 are unchanged.
Observation timestamps use monotonic elapsed time anchored to invocation start,
bounded by the provenance window, rather than claiming remote server time.

Actual native `mcp_tool_call` items retain their `id`, `server`, `tool`,
`arguments`, `status`, and consumed `result`/`error`, after the existing secret
sanitizer. The writer streams bounded events to the ordinary trace Artifact;
it adds no MCP client, public MCP operation category, provider search schema,
credential store or parallel tool journal. Existing frame/trace limits remain.

Cancellation, task timeout and native execution/result-normalization errors
retain trace and provenance through ordinary `ArtifactRegistered` events, with
no semantic result report, deliverables, exports or cache entry. Validated
resources also remain in the terminal task result used by the Agent output
freezer; events alone would not publish their bytes. Unknown usage
stays empty. A failed capture may have an empty trace when no valid event arrived.
The existing bounded cancellation handshake stops the owned process tree and
drains its already queued resource declarations. Resource validation remains
mandatory before registration, including on failed executions.

A reliable response-queue overflow remains an explicit ordinary output failure:
the overflowed channel cannot certify Artifact declarations. Sanitized partial
files may remain locally, but are not promoted to a successful result or claimed
as retained server evidence. Filesystem/publication failure likewise cannot be
reported as successful capture.

OctaCity reuses the existing immutable Artifact/document checks and Codex
provenance verifier. The capture-only boundary is
`verify_codex_native_capture(FactoryCodexNativeCaptureContract, documents)`.
It verifies original Build/Attempt/Job ownership, byte ceilings, exact bytes and
digests, pinned invocation identity, protected prompt/source identity, the
authoritative execution window, trace sequencing, timestamp bounds, terminal
placement, capture count and coverage. Its sealed result contains only exact
Artifact identities, lifecycle and record count. It cannot manufacture a model
result, accept an Issue effect or declare a successful search. Legacy or absent
native capture returns `CapabilityUnavailable`; malformed/misbound evidence is
rejected. Ordinary successful result/schema/usage validation remains separate.

## Common handoff for triage 4.4

1. Start from the original frozen Build/Attempt/Job and Role execution package,
   including exact plugin/executable/model, MCP aliases/configuration references,
   selected tools and credential-purpose/resource bindings. Trace arguments or
   model text cannot establish or replace that authority.
2. Resolve the exact trace/provenance Artifacts through `FactoryArtifactReader`
   under the original Project, retention and byte limits. Verify their identities
   through the capture-only contract above; never select the latest Attempt as
   a substitute for the original one.
3. Read actual native item events, paired by their native item ID. Identify
   retained arguments/results by Artifact digest, record sequence and JSON field
   path (for example `/event/item/result`). Timestamps identify local observation
   inside the verified execution window. Sanitized content is the retained
   content; no claim is made that it equals an unavailable unsanitized response.
4. Distinguish completed empty content, a refused call with consumed error
   content, transport/protocol failure without a result, and an interrupted
   started call without completion. The last case has an unknown remote outcome,
   even after the local client is stopped. It is never an empty-search answer.
5. Treat `coverage: emitted_native_events` only as the capture's scope. Remote
   search completeness, freshness and immutable provider revisions remain
   unknown unless an actually qualified server supplies verifiable facts. No
   revision or search scope is inferred from a model's prose.
6. Preserve failed/cancelled capture for inspection without feeding it into
   successful semantic Flow output. Triage owns its later evidence assessment
   and MCP tool selection; this change introduces no GitHub/Jira/GitLab response
   parser or server-side duplicate-search API.

The full packaged guest/stand journey, restart and immutable installation
qualification remain task 3.1. These local native and runner checks do not claim
that journey has already run.

## TDD evidence

Cargo commands reuse the existing target directory with `--locked`, two build
jobs, debug information disabled and incremental compilation disabled. Every
behavioral slice below had an observed Red before its production change.
Setup/compilation errors unrelated to the required API were corrected first and
are not counted as behavioral Red.

| Boundary and command/filter | Observed Red | Observed Green |
| --- | --- | --- |
| `python3 -m unittest tools.tests.test_factory_mcp_credentials.NativeMcpCredentialTests.test_native_mcp_cancellation_retains_observed_call_evidence`, explicit real Codex/plugin binaries | Published 0.7.3 receives a real MCP call, then cancels without a trace Artifact | Candidate retains the started call and cancellation provenance; no result report or credential values |
| `cargo test -p octa_plugin_codex --test plugin_manager` invalid native answers | SDK returns an error with no trace/provenance declarations | Failed normalization retains both Artifacts without a result report |
| Same SDK suite, native record observation times | Timestamp absent from retained native events | Every retained timestamp lies inside the provenance window |
| Same SDK suite, `retained_capture_identifies_complete_cancelled_and_failed_native_streams` | Capture lifecycle/content metadata absent | Complete/cancelled/failed stream metadata, counts and exact trace digest verified |
| `cargo test -p octa-cli --test e2e_test codex_retains` | Actual Codex failure/timeout is present but public Artifact list is empty | Both CLI executions fail as before and publish exactly trace/provenance, with no result report |
| `python3 conformance/codex_runner.py` terminal resources | Artifact events exist but the cancelled terminal task has no Artifacts for the Agent to freeze | Failed/cancelled terminal results preserve the same validated Artifacts without semantic outputs or reports |
| `cargo test -p octa_plugin_codex --bin octa_plugin_codex` capability contract; `python3 -m unittest conformance.test_release_metadata` | Native capture capability absent from SDK/release metadata | SDK and packaged compatibility/manifest metadata advertise the same capability |
| `cargo test -p octacity-server-application --lib native_capture_v2_preserves_verified_mcp_trace_provenance` | Existing verifier rejects format 2 as invalid schema | Validated ordinary semantic observation retains trace/provenance references |
| Same application suite, `native_capture_preserves_multiline_mcp_results` | Legitimate multiline/tabbed native result rejected | Ordinary escaped whitespace accepted; bounded/sensitive-content checks retained |
| Same application suite, `native_capture_handoff_keeps_partial_evidence_without_creating_a_model_result` | Required capture-only public API absent | Complete/cancelled/zero-event failed capture verified without a semantic result |
| Same application suite, `native_capture_handoff_rejects_unsupported_or_misbound_evidence` | A recomputed trace with an event after the terminal is accepted | Post-terminal content rejected alongside wrong Job/plugin/window, digest/count and invented coverage; legacy is an explicit capability blocker |
| `python3 -m unittest tools.tests.test_validate_octa_release_contract` | Required capability API absent; malformed plugin metadata escapes normal validation | Explicit native-capture qualification accepts the exact capability and rejects missing/unknown/malformed metadata |

## Native qualification and regressions

Actual native tests use Codex CLI 0.161.0, the candidate plugin, a finite local
Responses fixture, HTTPS and stdio MCP peers, explicit independent canary tokens
and denied ambient egress. The model-provider shim forwards MCP settings and
environment unchanged; it introduces no production client or paid model call.
All four tests passed, including successful and refused calls, matched native
start/completion IDs, observation timestamps, retained consumed content, empty
content, HTTP 503, malformed MCP content, cancellation and redaction. The
cancel test waits for both the actual peer request and the public native start
progress before cancelling, rather than racing event decoding.

Upstream regressions passed: 251 executor library tests; 116 plugin library,
4 main-binary and 23 plugin-manager tests (one explicit live-native test remains
ignored); all 71 CLI tests (one existing test ignored), including both failure/timeout
retention tests; Codex CLI, runner and secret conformance;
two release-metadata tests; Clippy for executor/plugin on all targets.
Runner conformance additionally verifies bounded trace overflow, cancellation,
timeout, process-tree termination, response backpressure and invalid deliverable
publication. Existing expectations were updated to the approved partial-capture
behavior without weakening terminal failure or resource validation.

Before the release pin, all 795 application library tests and another 63
output/lifecycle tests passed; application library Clippy was clean. After the
immutable 0.8.0 update, checks passed again: 64 Codex application tests, 52 Runner
tests, 126 Python release/staging/stand tests (three existing skips), all four
actual native tests, and `cargo check --locked -p octacity-server -p octacity-agent`.
Architecture remains 63 workspace packages and 261 dependency edges. Formatting,
strict OpenSpec validation and diff whitespace checks passed.

Pin TDD: `python3 -m unittest tools.tests.test_verify_local_stand_inputs` first
failed because the checked-in pin remained 0.7.3, then passed for exact published
0.8.0 bytes/source/capabilities. Both new staging tests first showed a correctly
checksummed bundle without native capture was accepted, then passed after the
existing callers required that capability. The 51 focused release/staging/input
Python tests passed before the broader 126-test regression.

Actual release qualification used `tools/stage_octa_release.py` for Linux ARM64
and macOS ARM64, then the native stand verifier for the downloaded Linux bundle.
`gh attestation verify --format json` verified both consumed archives; source,
tag, workflow and all ten signed subjects (five archives plus sidecars) were
checked. No full stand or VM execution was claimed; that remains task 3.1.

Free space stayed above 10 GB. Regenerable obsolete test binaries were removed
from `target` (418 MB followed by 1.83 GB) to keep a build reserve; no retained
stand data was removed.
