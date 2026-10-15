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

## Operations and reference updates

Added `docs/operations/factory.md` for default/opt-in mode selection, independent
readiness, exact profile/Agent preparation, the current Artifact-based Work
journey, guarded intent versus owner completion, classified provider failure,
cancellation/restart, backup consistency and separate release/rollout gates.
Updated the documentation index, architecture, threat model, server and stand
operations, Console journey, backup/restore, management REST, transport v1,
JobSpec v3 and backend qualification references to connect those contracts.
Relative link destinations were checked. The guide identifies quickstart,
Work-card and concrete ticket integrations as follow-ups rather than installed
behavior, and distinguishes provider-wire fixtures from real-model calibration.

## Dependency and license scans

All commands below passed against the unchanged committed lockfiles:

```shell
cargo audit
cargo audit --file fuzz/Cargo.lock
cargo deny --locked check licenses sources
cargo deny --locked --manifest-path fuzz/Cargo.toml --config fuzz/deny.toml \
  check licenses sources
# cargo-audit 0.22.2; cargo-deny 0.20.2.

# In ui/, using the packageManager version without install scripts:
npm exec --yes --ignore-scripts --package=pnpm@12.8.1 -- \
  pnpm audit --audit-level low
# No known vulnerabilities found.
```

The preinstalled global pnpm 9 and old Corepack launcher could not read/launch
the pinned manager; those environment failures are not audit outcomes. The
successful UI audit used pnpm 12.8.1. Logs are `audit-production`, `audit-fuzz`,
`licenses-production`, `licenses-fuzz`, and `ui-audit-pinned.log` in the task
evidence directory. No dependency, audit exception or package-manager policy
was changed to obtain these results.
