# Task 6.6 verification

## Scope and boundaries

The user explicitly authorized changes in both OctaCity and the adjacent Octa
repository. Existing uncommitted 6.4–6.5 work and unrelated Octa files are retained.
No commit, push, provider call, new executor, migration, journal, queue or Agent
protocol version is introduced.

Octa owns one provider-neutral `OperationObservation` DTO and its conservative
path validation in `octa-runner-protocol`. Codex maps directly reported
command/file boundaries to optional `progress.observation` metadata. Ordinary
progress remains replaceable; observations use the existing bounded command
response queue, so delayed consumption cannot coalesce start/completion records.
The executor preserves the extension in the existing sequenced runner stream.

Public metadata contains an opaque correlation digest, observed phase/time and
allowlisted command outcome or file path/change/failure. Original runner
run/step/command references identify the existing output events. OctaCity reuses
the DTO, validates it before fenced event append, and exposes only that closed
metadata through `RunnerEventPayload::public_operation_observation`. Raw payloads,
arguments, environment, diffs, private messages, prompts, transcripts, output and
usage-like prose cannot enter this projection. A start cannot claim a completed
exit status. Observation absence means Not reported, never invented activity.

The process fixture runs the real Codex plugin and runner around a controlled
supported CLI peer. It actually invokes a child command returning 17 and writes a
workspace file, then reports their start/completion. All four observations reach
the runner stream, retain original times and step identity, and link to the
producing output through the original run/step/command references. A failed tool
command does not itself decide the enclosing Run's semantic result or acceptance.
This qualifies controlled process production, not a live provider.

An application characterization verifies wrong-attempt rejection before authority
lookup or persistence, original fenced stream/sequence and timestamps on replay,
and no fabricated log chunks. Existing exact Build/Attempt/Job navigation remains
the authorized log target. Detailed Issue journal HTTP/Console work stays in
6.7–6.8; no parallel read or execution owner is added here.

Reported input, cached-input and output tokens remain separate counters in the
existing result/provenance usage contract. Missing usage remains an empty map.
Model token caching is distinct from existing Octa task-result cache events;
reserved budgets, prose and operation summaries do not become measured usage or
cost. Unsupported operation categories retain authorized log navigation.

## Red → Green

All Rust commands use `--offline`, `CARGO_INCREMENTAL=0`,
`CARGO_PROFILE_DEV_DEBUG=0`, `CARGO_PROFILE_TEST_DEBUG=0`, and
`CARGO_BUILD_JOBS=2`. Octa commands below run from OctaCity with
`CARGO_TARGET_DIR=/Users/roman/Projects/OctaCity/target`.

| Command / behavioral test | Observed Red | Observed Green |
| --- | --- | --- |
| `cargo test --manifest-path ../octa/Cargo.toml -p octa_plugin_codex --bin octa_plugin_codex public_command_observation` | Zero structured command observations instead of one | One safe failed command with exit 17; private fields excluded |
| Same command with `public_file_observations` | Zero file boundaries instead of two | Stable per-file correlation, reported start/completion, safe relative path only |
| `cargo test --manifest-path ../octa/Cargo.toml -p octa_plugin_codex --test plugin_manager real_codex_process_retains` | Delayed consumption lost all four operation observations | All four retained in order; measured tokens 11/7/3 preserved |
| `cargo test --manifest-path ../octa/Cargo.toml -p octa-output current_event_schema_accepts_closed` | Current event schema rejected the optional metadata | Accepts closed observation; rejects raw `arguments` |
| `cargo test --manifest-path ../octa/Cargo.toml -p octa-runner --test runner codex_operations_reach` | Actual runtime routing discarded all four observations | Real runner retains every observation, original timing, step and output linkage |
| `cargo test -p octacity-protocol public_operation_metadata` | Raw invocation field accepted before retention | Closed DTO rejects it before append |
| `cargo test -p octacity-protocol public_operation_projection` | Required public projection API absent | Safe typed projection; ignores prose, transcript, prompt and token claims |
| `cargo test --manifest-path ../octa/Cargo.toml -p octa_plugin_codex --bin octa_plugin_codex exposes_the_codex_task_contract` | Qualified capability absent | Explicit `public-command-file-observations.v1` capability |
| `cargo test -p octacity-protocol public_operation_start` | Start could masquerade as a completed command | Contradictory start/outcome metadata rejected |
| `python3 -m unittest tools.tests.test_check_architecture.ArchitecturePolicyTests.test_runner_observation_contract_is_protocol_specific` | Pure wire dependency rejected for `octacity-protocol` | Narrow allowance passes; other shared contracts and runtime dependency remain rejected |

Initial fixture setup/compilation failures were repaired and were not counted as
Red. Before changing progress DTOs, existing `octa-output` and `octa-plugin` tests
passed. Unknown process activity/missing usage and application append replay use
characterization tests because their existing behavior is preserved.

## Regression and repository checks

- Octa: `cargo test --manifest-path ../octa/Cargo.toml -p octa-executor -p octa_plugin_codex -p octa-plugin-manager -p octa-output -p octa-plugin -p octa-runner-protocol`: **630 passed**.
- Octa runner: `cargo test --manifest-path ../octa/Cargo.toml -p octa-runner --test runner`: **21 passed**, including real command/file production and existing task-cache behavior.
- OctaCity: `cargo test -p octacity-protocol`: **72 passed**.
- OctaCity application: `cargo test -p octacity-server-application --lib`: **614 passed** (including manual Issue, Source, execution and complete Factory journeys).
- OctaCity Agent runner: `cargo test -p octacity-runner --lib`: **48 passed**.
- `python3 -m unittest tools.tests.test_check_architecture`: **32 passed**.
- `python3 tools/check_architecture.py`: **63 packages / 256 edges**, no violations.
- Clippy on affected Octa crates and OctaCity protocol/application, with `--all-targets -- -D warnings` (OctaCity also `--all-features`): passed.
- `cargo fmt --all --check` and `git diff --check` in both repositories: passed.
- `openspec validate add-factory-work-card --strict`: passed.

For Octa process regression, build Codex/fixture, shell, tpl, junit and the existing
`octa_plugin_test` example first, as its CI does. Set
`OCTA_E2E_PLUGINS_DIR=/Users/roman/Projects/OctaCity/target/debug` for runner tests;
copy the test plugin into Octa's `plugins/` for plugin-manager tests, then remove
only that generated copy. Missing test executables were environment failures,
not product regressions.

Disk space was checked throughout compilation. Debug information and incremental
builds were disabled, concurrency limited, and a shared existing target reused.
Final free space: **6.7 GiB**; lowest observed during the checks: approximately
**6.2 GiB**. No user source or unrelated build/cache directories were removed.
