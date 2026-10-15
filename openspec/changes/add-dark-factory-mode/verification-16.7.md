# Task 16.7: documentation, specification sync and release gates

Status: in progress. Task 16.7 is not completed by specification sync alone.

## Specification synchronization

Synchronized the seven exact delta paths returned by `openspec status` for
`add-dark-factory-mode` into the corresponding main capabilities. Added 48
requirements and modified four; retained every pre-existing requirement and
scenario. Created `server/dark-factory` using the approved delta Purpose;
existing main-spec Purposes remain unchanged. No delta operation headings
remain in main specs. Reapplying the selected requirement blocks is idempotent.

The quickstart and Work-card proposals remain separate active changes; their
deltas were not synchronized as implemented behavior. The Dark Factory change
also remains active while qualification and calibration work is unfinished.

Validation:

```shell
openspec validate --specs
# 11 passed, 0 failed; informational long-requirement notices only.
openspec validate add-dark-factory-mode --strict
# valid.
git diff --check
# passed.
```

These are normative documentation changes, not evidence of runtime readiness
or provider/model promotion. Executable TDD is not applicable to this sync.

## Remaining qualification

The compatibility report in `verification-16.6.md` records the initial local
and Actions results and subsequent fixture corrections. Full workspace CI,
coverage, security scans, locked Console checks, package verification and the
required released ordinary/Factory slices must be recorded against their exact
source revisions before this task is checked. A skipped backend is not a
qualification. Real-model calibration and bounded-control promotion retain
task 16.8's independent gates.
