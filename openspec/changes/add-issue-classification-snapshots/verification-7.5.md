# Task 7.5: packaged Issue materials

Status: complete. Both native journeys passed on 2026-11-14; all 44 change tasks
are checked. No archive, spec synchronization, commit or real-provider effect was
performed by this task.

The [subsequent review fixes](verification-7.3-7.5-review-fixes.md) strengthen
selection cardinality, ownership rejection and visible selector checks, with
mutation regressions and a repeated complete native qualification. The original
results and identities below remain historical evidence.

## Boundary and fixtures

The existing packaged `factory_pilot` runs the real Console behind its existing
same-origin nginx launcher, real management HTTP, PostgreSQL, MinIO, ordinary
Agent Jobs and native Microsandbox. Playwright uses
`tests/released/issue-classification-snapshots.spec.ts` without HTTP interception.
The HTTPS/Git publication peer is disposable and has no production credentials.

Project settings publish an ordinary bounded text field and a choice field bound
to severity, alongside built-in component choices. The operator fills an Issue,
publishes two Issue-owned document versions, selects the first version, separately
links an exact shared Project publication, posts and selects one comment, reviews
the frozen input and explicitly confirms the consumer launch. Document publication
and comment append alone never select materials or create executions.

The Issue document is absent from Project catalogs, the shared-document picker
and the step document selector. A second Issue in the same Project cannot read
the publication or select it under a forged owner; rejection changes neither its
version nor materials and creates no execution.

Two read-only reasoning steps execute as ordinary native Jobs. The first projects
the explicitly selected fields and three texts. The second projects only the
component. The finite model's bounded `data_checks` inspect nested string values
in frozen input and separately selected immutable Context, excluding program
examples and compiler prose. They reject unexpected text before file/tool effects.
Their results are independently checked by downloading ordinary
`codex-run-result` artifacts through management HTTP.

The delivery retains protected-test authoring, coding, original validation,
isolated verification and accepted-PR crash/recovery. Later Issue/Project document
publications and an unselected comment must leave the Issue selection and admitted
snapshot unchanged, including after another Server/Agent restart. The existing
exact producer-result pilot separately creates and executes a fresh pipeline.

## Observed Red/Green slices

- Initial native Red: the Project catalog lacks `Material environment` (20.32 s).
  The packaged Console field-settings phase then passes.
- Material-selection Red: a new Issue has zero selected sources rather than
  three (25.42 s). The Console setup and original full native pilot then pass
  (208.86 s).
- Consumer Red: preview has no `context` instead of the three selected texts
  (35.32 s). The consumer now declares its field/material contract and explicit
  per-step input mappings.
- CLI input-check Red: previously unsupported data checks reject valid positive
  and negative programs. Green accepts both and rejects contradictory data before
  writing files. A separate Context-only Red/Green verifies the compiler's
  independent immutable Context section. The complete process suite passes
  (23 tests).

Intermediate qualification setup failures are not counted as Red: the desktop
form needed its existing always-visible Properties sidebar, the first node needed
a matching root input schema and declared Task Context, and native probes needed
the existing observer's output allowance for compiler-owned outputs. Additional
ordinary Jobs use the published Run time budget instead of the shorter legacy
pilot wait window. The shared completion/Build expectations include the two new
probes; later discussion is read through the existing timeline rather than a
nonexistent `GET /comments` collection. No production behavior was changed to
accommodate the fixture.

## Final observed checks

- `packaged_issue_materials_remain_owned_frozen_and_scoped`: **1 passed**,
  **285.92 s**, including all three real browser phases, two ordinary probe Jobs,
  protected delivery, later publications/comment and final Server/Agent restart.
- `packaged_delivery_recovers_the_accepted_real_review_after_server_restart`:
  **1 passed**, **226.48 s**, with a fresh Project, producer, explicit
  producer-result consumer, ordinary Jobs and confirmed test PR.
- Default `cargo test --locked --offline -p octacity-release-harness --test
  factory_pilot`: **10 passed, 18 ignored**. These defaults do not substitute for
  either explicitly executed native test above.
- `python3 -m unittest tools.tests.test_factory_pilot_cli
  tools.tests.test_local_stand_documentation`: **26 passed**, including the 23
  finite process regressions.
- From `ui`, `pnpm api:generate` left generated contracts unchanged;
  `pnpm typecheck`, `pnpm lint`, `pnpm format` and `pnpm test:unit` passed
  (**46 generator/script tests, 1,246 Vitest tests**). `pnpm build` passed its
  production bundle and architecture checks before Console packaging.
- Strict Clippy of `factory_pilot`, workspace Rust formatting, architecture
  validation (64 packages / 264 edges), strict OpenSpec validation, 133 local
  Markdown links/anchors and whitespace checks passed.

The observed native commands used the built `factory_pilot` binary through
task-local environment wrappers under `target/qualification/issue-snapshot-75`:
`run.py` for materials and `run-compat.py` for the direct-result control, each
with the exact test name, `--ignored --exact --test-threads=1 --nocapture`.
The compatibility control used separate short work/state directories and a
separate Microsandbox environment identity. Logs are in
`/tmp/octacity-75/native-material-green.log`, `native-direct-green.log`,
`native-fields-red.log`, `native-materials-red.log`, `native-snapshot-red.log`,
`cli-data-red.log`, `cli-context-red.log` and the corresponding check logs.

## Qualified identities

The production Server/Agent sources are unchanged from the qualified
`e0288d7c43135cbff2841709e36612264455a332` baseline. Official release packagers
assembled compatible 0.1.0 Server/Agent/Console layouts at working-tree HEAD
`293de8e0b70d8802085284b3d1d0a105b27a18d9`, retaining the verified Server/Agent
binaries and rebuilding Console assets. Checksums and compatibility validation
were not weakened.

| Component | Qualified identity |
| --- | --- |
| Host / backend | macOS ARM64 / native Microsandbox 0.7.6 |
| Octa | 0.9.0, source `fc389ac017f705a0464ffebd8c4e967ce880e1ca` |
| Server SHA-256 | `baf81067650abca4680471ac1536156c44269389911e40a2b73950ad4e536685` |
| Agent SHA-256 | `3042f38b60120af245325a6d5d8d77d89707e2f1675a37f8bb739bedfa235843` |
| Console index SHA-256 | `90570d9d853a15bfce8703acbaf7f88d85ffe544e8a0040ebe73a6fed4ba14b4` |
| Finite CLI SHA-256 | `2444c96028aeabe5af3e2f46fd1262a31e3172033af27100d1c7abd478393702` |
| Distinct verifier CLI SHA-256 | `866b9dbd61eb2d0d358b6c51ab52898fae436c85142a99c10cdef07ab1c7659a` |
| Linux ARM64 guest digest | `sha256:15746790d583fbb1335aec0c95e89cc9a2d3e7a674a58e70075bce07800ca10f` |
| Browser toolchain | Node 24.21.0 / pnpm 12.8.1 / nginx 1.31.8 |

## Reproduction and evidence

Use the qualified environment documented in
[backend contracts](../../../docs/testing/backend-contracts.md#packaged-issue-material-snapshots).
Server, Agent and Console must have compatible checksummed release manifests;
Server/Console share an installation parent and source identity. Provide the
released Console, UI root, pinned Node/pnpm, nginx and MIME table in addition to
the ordinary native pilot environment.

```sh
CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 \
  cargo test --locked -p octacity-release-harness --test factory_pilot \
  packaged_issue_materials_remain_owned_frozen_and_scoped \
  -- --ignored --exact --test-threads=1 --nocapture
CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 \
  cargo test --locked -p octacity-release-harness --test factory_pilot \
  packaged_delivery_recovers_the_accepted_real_review_after_server_restart \
  -- --ignored --exact --test-threads=1 --nocapture
python3 -m unittest tools.tests.test_factory_pilot_cli
```

`OCTACITY_RELEASE_EVIDENCE_DIR` names an existing absolute private directory.
The material journey writes mode-0600 browser phase logs and
`issue-materials-<run-id>.json`, containing the execution, retained snapshot,
completed Run and the two independently read ordinary Build/report references.
Source text is synthetic private qualification data. Failed browser traces remain
in the disposable case directory; they are not repository fixtures.

The final material proof is
`target/qualification/issue-snapshot-75/evidence/issue-materials-f842ba26-cdd2-4609-8ae6-16552417a85a.json`.
It retains Configuration `fa001a54-7925-4dbf-b50c-1270964c0897` v1, Issue
`0f9cd675-4531-4cbe-8a4c-985607ffd3e8` revision 6, execution
`051aa5c4-e117-425b-9265-c47dc45386fe` and Work
`de0231a0-3159-4e99-b7c1-f1b4328387b1`. The snapshot digest is
`389fa77a7d0debfc55507d2302d40db0f60ed036c828d0a8ccfb1f35b187e803`.
Ordinary probe Builds are `53d71bf4-57e3-5f99-b87c-7eee6740e8d7` and
`d3c609b0-19ad-56c5-b4aa-3757a2e9a397`; the completed Run is
`f842ba26-cdd2-4609-8ae6-16552417a85a`.

Only task-owned PostgreSQL/MinIO containers are removed after checking their
qualification labels. Private proof logs and verified release assets remain
outside tracked code. The user's running stand and token file are untouched.
