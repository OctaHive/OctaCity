# Review corrections for tasks 11.1–11.3

The six findings from the uncommitted implementation review are addressed without
changing the generic engine into a business-stage dispatcher. No commit is created.

## Corrections

1. Policy and frozen Plan share `ProtectedTestCoverage`. `max_files` and the
   operator's `reserved_inputs` fit the common bounded manifest capacity together.
   The reserve includes inventory, candidate and compiler-managed documents. The
   validation compiler supplies actual managed inputs to `wire_inputs`; the complete
   manifest is checked before signing for reserve, protocol count/bytes and paths.
2. Evidence and materialization use the same retained-source deadline calculation.
3. Predecessor and named-record views share the same bounded record projection.
4. The server architecture document describes implemented protection/isolation and
   distinguishes remaining production composition and live qualification work.
5. The supplied author and implementation select the accepted specification's
   candidate and exact manifest/bundle, plus the Work task. Their closed input
   schema retains accepted data and the task reference. Generic RFC 6901 presence
   guards select optional context; absent specification uses Work base/task only.
   Present but invalid references reject input. Control edges grant no private
   author outputs. The result gate explicitly selects the accepted part of that
   input. Fixture pool projections are configured for each fixture's input shape.
6. Evidence reconstruction requires the exact facts-output requirement and rechecks
   retained validation facts. Facts bind input/report/candidate/tests/classes and
   impose their own freshness deadline. Output/source retention cannot extend it.

The coverage, presence guards and validation contracts accept arbitrary configured
keys. Logical node names occur in the supplied example and its tests, not in runtime
selection. No database migration, dependency or lock-file change is needed.

## TDD observations

Commands use `CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0`.
Log names below are under `/tmp`.

| Public behavioral boundary / command | Observed Red | Observed Green |
| --- | --- | --- |
| `cargo test -p octacity-server-application --lib validation_freezes_both_inputs` | Evidence expiry was 900 rather than the independently specified facts deadline 500 (`octacity-11-fixes-evidence-red.log`) | Evidence expires at 500 and reconstruction at 600 rejects (`octacity-11-fixes-evidence-green.log`) |
| `cargo test -p octacity-server-application --lib protected_` | Policy lacked the requested reserved-capacity API (`octacity-11-fixes-input-budget-red.log`) | 249+7 and 252+4 accepted, each next file rejected; zero reserve rejected (`octacity-11-fixes-input-budget-green.log`) |
| Same protected boundary, exact signed validation inputs | Compiler-managed inputs could not participate in checked assembly (`octacity-11-fixes-managed-budget-red.log`) | Actual candidate/inventory/managed inputs fill the reserve; an extra managed document rejects (`octacity-11-fixes-managed-budget-green.log`) |
| `cargo test -p octacity-server-factory --lib optional_context_selections` | Explicit presence-guard configuration absent (`octacity-11-fixes-context-guard-red.log`) | Exact optional source/candidate selection, collection selection, invalid pointers and missing selected sources covered (`octacity-11-fixes-context-guard-green.log`, final core suite) |
| `cargo test -p octacity-server-application --lib accepted_specification_selects` | Author/implementation Context had no accepted candidate (`octacity-11-fixes-accepted-context-red.log`) | Both select the accepted revision and exactly task/manifest/bundle (`octacity-11-fixes-accepted-context-green.log`) |

Refactoring started with passing existing behavior: 209 core tests and nine
protected application contracts. After shared record projection, 54 configured
contracts passed; shared retention passed the protected subset. The corrected
queue projection passed the existing unknown-node/pool/restart contract. Full
regression also covers the supplied bypass and correction/fanout fixtures.
Documentation/formatting-only edits have no executable behavior and use the
documentation, architecture, OpenSpec and whitespace checks below.

## Final checks

```sh
cargo test -p octacity-server-factory -p octacity-server-store --lib
cargo test -p octacity-server-application --lib
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

PostgreSQL checks use a task-owned disposable PostgreSQL 18.6 container on a
dynamic localhost port through `OCTACITY_POSTGRES_URL`. The shared owner assertions
check accepted specification contexts after restart and every correction cycle.
The container is removed after checks. Artifact/provider ports in these owner
contracts remain qualified fixtures; live production qualification is separate.

| Boundary | Final result | Log under `/tmp` |
| --- | --- | --- |
| Factory core | 210 passed | `octacity-11-fixes-core-store-final.log` |
| Common store | 68 passed | `octacity-11-fixes-core-store-final.log` |
| Full application regression | 189 passed | `octacity-11-fixes-application-all-green.log` |
| Factory/application documentation contracts | 4 passed | `octacity-11-fixes-doctests.log` |
| PostgreSQL queue/restart/private-input isolation | 1 passed | `octacity-11-fixes-postgres-isolation.log` |
| PostgreSQL publication/correction/lost-response matrix | 1 passed, three scenarios | `octacity-11-fixes-postgres-correction.log` |

Clippy passes with warnings denied (`octacity-11-fixes-clippy-final.log`). Rustdoc
passes with warnings and missing documentation denied (`octacity-11-fixes-rustdoc.log`).
Architecture validation passes for 56 packages and 211 dependency edges. Strict
OpenSpec validation, workspace formatting and whitespace checks pass. The disposable
PostgreSQL container is removed. Free disk space is 12 GiB, above the authorized
5 GiB cleanup threshold. No commit, dependency or Cargo lock-file change is made.
