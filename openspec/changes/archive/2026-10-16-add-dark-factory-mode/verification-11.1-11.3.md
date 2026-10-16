# Tasks 11.1–11.3: protected acceptance checks

Test authoring and validation are installed technical capabilities over ordinary
Builds. Logical node names, routes, test classes, input selections, limits and
retention are configured. The common interpreter has no new business-stage
dispatch. No commit is created for this work.

## Implemented boundaries

- A frozen `ProtectedTestPlan` binds the accepted contract, exact subject,
  operator-selected classes, limits and original retention deadline.
  `FactoryStoredProtectedTestPlanSource` reconstructs it from persisted accepted
  ancestry and an exact immutable profile. It rejects another profile or source.
- Authoring verifies real published inventory/source bytes, digests, sizes,
  Build/Attempt/Job/Lease provenance, complete class coverage, unique paths and
  Artifact identities, aggregate byte limits and retention before common
  completion. The closed public handoff contains contract identity and classes.
  Raw source bytes and inventory contents are absent from the common result.
- The supplied implementation route follows an authoring control edge but selects
  its original input payload. Its frozen Context contains only the declared Work
  task, without private inventory/files or author transcripts. Artifact download
  authorization selects only signed inputs. Agent stages each Job in a private
  root, rechecks downloaded bytes and seals its inputs read-only after restart.
  The existing Codex compiler consumes only a bound Task Envelope and exact
  prompt, rejects substituted authority and uses the selected credential scope.
- Trusted command validation requires explicit exact candidate manifest/bundle
  and test inventory selections. Materialization checks the complete candidate
  and retained test contract; writable source/protected roots and their ancestors
  or descendants are rejected. The command/toolchain is an installed profile
  choice, not a node-name-selected test framework.
- A closed validation Report binds both identities, canonical materialization
  digest and every required class. The installed adapter verifies it and its
  independent byte/input/freshness facts before common completion. Evidence
  retains the complete candidate and all exact input Artifact references.
  Repeated materialization is stable; candidate, inventory, file, coverage or
  report substitution cannot reuse the accepted result.
- Terminal-Attempt output verification is shared with ordinary candidate capture.
  Candidate capture may retain a public primary result and a separately verified
  private manifest/bundle. Restoration uses fetched immutable bytes and common
  provenance rather than mutable stage pointers or a process cache.

Full composition-root/provider wiring remains task 16.1, and released backend
threat-model qualification remains 16.4. Tests here exercise real public contract,
Artifact, Agent staging and PostgreSQL boundaries with controlled Build/output
ports; they do not claim a live external provider run. Defect routing, independent
code review and implementation deliverable completeness remain 11.4–11.6.

## Editable base flow

The former `base-intake.json` is replaced by `base-flow.json` and 69 explicitly
listed parts in `base-flow/`: one schema or node per editable file, plus
transitions and terminal contracts. The largest part has 678 lines.
`FlowDefinitionTemplate::from_manifest` is a bounded, flat byte-resolver API;
the domain performs no filesystem/network access and interprets no file names.
Duplicate imports, definitions, missing parts and oversized assemblies fail.
The assembled template publishes identically through the existing immutable
publication boundary. Admitted runs never retain mutable file imports.

The example execution attempt ceiling is configured as 40 rather than 32:
additional validation and its gate must fit both existing correction paths.
A pre-change behavioral correction test exhausted the 32-attempt ceiling; the
same owner test passed after updating the configuration. Runtime limits remain
fully operator-configurable.

## TDD evidence

Every Cargo test uses `CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0`.
The table gives arguments after `cargo test`, observed Red before implementation,
and the subsequent passing run. Logs are local under `/tmp`; compilation errors
below concern newly required APIs or configuration, not an unavailable test
environment. Behavioral corrections each had their own Red/Green cycle.

| Command / public boundary | Observed Red | Observed Green / log |
| --- | --- | --- |
| `-p octacity-server-factory --lib protected_bundles_require` | Protected plan/bundle API absent | Contract passes; `octacity-11.1-contract-green.log` |
| `-p octacity-server-factory --lib protected_files_cannot_alias` | Required validated file identity API absent | Alias rejection passes; `octacity-11.1-file-identity-green.log` |
| `-p octacity-server-factory --lib protected_byte_limit` | A 160-byte inventory/source set was accepted against a 100-byte ceiling | Exact 100 accepted, 160 rejected; `octacity-11.1-aggregate-limit-green.log` |
| `-p octacity-server-application --lib protected_capture_verifies` | Verified capture API absent | Actual-byte/provenance negatives pass; `octacity-11.1-capture-green.log` |
| `-p octacity-server-application --lib configured_intake_research_unknown_step` | Frozen-plan and then stored-plan source APIs absent | Persisted ancestry/profile recovery passes; `octacity-11.1-plan-green.log`, `octacity-11.1-stored-plan-green.log` |
| `-p octacity-server-application --lib configured_test_policy` | Requested coverage-policy configuration absent | All declared families and a new custom class accepted; `octacity-11.1-config-policy-green.log` |
| `-p octacity-server-application --lib author_executor_keeps_private` | Installed author executor API absent | Public/private separation and restored record/intent/inventory pass; `octacity-11.1-author-executor-green.log` |
| `-p octacity-server-factory --lib configured_retained_record_can_select_metadata` | Record metadata selection rejected | Explicit metadata view passes; `octacity-11.2-record-view-green.log` |
| `-p octacity-server-factory --lib configured_predecessor_metadata` | Trusted record digest absent | Exact record identity available without provider payload; final core suite |
| `-p octacity-server-factory --lib supplied_author_handoff` | Closed handoff, safe implementation projection and explicit validation selections absent | Configured isolation passes; `octacity-11.3-context-reference-green.log` |
| `-p octacity-server-factory --lib supplied_validation_gate` | Exact candidate/test validation gate absent | Configured acceptance/replay negatives pass; final core suite |
| `-p octacity-server-application --lib protected_materialization_requires` | Private materialization API absent | Exact explicit selections, retention and repeat behavior pass; `octacity-11.3-materialization-green.log` |
| `-p octacity-server-application --lib retained_evidence_binds_exact` | General immutable Evidence input API absent | Arbitrary inputs and drift verification pass; `octacity-11.3-evidence-inputs-green.log` |
| `-p octacity-server-application --lib factory_node_capture_tests` | Restoration with a public primary result and private capture API absent | Six capture tests pass; `octacity-11.3-public-candidate-green.log` |
| `-p octacity-server-application --lib validation_freezes_both_inputs` | Materialization/report verification, then Evidence construction, then installed validation adapter APIs absent | Real ordinary completion and replay negatives pass; `octacity-11.3-validation-green.log`, `octacity-11.3-evidence-build-green.log`, `octacity-11.3-installed-validation-green.log` |
| Same validation command | A writable `/workspace/source` mount was accepted | Writable roots, parents and descendants rejected; `octacity-11.3-validation-mount-green.log` |
| `-p octacity-server-application --lib implementation_and_verification_defects` | Existing owner correction exhausted the example's attempt ceiling | Both correction origins complete; `octacity-11-regression-correction3.log` |
| `-p octacity-server-factory --lib modular_templates_publish_identically` | Flat manifest API absent | Exact assembled publication equality passes; `octacity-template-split-green.log` |

Shared capture/output refactoring and the file split started with passing existing
behavioral baselines. Additional Artifact/Agent isolation tests characterize
existing behavior without changing those production implementations.
Documentation and import/formatting edits have no separate executable behavior;
Rustdoc, formatting and whitespace checks validate them.

## Regression and quality checks

```sh
cargo test -p octacity-server-factory -p octacity-server-store --lib
cargo test -p octacity-server-application --lib
cargo test -p octacity-server-application --lib -- --skip factory_requirements_tests
cargo test -p octacity-server-application --lib protected_
cargo test -p octacity-job --lib protected_inputs::tests
cargo test -p octacity-server-factory -p octacity-server-application --doc
cargo test -p octacity-server --test factory_configured_flow configured_manual_intake_and_unknown_node -- --ignored
cargo test -p octacity-server --test factory_configured_flow specification_publication_and_correction -- --ignored
cargo clippy -p octacity-server-factory -p octacity-server-application -p octacity-job --all-targets --all-features -- -D warnings
RUSTDOCFLAGS='-D warnings -D missing_docs' cargo doc -p octacity-server-factory -p octacity-server-application -p octacity-job --all-features --no-deps
python3 tools/check_architecture.py
openspec validate add-dark-factory-mode --strict
cargo fmt --all -- --check
git diff --check
```

PostgreSQL checks use a task-owned disposable PostgreSQL 18 service and
`OCTACITY_POSTGRES_URL`; databases are reconstructed through the existing shared
owner/store contracts. No production data or credentials are used.

| Boundary | Result | Log under `/tmp` |
| --- | --- | --- |
| Factory core | 209 passed | `octacity-11-core-store-final.log` |
| Common store | 68 passed | `octacity-11-core-store-final.log` |
| Application full regression before final characterization | 186 passed | `octacity-11-application-regressions.log` |
| Final application regression excluding the three already-green requirements matrices | 184 passed | `octacity-11-application-final.log` |
| Protected application contracts, Artifact grants and compiler bindings (subset) | 9 passed | `octacity-11-protected-regressions-final.log` |
| Agent actual HTTP staging/private roots/restart | 6 passed | `octacity-11.2-agent-isolation-green.log` |
| Application/Factory documentation contracts | 4 passed | `octacity-11-doctests.log` |
| PostgreSQL configured owner, stored plan and implementation isolation | 1 passed | `octacity-11-pg-plan-isolation-final.log` |
| PostgreSQL publication/correction/lost-response matrix | 1 passed (three scenarios) | `octacity-11-pg-correction-final.log` |

Clippy and Rustdoc pass with warnings and missing documentation denied.
Architecture validation passes for 56 packages and 211 dependency edges.
The dependency graph and Cargo lock files are unchanged. Disk space stays above
the authorized 5 GiB cleanup threshold.

Subsequent review corrections, their Red/Green observations and final regression
results are recorded in [verification-11.1-11.3-review-fixes.md](verification-11.1-11.3-review-fixes.md).
