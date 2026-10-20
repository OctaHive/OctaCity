# Octa 0.6.0 dependency upgrade

## Release inputs

OctaCity consumes the published [Octa v0.6.0 release](https://github.com/OctaHive/octa/releases/tag/v0.6.0)
at `3996594d5ff724d7b43888ac7db6df0b647e7d65`. The three path dependencies
(`octa-runner-protocol`, `octa-cache-protocol`, `octa-plugin-lock`), Cargo lockfile,
CI source revision, local-stand input manifest, staging allowlist and backend
runner default all select that release. Unrelated package versions are unchanged.

The release validator now requires the exact released capability pair: blocking
tool authorization and `public-command-file-observations.v1`. Existing strict
identity, supported Codex CLI 0.161.0, checksums and source-revision checks remain.
The local-stand manifest carries the matching capability pair; its synthetic
installation fixtures were updated to the same released contract.

| Verified input | SHA-256 |
| --- | --- |
| `octa-Linux-arm64.tar.gz` | `1b0c1f14583359f638c4f98aec4cdafbdbc88628b43b23f552ed82d9aeb404df` |
| Codeload source archive for the pinned revision | `46c07fb2478ec0eb661e6b530150051afc794a854e1c36fb1b08f01d35af1bda` |

The source archive was streamed and hashed within the existing 64 MiB bound
(801,863 bytes received). The Linux release was staged through the real
`tools/stage_octa_release.py --version 0.6.0 --revision 3996594d5ff724d7b43888ac7db6df0b647e7d65 --platform linux-arm64 --output <temporary-directory>`
boundary: archive sidecar, release contract, exact compatibility metadata,
version, revision and platform passed. No Linux executable was run on macOS.

## TDD

The public boundaries are the existing release-contract validator and checked-in
local-stand input validation, followed by real release staging and clean-checkout
Cargo/UI consumers.

| Command | Observed Red | Observed Green |
| --- | --- | --- |
| `python3 -m unittest tools.tests.test_validate_octa_release_contract.OctaReleaseContractTests.test_accepts_the_released_observation_and_blocking_authorization_contract` | Released 0.6.0 capability pair rejected as unsupported identity | Released metadata accepted; full contract/staging suite: 13 passed |
| `python3 -m unittest tools.tests.test_verify_local_stand_inputs.LocalStandInputTests.test_pins_the_released_observation_capable_octa` | Checked-in manifest still selected 0.5.1 | Version, exact revision, release digest, capabilities and repository pin consistency passed |

Related input/staging/configuration tests passed together: 81 tests. Regression
also exposed a stale filtered-Docker-context generator fixture missing the
existing comment byte bound. Only its synthetic schema and expected generated
field were corrected; the generator's production behavior was unchanged.
Environment setup and that fixture repair are not claimed as new behavioral Red.

## Clean-checkout verification

Two temporary Git clones were used: OctaCity at code commit `7eb19ef3`, and sibling
Octa detached at the exact release revision. Both initially had empty porcelain
status. All three dependency paths resolved into that released sibling, with
version 0.6.0. No uncommitted Octa files, existing generated UI API, installed UI
dependencies or previous build output were copied into the verification clones.

Rust uses the pinned 1.99.0 toolchain, `--locked --offline`, two build jobs,
`CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0` and one
temporary target directory. UI uses SHA-256-verified Node 24.21.0 and pnpm 12.8.1.
Its dependencies were installed with `pnpm install --offline --frozen-lockfile`.

| Command | Result |
| --- | --- |
| `python3 -m unittest discover -s tools/tests` | 456 run: 448 passed, 8 environment-qualified skips |
| `cargo test --locked --offline -p octacity-protocol -p octacity-runner -p octacity-server-store -p octacity-server-application -p octacity-server-api-rest --lib` | 827 passed: protocol 72, runner 48, store 81, application 614, REST 12 |
| `cargo test --locked --offline -p octacity-server-api-rest --test v1_handlers` | 144 passed |
| `cargo clippy --locked --offline -p octacity-protocol -p octacity-runner -p octacity-server-application -p octacity-server-api-rest --all-targets --all-features -- -D warnings` | Passed |
| `pnpm --dir ui run api:generate` | Passed against clean exported REST schema |
| `pnpm --dir ui run format`, `lint`, `typecheck`, `test:unit`, `build` | Passed: 33 script tests, 600 unit/component tests; existing architecture/bundle budgets |
| `pnpm --dir ui exec playwright test tests/browser/issue-activity.spec.ts tests/browser/issue-executions.spec.ts tests/browser/issues.spec.ts` | 9 passed |
| `cargo fmt --all -- --check`, `python3 tools/check_architecture.py`, `git diff --check` | Passed |

Final documentation records the current release pins and the completed 6.4–6.6
contracts; documentation-only edits have no executable behavior for TDD.
PostgreSQL crash qualification and real upstream operation-process qualification
remain recorded in the original 6.4/6.6 verification reports; they are not claimed
as repeated database or live-provider runs during this dependency update.

Free disk remained approximately 12 GiB or higher during verification. The
temporary clones, verified Node distribution, staged Linux archive contents and
build target are removed after checks. No user sources or unrelated caches are
removed. Both verification checkouts retain clean tracked state and lockfiles
remain byte-identical to the committed versions. The final documentation commit
changes only documentation, measured UI README figures and OpenSpec completion
markers; it does not change the tested production code.
