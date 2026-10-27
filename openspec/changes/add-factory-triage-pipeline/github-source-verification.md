# Task 2.2: GitHub and managed Source qualification

## Delivered behavior

The existing GitHub `IssueSelector` now accepts optional `label_presence: any|none`.
Both ordinary scan and preview use its existing checked matching path, including
additional selectors in a group. `none` requires the complete observed label array
to be empty; nonempty required/alternative labels are rejected with a safe
`LabelPresence (Contradictory)` diagnostic. Existing literal-label normalization
and provider state semantics are unchanged. Omitted and explicit `any` retain the
legacy serialized selector representation.

The Work Source protocol adds the optional `source.label_presence` capability,
defaulting to false and omitted when false, and the bounded `labels_present`
preview reason. Source profile qualification requires both this capability on
the exact verified adapter and explicit `predicate_label_presence` on the reader,
under its existing unrestricted-base editing contract. Neither Reporter support
nor state editing alone qualifies the new feature.

Normal Source composition passes this qualification into the common authoring
handler. Enabled Project Issue imports also check it before publication; an old
profile cannot silently accept a strict predicate. Failure uses the existing
HTTP 400 field-validation contract and `/predicate/label_presence` diagnostics.
A rejected import leaves its original ID available for the ordinary valid first
publication.

Fresh stand installation publishes qualified `github-issues` reader version `v2`
and a matching adapter manifest. Retained installation bundles remain byte-for-byte
unchanged; no old profile, connection, input or history is silently upgraded.
The new Factory editing qualification is deliberately excluded from independent
task observation identity. Public HTTP tests retain the original task-reader pin
when Factory preparation and this qualification are added.

## Behavioral evidence and TDD

All checks were run on 2026-10-27 with `CARGO_PROFILE_DEV_DEBUG=0
CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2` for Rust builds.
Pre-change `cargo test -p octacity-github --tests -j2` passed 125 tests. The
uncommitted changes from tasks 1.1–2.1 were preserved.

| Public boundary / command | Observed Red | Observed Green |
| --- | --- | --- |
| `cargo test -p octacity-github --test issue_preview strictly_unlabeled -j2` | Strict native selector produced a permanent configuration failure | Scan/preview agree for single and grouped selectors, exclude PRs and future updates, preserve initial no-`since` behavior and default selector bytes |
| `cargo test -p octacity-github --test issue_selectors strict_unlabeled -j2` | A contradictory required-label predicate was accepted | Contradictions fail without echoing label values; bounded exclusions remain valid |
| The native scan/preview test extended to the verified process boundary | Manifest decoding rejected the missing new capability field | The actual locally built `octacity-github` executable runs through the shared verified process host, with its content digest checked, against a hermetic HTTPS GitHub peer; all four scan/preview/group cases pass (see architecture correction below) |
| `cargo test -p octacity-server --lib offline_label_presence -j2` | A qualified reader declaration was rejected | Public offline validation accepts a qualified adapter/reader pair, rejects an older adapter or missing unrestricted-base contract, and preserves legacy state-only profiles without executing a provider |
| `cargo test -p octacity-server --lib runtime::integration_management_tests::runtime_publishes_connection_credentials_and_project_access_without_enabling_inputs -- --ignored --exact --nocapture` with the unsupported-import extension | Old code returned HTTP 200 instead of the existing field-validation rejection | Unsupported import returns HTTP 400 with the exact field diagnostic; subsequent legacy publication succeeds |
| `python3 -m unittest tools.tests.test_factory_source_installation.FactorySourceInstallationTests.test_factory_preparation_is_bounded_and_does_not_pin_a_project_repository_or_target` | Fresh profile lacked qualification metadata | Fresh reader `v2` and matching manifest advertise support; the full installation suite passes |
| The normal HTTP journey extended to qualified Factory input/preview | Choice metadata was null instead of `["any", "none"]` | Trusted reader qualification is exposed and enforced by normal Source composition |

The import test initially assumed HTTP 422. Inspection of the established REST
field-validation contract corrected that expectation to 400 before the final
Red/Green cycle: without the new check the corrected test still failed with 200,
then passed with the exact diagnostic after the check was restored. The earlier
wrong expectation is not counted as a successful test result.

The qualified normal-runtime HTTP journey uses real disposable PostgreSQL/MinIO
and a verified hermetic provider peer. It proves strict negative checkpointing,
four original Work/Run admissions from two independent inputs, recovery after a
lost admission checkpoint, empty-filtered and ambiguous exclusive groups, and
read-only preview explanations. After adding labels to an enabled admitted input,
the next selection becomes empty while the original admission records remain
identical. Independent retained Issue pull still refreshes title/description
outside the Factory predicate and after input disablement, preserving acceptance,
business status and Run history without outbound effects.

`cargo test -p octacity-github --test issue_listing unlabeled_continuations -j2`
also passes: a completely filtered labeled page retains its continuation; the
next page honors the frozen upper bound, while the existing window regressions
retain coverage of both inclusive boundaries. Replaying that cursor returns the same page. Changing the window,
primary selector or group rejects the cursor before any provider call. The
incremental request retains its existing one-second `since` overlap. No new
pagination, identity or replay implementation was needed.

## Final regressions

| Command | Result |
| --- | --- |
| `cargo test -p octacity-github --tests` | 128 passed, none ignored; includes actual executable/verified-host HTTPS tests |
| `cargo test -p octacity-work-source-protocol -p octacity-server-work-source --tests` | 32 passed, none ignored |
| `cargo test -p octacity-server-application --lib factory_admission_tests::source_configuration` | 31 passed |
| `cargo test -p octacity-server-application --lib issue_tests::source` | 37 passed |
| `cargo test -p octacity-server-api-rest --tests` | 249 passed |
| `cargo test -p octacity-server --lib -- --include-ignored --test-threads=2` | 261 passed, 0 failed, 0 ignored; real disposable PostgreSQL/MinIO enabled all environment-gated library contracts |
| `python3 -m unittest tools.tests.test_factory_source_installation tools.tests.test_factory_local_stand tools.tests.test_local_stand_launcher` | 58 collected: 55 passed, 3 explicit stand pilot/bootstrap gates skipped |
| `cargo clippy -p octacity-github -p octacity-work-source-protocol -p octacity-server-work-source -p octacity-server-application -p octacity-server -p octacity-server-api-rest --lib --tests -j2 -- -D warnings` | Passed without suppressions |

`rustfmt --check`, `git diff --check`, local Markdown target validation and strict
OpenSpec validation pass. Logs are retained under `/tmp/octacity-triage-2-2-*.log`.
The existing macOS compact-unwind linker warning remains unchanged and does not
prevent the successful test runs.

## Review, resources and remaining scope

Matching stays in the existing GitHub selector and common Factory predicate;
there is no triage-specific interpreter, new journal, pagination owner, automatic
marker label or credential authority. The test-only dependency on the existing
verified host reuses its real executable validation rather than adding another
process launcher; it does not add a production GitHub-plugin dependency. New
public fields and compatibility behavior are documented at their definitions.

OpenAPI/generated-client and Console exposure of the selector fields, qualification
choices and mismatch vocabulary remain task 2.3. These backend HTTP tests do not
claim that the Console can already edit the new mode. Raw-ticket completion
preparation and real two-agent execution remain later tasks; existing acceptance
requirements are not weakened. No paid model, live GitHub request, real token or
personal stand modification was used.

Free disk space was checked before and throughout builds: about 16 GiB initially
and at least 15 GiB during checks. The 10 GB cleanup threshold was not crossed.
Build concurrency stayed at two with debug/incremental artifacts disabled. Only
task-owned PostgreSQL/MinIO containers and anonymous volumes were removed after
successful verification; tagged images, personal stand data and other changes
were preserved. No commit was created.


## Architecture correction: independent adapter test dependencies

A later CI gate caught an invalid dev-dependency from the GitHub provider to the
concrete Work Source host. `python3 tools/check_architecture.py` and
`python3 -m unittest tools.tests.test_check_architecture.ArchitecturePolicyTests.test_current_workspace_satisfies_the_policy`
both failed with `ARCH003_ADAPTER_SELECTION` before the correction. The existing
native strict-selection behavioral test passed before this structural refactor.

The provider test now depends only on the explicitly shared
`octacity-server-adapter-host` support package. It uses `verify_installed_program`
and `execute_process`, keeping executable digest verification, bounded process
execution, response correlation/validation and the same native scan/preview/group
assertions. It no longer constructs a duplicate registry directory and synthetic
manifest. Work Source-specific registry/capability qualification stays with the
Work Source host and server composition; this native test does not claim to
exercise that concrete registry. Production behavior and architecture policy are
unchanged, and no dependency exception was added.

Verification on 2026-10-27, with debug/incremental build artifacts disabled and two
Cargo build jobs:

- The architecture CLI passed: 63 packages and 261 dependency edges.
- `python3 -m unittest tools.tests.test_check_architecture`: all 33 passed,
  including the previously failing real workspace graph test (Red → Green).
- `cargo test -p octacity-github --test issue_preview strictly_unlabeled -j2`:
  the unchanged behavioral assertions passed after the refactor.
- `cargo test --locked -p octacity-github -p octacity-server-adapter-host
  -p octacity-server-work-source -j2`: all 148 tests passed, none ignored.
- `cargo clippy --locked -p octacity-github --all-targets -j2 -- -D warnings`,
  workspace format check, OpenSpec strict validation and `git diff --check` passed.

Free disk space stayed about 14–15 GiB, above the 10 GB cleanup threshold.
No live GitHub request, personal stand change or commit was made for this fix.
