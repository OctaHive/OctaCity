# Task 2.1: common label-presence predicate

## Delivered contract

The provider-neutral `WorkSourcePredicate` accepts `label_presence: any|none`.
Omission and explicit `any` both serialize without the field, retaining the
original canonical representation. A pre-change characterization fixes the
legacy JSON and its SHA-256 (`3f3c8a781028756ccabf932d54cd4a0437d7d8c477889bc54eb9851e28c789b2`).
Three empty label lists still match both labeled and unlabeled tasks.

`none` requires the entire observed label array to be empty. It rejects nonempty
`all_of`/`any_of`, permits bounded `none_of` and reports `labels_present` for a
labeled observation. Existing state, required/alternative/excluded label matching
and validation bounds keep their semantics.

The existing authorized Source validation/publication boundary requires separate
trusted label-presence support on the exact installed source capability. State
editing support alone is insufficient: unsupported `none` produces a safe
`/inputs/0/definition/predicate/label_presence` error before publication, with no
mutation evidence. The trusted installer can explicitly qualify this feature via
`with_predicate_label_presence`; query choices then advertise `any` and `none`.
Unqualified choices omit this new metadata, preserving their previous JSON shape.

States and label-presence qualification share one existing installed-predicate
entry. No second capability registry, selector implementation, journal, storage
migration, dependency or provider-specific business constant is introduced.
Existing Rust fixture initializers explicitly select the unchanged default.

The public offline runtime validator also rejects a legacy selector presentation
that tries to assert `none`. That presentation has no qualified label-presence
transport contract and must not silently drop the restriction when creating
editable Source choices.

## Red/Green evidence

Checked on 2026-10-27 using the existing checkout and preserving the uncommitted
1.1–1.2 work. All Rust commands below use `CARGO_PROFILE_DEV_DEBUG=0
CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2`.

| Boundary / command | Observed Red | Observed Green |
| --- | --- | --- |
| Pre-change `cargo test -p octacity-server-factory --lib input_predicate -j2` | Characterization of existing behavior; no new behavior to fail | Original predicate test and new exact-byte/digest characterization: 2 passed before implementation |
| `cargo test -p octacity-server-factory --lib strict_unlabeled_input_predicate -j2` | JSON rejected as unknown `label_presence` | Strict unlabeled matching, explicit default serialization and legacy predicate checks pass after adding the common enum and matching rule |
| `cargo test -p octacity-server-factory --lib strict_unlabeled_input_predicate_rejects_required -j2` | Contradictory required labels were accepted | `none` rejects required/alternative labels while preserving exclusions; focused predicate suite: 4 passed |
| `cargo test -p octacity-server-application --lib strict_unlabeled_inputs_require -j2` | Validation unexpectedly accepted `none` under a states-only legacy profile | Unsupported validation and publication reject the field without mutations: 1 passed |
| `cargo test -p octacity-server-application --lib advertised_label_presence -j2` | Required installer method and typed choice field were absent | Qualified publication, exact original Source/Configuration/Flow pins, immutable old bytes and retained version reads: 1 passed |
| `cargo test -p octacity-server --lib offline_predicate_editing_requires -j2` | Offline validation accepted unsupported `none` in selector presentation | Public runtime-file validator rejects that declaration: 1 passed |

The strengthened unchanged-label regression also independently checks missing
alternatives and present exclusions by their exact mismatch reasons. Tests use
public core values, authorized application queries/commands and the public
offline validator; no provider call or private database inspection is needed for
the new behavioral tests.

## Regression results

| Command | Result |
| --- | --- |
| `cargo test -p octacity-server-factory --lib` | 303 passed, none ignored |
| `cargo test -p octacity-server-factory --lib input_predicate -j2` after the final legacy assertion strengthening | 4 passed |
| `cargo test -p octacity-server-application --lib factory_admission_tests::source_configuration` | 31 passed, including examples, preview, reporting authority and both new publication contracts |
| `cargo test -p octacity-server-application --lib issue_tests::source` | 37 passed, including existing native-state mapping predicates with opaque states |
| `cargo test -p octacity-server --lib` | 213 passed; 47 existing environment-gated contracts ignored, not reported as passed |
| `cargo test -p octacity-server-api-rest --tests` | 249 passed across four targets |
| `cargo test -p octacity-server-store-postgres --test factory_discovery --test issues -- --ignored --test-threads=2` | 110 passed against a real task-owned PostgreSQL 18.6 service; verifies retained Source/Issue persistence contracts |

`cargo clippy -p octacity-server-factory -p octacity-server-application -p
octacity-server -p octacity-server-api-rest --lib --tests -j2 -- -D warnings`
passes. The standalone application test build retains its pre-existing unused
fixture warning; the combined supported test-feature Clippy target passes without
suppressions. The previously recorded macOS compact-unwind linker warning also
remains unchanged.

`rustfmt --check` for all modified Rust files, `git diff --check` and
`openspec validate add-factory-triage-pipeline --strict` pass. Final diff review
confirms one shared predicate and one installed-support owner, with no speculative
runtime layer, duplicated matching logic or credential/authority changes.

Full command output remains locally under `/tmp/octacity-triage-2-1-*.log`.
The shared pre-change Source application baseline also passed 29 tests. An early
incorrect test-name filter selected zero tests; it was corrected using the actual
listed test names before implementation, and is not counted as baseline coverage.

## Scope and resource limits

Actual GitHub listing/preview/continuation fences and installed native capability
wiring remain task 2.2. REST/OpenAPI generation and Console editing remain task
2.3. No shipped runtime profile is automatically upgraded or falsely advertised
as supporting the new predicate. Agent/VM qualification retains the limitations
recorded in [task 1.2](baseline-verification.md); it is not claimed by this task.

Disk space was checked before/during builds: about 16 GiB initially and at least
15 GiB throughout observed checks, above the 10 GB cleanup threshold. Build jobs
were limited to two with debug information and incremental output disabled. The
disposable PostgreSQL service was bound to loopback, used test-only credentials,
and its container/anonymous volume were removed after the contracts completed.
Personal stand data and tagged Docker images were preserved. No commit was made.
