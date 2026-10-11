# Tasks 10.4–10.6: configured publication, correction and requirement paths

The subsequent review corrections and their final verification are recorded in
[verification-10.4-10.6-review-fixes.md](verification-10.4-10.6-review-fixes.md).
The results below describe the implementation before those corrections.

The supplied journey publishes the exact independently reviewed specification
through an installed trusted action, then freezes readiness through a separate
deterministic gate. Implementation and verification can return a qualified typed
Requirements Defect to a fresh append-only cycle. Business node keys, routes,
requirement categories and policy conditions remain in configuration.

## Boundaries and guarantees

- `FactoryCandidatePublicationAction` uses the common action executor. Its port
  carries the frozen operation, Work, cycle, complete candidate, manifest/bundle,
  target, permissions, budget and deadline. Observation precedes publication;
  another operation/candidate cannot satisfy its receipt. Schema validation
  precedes effects, and accepted review validity narrows the attempt deadline.
- Connector implementations must verify immutable bytes and exact ancestry,
  deduplicate the complete intent and reject conflicts without rebasing. Live
  forge delivery and production provider qualification remain separate tasks.
  Stateful test ports exercise unknown responses and ancestry conflicts; these
  tests do not claim a new GitHub integration or model/backend qualification.
- Generic bounded `starts_cycle` transitions retain their causal attempt, close
  the previous cycle and derive an arbitrary new entry from the admitted graph.
  A restart outcome cannot simultaneously route another fanout branch. Exact
  trigger data crosses only an explicitly selected predecessor binding; ordinary
  records remain scoped to the new cycle. Old cycles and nested descendants lose
  execution/control/append authority while their history remains readable.
- Both stores validate the same journal. PostgreSQL persists the cycle's source
  attempt and reconstructs the same causal entry. Queue readiness uses the common
  interpreter and supports a fresh cycle starting at a non-root queued node.
- The supplied correction author uses the previously accepted specification
  candidate as its exact execution predecessor. Every capture, review, publication,
  readiness and downstream gate runs again. Existing Accepted Work digests are
  stable; previous evidence cannot accept the corrected cycle.
- Closed Context artifact collections select every declared immutable requirement
  from Work for author and reviewer. Structural comparison preserves the exact
  references; separately qualified facts establish preservation in the candidate.
  Feature, defect, researched and small specified Work share review/readiness.
- Local schema references remove repeated configuration shapes. Publication
  expands them into concrete immutable contracts under existing depth/size and
  bounded expansion ceilings. Missing and recursive choices are rejected.

The Factory schema is unreleased and contains development/test data only, as
confirmed by the user. Its existing migration is updated directly; no legacy
cycle fallback or historical stage compatibility path is introduced.

## TDD evidence

Every Cargo command below uses
`CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0`. Entries are arguments after
`cargo test`; logs live under `/tmp`. Environment errors and incorrect fixture
shapes are not counted as Red. Each behavioral correction was tested at the
existing public Flow, owner/store or action boundary before implementation.

| Command / boundary | Observed Red | Green log |
| --- | --- | --- |
| `-p octacity-server-application exact_candidate_publication_recovers --lib` | Publication action/port API absent | `octacity-10.4-publication-green.log` |
| `-p octacity-server-application publication_rejects_an_incompatible --lib` | Connector write occurred before receipt-schema rejection | `octacity-10.4-contract-green.log` |
| `-p octacity-server-application lost_publication_cannot --lib` | Review-validity action parameter unsupported | `octacity-10.4-expiry-green.log` |
| `-p octacity-server-application a_receipt_for_another --lib` | Receipt lacked operation identity | `octacity-10.4-receipt-green.log` |
| `-p octacity-server-factory supplied_publication_requires --lib` | Configured publication/readiness node absent | `octacity-10.4-readiness-green.log` |
| `-p octacity-server-factory a_declared_bounded_restart --lib` | Restart directive API absent | `octacity-10.5-cycle-green.log` |
| `-p octacity-server-factory correction_cycles_retain --lib` | Causal cycle APIs absent; historical edge could authorize the wrong fresh entry | `octacity-10.5-entry-green.log` |
| Same public history/entry contract | A stop in a later cycle invalidated restoration of an earlier boundary | `octacity-10.5-causal-stop-green.log` |
| `-p octacity-server-application configured_correction_recovers --lib` | Owner did not append a cycle; next slice lacked exact trigger input; old completed node remained executable; store admitted an old-cycle append | `octacity-10.5-owner-green.log`, `octacity-10.5-authority-green.log`, `octacity-10.5-store-authority-green.log` |
| `-p octacity-server --test factory_configured_flow declared_correction_cycles -- --ignored` | Persisted source-attempt identity was absent on reconstruction | `octacity-10.5-postgres-green.log` |
| `-p octacity-server-application a_correction_cycle_can_publish --lib` | Non-root fresh-cycle entry was rejected by its configured pool | `octacity-10.5-cycle-pool-green.log` |
| `-p octacity-server-factory a_cycle_boundary_cannot_share --lib` | Admission accepted simultaneous restart and fanout for one outcome | `octacity-10.5-exclusive-cycle-green.log` |
| `-p octacity-server-factory configured_exact_value_comparison --lib` | Structured exact comparison unsupported | `octacity-10.6-preservation-green.log` |
| `-p octacity-server-factory configured_specification_handoff --lib` | Declared-requirements handoff contract absent | `octacity-10.6-quality-green.log` |
| `-p octacity-server-factory configured_correction_author --lib` | Configured prior-candidate selection absent | `octacity-10.6-predecessor-green.log` |
| `-p octacity-server-factory corrected_specification_must --lib` | Exact execution-predecessor proof absent | `octacity-10.6-correction-capture-green.log` |
| `-p octacity-server-factory declared_artifact_collections --lib` | Closed artifact-collection selection unsupported | `octacity-10.6-context-list-green.log` |
| `-p octacity-server-factory supplied_reviewer_selects --lib` | Reviewer contract rejected declared requirement references | `octacity-10.6-review-docs-green.log` |
| `-p octacity-server-factory named_nested_schema_choices --lib` | Template schema references unsupported | `octacity-10-schema-reuse-green.log` |

Test fixture extraction/schema expansion refactoring followed green baselines
(`octacity-10.4-fixture-baseline.log`, `octacity-10-schema-reuse-baseline.log`,
`octacity-10-requirements-fixture-baseline.log`). Configuration
formatting and documentation updates have no separate executable TDD behavior.

The complete owner contracts additionally verify all four Work paths, unknown
publication response, ancestry conflict, expired review, human refusal, and
corrections originating from both implementation and verification. Their public
qualified-output ports retain exact ordinary Build/Attempt/Job provenance.
PostgreSQL contracts share these same owner assertions and seed ordinary Build
rows through the existing external test fixture. Real capture tests remain in
the application regression suite.

## Verification commands

```sh
cargo test -p octacity-server-factory -p octacity-server-store -p octacity-server-application --lib
cargo test -p octacity-server --test factory_configured_flow -- --skip requirements_contract
cargo test -p octacity-server --test factory_configured_flow specification_publication_and_correction -- --ignored
cargo test -p octacity-server --test factory_configured_flow -- --ignored --skip production_factory_owner_uses_installed_bindings_and_recovers_the_same_arbitrary_node --skip specification_publication_and_correction_recover_exact_contracts_in_postgres
cargo test -p octacity-server-store-postgres --test factory_discovery -- --ignored
cargo test -p octacity-server-application -p octacity-server-factory --doc
cargo clippy -p octacity-server-factory -p octacity-server-store -p octacity-server-application -p octacity-server-store-postgres -p octacity-server --all-targets --all-features -- -D warnings
RUSTDOCFLAGS='-D missing_docs' cargo doc --no-deps --all-features -p octacity-server-factory -p octacity-server-application -p octacity-server-store -p octacity-server-store-postgres -p octacity-server
python3 tools/check_architecture.py
openspec validate add-dark-factory-mode --strict
cargo fmt --all -- --check
git diff --check
```

The excluded PostgreSQL/MinIO production composition case belongs to task 16.1;
this slice uses a task-owned disposable PostgreSQL 18 service with
`OCTACITY_POSTGRES_URL`. Provider ports are qualified test implementations rather
than live external services. The lock-file repair is the separate authorized
commit `0ec2140`.


## Results

| Final boundary | Passing tests | Log under `/tmp` |
| --- | ---: | --- |
| Factory core (including exclusive cycle boundary and human composition) | 195 | `octacity-10.4-6-final-core-store.log` |
| Common store | 68 | `octacity-10.4-6-final-core-store.log` |
| Application, including full qualified requirement paths | 178 | `octacity-10.4-6-regression.log` |
| Configured owner portable contracts | 17 | `octacity-10.4-6-configured-flow.log` |
| PostgreSQL configured flow regression | 26 | `octacity-10.4-6-postgres-regression.log` |
| PostgreSQL publication/correction/lost-response matrix | 1 | `octacity-10.4-6-postgres-journey.log` |
| PostgreSQL store/discovery/diagnostics | 13 | `octacity-10.4-6-postgres-store.log` |
| Application/Factory doctests | 4 | `octacity-10.4-6-doctests.log` |

These suites contain **502 passing tests**, with no failures. The three full
qualified requirement-path cases are counted once, through the application
suite; the integration test includes the same module but excludes these portable
duplicates. The owner matrix verifies full runs at its public ports, including
fresh contracts and exact predecessor selection after restart.

After fixture cleanup, the remaining 175 application tests passed again
(`octacity-10.4-6-final-application.log`); the three full requirement-path tests
retain their preceding green behavioral baseline.

Warnings-denied Clippy and missing-documentation-denied Rustdoc passed for the
five changed packages and their applicable targets/features. Architecture
validation passed for 56 packages and 211 dependency edges. Formatting,
whitespace and strict OpenSpec validation passed. Free space stayed above the
5 GiB cleanup threshold; no build-directory deletion was necessary.
