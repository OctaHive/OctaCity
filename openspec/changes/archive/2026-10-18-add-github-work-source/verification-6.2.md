# Task 6.2 — Opt-in live qualification preparation

## Result

Preparation is complete under task 6.2's explicit missing-input alternative.
**Live GitHub qualification and activation remain blocked / unqualified.**
No live provider check, preview, credential operation, polling, admission, label
write, remote fixture creation or cleanup was performed in this task.

The concrete operator procedure is
[Opt-in live GitHub Work Source qualification](../../../../docs/reference/github-work-source-live-qualification.md).
It records required identities and grants, read-only checks/preview, reviewed
Factory/outbox effects, replay, rotation/revocation, owned cleanup and qualification
criteria. It uses the existing public management/process/runtime contracts without
adding an ad hoc live script, native write producer or second credential mechanism.

## Missing inputs recorded on 2026-10-18

| Prerequisite | Observed status |
| --- | --- |
| Explicitly selected disposable repository | Not supplied in the user request or selected task artifacts. The project repository is not a disposable selection. |
| Exact isolated deployment and connection | No live management endpoint/connection ID/version or deployment scope was supplied. |
| Project/resource/purpose grants | No live Project ID/access revision or scoped read/write grant selection was supplied. |
| Installed executable/configuration and profiles | No operator-selected live registry/profile paths or exact Source/Reporter/selector/mapping pins were supplied. |
| Execution fixture and owned remote resources | No live fixture target/input identities, native test Issue IDs or ownership manifest were supplied. |

A bounded local preparation check found no environment variable names starting
with `OCTACITY_` and selecting `SOURCE`, `GITHUB`, `PILOT`, `STAND` or `CONFIG`.
`github-work.toml` and `config/github-work.toml` are absent in the checkout, and
the filename inventory contained no selected live-qualification installation.
Only selector names and file existence were inspected. No environment values,
credential contents, browser accounts, GitHub CLI login or external private
directories were used to infer a live target. This does not claim that no operator
installation exists elsewhere; none was selected for this task.

Task 6.2 explicitly says:

> If inputs are absent, record the blocker and keep live activation unqualified rather than invent credentials or silently mutate a user's repository.

Therefore the preparation/checklist item can complete with this recorded blocker;
it does not mark a live pilot as passed. The owner can later supply the exact
non-secret scope above and execute the runbook. Credentials belong only in the
existing protected write-only installation flow, not in chat or a committed file.

## Source checks used to prepare the runbook

- `server/api/octacity-server-api-rest/src/v1/openapi/operations/integrations.rs`
  supplies the exact connection/access/check/input/preview/diagnostic routes.
- `docs/reference/integration-management-contracts.md` supplies protected ingress,
  current-version/idempotency, explicit activation and separate purpose contracts.
- `server/infrastructure/octacity-github/src/protocol_io.rs` and `work_config.rs`
  supply the installed working-directory configuration, bounded private scope,
  executable/provider configuration and independently scoped handles.
- `server/infrastructure/octacity-github/src/connection_check.rs` confirms GET-only
  native identity checks and explicitly unverified Reporter write readiness.
- `docs/reference/factory-source-diagnostics.md` and `work-source-observations.md`
  supply no-mutation preview, retained Work/Run correlation and optional independent
  observations without a task-card dependency.
- The completed hermetic 6.1 pilot retains its existing qualification. It is not
  rerun or represented as a live GitHub account result here.

## Validation and TDD applicability

This slice changes documentation and task/evidence references only. It adds no
executable script, configuration, migration or observable production behavior;
the repository's documentation-only TDD exemption applies. No Red/Green result or
new live test pass is claimed. Executable live steps remain unrun for the recorded
missing-input reason, not substituted with compile or fixture results.

Validation commands:

```sh
openspec validate add-github-work-source --strict
openspec validate add-factory-work-card --strict
git diff --check
```

Local Markdown link targets and the runbook's method/path pairs were checked
against existing files and the authoritative REST operation declarations. All
checks passed. No compilation, dependency installation or service startup was
needed. Available disk space remained approximately 12 GiB; existing uncommitted
6.1 work was preserved. Task 6.3 and the broader 6.4 regression gate remain open.
