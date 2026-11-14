# Task 7.1 — Packaged classification to independent delivery

The subsequent CLI framing and exact-source attribution regressions are recorded
in [the tasks 7.1–7.2 review follow-up](verification-7.1-7.2-review-fixes.md).

## Qualified boundary

The existing `factory_pilot` journey now has
`packaged_issue_classification_delivers_from_its_snapshot_without_a_predecessor`.
It uses the current packaged Server/Agent, PostgreSQL, retained object storage,
pinned Octa/Microsandbox, actual isolated Jobs and the existing disposable
HTTPS/Git publication peer. No production repository or user token is used.

The fixture publishes Project components, discovers the shared classification
field contract and publishes a triage Flow. An ordinary reasoning Build produces
the typed result; the installed local action applies priority `high`, component
`api`, type `feature_request` and size `small` to the Issue. No Issue origin or
accepted producer record is seeded by the test.

A separately published delivery Flow selects canonical Issue fields through
`issue_snapshot`. Its requirements gate projects that flat input into the
existing reviewed grammar. Review derives scheduling priority 80 from the pinned
policy. Admission uses the current Issue ETag and reviewed snapshot digest, with
neither `predecessor_run` nor caller-supplied classification.

The existing protected-test author, coder, original-test validation, isolated
verifier, review and trusted publisher execute. The HTTPS/Git peer confirms the
actual candidate PR. After the existing accepted-publication Server restart, the
test connects the completed Run to the original execution request and digest,
compares its complete snapshot with the review, checks Issue revision 2 and
Project definition head 1, and checks all four original pipeline attributions.
Candidate SHA, changeset identity, remote head, review URL/state and the single
publication POST remain covered by the existing delivery assertions.

The manual-only launch policy remains separate from the installed local writer.
The local management configuration has no integration catalog; this test does
not install Source intake or an external Issue Reporter. Earlier exact-producer
cases retain their predecessor-bound requests.

## Observed Red/Green

The test seam is the existing packaged Issue/Factory REST journey, including its
real Job and local PR boundaries. The mandatory process prerequisites use the
existing finite CLI process contract.

```sh
python3 -m unittest tools.tests.test_factory_pilot_cli tools.tests.test_factory_pilot_launcher

CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 \
  cargo test --locked -p octacity-release-harness --test factory_pilot \
  packaged_issue_classification_delivers_from_its_snapshot_without_a_predecessor \
  -- --ignored --exact --nocapture
```

Observed process cycles:

- Compiler guidance between the JSON program and frozen input: exit 64 Red,
  followed by 19 passing CLI/launcher tests. Evidence:
  `/tmp/octacity-71-prompt-red.log` and `prompt-green.log` with the same prefix.
- Pinned Octa's `--cd`/`--add-dir` arguments: exit 64 Red, followed by 20 passing
  tests, including relative writes to the selected working directory while
  retaining explicit runtime-option checks. Evidence:
  `/tmp/octacity-71-workspace-red.log` and `workspace-green.log` with the same
  prefix. Both embedded native CLI executables were rebuilt before qualification.

The corrected packaged control without the local apply action failed after
50.84 seconds: public `Issue.content.priority` was null instead of `high`.
Evidence: `/tmp/octacity-71-classification-corrected-red.log`. An earlier assertion
incorrectly read top-level `priority`; that result is excluded as Red evidence.

The complete snapshot case passed after 229.07 seconds, including confirmed PR,
restart and retained attribution assertions:
`/tmp/octacity-71-qualified-green.log`. The original exact-producer packaged
baseline passed after 229.05 seconds following the CLI corrections:
`/tmp/octacity-71-baseline-workspace-fixed.log`.

Actual commands used the ignored local environment wrapper
`target/qualification/issue-snapshot-71/run.py` to supply the qualified paths and
disposable service endpoints. After `cargo test --no-run`, the wrapper invoked
the resulting `target/debug/deps/factory_pilot-*` binary with the same exact test
name and `--ignored --exact --nocapture` arguments. Compilation alone is not
counted as qualification.

Environment and fixture setup failures are excluded from behavioral Red:
overlong macOS Unix socket paths, duplicate coder/verifier executable identities,
the unqualified Homebrew Git trust chain, TOML insertion and graph budget/input
schema mistakes. The actual environment used short task-owned paths, distinct
native CLI identities and the qualified system Git. No product validation was
relaxed to accommodate the fixture.

## Regression checks

All commands completed successfully:

```sh
CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 \
  cargo test --locked --offline -p octacity-release-harness --test factory_pilot \
  -- --test-threads=2
# 7 passed; 16 environment-dependent tests ignored.

python3 -m unittest tools.tests.test_factory_pilot_cli tools.tests.test_factory_pilot_launcher
# 20 passed; /tmp/octacity-71-cli-final.log.

CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 \
  cargo clippy --locked --offline -p octacity-release-harness --test factory_pilot -- -D warnings
# /tmp/octacity-71-clippy-qualified.log.

python3 tools/check_architecture.py
# 64 packages, 264 dependency edges.

openspec validate add-issue-classification-snapshots --strict
```

Targeted Rust formatting and `git diff --check` also passed. The testing guide
documents the process compatibility requirements and the independent snapshot
journey. Documentation has no executable behavior; its checks do not replace the
Red/Green cycles above.

## Scope limits

This qualifies one small feature request prepared by a real triage Build. Tasks
7.2–7.5 retain their separate manual/replacement triage, negative applicability,
broader regression, rollout-documentation and selected-material qualifications.
No UI, production API, persistence format or runtime authorization was changed
for task 7.1.
