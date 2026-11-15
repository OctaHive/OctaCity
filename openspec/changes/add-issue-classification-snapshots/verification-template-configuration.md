# Task 10.5: configured templates and common stand capability

## Scope

Move the example's selected fields and business vocabulary into configuration,
resolve them through ordinary Project/Flow contracts, and install one local save
capability without granting it to Agent tools or enabling intake. REST/Console
selector work remains in tasks 10.6–10.8; native UI qualification remains 10.10.

## Observed TDD slices

All Cargo commands use `CARGO_INCREMENTAL=0`, `CARGO_PROFILE_DEV_DEBUG=0` and
`CARGO_PROFILE_TEST_DEBUG=0` to bound disk usage.

- `python3 -m unittest discover -s tools/tests -p test_factory_stand_triage.py -k native_triage_installation_keeps_model_login_and_project_mcp_separate`:
  Red installed `stand.issue-classification` with triage ownership; Green installs
  the configured `stand.issue-fields` / `issue-fields` capability, retains the
  Project ceiling, leaves Agent tools unchanged, and installs no Flow or effects.
- `cargo test --locked -p octacity-server-factory --lib ordinary_template_retains_the_same_selected_issue_form -- --nocapture`:
  Red rejected `issue_fields` as unknown template outcome data; Green retains the
  same selected form on the ordinary published outcome without an apply node.
- `cargo test --locked -p octacity-server-application --lib triage_template_resolves_a_custom_project_form -- --nocapture`:
  Red lacked the selected-form parameter API; Green resolves an ordinary three-step
  triage draft using a required built-in and optional custom property from a Project
  with no classification roles. The schema and save set share the exact form;
  resolution writes no Project/Issue values or execution state. Initial fixture
  visibility/manifest errors are not counted as behavioral Red.
- `cargo test --locked -p octacity-server-application --lib delivery_template_reads_configured_property_ids -- --nocapture`:
  Red blocked `/issue_snapshot/fields/recommended_route` when roles were absent;
  Green reads the configuration's direct property IDs from the same Project catalog.
  The prospective Configuration stays disabled and no Source input is activated.
- `python3 -m unittest discover -s tools/tests -p test_factory_stand_flow.py -k common_template_resolver`:
  Red still reconstructed schemas in the Python launcher; Green delegates the
  configured example to the ordinary read-only preset resolver and retains the
  explicit completion criteria. No schema-by-schema implementation remains there.
- `python3 -m unittest discover -s tools/tests -p test_factory_stand_flow.py -k example_fields_seed`:
  Red lacked the one-time Project setup boundary; Green seeds only an unpublished
  catalog from configuration, preserves the server's built-in defaults, and leaves
  operator publications, Issue values and subsequent restarts unchanged.

- `python3 -m unittest discover -s tools/tests -p test_factory_stand_triage.py -k triage_selection_pins`:
  Red omitted the common writer from the root Flow ceiling; Green retains that
  explicit local grant without modifying the Agent permission selection.
- `cargo test --locked -p octacity-server-application --lib selected_field_template_keeps_local_save_authority -- --nocapture`:
  Red rejected a Step-bound executor when the node also selected the local save
  capability; Green checks execution permissions independently of local accepted
  effects. The corrected fixture supplies the installed Step compiler contract;
  its initial incomplete compiler setting is not counted as behavioral Red.
- `cargo test --locked -p octacity-server-factory --lib local_issue_save_permission_is_not_exported -- --nocapture`:
  Red exported the local writer in the signed Build intent; Green preserves it on
  the Flow node while excluding it from Agent execution grants.
- `cargo test --locked -p octacity-server-application --lib template_discovery_and_resolution_require_the_configured_choice_threshold -- --nocapture`:
  Red advertised only the specialized Size parameter; Green advertises the configured
  choice condition and requires an explicit active threshold. No omitted-size fallback remains.
- `python3 -m unittest discover -s tools/tests -p test_factory_stand_triage.py -k configured_agent_aliases`:
  Red leaked Project MCP connections when the analyst alias changed; Green reads
  its explicit seed connection scope independently of the alias.
- `python3 -m unittest discover -s tools/tests -p test_factory_stand_triage.py -k preparation_uses_the_configured_selected_field` and
  `cargo test --locked -p octacity-server-application --lib packaged_triage_completion_prepares_title_only_input_without_a_prior_build_or_upload -- --nocapture`:
  Red still selected completion v2; Green uses configured completion v3 and the
  existing trusted document preparation owner. Historical exact document fixtures
  remain explicit inputs, without a request fallback adapter.
- `python3 -m unittest discover -s tools/tests -p test_factory_stand_policies.py -k common_writer_cannot`:
  Red silently widened a reviewed ceiling and retained a manufactured legacy alias;
  Green rejects the missing grant before modifying the catalog. Current-format
  recovery fixtures now start from a reviewed ceiling containing that capability.
- `cargo test --locked -p octacity-server-application --lib issue_delivery_routes_frozen_kind_size_component_and_priority -- --nocapture`:
  Red skipped reproduction for an ordinary manually supplied `reproduced` property;
  Green routes it to the independently observed command (or existing review branch
  when no command is selected). A Project catalog without classification roles is
  used for this custom choice. Initial invalid fixture choices are not counted as Red.
  The standalone reproduction manifest now imports the same conservative route
  data; its existing command/reporting regression passes with indeterminate findings
  remaining unresolved, rather than treating a scalar as verified evidence.
- `python3 -m unittest discover -s tools/tests -p test_verify_local_stand_context.py -k configured_field_templates`:
  Red excluded all ten new reviewed assets from the Docker context; Green includes
  only those named files and verifies required manifest parts/metadata. An initial
  incorrect helper name in the test is not counted as Red.
- `cargo test --locked -p octacity-server-application --lib triage_template_resolves_a_custom_project_form -- --nocapture` (additional slice):
  Red left triage with an implicit code binding; Green makes code-free drafts explicit
  for all examples. Operators attach sources separately; Work subject pinning remains
  independent.

## Configuration boundaries

The release hash includes its field/branch/condition parameters as well as the
ordinary manifest and completion document. Stand setup uses the existing resolver
rather than a second schema compiler. Existing Flow data mappings and predicates
remain configuration data; the request adds no mapping language or path editor.
The two consumer examples are intentional input modes: direct Issue and explicit
accepted producer result. Neither mode is an automatic old-request adapter.

Root permission ceilings must explicitly contain the selected local writer.
Changing a seed agent alias cannot add Project connections, credentials or save
authority. The installer preserves reviewed current-format catalogs, exact retained
policy/configuration references and disabled intake without publishing a replacement
pipeline on restart. Superseded development catalogs missing the configured grant
require explicit review rather than a hidden widening or destructive reset.

## Regression and installation qualification

All checks below passed on the final implementation:

| Boundary | Command / outcome |
| --- | --- |
| Core Flow/template/permission regressions | `cargo test --locked -p octacity-server-factory --lib`: 356 passed |
| Application owners, template resolution, admission, requirements and publication | `cargo test --locked -p octacity-server-application --lib`: 1173 passed |
| Real PostgreSQL automatic writes, recovery and Context provenance | `OCTACITY_POSTGRES_URL=<disposable-test-db> cargo test --locked -p octacity-server --test factory_configured_flow ordinary_form_ -- --ignored --nocapture`: 7 passed |
| Installer template delegation and one-time catalog seed | `python3 -m unittest discover -s tools/tests -p test_factory_stand_flow.py`: 3 passed |
| Common capability, scopes, configured agents and ordinary HTTP publication | Same discovery command with `test_factory_stand_triage.py`: 15 passed |
| Policy ceilings and writer rejection | Same discovery command with `test_factory_stand_policies.py`: 6 passed |
| Current-format restart and retained operator settings | Same discovery command with `test_factory_stand_recovery.py`: 17 passed |
| Launcher CLI | Same discovery command with `test_factory_local_stand.py`: 22 tests, 3 platform/opt-in skips |
| Delivery installation | Same discovery command with `test_factory_delivery_setup.py`: 22 tests, 1 opt-in skip |
| Filtered Docker context and generator inputs | Same discovery command with `test_verify_local_stand_context.py`: 29 passed |
| Ordinary real Server installation and restart | `OCTACITY_TEST_TRIAGE_SERVER=<fresh-binary> OCTACITY_TEST_TRIAGE_ENV=<test-peer-config> OCTACITY_TEST_TRIAGE_POSTGRES=<disposable-container> python3 -m unittest discover -s tools/tests -p test_factory_triage_setup_rest.py -k default_resources_runtime_roles_context_and_disabled_flow_use_ordinary_owners`: 1 passed |
| Production Clippy | `cargo clippy --locked -p octacity-server-factory -p octacity-server-application -p octacity-server --lib -- -D warnings`: passed |
| Integration-target Clippy | `cargo clippy --locked -p octacity-server --test factory_configured_flow -- -D warnings`: passed |
| Architecture | `python3 tools/check_architecture.py`: passed, 64 packages / 264 edges |
| Planning consistency | `openspec validate add-issue-classification-snapshots --strict`: passed |
| Formatting and patch hygiene | `cargo fmt --all --check`, changed JSON parsing and `git diff --check`: passed |

The ordinary REST check uses a fresh Server binary and real task-owned PostgreSQL
and S3. It creates its own database, installs exact runtime/Role/Profile/Context
selections, resolves and publishes the three-node triage form, verifies its eight
selected fields and explicit absence of code attachments, then restarts Server.
It checks retained Role history, operator policy edits, exact disabled Configuration
pins and zero Runs. The test environment now declares the same verified management
principal as the ordinary stand; its original missing principal and an assertion
expecting a serialized empty role map were fixture errors, not behavioral Red.
No provider, native inference or Source scan is executed. The task-owned containers,
network and temporary peer configuration are removed after verification.

Documentation-only edits and formatting have no executable behavior for TDD;
they are checked through the validation/hygiene steps above. Native UI/model/PR
qualification is still task 10.10, following the REST/Console selector work in
10.6–10.8. This report establishes task 10.5 only and does not claim that those
later UI changes are complete.
