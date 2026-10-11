# Task 10.3: independent configured specification review

Scope: the supplied authoring route enters an independently configured reasoning
Build followed by a deterministic quality gate. Reviewer contexts and profiles
use the common node/Build contracts. No business-stage type, phase-name dispatch,
new execution path, commit, staging or push was added.

## Behavior and configuration

- The review input explicitly selects the exact candidate revision, manifest and
  bundle references/digests, original Work/base identities and authoring
  acceptance/regression criteria. The original acceptance Artifact is selected
  directly from immutable Work rather than trusting the author to replace it.
- A generic `FlowContextBinding` pins the selected revision and a closed map of
  exact source categories and immutable references. Missing, extra or substituted
  sources fail closed at input publication, Build intent creation and restoration.
  The trusted collector can resolve `selected_sources` and `candidate_revision`
  without interpreting node names. Content access, byte verification and receipt
  validation remain separate trusted boundaries.
- Published reviewer skill/configuration documents can be required exact policy
  entries. The dispatch contract test adds a reviewer skill Artifact and pins
  distinct reviewer task/model parameters. Context and profile remain identical
  on intent restoration. Neither Work's task Artifact nor author transcript is
  implicitly forwarded.
- The supplied quality schema declares completeness, consistency, feasibility,
  testability and scope, plus bounded typed findings and remediation. These
  criteria are configuration choices. The generic gate checks exact Work, base,
  candidate, input, manifest/bundle and review-byte identities, independently
  accepted verification facts and trusted publication/freshness deadlines.
- Outcomes are `publication_pending`, `changes_required`, `needs_human_input` and
  `invalid`. No review outcome creates Accepted Work or dispatches implementation.
  An optional human-gate contract inserts an arbitrarily named confirmation node
  after successful review without changing the interpreter.

Publication instructions are in `config/factory/flows/README.md`. Qualified
execution must select read-only source access, separate writable scratch/output
roots, the exact reviewer profile and its complete closed context, and the
required independently verified `specification_review_facts` Report. Supplied
permissions continue to default to deny-all. Review does not obtain delivery
credentials or readiness authority.

## TDD evidence

All Cargo commands use `CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0`.
Commands below are arguments after `cargo test`; logs are under `/tmp`.

| Public boundary / command | Observed Red before implementation | Green evidence |
| --- | --- | --- |
| `-p octacity-server-factory a_node_context_contract_excludes_undeclared_sources_and_substituted_candidates --lib` | Node wire rejected `context_binding` | `octacity-10.3-context-green.log`; exact context passes, transcript/extra sources, substituted skill bytes, missing sources and another candidate fail; frozen input restores |
| Same boundary, collector selection slice | The public declarative source/revision selection APIs were absent | `octacity-10.3-context-selection-green.log`; immutable skill reference and selected revision resolve through the common contract |
| `-p octacity-server-factory a_context_contract_requires_a_typed_input_at_publication --lib` | A definition accepted a context mapping with no input schema | `octacity-10.3-context-publication-green.log`; both construction and definition revalidation require typed input |
| `-p octacity-server-factory configured_specification_review_checks_quality_and_exact_candidate_without_granting_readiness --lib` | Supplied review gate was absent | `octacity-10.3-quality-green.log`; satisfied, violated and indeterminate criteria route distinctly; substituted identities, stale outputs and invalid verification fail closed |
| `-p octacity-server-application configured_specification_review_dispatches_only_its_frozen_candidate_and_reviewer_context --lib`, original acceptance slice | Reviewer input schema rejected the required original acceptance Artifact | `octacity-10.3-original-acceptance-green.log`; dispatch includes it with exact candidate references, reviewer skill and frozen parameters |

Red logs use the corresponding `*-red.log` names. Configuration editing remained
test-first; intermediate schema/enum/transition syntax mistakes were repaired
before Green and are not counted as additional feature Reds.

The unchanged ordinary Build adapter was characterized through its public
dispatch, completion and restoration boundary
(`octacity-10.3-review-dispatch.log`). The existing human primitive was composed
through template publication (`octacity-10.3-optional-human.log`); neither needed
new execution logic. JSON whitespace compaction and prose documentation updates
have no executable behavior requiring separate Red/Green cycles.

## Final verification

| Suite | Result | Log under `/tmp` |
| --- | --- | --- |
| Factory/application/store libraries | 179 + 169 + 68 = 416 passed | `octacity-10.3-final-libraries.log` |
| Ordinary configured-flow contracts | 15 passed; external-service cases run separately | `octacity-10.3-configured-flow.log` |
| PostgreSQL configured owner, restart, human response and Build admission contracts | 25 passed | `octacity-10.3-postgres.log` |
| Application/Factory doctests | 4 passed | `octacity-10.3-doctests.log` |

These final suites total **460 passing tests**, without failures.

Commands:

```sh
cargo test -p octacity-server-factory -p octacity-server-application -p octacity-server-store --lib
cargo test -p octacity-server --test factory_configured_flow
cargo test -p octacity-server --test factory_configured_flow -- --ignored --skip production_factory_owner_uses_installed_bindings_and_recovers_the_same_arbitrary_node
cargo test -p octacity-server-application -p octacity-server-factory --doc
cargo clippy -p octacity-server-factory -p octacity-server-application -p octacity-server-store -p octacity-server --all-targets --all-features -- -D warnings
RUSTDOCFLAGS='-D missing_docs' cargo doc --no-deps --all-features -p octacity-server-factory -p octacity-server-application -p octacity-server-store -p octacity-server
python3 tools/check_architecture.py
openspec validate add-dark-factory-mode --strict
cargo fmt --all --check
git diff --check
```

Clippy passed with warnings denied for all four selected packages and
targets/features (`octacity-10.3-clippy.log`). Public API documentation passed with
missing documentation denied (`octacity-10.3-doc.log`). Architecture validation passed for
56 packages and 211 edges. PostgreSQL tests used a disposable PostgreSQL 18
service through `OCTACITY_POSTGRES_URL`; the unchanged PostgreSQL/MinIO production
composition test was excluded.

This slice verifies the configured policies and public contracts, including an
actual ordinary Build dispatch/retained completion at its external test port.
It does not claim a live model call or new real-backend qualification. Trusted
specification publication/readiness remains task 10.4, full production capability
composition remains 16.1, and real-provider qualification remains 16.4. The common
candidate materializer continues to verify the full ChangeSet subject and retained
capture artifacts before source execution.

Final formatting, whitespace and strict OpenSpec validation passed. HEAD remains
`5fd87eb` and the Git index is untouched. Free space remained above 5 GiB
(approximately 7.7 GiB after validation), so compilation cleanup was unnecessary.
Only the task-owned disposable PostgreSQL container was stopped and removed.
