# Security release gates

The scheduled `security` workflow keeps slow, network-dependent checks out of
the portable commit loop while still making them reproducible. The release
workflow calls it with fuzzing enabled and cannot package before it succeeds.
The complete trust analysis and remaining deployment assumptions are recorded
in [the server and Agent threat model](../architecture/threat-model.md).

## Dependencies, licenses, and sources

The workflow audits both the production and fuzz lockfiles against RustSec:

```shell
cargo audit
cargo audit --file fuzz/Cargo.lock
```

It also applies separate checked-in policies to the production and fuzz
dependency graphs:

```shell
cargo deny --locked check licenses sources
cargo deny --locked --manifest-path fuzz/Cargo.toml --config fuzz/deny.toml check licenses sources
```

The production policy is stored in `deny.toml`; the independent fuzz workspace
uses `fuzz/deny.toml` because its libFuzzer dependency graph requires a
different reviewed license set. Both policies reject unknown registries and Git
sources and do not exempt private workspace packages from license metadata. A
new license or source requires an explicit reviewed policy change.

The workflow pins `cargo-audit`, `cargo-deny`, Gitleaks, and `cargo-fuzz`;
version changes therefore remain reviewable. Vulnerability, license, source,
and detected-secret findings fail the job. Informational RustSec warnings
remain visible and must be evaluated rather than silently ignored or converted
into permanent blanket exceptions.

### zstd advisory follow-up (2026-10-27)

The direct workspace dependency now requires `zstd >=0.14.3`; the production
lockfile resolves `zstd-safe 8.2.0` and `zstd-sys 2.1.2+zstd.1.5.7` for that
branch. This also updates the compatible `async-compression` branch and removes
the affected `zstd-sys` version from the entire graph. The separate fuzz lockfile
has no affected zstd dependency and remains unchanged.

The Linux Microsandbox branch still resolves
`microsandbox 0.7.6 → microsandbox-filesystem → msb_krun 0.1.39 →
msb_krun_vmm 0.1.40 → zstd 0.13.3 → zstd-safe 7.3.0`.
All published `msb_krun_vmm` versions through `0.1.42` still require `zstd 0.13`.
Consequently the production audit still reports
[RUSTSEC-2026-0391](https://rustsec.org/advisories/RUSTSEC-2026-0391),
[RUSTSEC-2026-0398](https://rustsec.org/advisories/RUSTSEC-2026-0398) and
[RUSTSEC-2026-0403](https://rustsec.org/advisories/RUSTSEC-2026-0403), plus the
related informational unsoundness warnings for `7.3.0`. No advisory ignore, platform
filter, local fork or vendored VMM is added: the owner explicitly deferred that
branch on 2026-10-27 pending an upstream release. Next review: 2026-11-03, or when
a patched upstream VMM is published, whichever occurs first. The production
security job intentionally remains blocked by those three findings.

The regression at the public zstd stream/cache boundary is reproducible with:

```shell
cargo test --locked -p octacity-server-cache zstd_context_reuse
```

Before the update, decompression of the next completed stream returned the
abandoned stream's plaintext prefix as well as its own input (Red). With `0.14.3`,
the same test returns exactly the next input and accepts its cache descriptor
(Green). Full cache/API and Microsandbox adapter tests qualify compatibility;
they do not replace the remaining upstream remediation or a real VM run.

Verification on 2026-10-27: `cargo test --locked -p octacity-server-cache
-p octacity-server-api-cache` passed 8 cases; `cargo test --locked
-p octacity-execution-microsandbox --lib` passed 14. Scoped Clippy with
`--all-targets -- -D warnings`, formatting, architecture and both license/source
policies passed. Production audit decreased from 7 findings to the 3 deferred
above; fuzz audit reports no vulnerabilities. No exception policy was changed.

## Secret scanning

Gitleaks scans both source trees used to produce a release: this repository and
the pinned Octa checkout. It uses the maintained default rules and redacts any
detected value from CI output:

```shell
gitleaks dir --config octacity/.gitleaks.toml --redact --no-banner --no-color octacity
gitleaks dir --config octacity/.gitleaks.toml --redact --no-banner --no-color octa
```

Do not add a baseline for a real credential. Revoke it, remove it from the
revision, and rerun the gate. A demonstrable false positive may receive the
narrowest possible path-and-rule exception with a reason and a regression test;
global rule disablement is not acceptable. Runtime leakage remains covered by
the security contract matrix and redaction tests because a repository scanner
cannot observe generated logs, events, artifacts, or diagnostics.
The Agent filesystem/supply-chain security contract also runs
`octacity-lifecycle`, including the public hook-parser redaction and lease-bound
broker cancellation tests. Those portable tests accompany, rather than replace,
the released-backend qualification described in the threat model.

The checked-in configuration has one such exception: the pinned Octa source
contains a public test-only TLS private-key fixture used by cache protocol
tests. Only the `private-key` rule at that exact fixture path is exempted.

## Factory adversarial contracts

`python3 tools/security_matrix.py run --evidence-dir /tmp/octacity-security`
includes the `factory-authority` group. It executes the Factory core and
Decision Signal connector tests, including definition closure, context,
evidence and provider identity negatives. The existing application and Agent
groups exercise their public dispatch, result, filesystem and broker boundaries.
A failing group fails the matrix while retaining all group outcomes and logs.
Run these tests even when Factory is disabled in the deployment: disabled mode
must not bypass the authority checks or turn an unsupported route into Host
execution.

The coverage map below identifies the owning tests, not logical Flow or node
names. Test inputs may use arbitrary names; runtime dispatch selects installed
immutable capabilities.

| Attack | Executable contract and required refusal |
| --- | --- |
| Prompt injection | Core `task_contract_tests::prompt_injected_fragments_are_non_authoritative_supplemental_context`: repository instructions remain data; `model_results_cannot_smuggle_control_or_apply_self_modification` rejects invented authority. |
| Repository-controlled Flow changes | Core `flow_data_tests::persisted_admission_rejects_a_replacement_definition_even_with_the_same_flow_version` refuses a fresh valid closure whose digest differs from the retained root Run. |
| Undeclared routes | Core `flow_interpreter_tests::interpreter_uses_only_the_declared_outcome_route_from_authoritative_facts` refuses an invented outcome. |
| Recursive or unbounded execution | Core `rejects_provider_created_edges_recursion_and_unbounded_cycles` and `rejects_node_fanout_wip_and_budget_beyond_admission_bounds` refuse unsafe admission. |
| Context-edge leakage | Core `interpreter_carries_only_declared_context_and_data_into_the_successor` withholds predecessor data without an explicit projection. |
| Protected-test disclosure | Application `factory_node_capture_tests::candidate_context_keeps_binary_source_inputs_out_of_declared_model_text` excludes private source inputs; physical input projection contracts verify absence from the guest. |
| Provider/model substitution | Core `decision_signal_tests::request_requires_exact_provider_model_semantics_and_purpose` and application `factory_node_build_tests::verification_dispatch_rejects_endpoint_substitution_and_embedded_credentials` refuse substituted identities. |
| Path/mount escape | Core `permission_tests::command_and_path_intersection_rejects_incompatible_or_lookalike_authority` and Agent guest capture contracts deny paths outside the signed scope. |
| Credential crossing | Application `factory_credentials_tests::coding_cannot_materialize_delivery_identity` and `factory_codex_tests::compiler_rejects_cross_stage_credentials_and_writable_evaluation_source` refuse another role's authority. |
| Forged evidence | Application `factory_review_planning_tests::identical_evidence_json_without_trusted_facts_cannot_create_a_review_plan` refuses evidence-shaped JSON without trusted provenance. |
| ChangeSet tampering | Application `factory_node_capture_tests::candidate_materialization_requires_the_explicit_frozen_context_and_retained_artifacts` and `stored_candidate_context_rejects_short_retention_foreign_capture_and_late_owner_expiry` refuse mismatched or unavailable retained inputs. |
| Stale cycles/fences | Core `lifecycle_decision_tests::claims_reject_stale_fences_and_exclusive_deadlines` refuses expired ownership; restored admission rechecks its initial cycle, and application observers recheck authority after output I/O. |
| Duplicate side effects | Application `factory_flow_action_tests::exact_candidate_publication_recovers_a_lost_response_without_another_write` and `factory_decision_signal_tests::retrying_an_unknown_signal_within_the_deadline_cannot_dispatch_the_provider_again` preserve one logical operation. |
| Unqualified fallback | Core `permission_tests::unknown_and_unenforceable_authority_fail_closed` and Agent `execution_v2::never_falls_back_to_host_for_a_factory_job` deny dispatch without the exact qualified route. |

Portable tests establish logical authority, not physical isolation. Run the
[released Factory backend contracts](backend-contracts.md) with the pinned
Octa release and the selected backend to establish filesystem and process
behavior. Their hostile repository fixture must attempt real reads and writes;
an unconfined fixture run must demonstrate that the same probes can succeed.
Canaries must remain absent from retained logs, events, reports and failure
diagnostics. Keep the full nested-flow restart/promotion pilot and qualification
of other operating systems as separate release requirements; a local negative
contract cannot substitute for them.

## Protocol fuzzing

Four fuzz targets feed arbitrary bytes to the public deserializers at each
process or network boundary:

```shell
cargo fuzz run server-agent-json
cargo fuzz run runner-json
cargo fuzz run source-plugin
cargo fuzz run server-protocols
```

`server-agent-json` covers every public coordinator request, response, nested
wire DTO, and signed JobSpec decoder,
`runner-json` covers command deserialization and the exact bounded output-frame
decoder used by the supervisor. `source-plugin` covers the shared bounded JSONL
decoder in both directions plus the operator TOML manifest. `server-protocols`
covers the exact bounded request and correlated-response decoders for artifact
transfer, VCS, webhook-provider, and agent-provisioning processes. The
server-Agent target also
constructs correctly signed arbitrary payloads so signature verification,
payload decoding, and JobSpec validation are reached. Asynchronous I/O and
lifecycle behavior retain ordinary unit and contract tests; fuzzing complements
those tests rather than replacing timeout or state-machine assertions.

Corpus and crash artifacts are intentionally untracked. Reproduce a reported
crash with the exact workflow artifact and pinned toolchain before minimizing
it; add the minimized input as a normal regression test at the owning protocol
boundary.

## Release composition

`ci.yml` is also a reusable workflow. `release.yml` requires it, this security
workflow with fuzzing enabled, and the release-qualified backend contract
workflow before any package job starts. This binds Clippy with `-D warnings`,
rustdoc with `-D missing_docs`, the architecture guard, the 80% production-line
coverage floor, security checks, and released-backend evidence to the exact
revision being packaged.
