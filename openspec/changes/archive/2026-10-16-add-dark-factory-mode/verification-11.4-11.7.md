# Protected development classification, review and deliverables

## Scope and boundaries

Tasks 11.4–11.7 extend the supplied configuration and existing technical
capabilities. No runtime dispatch depends on a logical node or outcome name.
New names, five classification choices, quality criteria, repository paths,
required categories, transitions, correction-cycle boundaries and repeat ceilings
are operator configuration. The generic mapping language gained bounded explicit
array construction and presence-based selection; neither mechanism grants inputs.

The owner contracts run accepted small Work and accepted specifications through
protected authoring, implementation, exact candidate/test validation and independent
review. A fresh owner/input preparer is used on each observation. Tests cover
rework from the immediately preceding code candidate, test correction, a new
requirements cycle, finite negative dispositions, bounded exhaustion and ordinary
validation retry/cancellation. PostgreSQL reopens the store over the same journal.

Owner tests use deterministic external Build/publication ports. Actual trusted
capture, protected materialization, Artifact identity checks and signed-ready
validation acceptance are exercised through their public contracts; real Git
capture/rematerialization independently recomputes candidate blob hashes. This
qualification does not exercise live model providers or production composition:
those remain tasks 16.1/16.4. No implementation transcript or prior model session
supplies execution state or authority.

Implementation and review use distinct pinned profile/model/task identities.
The review Context contains only task and exact candidate bundle/manifest;
classifier Context contains only the task and its closed bounded diagnostic input.
Existing Evaluate compiler contracts require read-only candidate access and reject
coding/delivery credentials. Model reports never choose an undeclared edge or
substitute for independent input/report/candidate/freshness evidence.

A deliverable contract freezes verified acceptance, selected parent, complete input
and applicable operator policy. The current delta is checked against writable
scope; mandatory deliverables are checked against cumulative changes from the
immutable Work source revision. A rework therefore retains earlier documentation,
tests and scripts without requiring those files to change again. Deleted or
reverted additions cannot satisfy mandatory updates. Capture rejects missing
mandatory updates, forbidden/out-of-scope current edits and absent non-deletion
content hashes. Validation requires exact retained capture proof,
matching policy/acceptance and selected private test identities, and rejects exact
protected-file copies even on permitted public paths. The validation input digest
includes the deliverable contract. Hash comparison detects complete file copies;
broader disclosure prevention relies on protected input/credential/Artifact isolation.

Named deliverable policies resolve from a bounded local catalogue before publication;
published profiles contain the concrete immutable policy. Templates may narrow an
installed policy, never broaden its paths, remove exclusions
or weaken required classes. Explicit allowlists can use an empty exclusion list.
Scripts/documentation are applicable policy choices, not engine requirements; the
supplied example demonstrates all three public deliverable categories. No
compatibility data migration is needed for this development-only schema change.

## TDD evidence

Commands below use `CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0` and
arguments after `cargo test`. Logs are retained locally under `/tmp`. Required API
failures were observed before implementation; fixture/compiler mistakes and
environment failures are not counted as Red. Extra owner/negative tests characterize
and integrate already-tested behavior rather than backfilling untested production code.

| Public boundary / command | Observed Red | Green evidence |
| --- | --- | --- |
| `-p octacity-server-factory --lib configured_validation_classification` | Classification gate absent; explicit Context binding absent | Five finite routes, exact independent proof and closed projection pass |
| `-p octacity-server-factory --lib flow_mapping_tests` | Conditional and array mapping variants absent | Explicit selections, null/absence semantics and failed selection rejection pass |
| `-p octacity-server-factory --lib configured_implementation_handoff` | Normalized entry absent; verified acceptance field not accepted | Small Work/rework parent and acceptance identity preserved without private grants |
| `-p octacity-server-factory --lib execution_feedback_cannot_replace` | Rework execution revision rejected against original spec revision | Exact selected execution lineage accepted; specification replacement still rejected |
| `-p octacity-server-factory --lib configured_code_review` | Independent review gate absent | Exact candidate/input/report/freshness and finite quality dispositions pass |
| `-p octacity-server-factory --lib immutable_candidate_deliverables` | Required deliverable contract APIs absent | Required classes, deletions, forbidden paths, copied private content and immutable restoration pass |
| `-p octacity-server-factory --lib capture_policy_is_frozen` | Closed binding rejected the new immutable parameter | Profile round-trip preserves policy |
| `-p octacity-changeset --lib creates_reproducible_hook_free` | Captured non-deletion file hashes absent | Actual Git blob hashes recorded; full real Git suite and tampered-hash rematerialization pass |
| `-p octacity-server-application --lib trusted_capture_rejects_missing_deliverables` | Successful execution accepted empty required changes | Trusted capture rejects the candidate |
| `-p octacity-server-application --lib validation_requires_captured` | Qualified capture-proof APIs absent | Exact proof required; copied protected source rejected; ordinary validator acceptance and replay checks pass |
| `-p octacity-server-factory --lib configured_validation_requires` | Base validator had no deliverable policy | Same applicable policy and explicit acceptance projection required |
| `-p octacity-server-application --lib ordinary_build_receives` | Build causality lacked the required public contract API | Actual ordinary dispatch carries frozen acceptance, selected subject and policy without protected grants |
| `-p octacity-server-factory --lib capture_dispatch_requires` | Missing acceptance projection allowed a Build intent | Rejected before external dispatch |
| `-p octacity-server-factory --lib template_deliverables_cannot` | Template broadened the installed path ceiling | Broadened paths and removed mandatory category rejected |
| `-p octacity-server-factory --lib an_allowlisted_deliverable` | Empty exclusions unnecessarily rejected a valid allowlist | Allowlist accepted while outside paths remain rejected |

The owner integration tests `validation_rework_retains`,
`validation_classification_owner`, `classified_requirements_correction` and
`late_validation_` verify restart, correction, exhaustion, cancellation and exact
retry inputs through the existing public owner/control/store boundaries. Technical
retry retains payload, Context and source records but creates a new execution
generation/operation. Cancellation appends history without inventing validation or
review success. Test observations do not depend on returned row order.

Documentation/formatting edits have no executable behavior; Rustdoc, OpenSpec,
formatting and whitespace checks validate them.

## Initial implementation checks

These checks preceded the review fixes below. Cargo commands use the reduced
debug environment above.

| Boundary / command | Observed result | Log under `/tmp` |
| --- | --- | --- |
| `cargo test -p octacity-server-factory -p octacity-server-store -p octacity-protocol --lib` | Factory 221, store 68, protocol 62 passed | `octacity-11-core-store-protocol-final.log` |
| `cargo test -p octacity-server-application --lib` | 196 passed before the final public dispatch-contract field | `octacity-11-app-complete-final.log` |
| `cargo test -p octacity-server-application ordinary_build_receives --lib` | New ordinary dispatch contract test passed after its required API Red | `octacity-11.6-task-contract-green.log` |
| `cargo test -p octacity-server-application --lib -- --skip factory_requirements_tests` | Final 190 passed; seven already-green requirements/owner matrices excluded | `octacity-11-app-final-after-dispatch.log` |
| `cargo test -p octacity-changeset --lib` | 13 actual Git tests passed, including tampered content digest | `octacity-11-agent-final.log` |
| `cargo test -p octacity-server-factory -p octacity-server-application -p octacity-changeset -p octacity-protocol --doc` | Four compile-fail/documentation contracts passed | `octacity-11-doctests.log` |
| `cargo test -p octacity-server --test factory_configured_flow configured_manual_intake_and_unknown_node -- --ignored` | PostgreSQL owner/context/stored plan test passed | `octacity-11-pg-owner.log` |
| `cargo test -p octacity-server --test factory_configured_flow specification_publication_and_correction -- --ignored` | PostgreSQL publication/correction/recovery matrix passed | `octacity-11-pg-correction.log` |

PostgreSQL checks used `OCTACITY_POSTGRES_URL` against a task-owned disposable
PostgreSQL 18.6 service. That service was removed after successful verification.

```sh
cargo clippy -p octacity-server-factory -p octacity-server-application -p octacity-protocol -p octacity-changeset --all-targets --all-features -- -D warnings
RUSTDOCFLAGS='-D warnings -D missing_docs' cargo doc -p octacity-server-factory -p octacity-server-application -p octacity-protocol -p octacity-changeset --all-features --no-deps
cargo fmt --all -- --check
git diff --check
python3 tools/check_architecture.py
openspec validate add-dark-factory-mode --strict
```

## Review fixes and TDD evidence

All four review findings were corrected:

- Mandatory deliverables use the trusted cumulative Work index, while writable
  scope remains a property of the current delta. Both indices are independently
  recomputed during materialization, including path status, mode and content hash.
  The supplied capture schema accepts the additional bounded index and reuses a
  single path schema.
- Test correction receives the bounded classifier reason and exact candidate and
  validation identities through explicit mappings. Implementation receives only
  its selected accepted input and public handoff; the correction reason stays in
  the author input.
- Implementation and validation select one named local policy. Resolution occurs
  before immutable publication; unknown references, extra reference fields and
  weakening of the installed policy fail closed.
- Capture hashes each current blob during its existing policy inspection and
  reuses that observation. It reads only additional inherited blobs for the
  cumulative index. Independent rematerialization retains its own hash check.

The following commands use the same reduced debug environment. Each behavior
change had its own observed Red before implementation. The read-reuse refactor
used the existing 13 real Git tests before and after, with both runs passing.

| `cargo test` arguments / boundary | Observed Red | Observed Green / log under `/tmp` |
| --- | --- | --- |
| `-p octacity-changeset --lib rework_retains_cumulative` | Real rework capture lacked the original Work index | Source-only rework retains prior updates; a deleted addition disappears: `octacity-11-fixes-cumulative-green.log` |
| `-p octacity-changeset --lib creates_reproducible_hook_free` | Materialization accepted a tampered cumulative digest | Both indices independently verified: `octacity-11-fixes-agent-green.log` |
| `-p octacity-server-factory --lib immutable_candidate_deliverables` | Required two-index public contract API absent | Earlier mandatory updates accepted; missing required updates, forbidden current edits and protected copies rejected: `octacity-11-fixes-deliverables-green.log` |
| `-p octacity-server-application --lib trusted_rework_preserves_cumulative` | Valid source-only rework rejected as invalid evidence | Trusted original-base proof accepted; removed deliverable and foreign base rejected: `octacity-11-fixes-trusted-rework-green.log` |
| `-p octacity-server-application --lib validation_classification_owner` | Corrected author input contained no defect reason | Finite owner routes carry exact author correction without leaking it to implementation: `octacity-11-fixes-test-correction-green.log` |
| `-p octacity-server-factory --lib named_deliverable_policy_is_resolved` | Catalogue unsupported; subsequently extra reference fields accepted | Shared resolution, immutable prior publication, missing/invalid references and installed ceiling checked: `octacity-11-fixes-shared-policy-green.log`, `octacity-11-fixes-policy-closed-green.log` |
| `-p octacity-server-factory --lib supplied_capture_schema_accepts` | Supplied primary-output contract rejected the real cumulative index | Updated contract passes: `octacity-11-fixes-core-final.log` |

## Checks after review fixes

| Command | Observed result | Log under `/tmp` |
| --- | --- | --- |
| `cargo test -p octacity-server-factory --lib` | 223 passed | `octacity-11-fixes-core-final.log` |
| `cargo test -p octacity-server-application --lib factory_requirements_tests` | Seven passed after the final capture-schema update | `octacity-11-fixes-requirements-final.log` |
| `cargo test -p octacity-server-application --lib -- --skip factory_requirements_tests` | Remaining 191 passed; together with the preceding command, all 198 application tests cover the final state | `octacity-11-fixes-app-final.log` |
| `cargo test -p octacity-protocol -p octacity-changeset --lib` | Protocol 62 and real Git capture/materialization 14 passed | `octacity-11-fixes-wire-agent-regression.log` |
| `cargo test -p octacity-server-store --lib` | 68 passed | `octacity-11-fixes-store-regression.log` |
| `cargo test -p octacity-server-factory -p octacity-server-application -p octacity-changeset -p octacity-protocol --doc` | Four documentation contracts passed | `octacity-11-fixes-doctests.log` |

The Clippy and Rustdoc commands above were repeated after the Rust changes and
passed with warnings/missing documentation denied. Formatting, whitespace,
architecture and strict OpenSpec checks also pass. Architecture validation reports
56 packages and 211 dependency edges; dependencies and lock files are unchanged.
Independent Standards and Spec reviews found no further concrete defect in these
fixes. PostgreSQL checks in the initial table were not repeated for the review
fixes; final owner/recovery regressions use the deterministic public store ports.
Free disk space remains about 11 GiB, above the authorized 5 GiB cleanup threshold.
No commit was created.
