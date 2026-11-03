# Exact Configuration/Repository readiness (task 7.2)

Verification date: 2026-10-29. This delivers the shared metadata query; it does not
qualify an executable deployment or complete bugfix quickstart.

## Delivered contract

`POST /api/v1/factory-configurations/{id}/versions/{version}/readiness` accepts a
closed selection containing an exact Repository and, optionally, an exact retained
Source input. It requires the existing Configuration View authorization and rechecks
Project, Configuration and Repository visibility after asynchronous dependency reads.
The response is `Cache-Control: no-store` and preserves the exact Configuration,
Repository and frozen root Flow references.

Each observation has a stable aspect/reason, requiredness, observation/deadline,
Project-scoped repair code and, for declared Context/MCP/action dependencies, an
exact definition/node/logical dependency subject. Hidden paths, credentials and
unavailable private entity identities are not returned. Required blocked facts take
precedence over unknown facts; unknown required facts never aggregate to ready.

- Installation checks reuse current visible choices and ordinary frozen-Flow
  compatibility. Required Build bindings use the ordinary execution context,
  inherited Project policy, enabled accepting Pools and authorized Agent inventory.
  The existing scheduler matcher is shared with dispatch, without synthetic Jobs.
- Agent health comes from fenced retained snapshots. Backend messages are omitted.
  Readiness expires at the earlier health freshness or current registration deadline.
  New process registration clears the previous process's resources; fenced old
  samples cannot repopulate them. Busy compatible physical capacity is a queue
  observation, without a capacity reservation. Unobserved tool-control contracts
  remain unknown for tool-using nodes.
- Context reads use retained Artifact metadata only. Required dynamic/collection
  dependencies stay required and unobserved. Root guards are unconditional. Literal
  and collection declarations with the same alias remain separate observations.
- Selected Source inputs reuse current installed read and independent update-purpose
  choices, exact connection/access revisions and ordinary predicate/effect checks.
  Every frozen Source reporting action independently reuses the existing metadata
  authorizer even when no optional input was selected.
- MCP observations use exact retained/installed configurations and the selected
  Runtime's transport support. Per-connection secret-reference mappings are checked
  without resolving their values. A compatible transport or mapped reference does
  not establish model authentication, MCP authentication, enforced resource scope,
  read-only behavior, native capture or OctaCity scoped authentication. The existing
  metadata contracts do not provide fresh independent proof for those claims, so
  they remain required unknown observations. No extra registry or live probe is
  introduced to manufacture proof.
- Native write permission is separately `unknown` / `unverified`; configured
  reporting scope is not a successful delivery receipt or provider permission proof.

The observation count uses the existing shared choice bound. Oversized closures
return required unknown coverage before dependency I/O rather than truncating to a
ready result. Agent metadata/matcher work has the same bounded work allowance.
Freshness is finalized after the last awaited authorization fence; neither a query
nor a reread renews an Artifact, resource sample or registration's original deadline.
The runtime supplies maximum age from existing Agent lease/registration policy.

## TDD evidence

All Cargo commands used `--locked -j2`, `CARGO_PROFILE_DEV_DEBUG=0`,
`CARGO_PROFILE_TEST_DEBUG=0` and `CARGO_INCREMENTAL=0`.

The HTTP boundary command is:

```sh
cargo test --locked -j2 -p octacity-server-api-rest --test v1_handlers <filter> -- --nocapture
```

| Filter / boundary | Observed Red | Observed Green |
| --- | --- | --- |
| `exact_configuration_repository_readiness` | route returned 404 | exact pins, no-store, disabled target blocked, mutation counters unchanged |
| `readiness_observes_exact_declared_context` | report deadline 31000 instead of Artifact deadline 2000 | retained/expired Context and exact actionable subjects |
| `readiness_rechecks_withdrawn_installation` | unknown instead of blocked after installation withdrawal | current installation withdrawal blocks |
| `readiness_does_not_infer_scoped_mcp` | missing MCP facts; later qualified transport incorrectly unknown | qualified transport/reference observations with independent authentication/capture unknown |
| `readiness_distinguishes_observed_absent` | required placement metadata API absent | observed empty capacity blocked, unobserved capacity unknown |
| `readiness_uses_current_qualified` | qualified inventory unknown; busy inventory blocked; registration deadline ignored | ready, queued, drained, stale, unsupported and registration-expired observations |
| `readiness_reuses_independent_source` | Source observation API absent | current read authority, withdrawal and independent unverified native write |
| `readiness_rechecks_exact_subject` | report disclosed after subject visibility withdrawal | revoked Configuration/Repository prevents report disclosure |
| `readiness_bounds_required` | oversized closure reached forbidden Artifact metadata reader | bounded required unknown coverage before dependency reads |
| `readiness_does_not_return_ready` | completion timestamp remained 1000 after metadata advanced clock to 2100 | expired in-flight facts downgraded |
| `readiness_checks_frozen_reporter` | frozen-action authorizer API absent | configured actions observed without input; withdrawn writer blocks |
| `readiness_review_` | final authorization expiry ignored; tool-using Agent incorrectly ready | final-fence expiry respected; missing exact tool control unknown |

Additional actual boundaries:

```sh
cargo test --locked -j2 -p octacity-server-factory --lib metadata_context_ -- --nocapture
cargo test --locked -j2 -p octacity-server-store-postgres --test lease_placement current_agent_resources_survive_restart -- --ignored --nocapture
cargo test --locked -j2 -p octacity-server --test factory_presets -- --ignored --nocapture
```

Core Context Red included missing requiredness and a same-alias dependency count of
1 instead of 2. The persistence Red included missing backend health and registration
expiry metadata, then inherited health after actual process re-registration. The
runtime Red returned uninstalled-service 503 instead of the ordinary missing-target
404; Green verifies the released composition before Factory execution is installed.
Test setup/compilation-environment mistakes were corrected and were not counted as
behavioral Red evidence.

## Side effects and dispatch

The query is composed only from metadata readers, existing management authorization
and the pure placement matcher. Tests make object-content reads fail immediately,
reject dependency work on oversized closures, and compare actual mutation evidence
before/after queries. No model discovery refresh, provider request, MCP tool call,
secret-value reader, publication, execution, budget reservation or Lease API is
invoked. Dispatch, Agent validation, current grants, credential-purpose enforcement,
Source/Reporter execution and delivery receipts retain their ordinary checks.

## Regression and review

Final regression results are recorded after the targeted Red/Green slices. Review
ran independently against repository standards and the originating readiness spec.
The four concrete findings (frozen reporting authority, tool-control qualification,
final-fence freshness and Context namespace collisions) received regression tests
and fixes. No commit is created for this task. Only task-owned disposable PostgreSQL
data is used; disk space is checked throughout and no broad cleanup is performed.

Final checks passed:

```sh
cargo test --locked -j2 -p octacity-server-api-rest --test v1_handlers
# 220 passed
cargo test --locked -j2 -p octacity-server-factory -p octacity-server-job -p octacity-server-scheduler --lib
# Factory: 324 passed; scheduler: 6 passed; Job library compiled (no unit tests)
cargo test --locked -j2 -p octacity-server-application --lib source_configuration
# 33 passed
cargo test --locked -j2 -p octacity-server-application --lib factory_flow_action_tests::result_reporting
# 14 passed
cargo test --locked -j2 -p octacity-server-application --lib factory_command_materializer_tests
# 3 passed before/after shared fixture refactoring
cargo test --locked -j2 -p octacity-server-store-postgres --test lease_placement -- --ignored --nocapture
# 10 passed against disposable PostgreSQL
cargo test --locked -j2 -p octacity-server --test factory_presets -- --ignored --nocapture
# 1 passed against disposable PostgreSQL
cargo clippy --locked -j2 -p octacity-server-application -p octacity-server-api-rest -p octacity-server -p octacity-server-factory -p octacity-server-job -p octacity-server-scheduler -p octacity-server-store -p octacity-server-store-postgres --all-targets --all-features -- -D warnings
# Passed after replacing a duplicate test-module load with one shared fixture
cargo fmt --all -- --check
python3 tools/check_architecture.py
# 64 packages, 263 dependency edges
openspec validate add-factory-triage-pipeline --strict
pnpm --dir ui api:generate
pnpm --dir ui typecheck
node --test ui/scripts/factory-authoring-schemas.test.mjs ui/scripts/issue-source-constraints.test.mjs
# 7 passed
```

Both review axes have no outstanding findings after the corrections. Available disk
space stayed between 46 and 48 GiB; no build-cache cleanup or commit was performed.
Task 7.3 and later tasks remain unchecked.


## Follow-up review corrections

The three follow-up findings are corrected in the same uncommitted worktree:

1. Native Command/Codex profiles expose their exact single-Job Pipeline contract
   through existing installation choices. Readiness and both materializers share
   the same pure shape check. Multiple Jobs or the wrong Job name are blocked;
   absent or conflicting installed declarations remain unknown. Metadata neither
   adds selectable identities nor changes native installation authority.
2. Pool observations are cached only within one readiness request, including
   unavailable results. Each authorized Agent inventory and health snapshot is
   prepared once. The existing work bound and original freshness deadlines remain
   unchanged; repeated profiles no longer spend the allowance rereading one Pool.
3. Preset input resolution uses `WorkSourceInputChoices::for_binding` for both
   read and update purposes. The independent Reporter-presence invariant remains;
   duplicated connection/access/source matching was removed.

All Cargo commands retain the environment and `--locked -j2` flags above.

| Command / test filter | Before correction | After correction |
| --- | --- | --- |
| `cargo test -p octacity-server-api-rest --test v1_handlers readiness_rejects_pipeline_shapes_that_the_selected_materializer_cannot_build` | Red: shared installed Pipeline contract API absent | Green: incompatible multi-Job Pipeline blocked through HTTP |
| `cargo test -p octacity-server-factory --lib competing_installed_pipeline` | Red: competing advisory declaration rejected instead of remaining unqualified | Green: ambiguous declaration resolves to no readiness contract |
| `cargo test -p octacity-server-api-rest --test v1_handlers readiness_shared_pool_supports_many_required_profiles_within_the_observation_budget` | Red: legal 86-binding configuration returned `unknown` / `observation_limit` | Green: shared compatible Agent remains `ready` within the existing bound |
| `cargo test -p octacity-server-api-rest --test v1_handlers preset_resolution` and `selected_source` | Existing behavioral baseline passed: 12 tests and 1 test | Full HTTP regression passed after the Source/Reporter refactor |

The Source/Reporter change preserves behavior, so its TDD evidence is the existing
behavioral baseline followed by regression, without an artificial Red. No database
behavior changes in these corrections; the disposable PostgreSQL checks recorded
above belong to the preceding implementation verification.


Follow-up regression checks:

```sh
cargo test --locked -j2 -p octacity-server-api-rest --test v1_handlers
# 222 passed
cargo test --locked -j2 -p octacity-server-factory -p octacity-server-job -p octacity-server-scheduler --lib
# Factory: 325 passed; scheduler: 6 passed; Job library has no unit tests
cargo test --locked -j2 -p octacity-server --lib factory
# 78 passed; 4 infrastructure tests remain explicitly ignored
cargo test --locked -j2 -p octacity-server-application --lib source_configuration
# 33 passed
cargo test --locked -j2 -p octacity-server-application --lib factory_flow_action_tests::result_reporting
# 14 passed
cargo test --locked -j2 -p octacity-server-application --lib factory_command_materializer_tests
# 3 passed
cargo clippy --locked -j2 -p octacity-server-application -p octacity-server-api-rest -p octacity-server -p octacity-server-factory -p octacity-server-job -p octacity-server-scheduler -p octacity-server-store -p octacity-server-store-postgres --all-targets --all-features -- -D warnings
# Passed after replacing the request-local cache's contains_key/insert pair with Entry
cargo fmt --all -- --check
git diff --check
python3 tools/check_architecture.py
# 64 packages, 263 dependency edges
openspec validate add-factory-triage-pipeline --strict
```

Free disk space during the follow-up stayed at 45–46 GiB. No commit, staging or
cache cleanup was performed. Task 7.3 remains unchecked.
