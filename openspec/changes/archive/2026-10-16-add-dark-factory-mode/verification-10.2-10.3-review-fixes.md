# Review fixes for tasks 10.2–10.3

Scope: the three findings from the review of uncommitted changes above HEAD
`5fd87eb`. No later plan task was implemented, and no commit or staging was
performed.

## Corrected contracts

- Portable Factory repository paths reject `:` as the signed ChangeSet contract
  already does. Capture scopes cannot pass Flow publication with paths that a
  signed Job will reject.
- Optional signed `factory.source_write_paths` explicitly selects nonempty,
  sorted, disjoint directory roots below source. It requires an existing source
  read/write grant and only narrows that authority. Agent prepares and validates
  these roots before credentials, then supplies read-only source plus writable
  overlays to the existing backend contract. Capture filtering retains its
  separate file/prefix semantics; file and overlapping capture prefixes no
  longer create directories or become mount declarations.
- When a frozen node declares `capture_write_paths`, every materialized Job must
  retain both that exact capture filter and the same signed writable roots.
  PostgreSQL admission rejects absent, substituted or partially narrowed roots
  without creating an ordinary Build operation.
- The supplied author has one scope setting, `capture_write_paths`. Its input
  selects the resolved admitted setting using the generic `node_configuration`
  source. The gate selects the same scope from retained Build profile metadata.
  Neither provider output nor incoming data supplies the authoritative scope.
- Generic `repository_path_within_any` checks a path against an explicitly
  selected root list. Malformed roots remain unknown even after an earlier
  match, and each comparison consumes the existing shared evaluation budget.
  Names of flows, nodes and repository layouts select no Rust execution branch.

Public API comments, the Flow publication guide and the signed JobSpec v3
reference document these contracts. The example retains one literal whitelist;
author instructions and gate predicates no longer keep their own copies.

## TDD evidence

All Cargo test commands use `CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0`.
Logs below are retained under `/tmp`; each row records a Red observed before
the corresponding implementation.

| Public command after `cargo test` | Observed Red | Green log |
| --- | --- | --- |
| `-p octacity-server-factory published_capture_scopes_require_portable_signed_job_paths --lib` | Publication accepted `contracts:next` | `octacity-review-path-green.log` |
| `-p octacity-job capture_filters_preserve_file_and_overlapping_prefixes_without_mount_grants --lib` | An existing-file filter stopped execution before the runner | `octacity-review-capture-green.log`; all 41 Agent Job tests passed, including existing-file, new-file and overlapping-filter cases and explicit directory projection |
| `-p octacity-protocol signed_source_write_roots_are_explicit_disjoint_and_require_write_authority --lib` | Strict wire decoding rejected the new explicit directory-scope field | `octacity-review-signed-roots-green.log`; valid scope accepted, root/empty/overlap/order/size/portable-path and read-only-authority violations rejected |
| `-p octacity-server --test factory_configured_flow signed_job_capture_cannot_widen_the_frozen_node_write_scope -- --ignored` | Store accepted a capture Job missing the frozen writable roots | `octacity-review-build-scope-green.log`; absent/different roots and capture policies rejected atomically, exact DAG accepted |
| `-p octacity-server-factory configured_node_projects_only_explicit_fields_from_its_pinned_configuration --lib` | `node_configuration` was unavailable | `octacity-review-setting-green.log`; exact selected setting restored, missing setting rejected |
| `-p octacity-server-factory configured_path_scope_uses_a_bounded_list_and_rejects_unknown_prefixes --lib` | The list containment comparison was unavailable | `octacity-review-scope-list-green.log`; boundary/invalid-root/negation/evaluation-exhaustion cases passed |
| `-p octacity-server-factory changing_one_published_scope_updates_author_input_and_capture_gate --lib` | Author input retained the old literal whitelist after changing the node scope | `octacity-review-config-scope-green.log`; new paths accepted, old paths rejected, incoming scope ignored; all 10 supplied-policy tests passed |

Red logs use the corresponding `*-red.log` names. Test fixture schema mistakes
were corrected before observing the feature Reds and are not counted as Red
evidence. The existing specification policy test passed before its common input
fixture was extracted (`octacity-review-config-baseline.log`). Documentation and
formatting changes do not have executable behavior requiring a separate TDD
cycle.

## Final verification

| Suite | Passing tests | Log under `/tmp` |
| --- | ---: | --- |
| Protocol, execution, Microsandbox, Agent Job, Factory, application, store and server Job libraries | 543 | `octacity-review-final-libraries.log` |
| Server Job signing/template and state-machine integration contracts | 12 | `octacity-review-final-job-contracts.log` |
| Ordinary configured-flow contracts | 15 | `octacity-review-final-flow.log` |
| PostgreSQL configured owner, recovery and Build admission contracts | 25 | `octacity-review-final-postgres.log` |
| Application and Factory doctests | 4 | `octacity-review-final-doctests.log` |

Total: **599 passing tests**, zero failures in final suites.

```sh
cargo test -p octacity-protocol -p octacity-execution -p octacity-execution-microsandbox -p octacity-job -p octacity-server-factory -p octacity-server-application -p octacity-server-store -p octacity-server-job --lib
cargo test -p octacity-server-job --tests
cargo test -p octacity-server --test factory_configured_flow
cargo test -p octacity-server --test factory_configured_flow -- --ignored --skip production_factory_owner_uses_installed_bindings_and_recovers_the_same_arbitrary_node
cargo test -p octacity-server-factory -p octacity-server-application --doc
cargo clippy -p octacity-protocol -p octacity-job -p octacity-execution -p octacity-execution-microsandbox -p octacity-execution-containerd -p octacity-server-factory -p octacity-server-store -p octacity-server-application -p octacity-server-job -p octacity-server --all-targets --all-features -- -D warnings
RUSTDOCFLAGS='-D missing_docs' cargo doc --no-deps --all-features -p octacity-protocol -p octacity-job -p octacity-execution -p octacity-execution-microsandbox -p octacity-server-factory -p octacity-server-store -p octacity-server-application -p octacity-server-job -p octacity-server
cargo fmt --all --check
python3 tools/check_architecture.py
openspec validate add-dark-factory-mode --strict
git diff --check
```

Clippy and public API documentation checks passed
(`octacity-review-final-clippy.log`, `octacity-review-final-doc.log`). Architecture
validation passed for 56 packages and 211 dependency edges. Formatting,
whitespace and strict OpenSpec validation passed.

PostgreSQL tests used the task-owned disposable PostgreSQL 18 service through
`OCTACITY_POSTGRES_URL`; it was stopped and removed after testing. The unchanged
PostgreSQL/MinIO production composition case was excluded. Tests ran on macOS;
Linux containerd execution, live model calls and real-backend qualification were
not claimed by this slice. Publication/readiness remains task 10.4, production
composition remains 16.1, and live qualification remains 16.4.
