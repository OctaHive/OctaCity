# Task 4.4 — retained native investigation evidence

## Ownership and consumed support

This task consumes the published Octa 0.8.0 / Codex 0.161.0 version-2 native
trace/provenance contract qualified by upstream
[capture and cancellation](../archive/2026-10-28-enable-local-stand-runtime-mcp/mcp-evidence-verification.md)
and [packaged execution](../archive/2026-10-28-enable-local-stand-runtime-mcp/packaged-mcp-verification.md).
Task [4.3 execution qualification](mcp-execution-verification.md) remains the
composition evidence for isolated HTTP/stdio credentials and actual OctaCity
Issue search/detail calls. No additional MCP client, provider search adapter,
credential registry, persistence owner or result journal was added.

The public native-capture verifier pairs actual start/completion records by
native item identity and preserves their connection, tool, sanitized arguments,
consumed result/error, lifecycle, sequence and local time. An exact evidence
reference contains the retained trace Artifact identity/content digest/size,
record sequence, native call ID and sanitized result digest when available.
Missing completions remain interrupted observations; malformed MCP content is
distinct from a successful empty content array. Duplicate/mismatched records and
more than 64 calls fail without silently truncating evidence.

Connection authority is bound to the exact frozen Role package, compiled prompt,
toolchain and selected immutable transport/authentication configuration. The
resource reference comes from that configuration, not model text. Native result
fields preserve candidate identity, actual remote revision, scope, continuation
and coverage when supplied. Otherwise the retained response digest and local
observation time identify the observation without inventing a provider version.

## Ordinary result path and common conclusion

`FactoryCodexBuildOutputSource` reuses its existing signed-Job, retained input
manifest, compiler reconstruction, measured usage and `FactoryArtifactReader`
checks. Only an explicit ordinary `result_tool` pin selecting
`factory_mcp_evidence_verifier()` enables this verification. Existing generic and
reviewer result paths retain their meaning. Selected MCP evidence requires the
original retained Role; absent/legacy capture returns capability unavailable.

`factory_mcp_conclusion_shape(assessment)` wraps the stage's own assessment
schema. The model cites its native connection/tool, opaque JSON arguments,
consumed JSON value and JSON Pointer, optionally decoding a standard MCP text
block as JSON. The owner resolves a unique actual call and supplies future
Artifact IDs and record/digest references. Ambiguous matching is rejected rather
than silently selecting the latest call. Authored references, if present, must
resolve exactly. This is a generic content selector, without provider-specific
query or candidate schemas/parsers.

The final verified Flow data retains the model assessment separately from
owner-generated invocation/Build/Attempt/Job and trace/provenance metadata, all
bounded call observations, and resolved claim references. The original native
Artifacts remain the source of raw queries and responses. Model-authored
verification flags cannot establish authority. Empty claims, unusable results
and incomplete capture cannot yield `observed` evidence; failed/cancelled
execution cannot yield successful semantic data. Known sensitive material is
rejected by the existing redaction/verification boundaries.

`observed` means only that the cited values were captured and matched. Remote
coverage remains `unknown`; `global_absence_proven` and `duplicate_proven` are
always false. These fields cannot accept absence/duplicate proof even when the
model claims certainty or a truncated response looks successful. Task 5.2 owns
the subsequent business eligibility policy and its candidate/coverage
obligations. Tasks 5.1 and 8.1 still own the two-stage preset and full packaged
triage journey; this task does not claim their completion.

## TDD evidence

The existing public seams were used: native capture verification, frozen Role
binding and ordinary published Build results. Each behavior below was tested
before its implementation. Commands used this bounded build environment:

```sh
export CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0
cargo test --locked -j2 -p octacity-server-application --lib TEST_FILTER
```

| Test filter | Observed Red | Observed Green |
| --- | --- | --- |
| `native_mcp_capture_exposes_the_actual` | Missing public call observation API | Actual native query/result, Artifact identity, sequences, time and known response digest retained |
| `native_mcp_capture_distinguishes_a_malformed` | Missing malformed observation status | Malformed content differs from successful empty content |
| `native_mcp_capture_rejects_mispaired` | 65-call trace incorrectly accepted | Overflow rejected; missing/duplicate/altered pair records also rejected |
| `native_mcp_evidence_references_resolve_only` | Missing exact reference resolution API | Forged Artifact, sequence, call and content digest rejected |
| `native_mcp_evidence_binds_the_actual` | Missing frozen connection-binding API | Exact Role/configuration/tool/resource metadata retained |
| `mcp_stage_conclusion_retains_verified` | Missing common conclusion verification API | Owner-generated provenance retained and forged proof flags overwritten false |
| `ordinary_mcp_evidence_result_requires_observable` | Model-only legacy output incorrectly accepted | Required unobservable native capture blocks ordinary result acceptance |
| `ordinary_mcp_stage_output_retains_iterative` | Missing shared conclusion shape, then invalid digest representation and model-owned verification fields | Ordinary signed-Job/Artifact path retains three iterative calls and owner-generated references |
| `mcp_model_can_cite_observed` | Valid model-visible native query/value rejected because future server Artifact IDs were required | Server resolves the observed values and supplies exact immutable references |

Environment/setup errors were corrected separately and did not establish Red.
Additional regression cases cover forged candidate values and selectors,
unlisted/duplicate references, secret canaries, unavailable/failed/malformed/
empty/truncated results and cancellation. No compilation-only check substitutes
for the ordinary result assertions.

## Verification results

All commands use the bounded Cargo environment shown above where applicable.

- `cargo test --locked -j2 -p octacity-server-application --lib --test factory_artifact_reader`:
  **813 application tests and 6 Artifact-reader tests passed**, with no failures
  or ignored tests. The full application run completed in 391.15 seconds.
- `cargo test --locked -j2 -p octacity-server-application --lib mcp_`:
  **31 passed**, including ordinary iterative lookup and adversarial results.
- `cargo test --locked -j2 -p octacity-server-application --test native_mcp_evidence -- --ignored --nocapture`:
  **1 passed** with the real published plugin/native executables below.
- `python3 -m unittest tools.tests.test_factory_mcp_credentials` with those same
  executable environment variables: **4 passed, 1 skipped**. The skipped test
  requires the separately launched real OctaCity MCP router, qualified in 4.3.
- `cargo clippy --locked -j2 -p octacity-server-application --lib -- -D warnings`:
  **passed**. `cargo clippy --locked -j2 -p octacity-server-application --tests`:
  **passed**, retaining only the pre-existing unused
  `pending_factory_profile_fixture` warning.
- `cargo fmt --all -- --check`, `python3 tools/check_architecture.py`
  (**64 packages, 263 dependency edges**), strict OpenSpec validation,
  local documentation-link checks and `git diff --check`: **passed**.

Documentation changes have no executable behavior; link/spec/format validation
applies to them instead of a fabricated Red/Green cycle.

The opt-in `server/application/tests/native_mcp_evidence.rs` runs the real
published plugin and native client against finite HTTP/stdio peers. It exports
only their sanitized native trace/provenance into a test-owned directory, then
passes those bytes through the public capture verifier. Four real calls retain
their exact references: two successful reads, a refused resource read and a
refused write. Model/HTTP/stdio/Source/Reporter token canaries stay absent. The
fixed source revision is an explicit fixture input, not a claim about the
checkout's current revision. Its Artifact metadata is owned by the test; the
separate ordinary output tests verify the actual application Artifact lifecycle.

```sh
OCTACITY_TEST_CODEX_BIN=/tmp/octa-settings-official-cli/codex-aarch64-apple-darwin \
OCTACITY_TEST_CODEX_PLUGIN_BIN=/tmp/octacity-octa-0.8.0-native-qualified/plugins/octa_plugin_codex \
CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 \
cargo test --locked -j2 -p octacity-server-application --test native_mcp_evidence \
  -- --ignored --nocapture
```

## Disk and working tree

Disk space was checked before and during builds. Available space fell from
approximately 12.7 GiB to 9 GiB. A guarded attempt to remove only inactive,
regenerable incremental build output stopped before deletion. The entire
`target` directory subsequently disappeared outside that operation, raising
available space to about 47 GiB; tests were rebuilt with two jobs, debug info
disabled and incremental compilation disabled. The agent deleted no source,
stand data, credentials or installed executables. No commit was requested or
created; preceding task 4.3 changes remain in the same working tree.
After rebuilding and verification, approximately **42 GiB** remained available.

## Six review corrections — 2026-10-29

The follow-up review identified three design issues and three specification/test
issues. All corrections use the same public capture, conclusion and ordinary
Build-result seams; no commit or additional persistence owner is introduced.

1. Removed the unused reference-only claim grammar and untagged dispatch.
   Direct adversarial tests now use the actual model-facing connection/tool/
   arguments/value grammar. Exact retained-reference resolution remains its
   existing independent public operation.
2. Moved model-schema projection into the MCP schema owner. It derives the model
   contract from the retained typed shape, preserving required/optional fields
   and array bounds. The ordinary compiler delegates without navigating or
   editing MCP-specific JSON Schema properties.
3. Reused the capture-count constant for schema arrays. Pointer handling now
   shares the existing provider-neutral Flow bound, exported as
   `MAX_FLOW_POINTER_BYTES`; the redundant MCP length check is removed. A new
   regression exposed a real discrepancy: the MCP schema advertised 1024 bytes
   while Flow mapping validation already allowed only 256. Both now agree on
   the existing 256-byte bound.
4. Applied the existing sensitive-material checker to decoded arguments/value
   JSON and the normalized retained conclusion as well as the incoming
   envelope. Unicode escapes cannot reveal a blocked marker only after the
   earlier check has passed. The checker remains the existing known-pattern
   protection; native execution still owns actual credential-value redaction.
5. Required presence of the native `arguments` field in each start/completion
   record. Missing captured arguments cannot silently become observed null;
   explicit JSON null retains its meaning.
6. Kept adversarial model claims in failed/malformed/unavailable/empty/truncated/
   cancelled fixtures. Failed partial content and malformed content with a
   readable candidate produce review results with nonempty claims and their
   actual observation status. Unsupported candidate claims are rejected.
   Truncated captured fields can remain observations with unknown coverage and
   false proof flags. The no-claims review case is tested separately.

The bounded Cargo environment remains:

```sh
export CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0
cargo test --locked -j2 -p octacity-server-application --lib TEST_FILTER
```

| Test filter / change | Observed baseline or Red | Green / preserved behavior |
| --- | --- | --- |
| `mcp_conclusion_rejects_sensitive_json_after_decoding` | Unicode-escaped `ghp_` marker incorrectly accepted | Decoded `ghp_`, bearer and password-key canaries rejected; 32 MCP tests passed |
| `native_mcp_capture_requires_observed_arguments` | Both records lacking arguments incorrectly accepted | All missing-field combinations rejected, explicit null retained; 5 capture tests passed |
| `mcp_conclusion_rejects_the_undocumented_reference_only_claim_grammar` | Old grammar accepted; 14 migrated behavioral tests already passed | Old grammar rejected; all 15 evidence tests passed |
| Typed model-schema projection | Exact emitted fields/requirements and ordinary results characterized before refactoring | The same 15 evidence tests passed after delegation to the MCP owner |
| Adverse-result test correction | Strengthened cases passed with candidate claims retained | No production behavior change needed; review/error/unknown-coverage assertions no longer depend on cleared claims |
| `mcp_conclusion_schema_and_verifier_share_the_flow_pointer_boundary` | Schema accepted a 257-byte pointer rejected by the verifier | Shared existing Flow bound; 256-byte success and 257-byte rejection checked through both public contracts |

The schema/count extraction is a behavior-preserving refactor except for the
advertised pointer-bound correction, which has its own observed Red above.
Existing core Flow tests passed before exposing the shared constant (**135
tests**). Compiler/setup mistakes were corrected separately and were not
counted as behavioral Red. Independent standards and specification follow-up
reviews found all six original findings resolved, without new actionable issues.

Final follow-up verification on macOS, using the bounded Cargo environment above:

| Command | Result |
| --- | --- |
| `cargo test --locked -j2 -p octacity-server-application --lib mcp_` | 35 passed, including all four new behavioral regressions |
| `cargo test --locked -j2 -p octacity-server-application --lib --test factory_artifact_reader -- --quiet` | 817 application tests and 6 Artifact-reader tests passed; application run 406.66 seconds |
| `cargo test --locked -j2 -p octacity-server-factory --lib flow_ -- --quiet` | 135 passed after the shared-constant extraction |
| `cargo test --locked -j2 -p octacity-server --test process_lifecycle -- --quiet` | 11 passed, including the portable credential-reference fixture |
| `cargo test --locked -j2 -p octacity-server-application --test native_mcp_evidence -- --ignored --nocapture` with the pinned executable variables shown earlier | 1 actual native-client/plugin test passed |
| `cargo clippy --locked -j2 -p octacity-server-application -p octacity-server-factory --lib -- -D warnings` | Passed |
| `cargo clippy --locked -j2 -p octacity-server-application --tests` | Passed; only the pre-existing unused `pending_factory_profile_fixture` warning |
| `cargo fmt --all -- --check`, `python3 tools/check_architecture.py`, `openspec validate add-factory-triage-pipeline --strict`, local documentation links, `git diff --check` | Passed; architecture remains 64 packages / 263 dependency edges |

The final regression sets contain **970 distinct Rust tests**; the 35-test MCP
subset is included in the application total. No failures or ignored tests remain
in those explicitly selected runs. The server linker retains its existing macOS
compact-unwind-size warning. Windows was not executed on this host.
Documentation/formatting-only corrections use validation rather than fabricated
Red/Green evidence. Free disk stayed around **38 GiB** throughout this follow-up;
no cleanup, dependency addition, staging or commit was performed.
