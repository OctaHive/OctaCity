# Tasks 7.3–7.5: qualification review fixes

Scope: three false-positive checks in the task 7.5 qualifier and isolation of
native evidence from synthetic HTTP controls. Production Issue, document,
admission and execution behavior is unchanged. Earlier verification reports
retain their original observed results.

## Complete selection after restart

The HTTP observer now compares the complete current material list with the
retained selections, including order and cardinality. Comparing a common prefix
with `zip` could accept a missing last comment or an additional current source.

`workflow_issue_materials_tests.rs` serves the queried public HTTP metadata and
checks an exact selection, a missing comment and an additional comment. It uses
the same observer as the native journey, without querying the database.

```sh
CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 \
  cargo test --locked --offline -p octacity-release-harness --test factory_pilot \
  workflow_issue_materials::tests
```

Red: the exact selection passed, but both mutation controls failed because the
qualifier accepted them (1 passed / 2 failed). Green: all three pass, including
observed rejection of each mutation. Logs:
`/tmp/octacity-75/review-fix-selection-{red,green}.log`.

## Ownership rejection rather than an arbitrary HTTP failure

The released journey first publishes/selects the sibling Issue's own document
successfully. It then uses the returned current ETag to attempt selection of a
publication owned by the original Issue. Rejection must be HTTP 400 with the
public `invalid_request` code; the sibling's full material list/version and
execution count must remain unchanged. This API intentionally supplies a generic
invalid-request error rather than exposing foreign owner metadata. The successful
control establishes route availability, authority and a usable precondition;
no production error contract was widened to support the test.

## Selector absence in the visible start form

`reviewIssueSnapshotLaunch` checks the actual **Earlier Issue run** control,
using the English catalog selected by this packaged journey, before clicking
**Review pipeline launch**. Checking a wrong label inside the resulting modal
could never observe that background control. The regression includes a modal
that makes its start form inert, so moving the check after modal opening also
fails the control.

The small released-journey helpers are shared with
`tests/browser/issue-material-qualification.spec.ts`. The regression uses real
loopback HTTP responses and visible DOM controls; the packaged journey continues
to use the actual Server and Console without browser HTTP interception.

```sh
# From ui, with the repository-pinned Node/pnpm on PATH:
pnpm exec playwright test tests/browser/issue-material-qualification.spec.ts
```

Ownership Red: the valid response passed, but 500, 403, 409 and an unrelated 400
all incorrectly satisfied the old check (1 passed / 4 failed). Green: 5 passed.
Selector Red: 6 passed / 1 failed, because the old observer accepted the visible
earlier-run selector. Green: all 7 browser controls pass, and a selector blocks
review before a dialog opens. Logs:
`/tmp/octacity-75/review-fix-{ownership,selector}-{red,green}.log`.

## Qualification and regression checks

The pre-change baselines passed: 10 default pilot tests and both EN/RU Issue
snapshot browser journeys. After the fixes, all 13 default pilot tests pass;
the 18 ignored scenarios are not counted as executed native coverage.
The focused browser controls pass (7). Running them together with both existing
EN/RU snapshot journeys passes all 9 browser tests (9.1 s). Type checking, UI lint,
UI formatting, strict Clippy, workspace Rust formatting, architecture checks
(64 packages / 264 edges), strict OpenSpec validation and whitespace checks pass.

The full native material journey passes: **1 passed, 289.79 s**, with the existing
checksummed Server/Agent/Console, finite CLI and guest identities from
[task 7.5](verification-7.5.md). Its task-owned PostgreSQL/MinIO containers use
the separate `issue-snapshot-75-review-fixes` label. All three actual Console
phases, both ordinary probe Jobs, protected delivery, later publications/comment
and final restart execute with the strengthened checks.

```sh
CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 \
  cargo test --locked --offline -p octacity-release-harness --test factory_pilot \
  packaged_issue_materials_remain_owned_frozen_and_scoped \
  -- --ignored --exact --test-threads=1 --nocapture
```

The observed invocation uses the same task-local `run.py` environment wrapper
as task 7.5 and its current built `factory_pilot` binary. Log:
`/tmp/octacity-75/review-fix-native.log`. The direct producer-result journey's
previous successful qualification remains recorded in task 7.5; that unrelated
journey was not rerun for these observer-only fixes.

Repeated native proof:
`target/qualification/issue-snapshot-75/evidence/issue-materials-f27df439-63ef-4ce7-a5d8-bd41c4d9e2f5.json`.
It retains completed Run `f27df439-63ef-4ce7-a5d8-bd41c4d9e2f5`, Issue
`effaf77c-c681-4fd4-87f0-c541a8648366`, execution
`e98ca3c3-7149-4a63-b5f7-6318d40d52a2` and snapshot digest
`6a0ff7e08f4516ac64ae4e0a1960f153525c616291dc55912b2b45c18def9819`.

The two disposable service containers are removed only after verifying their
review-fix label. Evidence and browser phase logs remain private (mode 0600).
The user's stand, token file and unrelated uncommitted changes are preserved.

## Isolate native evidence from ordinary HTTP observation

`verify_retained` now only reads/asserts public HTTP state and returns the
observed execution and snapshot detail. `save_native_evidence` separately owns
the optional evidence directory and mode-0600 file publication. Only the native
`MaterialSnapshot` journey invokes that writer after checking the real ordinary
probe Jobs. No persistence flag, test-name dispatch or global environment mutation
was added to the observer.

The subprocess regression launches the existing positive HTTP observation test
with `OCTACITY_RELEASE_EVIDENCE_DIR` set to a private temporary directory. It
requires a successful child run with exactly one test, then checks that the
directory is empty. Its environment override is confined to the child, keeping
parallel parent tests isolated.

```sh
CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 \
  cargo test --locked --offline -p octacity-release-harness --test factory_pilot \
  ordinary_http_observation_does_not_create_native_evidence
```

Red: the child successfully observed the synthetic HTTP state but created a
qualification file; the parent failed its empty-directory assertion (1 failed,
0.06 s). Green: all four material HTTP controls pass, including the subprocess
isolation regression (4 passed, 0.06 s). Logs:
`/tmp/octacity-75/evidence-fix-{red,green}.log`.

All 14 default pilot tests pass; 18 native scenarios remain explicitly ignored
in that default run. Strict Clippy, workspace formatting and architecture checks
pass. Repeating all four material controls with the release evidence variable
set on the parent process also leaves its evidence directory empty
(`/tmp/octacity-75/evidence-fix-release-env.log`).

The complete native material journey passes again: **1 passed, 289.21 s**.
The explicit native writer retains the original report shape with a completed
Run, execution/snapshot and two independently checked ordinary probe Jobs;
the resulting file has mode 0600. Native log:
`/tmp/octacity-75/evidence-fix-native.log`. The same existing native command and
checked package/image identities above were used; no runtime or provider
configuration was changed for this separation. Task-owned disposable services
are removed after checking their review-fix labels.

Evidence-isolation native proof:
`target/qualification/issue-snapshot-75/evidence/issue-materials-8470bc41-4629-45e4-b280-ad110ae011a8.json`.
It retains completed Run `8470bc41-4629-45e4-b280-ad110ae011a8`, two ordinary
probes and snapshot digest
`7f3328ae4911f202f70988bdb579e271e3552b7d663e170141988c3c47ca3cf6`.
