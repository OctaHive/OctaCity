# Task 10.2: configured specification authoring

Completed scope: an isolated reasoning Build can retain an exact captured
specification candidate and typed handoff through the common node contracts.
The supplied flow routes specification work to configurable authoring and
validation nodes. Neither node names nor feature/defect policy select Rust
execution branches. No commit, staging or push is part of this task.

## Contracts and authority

- `result_tool` selects an independently verified primary output without
  replacing the executed reasoning harness in Build causality. Trusted capture
  checks the canonical manifest and bundle, exact selected predecessor, original
  Work subject, input and complete Build/Attempt/Job identities.
- The supplied closed handoff schema carries Work, original base, predecessor,
  candidate and input identities, applicable contract/change paths and acceptance
  criteria. Required Reports retain their exact schemas, producer identities,
  content digests and trusted freshness deadlines.
- `capture_write_paths` narrows the published profile and freezes the exact
  whitelist in the node Build intent. Durable ordinary Build admission checks
  every materialized signed Job against it before dispatch. Missing, broader or
  different narrower capture policies cannot substitute for the frozen policy.
- Agent prepares bounded directory overlays before credentials are provisioned.
  Source remains read-only outside the signed whitelist; scratch and declared
  outputs retain separate writable roots. Source write authority is still
  required by JobSpec v3. Symlink/reparse paths, overlapping roots, file roots,
  escapes and insufficient provider projection capacity fail closed.
- The example's specification directories and feature/defect acceptance rules
  are configuration data. A generic shared gate precondition checks retained
  identities and freshness; portable path-containment predicates check all
  captured paths. Existing predicate, byte, depth and evaluation ceilings remain
  unchanged.
- Authoring ends at `specification_authored` with `review_pending`. It cannot
  create Accepted Work or dispatch implementation. Missing exact capture or
  required handoff cannot be replaced by model prose.

See `config/factory/flows/README.md` for publication requirements and the supplied
schemas and bindings. Writable prefixes currently designate directory roots;
an existing regular file cannot be used as an overlay root.

## Test-first evidence

All commands use `CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0`.
The commands in the table are arguments after `cargo test`. Log names are under
`/tmp`; each Red was observed before implementing its missing behavior.

| Public behavior / command | Observed Red | Green evidence |
| --- | --- | --- |
| `-p octacity-server-application configured_capture_executor_returns_the_verified_manifest_through_the_common_node_contract --lib` | Published binding rejected the independent `result_tool` selection | `octacity-10.2-independent-capture-green.log`; extended exact predecessor, original base, typed handoff, restoration and missing-output checks pass in the final application suite |
| `-p octacity-job signed_capture_paths_narrow_the_runner_write_projection_before_authoring --lib` | The required public layout API had no scoped write projection | `octacity-10.2-scoped-writes-green.log`; signed directory overlays reach the runner before authoring |
| `-p octacity-execution validates_the_factory_layout_before_backend_projection --lib` | 257 overlays exceeded the signed count ceiling but were accepted | `octacity-10.2-final-libraries.log`; bounded valid overlays pass, excess and whole-source grants fail |
| `-p octacity-server-factory configured_path_comparisons_respect_components_and_reject_nonportable_paths --lib` | Portable repository containment was not a supported comparison | `octacity-10.2-path-gate-green.log`; component boundaries and invalid portable paths are checked |
| `-p octacity-server-factory a_configured_precondition_fails_closed_before_any_routing_rule --lib` | Gate wire rejected the shared `precondition` | Final Factory suite; false or unknown preconditions select the declared fallback |
| `-p octacity-server-factory configured_specification_authoring_requires_exact_capture_and_typed_handoff_before_review --lib` | Authoring gate absent; subsequent slices exposed missing trusted output deadlines and a null frozen write whitelist | `octacity-10.2-author-config-green.log`, `octacity-10.2-author-freshness-green.log`, `octacity-10.2-frozen-write-scope-green.log`; feature/defect criteria, identity substitution, stale outputs and out-of-scope paths are covered |
| `-p octacity-server --test factory_configured_flow signed_job_capture_cannot_widen_the_frozen_node_write_scope -- --ignored` | PostgreSQL admission accepted a Job whose capture whitelist widened the frozen intent to `.` | `octacity-10.2-write-scope-admission-green.log`; substituted/absent policies are rejected atomically, the exact two-Job Build is accepted |

Red logs have the corresponding `*-red.log` names. The scope bound uses
`octacity-10.2-projection-bound-red.log`, and the independent precondition uses
`octacity-10.2-common-guard-red.log`. The existing owner routing test was updated
with the authoring transition and verifies the exact prepared Work/base and
accepted predecessor payload (`octacity-10.2-owner-routing-green.log`). Existing
capture behavior passed before the adapter refactoring
(`octacity-10.2-capture-baseline.log`). Documentation-only updates have no
executable behavior requiring a Red/Green cycle.

## Final checks

| Suite | Result | Log under `/tmp` |
| --- | --- | --- |
| Eight library packages: execution, Job, Microsandbox, application, Factory, store, protocol, ChangeSet | 545 passed | `octacity-10.2-final-libraries.log` |
| Ordinary configured-flow contracts | 15 passed; 26 external-service tests excluded here | `octacity-10.2-final-configured-flow.log` |
| Configured-flow PostgreSQL contracts | 25 passed | `octacity-10.2-final-postgres.log` |
| Application/Factory doctests | 4 passed | `octacity-10.2-final-doctests.log` |

These suites total **589 passing tests**, with no failures.

Commands:

```sh
cargo test -p octacity-execution -p octacity-job -p octacity-execution-microsandbox -p octacity-server-application -p octacity-server-factory -p octacity-server-store -p octacity-protocol -p octacity-changeset --lib
cargo test -p octacity-server --test factory_configured_flow
cargo test -p octacity-server --test factory_configured_flow -- --ignored --skip production_factory_owner_uses_installed_bindings_and_recovers_the_same_arbitrary_node
cargo test -p octacity-server-application -p octacity-server-factory --doc
cargo clippy -p octacity-server-factory -p octacity-server-store -p octacity-server-application -p octacity-job -p octacity-execution -p octacity-execution-microsandbox -p octacity-execution-containerd --all-targets --all-features -- -D warnings
RUSTDOCFLAGS='-D missing_docs' cargo doc --no-deps --all-features -p octacity-server-factory -p octacity-server-store -p octacity-server-application -p octacity-execution -p octacity-job -p octacity-execution-microsandbox -p octacity-execution-containerd
python3 tools/check_architecture.py
openspec validate add-dark-factory-mode --strict
cargo fmt --all --check
git diff --check
```

Clippy and public API documentation passed for all seven selected packages and
features (`octacity-10.2-clippy.log`, `octacity-10.2-doc.log`). Architecture
validation passed for 56 packages and 211 dependency edges. PostgreSQL tests
used an explicitly disposable PostgreSQL 18 service through
`OCTACITY_POSTGRES_URL`.

Independent requirements review and specification publication remain tasks 10.3
and 10.4. Production capability composition remains task 16.1. This task tests
the public contracts and provider projections; it does not claim live reasoning
or real sandbox qualification. Containerd's Linux-only mount translation cannot
execute on this macOS host; real-provider qualification remains task 16.4. The
unchanged PostgreSQL/MinIO production-composition test was excluded from this
task's PostgreSQL run. Free disk space remained above the 5 GiB cleanup threshold
(approximately 10 GiB after validation). HEAD remained `5fd87eb`; the Git index
was untouched.
