# Tasks 7.1–7.2 — Review regressions

This follow-up fixes CLI framing and provenance defects without changing the production
Issue/Factory contracts. The original journeys and their earlier evidence remain
documented in `verification-7.1.md` and `verification-7.2.md`.

## Program framing

The finite CLI previously treated an English compiler-guidance sentence as a
program boundary. A legitimate declared invocation prefix containing that same
sentence made the CLI reject the installed program with exit 64. It now selects
the last version-tagged JSON program line before frozen input, then validates
the program's version, allowed fields and exact invocation prefix/options/model/
environment. Compiler prose and unrelated JSON examples do not frame the program.
An earlier program example in the declared prefix is permitted, but an invalid
installed program cannot fall back to that example.

A later review found that syntax errors were still skipped: an earlier valid
example remained selected when the installed tagged program could not be parsed.
The parser now invalidates that earlier candidate on a malformed version-tagged
line. It still permits a later valid installed program after examples in its
prefix and ignores unrelated compiler prose.

The regression seam is the existing CLI subprocess with frozen input and
independently expected output:

- Repeated guidance in the prefix failed with exit 64 for both the original and
  revised compiler prose: `/tmp/octacity-7172-review-cli-red.log`.
- A declared prefix containing an earlier program example exposed the initial
  candidate-uniqueness restriction: exit 64 instead of the installed result,
  `/tmp/octacity-7172-review-cli-example-red.log`.
- Selecting candidates by prefix compatibility allowed a malformed installed
  program to fall back to that example: exit 0 instead of 64,
  `/tmp/octacity-7172-review-cli-prefix-red.log`.
- Selecting the tagged program first and validating its prefix afterward passes
  all three regressions and the existing CLI/launcher suite:
  `/tmp/octacity-7172-review-cli-verified.log`, 23 passed in 11.497 seconds.

```sh
python3 -m unittest tools.tests.test_factory_pilot_cli tools.tests.test_factory_pilot_launcher
```

### Malformed JSON follow-up

The subprocess regression supplies an independently constructed prefix example
that would write `unexpected-write`, followed by the installed JSON program with
either its opening or closing brace removed. Both variants must return exit 64,
emit no stdout, report the existing bounded error and leave the file absent.

- Red: both variants returned exit 0 instead of 64;
  `/tmp/octacity-7172-malformed-cli-red.log` (one test, two failing subcases).
- Green: both variants reject the prompt without executing the example;
  `/tmp/octacity-7172-malformed-cli-green.log` (one test passed).

```sh
python3 -m unittest \
  tools.tests.test_factory_pilot_cli.FactoryPilotCliTests.test_model_process_rejects_malformed_program_without_executing_a_prefix_example
```

The final CLI/launcher regression suite passed all 24 tests in 10.871 seconds,
recorded in `/tmp/octacity-7172-malformed-cli-regressions.log`. `node --check`,
`git diff --check` and strict OpenSpec validation passed. The native results below
precede this final malformed-JSON guard; this follow-up is verified through the
CLI process boundary and does not claim another native qualification run.

## Exact source attribution

The shared retained-snapshot verifier previously checked only the pipeline Run
ID. It now independently reads the producer Run's Work ID and accepted
`assess`/`eligible` completion digest through the public management HTTP API.
Every pipeline-authored field must reference all three exact source identities.
These observer assertions apply to both the original reasoning triage and the
replacement deterministic triage; delivery still consumes only its Issue
snapshot, without a producer selector or producer transcript.

Three tests drive the verifier through an HTTP metadata peer. They use typed
attribution with valid UUIDs and digest formats, one accepted assessment record
and a distinct classification receipt. Preview and retained snapshot deliberately
agree even in the negative cases, while the independently served producer
metadata remains correct. One field substitutes either the receipt digest from
the same Run or an unrelated Work ID; both must fail. The exact-source control
must pass.

- Red: the positive control passed, but both substituted-source cases were
  accepted: `/tmp/octacity-7172-review-provenance-red.log` (1 passed, 2 failed).
- Green: the positive control passed and both substitutions were rejected:
  `/tmp/octacity-7172-review-provenance-green.log` (3 passed).

```sh
CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 \
  cargo test --locked --offline -p octacity-release-harness --test factory_pilot \
  workflow_issue_snapshot::tests
```

## Regression checks

Both native journeys were rerun sequentially with the rebuilt, separately
identified coder/verifier CLI executables, qualified guest archive and packaged
Server/Agent used by the original qualification. The current harness binary ran
both exact filters through the ignored local environment wrapper:

```sh
python3 target/qualification/issue-snapshot-review/run.py \
  target/debug/deps/factory_pilot-a93451d2d2b237dd \
  packaged_issue_classification_delivers_from_its_snapshot_without_a_predecessor \
  packaged_delivery_reuses_one_configuration_for_manual_and_replacement_triage \
  --ignored --exact --test-threads=1 --nocapture
```

Result: **2 passed, 0 failed**, 572.53 seconds;
`/tmp/octacity-7172-review-native.log`. Both cases exercised the new program
parser and the independent Run/Work/assessment-record checks. The manual case
continued to require manual attribution without a triage Run. All Jobs and
publication/restart assertions ran against disposable PostgreSQL, object storage
and the local HTTPS/Git peer; no production stand or provider credential was used.

The reuse journey exported its mode-0600 proof:

`target/qualification/issue-snapshot-review/evidence/issue-snapshot-reuse-da3a2c5a-4449-49aa-8c52-fc05dc1e7865.json`

```sh
CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 \
  cargo test --locked --offline -p octacity-release-harness \
  --test factory_pilot --test factory_delivery_target
# 10 harness tests and 3 HTTPS/Git peer tests passed.
# /tmp/octacity-7172-review-harness.log

CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 \
  cargo clippy --locked --offline -p octacity-release-harness \
  --test factory_pilot --test factory_delivery_target -- -D warnings
# Passed; /tmp/octacity-7172-review-clippy.log

python3 tools/check_architecture.py
# Passed: 64 workspace packages, 264 dependency edges.

openspec validate add-issue-classification-snapshots --strict
# Passed; /tmp/octacity-7172-review-openspec.log
```

The HTTP tests require permission to bind loopback sockets. An initial sandboxed
suite invocation failed on that environment restriction; it is not counted as a
behavioral Red. The permitted rerun above executed the tests successfully.
Targeted Rust formatting and `git diff --check` also passed. Documentation-only
updates have no executable behavior and do not need a separate TDD cycle.
