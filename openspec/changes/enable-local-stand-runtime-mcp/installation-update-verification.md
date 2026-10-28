# Task 1.3 — non-destructive installation updates

## Implementation boundary

The existing `factory-stand` → `local-stand up` entry point accepts an explicit
`--update-factory-installation` with a complete catalog and independent Agent
policy. Ordinary startup retains its immutable-installation checks. No update
registry, profile migration, credential owner or Work admission path was added.

The lifecycle owner verifies the current installation, stages private candidate
files, validates the native Agent and runs the packaged server's offline file
validator. Only a successful preflight can stop the current stand. An atomic
directory exchange activates server/Agent configuration together; an unsuccessful
startup stops candidate services and restores the previous configuration. Repeated
identical installation inputs do not exchange files or stop the stand for an
update. Concurrent edits fail closed before replacement.

PostgreSQL and object-store volumes are never reset. Agent credentials, work/cache
paths and demonstration receipts remain outside the exchanged configuration.
Installed optional Runtime/Delivery/Signal bundles and integration authority are
retained. The generated integration server configuration follows the new base
configuration without rewriting its management authority. Project Runtime
settings, immutable transport/profile history and admitted Runs retain their
existing server owners; this deployment operation does not publish or repin them.

## Observed Red → Green cycles

Each command below was run before the corresponding implementation and again
after it. All Python tests use the existing private stand/installer/lifecycle
fixtures and public CLI/configurator/update boundaries.

```shell
python3 -m unittest tools.tests.test_configure_local_stand_agent.LocalStandAgentConfigurationTests.test_stages_updated_factory_policy_without_replacing_identity_or_live_configuration
python3 -m unittest tools.tests.test_configure_local_stand_agent.LocalStandAgentConfigurationTests.test_updates_factory_installation_and_reuses_it_without_touching_retained_state
python3 -m unittest tools.tests.test_configure_local_stand_agent.LocalStandAgentConfigurationTests.test_restores_usable_factory_installation_when_candidate_startup_fails
python3 -m unittest tools.tests.test_factory_local_stand.FactoryLocalStandTests.test_explicit_update_uses_ordinary_preflight_without_seeding_work
python3 -m unittest tools.tests.test_configure_local_stand_agent.LocalStandAgentConfigurationTests.test_ordinary_launcher_applies_explicit_update_then_restarts_with_the_same_identity
python3 -m unittest tools.tests.test_local_stand_launcher.LocalStandLauncherTests.test_factory_update_preflight_validates_packaged_files_offline_without_durable_mounts
python3 -m unittest tools.tests.test_configure_local_stand_agent.LocalStandAgentConfigurationTests.test_factory_update_refreshes_generated_integration_config_and_retains_its_authority
python3 -m unittest tools.tests.test_configure_local_stand_agent.LocalStandAgentConfigurationTests.test_factory_update_retains_runtime_support_files_at_the_existing_installation_depth_limit
python3 -m unittest tools.tests.test_configure_local_stand_agent.LocalStandAgentConfigurationTests.test_factory_update_refuses_concurrent_operator_edits_before_stopping
```

Red observations were the absent staging/update API and CLI flag, unchanged
immutable-policy rejection in the launcher, configuration remaining replaced after
a failed start, a stale integration base configuration, rejection of legitimate
Runtime support depth, and stopping before rejecting concurrent edits. Green
preserves the old files/identity on refusal or rollback and reuses the new
installation on restart. Additional failure-characterization coverage checks that
rejected packaged preflight never reaches shutdown or activation.

The packaged-preflight boundary also ran Red → Green for a timed-out Docker
client: the original timeout escaped without stopping its daemon-owned container.
Green removes only the uniquely named temporary validator and returns a bounded,
safe timeout diagnostic before deleting staged files.

The real packaged validator initially accepted an invalid Factory catalog. The
public server file validator now reuses the startup loader and verifies native
discovery-program identity without invoking discovery or a model:

```shell
CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 cargo test -p octacity-server --lib offline_validation_checks_factory_catalog_with_the_startup_parser
CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 cargo test -p octacity-server --lib offline_validation_verifies_installed_discovery_identity_without_invoking_the_model
```

Both tests first failed because invalid catalog fields / substituted executable
bytes were accepted, then passed with the ordinary installation error and no
private diagnostics. Existing lazy model-discovery behavior remains covered.

Packaged compilation also exposed two omitted `include_bytes!` inputs in the
default-deny Docker context. The context-policy test first failed for excluded
presets and then for an undetected missing preset. Green allows only the two
canonical JSON files and requires them in repository context verification:

```shell
python3 -m unittest tools.tests.test_verify_local_stand_context.LocalStandContextTests.test_compiled_factory_presets_are_available_without_opening_operator_configuration
```

## Regression and qualification

```shell
python3 -m unittest discover -s tools/tests
CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 cargo test -p octacity-server --lib
CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 cargo clippy -p octacity-server --lib -- -D warnings
cargo fmt --all -- --check
python3 tools/check_architecture.py
python3 tools/verify_local_stand_context.py --repository /Users/roman/Projects/OctaCity
openspec validate enable-local-stand-runtime-mcp --strict --no-interactive
git diff --check
OCTACITY_TEST_SERVER_IMAGE=octacity/server:mcp-update-check python3 -m unittest tools.tests.test_configure_local_stand_server.LocalStandServerConfigurationTests.test_packaged_factory_preflight_accepts_valid_files_and_rejects_invalid_native_catalog
```

Server library regression: **220 passed, 51 ignored**. Clippy, Rust formatting,
architecture and bounded context checks passed. The focused Python lifecycle,
installer, bootstrap and documentation regression passed **121 tests, 7 skipped**;
the additional context/build regression passed **32 tests**. Full repository-tool
regression passed **500 tests, 11 skipped**. The real image preflight passed its
valid/invalid catalog and diagnostic-redaction checks (**1 passed**).

The qualification image uses the ordinary pinned builder and image verification
against an isolated source snapshot with current working changes overlaid; the
production builder still selects committed source. The personal stand and its
production image tag are not replaced. Qualification calls `validate` with no
network and no durable mounts; it does not start application services or models.
The existing registration-without-network-mutation and retained demo-receipt
fixtures remain the restart contract. Full live MCP/auth/capture and retained
Project-history qualification belongs to task 3.1.

The verified qualification image digest was
`sha256:f95fad33994187310e050d6c9adeb43eb6882f990c799a919ff84abffa0d7bc3`;
its task-owned image tag was removed after qualification.

Disk was checked throughout. At about 11 GiB free, the separate regenerable
`/tmp/octacity-runtime-settings-target` Cargo cache (~9.8 GiB) was removed. Stand
volumes, credentials and working files were preserved; subsequent checks showed
about 22 GiB free.

## Review corrections after task 1.5

The initial explicit installation now runs the same packaged offline validator as
an update, before Compose, bootstrap or Agent activation. The CLI test first
failed because the validator rejection was bypassed, then passed with no service
activation:

```shell
python3 -m unittest tools.tests.test_configure_local_stand_server.LocalStandServerConfigurationTests.test_initial_explicit_installation_rejects_packaged_catalog_before_service_activation
```

Update snapshots now retain normalized modes and SHA-256 fingerprints instead of
whole file contents. Staging copies files in bounded chunks. Both the configurator
and updater use the existing private filesystem module for tree traversal, so
owner, link, mode, depth, entry-count, byte and stable-handle checks have one
implementation. A public update test with a 16 MiB executable first measured
16,834,106 retained Python bytes at preflight; Green keeps the additional retained
allocations below 4 MiB and verifies both copied and original contents. This is a
retained-memory assertion at preflight, not a total-process peak-memory limit.

```shell
python3 -m unittest tools.tests.test_configure_local_stand_agent.LocalStandAgentConfigurationTests.test_factory_update_does_not_retain_executable_contents_during_preflight
```

The server configurator now owns export of installed Signal connector inputs and
shares its canonical file names with installation. The updater neither reproduces
that encoding nor calls the configurator's private executable-bundle helper.
Before and after this behavior-preserving refactor, the following characterization
test passed with a nonempty catalog, unchanged opaque configuration/credential
bytes and private file permissions:

```shell
python3 -m unittest tools.tests.test_configure_local_stand_agent.LocalStandAgentConfigurationTests.test_factory_update_preserves_installed_connector_inputs_when_omitted
```

Full tool regression also exposed stale Octa 0.7.2 entries in `fuzz/Cargo.lock`
against the workspace's 0.8.0 contract. The existing public lockfile-contract test
failed for all three protocol packages. Updating only those three entries through
Cargo made the security-matrix suite pass (8 tests); unrelated dependency versions
remain unchanged.

```shell
python3 -m unittest tools.tests.test_security_matrix
```

These corrections change Python lifecycle tools and the fuzz dependency lockfile;
they do not require a new native build. Documentation has no executable behavior
and is checked with strict OpenSpec validation and whitespace validation.

Final regression: `python3 -m unittest discover -s tools/tests` ran 523 tests
successfully (508 passed, 15 skipped). Architecture validation passed for 63
workspace packages and 261 dependency edges; strict OpenSpec validation,
`git diff --check`, Python compilation of the four changed lifecycle modules and
`cargo metadata --offline --locked --no-deps --manifest-path fuzz/Cargo.toml
--format-version 1` passed. Disk checks remained above 10 GB, with 11.2 GB free
at completion; no stand data or unrelated working files were removed.
