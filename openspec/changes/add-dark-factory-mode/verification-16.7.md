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

## Workspace, tools and public documentation

```shell
CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 \
  cargo test --locked --jobs 2 --workspace --all-features
# 1,879 passed; 0 failed; 205 provisioned/environment scenarios ignored.
# 222 test-result groups; 1,020.23 seconds.

python3 -m unittest discover -s tools/tests -v
# 429 run, 6 environment-gated skips, 0 failures; 67.44 seconds.

RUSTDOCFLAGS='-D missing_docs' CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0 \
  cargo doc --locked --jobs 2 --workspace --all-features --no-deps
# passed; 41.22 seconds.
cargo fmt --all -- --check
python3 tools/check_architecture.py
# passed; 59 packages, 237 dependency edges.
```

The full Rust run preceded the final transfer-expiry fixture regression. Its
follow-up `cargo test --locked --jobs 2 -p octacity-job --lib --
--test-threads=2` passed all 51 tests, including the newly added expiry case;
strict Clippy for that package also passed. This follow-up is documented with
its Red/Green command in `verification-16.6.md`. The new offline calibration
CLI followed independent Red/Green slices, documented in `verification-16.8.md`;
all 12 CLI tests are included in the 429-tool regression. No ignored provisioned
test is counted as a successful backend run.

Receipts: `workspace-final-baseline`, `factory-transfer-expiry-green`,
`tools-calibration-final`, `docs-strict-final`, `format-final-qualification`,
`architecture-final`, `specs-final` and `openspec-final`. Documentation-only
changes have no executable behavior for TDD; their validation is specification,
link, formatting and public-documentation checks.

The existing advisory-authority negative was tightened to supply a valid prior
shadow report, so it cannot pass merely because that precondition was missing.
This test-fixture refactoring changes no CLI behavior. The complete 12-test
module passed before and after; `tools-calibration-preconditions` then repeated
the full 429-tool regression successfully (six skips, 66.42 seconds).

## Secret scanning and bounded fuzzing

Gitleaks 8.30.1 scanned the committed source archive at
`3d62d85bc83b53000fb228f3afb97c98ec8f2a27`, including the new calibration tool,
tests and seed corpus, without findings. The separately scanned Octa source is
the pinned `e0a3c65fe010220c5a8162c52368b7c8f624ddb5`; it also passed. Existing
scanner rules and the exact test-only TLS-key exception were unchanged. The
downloaded scanner checksum was verified against its release asset digest.

All four protocol fuzz targets passed with cargo-fuzz 0.13.2 and pinned
`nightly-2026-10-01`, each using the workflow's 30-second bound and separate
task-local corpus/artifact directories:

| Target | Executions | Result |
| --- | ---: | --- |
| `server-agent-json` | 57,615 | passed |
| `runner-json` | 452,775 | passed |
| `source-plugin` | 943,846 | passed |
| `server-protocols` | 557,559 | passed |

```shell
RUSTUP_TOOLCHAIN=nightly-2026-10-01 CARGO_BUILD_JOBS=2 \
  cargo fuzz run TARGET /tmp/octacity-166-evidence/fuzz/corpus/TARGET -- \
  -max_total_time=30 \
  -artifact_prefix=/tmp/octacity-166-evidence/fuzz/artifacts/TARGET/
```

The first build failed because local Apple clang could not locate SDK libc++
headers (`cassert`/`cstdint`). A standalone include probe reproduced it; adding
the installed SDK's libc++ include directory and sysroot through task-local
`CXXFLAGS` restored the build. That environment failure is not fuzzing evidence
or a behavioral Red. No repository compiler/configuration exception was added.
The successful logs/receipts are `fuzz-<target>`; Gitleaks receipts are
`gitleaks-code-final` and `gitleaks-octa-final`. Fuzzing is bounded
decoder coverage and does not replace runtime isolation or adversarial tests.

## Fresh candidate package verification

Built Server, Agent and Git source plugin with locked dependencies at
`0b3c48b7af136c412783afca8b9047145b96ae70`, then assembled deterministic
macOS ARM64 candidate packages with the normal packaging tools. Source plugin
metadata comes from that built executable. Both manifests retain the exact
source revision; every listed checksum was independently verified (Server six
files, Agent nine). These are current development-binary candidates in the
release distribution format, not published optimized releases. No release tag
was created.

Receipts: `package-current-build`, `package-current` and
`current-packages/verification.json`. Final platform outcomes remain separate
required results until recorded below.

## Shadow pilot response budget regression

The first fresh composed pilot failed after 205.68 seconds, with six successful
ordinary Builds and one failed tool Job. The original Source contains the
eight-byte text `baseline`; the standalone controlled-tool fixture contains
five-byte `facts`. The Shadow scenario inherited that standalone fixture's
five-byte response ceiling. The Agent correctly denied the oversized read;
its independent permission and byte-budget checks were not weakened.

Existing fast pilot contracts passed before and after extracting the unchanged
fixture limit (six passed, eleven provisioned scenarios ignored). A new test
then used the public `RootedReadDirectory` boundary with an eight-byte original
and nine-byte overrun. It failed on the valid original under the old limit
(`InvalidData: regular file exceeds its read bound`). Only this Shadow scenario
now declares an eight-byte per-call and total limit. The test then accepted the
whole original and refused the overrun; all seven fast pilot tests and strict
harness Clippy passed. The standalone five-byte fixture remains unchanged.

```shell
cargo test --locked --jobs 2 -p octacity-release-harness --test factory_pilot \
  shadow_reader_allows_the_original_baseline_but_refuses_a_larger_response \
  -- --nocapture
# Red before the fixture correction; Green after it.
cargo test --locked --jobs 2 -p octacity-release-harness \
  --test factory_pilot -- --test-threads=2
# Green: 7 passed, 11 provisioned scenarios ignored.
cargo clippy --locked --jobs 2 -p octacity-release-harness \
  --all-targets --all-features -- -D warnings
# passed.
```

Receipts: `factory-pilot-response-limit-red`, `pilot-fixtures-before-limit`,
`pilot-limit-extraction`, `pilot-response-limit-red`, `pilot-response-limit-green`
and `pilot-limit-clippy`. The earlier incorrect sibling installation layout was
also corrected before this run; its immediate environment failure is retained
as `factory-pilot-installation-environment` and is not behavioral Red evidence.

## Fresh composed packaged Factory pilot

After the Shadow fixture correction, the complete composed contract passed in
654.33 seconds: one test passed, zero failed or ignored. It uses the fresh
checksummed Server/Agent candidates built from `0b3c48b`, the pinned Octa,
qualified real Microsandbox image, disposable PostgreSQL/object storage and the
existing immutable reader, model, verifier, Git delivery and deployment fixture
installations. The harness includes the response-limit correction committed
as `3d62d85`. No source compilation or package verification was skipped.

```shell
python3 /tmp/octacity-166-run-pilot-current.py
# Invokes, with the private qualified environment and current sibling packages:
cargo test --locked --jobs 2 -p octacity-release-harness --test factory_pilot \
  -- --ignored --exact \
  packaged_nested_criteria_drive_private_tests_before_review_delivery_and_promotion \
  --nocapture --test-threads=1
# 1 passed; 654.33 seconds in the test, 675.94 seconds including compilation.
```

The assertions verify A's 23 completions and 12 ordinary Jobs, B's reversed
optional phases (nine completions, four Jobs), and C without those phases
(seven completions, two Jobs): 39 completions and 18 Jobs including bootstrap.
They cover actual accepted-criteria/private-test lineage, candidate capture,
an independent verifier Agent, retained EvidenceManifest/EvaluationPlan/
Assessment/Decision, HTTPS Git review and lost-response replay, actual staged
candidate deployment/readback, disagreeing routing/tool-risk Shadow signals,
and unchanged immutable records and append-only audit after Server and both
Agents restart. Logs and the command/exit/duration receipt are retained as
`factory-pilot-current.log` and `.json`; package manifest/checksum evidence is
retained separately. Private temporary runtime state is cleaned by the test.

The provider wire and model are hermetic installed fixtures. CI/CD delivery and
staged-deployment promotion in this journey do not qualify real JEV behavior or
authorize Signal bounded-control promotion. Task 16.8 remains independent.

## Fresh independent ordinary CI/CD slice

The same current candidate installation also passed the real packaged macOS
Microsandbox ordinary matrix with Factory disabled: one test passed, zero failed
or ignored, in 64.61 seconds. The optional ordinary Codex fixture was disabled;
no Factory Configuration or model credential was installed in this scenario.

```shell
python3 /tmp/octacity-166-run-ordinary-current.py
# Invokes with current sibling packages and OCTACITY_RELEASE_CODEX_FIXTURE=0:
cargo test --locked --jobs 2 -p octacity-release-harness \
  --test release_vertical_slice -- --ignored --exact \
  released_macos_microsandbox_matrix_satisfies_the_end_to_end_contract \
  --nocapture --test-threads=1
```

This covers manual admission/replay, scheduled and downstream Builds,
cancellation, distinct retry Attempts, Server/Agent recovery, TLS remote-cache
trust, Artifact download identity, log search, two Agents and cleanup. Retained
evidence: `ordinary-current.log`, `ordinary-current.json` and
`ordinary-current/microsandbox-matrix.json`. The packaged Factory and ordinary
results are independent; neither relies on a model's claimed outcome.

## Hosted Console and adversarial evidence

The [main Console job at e8fbb5b](https://github.com/OctaHive/OctaCity/actions/runs/38818531599/job/116709134183)
passed locked dependency installation, generated-contract checks, formatting,
lint/type/build checks, 397 unit tests, 69 Playwright tests and the packaged
browser release slice. Its log is retained as `github-console-e8.log`.

The [hosted security matrix at 52ea449](https://github.com/OctaHive/OctaCity/actions/runs/38810688530/job/116682911848)
passed the complete matrix, including Factory authority negatives; its log is
`github-security-matrix.log`. This is evidence at the stated revision, not a
substitute for a fresh complete backend qualification of the later corrections.

The [final-code Console job at 3d62d85](https://github.com/OctaHive/OctaCity/actions/runs/38822065162/job/116720737781)
also passed. Both final-code stand jobs and the [complete released Host matrix](https://github.com/OctaHive/OctaCity/actions/runs/38822065251)
passed. Portable Rust tests and coverage are still being observed. The source is
already fast-forwarded into `main`. Task 16.7 remains unchecked pending the
remaining exact-source qualification gates, including fresh full Linux Native
performance evidence; no performance threshold or qualification policy changed.

## Fresh candidates after the Git path correction

After the Windows canonical-path Reds documented in `verification-16.6.md`,
rebuilt the locked Server, Agent and Git source plugin at
`1aa1aa4e8316a26753cd86cb295de2d3a49aecf4`. The normal packaging tools produced
new sibling macOS ARM64 candidates, with pinned Octa in the same installation
directory. These are development binaries in the release distribution format;
they are not published optimized releases. Independently verified every listed
checksum: six Server files, nine Agent files and fourteen pinned Octa files.

The Server executable SHA-256 remains
`aea9b94c828be540c5960f940fa6d41f47543a04038f2a8924e20fab9a39d78f`;
the rebuilt Agent is
`947afb3a1cbfbe9a905b633424db1d6a10d3d36255fd45267dbe4ff85c8aa09d`.
Both manifests bind the exact `1aa1aa4` source revision. Package build/create
receipts are `canonical-package-build` and `canonical-package-create`;
independent manifest/component verification is
`/tmp/octacity-166-evidence/canonical-packages/verification.json`.

Both independent packaged journeys passed again with this installation:

```shell
python3 /tmp/octacity-166-run-pilot-canonical.py
# Same exact composed factory_pilot contract as above:
# 1 passed, 0 failed/ignored; 728.29 seconds test / 729.19 seconds command.
python3 /tmp/octacity-166-run-ordinary-canonical.py
# Same exact released_macos_microsandbox matrix as above, Factory disabled:
# 1 passed, 0 failed/ignored; 68.32 seconds test / 73.73 seconds command.
```

The composed journey retains all 39 completion / 18 ordinary Job assertions,
private-test and capture lineage, independent verification, actual Git review
and staged deployment/readback, restart/audit continuity and Shadow authority
negatives described above. The ordinary matrix independently checks unchanged
CI/CD behavior. Runtime model/provider fixtures remain hermetic; this does not
qualify real JEV or authorize bounded control. No assertion or budget changed.

Retained receipts/logs: `factory-pilot-canonical`, `ordinary-canonical`,
`canonical-sequence`, and `ordinary-canonical/microsandbox-matrix.json`.
The sequence removed only its two PostgreSQL/MinIO containers after verifying
their exact ownership label and pinned image. Two setup failures are retained
separately with `-layout-preflight` and `-storage-preflight` suffixes: the first
used Octa from a different installation directory; the second lacked the fresh
MinIO bucket. Restoring the sibling layout and expected bucket resolved them.
These environment failures are not behavioral Reds or qualification outcomes;
product readiness guards and timeouts remain unchanged.

During package preparation, free space fell below 5 GiB. The task monitor
validated the exact repository `target` path, owner and `CACHEDIR.TAG`, removed
only that directory and reported 15.89 GiB free. Built candidate inputs had been
retained independently; the harness then rebuilt successfully in 3m37s.

The new dependency-policy test passed its own Red/Green cycle, and all 430
repository-tool tests passed (six environment-gated skips). Latest source
security, both stand jobs and Console passed in
[branch CI](https://github.com/OctaHive/OctaCity/actions/runs/38832053570) and
[security](https://github.com/OctaHive/OctaCity/actions/runs/38832053600).
Windows quality, remaining platform quality and coverage for `1aa1aa4` are
still being observed. Tasks 16.6 and 16.7 remain unchecked until their complete
required qualification is confirmed, including fresh Linux Native performance.
