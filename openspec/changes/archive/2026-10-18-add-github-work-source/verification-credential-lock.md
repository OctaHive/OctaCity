# Credential lock lifetime regression

## Cause and correction

The intermittent `Conflict` was a failed nonblocking directory `flock`, not a
credential generation or replay conflict. A parallel test/process spawn can
retain a reference to the locked open file description. Closing the parent's
descriptor alone does not end that lock while another reference survives;
`CLOEXEC` acts when the child executes, not throughout process creation.

The file adapter now returns a small `CredentialLock` guard. Its destructor
explicitly unlocks the existing descriptor before closing it, so the operation
owns the lock lifetime. Every existing private publication caller already holds
that guard for its scope, including early returns. Nonblocking acquisition and
genuine concurrent-operation conflicts remain unchanged. There are no retries,
new waits, timeout changes, test serialization, dependencies or public credential
port changes.

The OS lifetime rule is documented by
[Apple's flock manual](https://developer.apple.com/library/archive/documentation/System/Conceptual/ManPages_iPhoneOS/man2/flock.2.html)
and the [Linux flock manual](https://man7.org/linux/man-pages/man2/flock.2.html).
The deterministic regression models that rule with `dup`, which shares the open
file description just as descriptor inheritance does. It uses the file adapter's
existing lock interface to arrange the OS state, then asserts outcomes through
the public `ProviderCredentialPublisher::discard` contract. No private lock field,
scheduler delay, fork hook or production test injection is needed.

## Reproduction and TDD

Builds ran one at a time with:

```sh
export CARGO_INCREMENTAL=0
export CARGO_PROFILE_DEV_DEBUG=0
export CARGO_PROFILE_TEST_DEBUG=0
export CARGO_BUILD_JOBS=2
```

1. `cargo test --locked -p octacity-github --test work_credentials` initially
   passed all 29 tests. Running that produced test binary 20 times with the
   ordinary parallel harness reproduced four failures: one cleanup-after-crash
   failure and three recoverable-publication failures, all unexpected `Conflict`.
2. A temporary, secret-free diagnostic at lock acquisition confirmed
   `EWOULDBLOCK`/`WouldBlock`. It was removed before the regression and fix.
3. **Red:**
   `cargo test --locked -p octacity-github --lib completed_credential_scope`
   failed deterministically: `discard` returned `Err(Conflict)` instead of
   `Ok(())` after the original scope ended while its duplicated descriptor stayed
   open. The first fixture attempt required correcting its private root directory
   permissions; that setup failure is not counted as Red.
4. **Green:** the same command passed after explicit guard-owned unlock. The test
   also asserts that an active scope still rejects competing cleanup with
   `Conflict` and that repeated cleanup after release remains idempotent.

The original cleanup/recovery tests keep their existing timeouts, assertions and
parallel scheduling. No failure is skipped or retried within a test.

## Qualification

- `cargo test --locked -p octacity-github --no-fail-fast`: 108 passed, including
  the deterministic regression, all 29 credential tests, native Source/Reporter
  checks and existing independent Delivery coverage.
- The produced `tests/work_credentials` binary passed 50 complete runs with its
  ordinary parallel harness and another 50 with `--test-threads=64`: 100/100
  suites, or 2900 passing credential test executions, without per-test retries.
  The increased-concurrency runs overlap test execution rather than serialize it.
- `cargo clippy --locked -p octacity-github --all-targets --all-features -- -D warnings`:
  passed.
- `python3 tools/check_architecture.py`: passed, 63 packages / 253 dependency edges.
- `python3 tools/security_matrix.py validate`, `cargo fmt --all -- --check` and
  `git diff --check`: passed. Security inventory validation is not a full
  cross-platform security matrix run; this session exercises macOS, while the new
  OS-primitive regression is also enabled on Linux.

The parallel reproduction/qualification loop invokes the test executable from
Cargo's recorded `Running tests/work_credentials.rs (...)` line. This is a
reproduction recipe with the full-package output saved at the shown path:

```sh
python3 - <<'PY'
import pathlib, re, subprocess
text = pathlib.Path('/tmp/octacity-credential-regressions.log').read_text()
binary = re.search(r'Running tests/work_credentials.rs \(([^)]+)\)', text).group(1)
for arguments in ([], ['--test-threads=64']):
    for run in range(50):
        subprocess.run([binary, *arguments], check=True, capture_output=True, timeout=30)
PY
```

The subprocess timeout is only the diagnostic harness's hang watchdog; no existing
test or production timeout was modified. The deterministic regression itself uses
no sleeps, timeout assertions, process-spawn race or stress loop.

The change stays within private-file lock ownership. Provider identities, role
classes, native resource ceilings, manual Factory intake and future internal
Issue compatibility are unaffected. No live provider or local-stand database is
used. Work remains on `main`; no commit is created without a separate request.
At completion, about 18 GiB remain free and `target` occupies 5.4 GiB. Hash checks
confirmed all 70 unrelated previously changed files remain untouched; only the
5.5 report gained a link closing its original flaky-test finding. Build profiles,
dependency manifests and lockfiles are unchanged.
