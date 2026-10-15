# Task 16.6: ordinary CI/CD compatibility and qualified v3 contracts

Status: in progress. Task 16.6 remains unchecked until the corrected revision
passes the required GitHub Actions contracts, including Windows and Linux
containerd/Microsandbox. No Factory platform is qualified by a skipped job.

## Revision and environment

- Initial corrected source: `52ea44971e04be77ae4db946bb6c6cf8da0915e5` on
  `codex/factory-compatibility-16-6`, based on
  `b974557645e6134f73fd8edbc86b96a9385043f8`.
- Rust `1.99.0`; macOS ARM64 host; PostgreSQL `18.6` in a separate disposable
  container; separate MinIO service. The existing local Factory stand is not
  stopped or reconfigured.
- Octa `0.5.1`, revision `e0a3c65fe010220c5a8162c52368b7c8f624ddb5`.
- Microsandbox `0.7.6`, verified ARM64 agentd digest
  `f858a7b227308b40e06387b99c5f9a99f5fb9f1aeaa70f47080a8998b87dd2ba`.
- Real VM contracts reuse the exact operator-installed firmware, image and
  private runtime roots qualified by 16.5. Operator packages assembled here
  use the normal checksummed distribution format with current development
  binaries; they are not a previously published release.
- Local logs and command/exit-code receipts are retained under
  `/tmp/octacity-166-evidence/`. Credentials and private environment files are
  not copied into this change.

## Confirmed local contracts

| Boundary | Observed result | Evidence |
| --- | --- | --- |
| Protocol, inventory, Job executor, placement, Agent API and REST | 259 passed | `portable-macos.log` |
| Ordinary Build admission, CQRS, store and Agent registration | 49 passed | `ordinary-builds.log` |
| Headless REST and PostgreSQL pipeline, Factory disabled and enabled-but-unused | 2 passed | `headless-postgres.log` |
| Migrations, ordinary retention and Factory retention | 5 + 4 + 7 passed after fixture correction | `postgres-green.log` |
| Lifecycle and protected reader | 47 passed before and after Unix-only helper gating | `lifecycle-baseline.log`, `lifecycle-green.log` |
| Real macOS Host execution and cancellation | 1 passed | `backend-host-macos.log` |
| Real Microsandbox v2/v3 positive and negative contracts | 1 passed with an explicit 2 MiB test stack | `backend-stack-green.log` |
| Release installation and compatibility harness | 22 passed | `release-harness.log` |
| Release fixture regression after the queue correction | 22 harness + 24 fixture tests passed; 8 provisioned scenarios ignored | `release-fixtures-queue-regression.log` |
| Packaged ordinary CI/CD matrix on a real Microsandbox VM, Factory disabled | 1 passed in 66.57 seconds | `ordinary-release.log`, `ordinary-release/microsandbox-matrix.json` |
| UI scripts and unit tests | 26 + 397 passed | `ui-unit.log` |
| Browser navigation, commands, accessibility and visual contracts | 69 passed | `ui-browser-macos.log` |
| Repository tools, including default stand lifecycle and platform exclusions | 417 run, 8 environment-gated skips | `tools-macos.log` |
| Immutable stand inputs and bounded build contexts | passed | `stand-inputs.log`, `stand-context.log` |
| Workspace architecture | passed: 59 packages, 237 edges | `architecture.log` |
| Strict Clippy for all changed packages, all targets/features | passed | `clippy.log` |
| Formatting and strict OpenSpec validation | passed | `format-final.log`, `openspec.log` |

The packaged ordinary matrix exercises manual admission and exact replay,
scheduled and internal downstream Builds, cancellation, distinct retry Attempts,
server/Agent recovery, remote-cache trust, Artifact download identity, log search,
two separately enrolled Agents and cleanup. No Factory Configuration or model
credential is installed. Its optional ordinary Codex fixture is explicitly off.

The protocol suite checks unchanged canonical v1/v2 bytes, old-Agent eligibility
for ordinary Jobs, and rejection of v3 Host fallback. Placement and executor
tests reject missing semantic capabilities before source acquisition or spawn.
The real v3 contract additionally compares the execution evidence with the
signed target and rejects Host provider evidence. Windows WHP preview and
Apple VF do not gain v3 qualification from these results.

## Corrections driven by the compatibility runs

### Migration expectations

The initial PostgreSQL run passed Factory retention but failed three migration
checks. Their table inventory predated migrations 56–58, and the obsolete
stage-journal blacklist incorrectly included the durable Decision Signal
request/receipt tables. The test inventory now explicitly includes the four
input/Signal tables, while retaining exact schema equality, previous-schema
absence, primary-key, foreign-key, document-bound, upgrade, reentrancy and
snapshot rollback checks. No migration or production store behavior changed.

The same command went from Red (three migration failures) to Green (all sixteen
selected migration/retention tests passing):

```shell
cargo test --locked --jobs 2 -p octacity-server-store-postgres \
  --test migrations --test retention --test factory_retention \
  -- --ignored --test-threads=2
```

### Real backend test stack

The original real Microsandbox test aborted with stack overflow locally and on
the Linux Actions runner. Extending its v3 assertions to require exact backend
evidence reproduced the same Red. Boxing the complete OCI scenario at the test
entry points removes its large nested Future from the test thread stack.
Production executor/backend code, enforcement requirements, negative cases and
resource bounds are unchanged. An enlarged stack was used only as a diagnostic;
the final Green explicitly uses the ordinary 2 MiB stack:

```shell
RUST_MIN_STACK=2097152 cargo test --locked --jobs 2 -p octacity-job \
  --test backend_contract microsandbox_backend_satisfies_the_real_runner_contract \
  -- --ignored --exact --nocapture
```

Evidence: `backend-stack-red.log`, `backend-stack-green.log`.

### Windows compilation scope

Windows strict Clippy rejected `ProtectedReadDispatcher::channel_scope` as
unused. Its sole consumer is the Unix private channel. The helper now has the
same `cfg(unix)` scope; no workload behavior or permission policy changes.
Existing lifecycle tests passed before and after this refactoring. Windows
Actions at `52ea449` confirmed that warning was resolved, then exposed unused
imports and `FixedInventory` in the rooted-reader test module. That module's
only test was already Unix-only; the same platform condition now covers its
fixtures too. All 47 lifecycle tests passed before and after moving this
condition (`lifecycle-fixtures-baseline.log`, `lifecycle-fixtures-green.log`).
These are compilation-scope corrections without new executable behavior;
existing behavioral tests are retained. Final Windows verification requires
Actions on the follow-up revision.

The follow-up Windows job passed strict workspace Clippy and rustdoc, then
exposed a Unix path literal in the OCI projection test. `/installed/readonly-tools`
is not an absolute Windows host path, so the existing production absolute-path
guard correctly rejected it. The fixture now supplies a temporary host-absolute
root to the engine and expected result. No production path check or backend
qualification changes. The original test passed on macOS before the fixture
refactoring; the full OCI library suite is rerun afterward.

Windows at `d6fd0d1` passed that OCI correction, then exposed an unrepresentable
download-capability date in the private-credential Job fixture. Its shared
transfer helper used `u64::MAX` milliseconds; Windows `SystemTime::checked_add`
correctly refused it before download. A portable regression first reproduced
the unbounded expiry locally. The helper now issues a real one-minute future
expiry. Production expiry/credential checks and Windows backend qualification
are unchanged. The original credential test passed locally before the change.

```shell
cargo test --locked --jobs 2 -p octacity-job --lib \
  protected_transfer_fixtures_have_a_bounded_future_expiry -- --nocapture
# Red: the u64::MAX fixture exceeded the independent one-minute bound.
cargo test --locked --jobs 2 -p octacity-job --lib -- --test-threads=2
# Green includes the transfer-expiry and private-credential behavioral tests.
```

Evidence: `factory-credentials-portability-baseline.log`,
`factory-transfer-expiry-red.log`, `factory-transfer-expiry-green.log`.

Windows at `e8fbb5b` passed the expiry correction, then reached the runner's
independent rooted-read proof. `RootedReadDirectory` explicitly refuses Windows
because that physical boundary is not qualified there. The test had wrongly
expected a successful synthetic Factory execution on every OS. It now keeps
the successful secret-injection/redaction assertions on Unix and asserts the
existing safe refusal, zero backend starts and absence of the secret in failure
diagnostics on Windows. No rooted-read guard, capability or qualification was
changed. The existing Unix behavior passed before the test-fixture correction;
all 51 Job tests passed afterward. Final Windows verification remains pending.

```shell
cargo test --locked --jobs 2 -p octacity-job --lib \
  installed_private_credentials_reach_only_the_runner_and_are_redacted_from_results \
  -- --nocapture
# Before fixture refactoring: passed on macOS; Windows Actions showed refusal.
cargo test --locked --jobs 2 -p octacity-job --lib -- --test-threads=2
# After refactoring: 51 passed on macOS; Windows refusal assertions await CI.
```

Receipts: `github-windows-e8.log`, `credential-rooted-scope-baseline`,
`credential-rooted-scope-green` and `credential-rooted-scope-clippy`.

### Release fixture TLS provider

A new HTTPS round-trip test first reproduced a panic when both Rustls crypto
provider features were present. The cache proxy now chooses its provider on
its own server builder, without modifying process-global provider state.
The Green verifies the exact upstream response through the declared CA and
rejects the same server without that CA:

```shell
cargo test --locked --jobs 2 -p octacity-release-harness \
  --test release_vertical_slice cache_proxy_supports_tls_with_its_declared_ca \
  -- --nocapture
```

Evidence: `cache-tls-red.log`, `cache-tls-green.log`. This changes a release-test
fixture, not the product's TLS configuration.

### Queue-latency measurement order

The corrected Linux Native direct and packaged contracts passed, but its
performance gate reported a 6,904 ms queue sample against the unchanged
5,000 ms limit. The recorder selected the first Job in management collection
order, which can be a dependent Job after its predecessor already executed.
A regression drives the serialized measurement boundary with the dependent
first and then reverses the same Jobs: the first order returned 6,000 ms
instead of the independently specified 100 ms initial acquisition delay.
The recorder now selects the earliest preparation timestamp across Jobs;
both orders produce 100 ms. No scheduling behavior or performance budget changes.

```shell
cargo test --locked --jobs 2 -p octacity-release-harness \
  --test release_vertical_slice \
  queue_latency_uses_the_first_lease_instead_of_job_list_order -- --nocapture
```

Evidence: `queue-measurement-red.log`, `queue-measurement-green.log`.
The old measurements remain rejected; only a fresh released run can attest
to the corrected performance gate.

## GitHub Actions evidence

- [Portable CI, console and local-stand gates](https://github.com/OctaHive/OctaCity/actions/runs/38807864983)
  run the original `b9745576` source. Windows strict Clippy exposed the helper
  scope issue above; both local-stand staging/container jobs passed.
- [Cross-platform packaged Host contracts](https://github.com/OctaHive/OctaCity/actions/runs/38807865077)
  cover Linux AMD64, macOS ARM64 and Windows AMD64 at `b9745576`.
- [Operator-dispatched hosted backend matrix](https://github.com/OctaHive/OctaCity/actions/runs/38808334710)
  uses the same exact revision. The Linux Microsandbox job reproduced the
  default-stack overflow; the Linux Native packaged matrix also reproduced the
  ambiguous Rustls provider panic.
- [Corrected portable CI, console and local-stand gates](https://github.com/OctaHive/OctaCity/actions/runs/38810267781)
  run `52ea44971e04be77ae4db946bb6c6cf8da0915e5`; Windows exposed the additional
  rooted-reader test-fixture scope issue. Other results are pending.
- [Corrected operator-dispatched hosted backend matrix](https://github.com/OctaHive/OctaCity/actions/runs/38810688530)
  runs the same corrected revision, with the `hosted` suite. Real Linux
  Microsandbox and containerd direct v2/v3 and packaged ordinary slices passed,
  as did all three released Host platforms, lifecycle, failure and security
  matrices. Linux Native direct and packaged functional slices passed; its
  performance gate failed on the collection-order metric corrected above.
  The aggregate evidence job therefore correctly failed. The old report remains
  rejected; it has not been rewritten to claim a passing performance gate.
- [Follow-up main CI](https://github.com/OctaHive/OctaCity/actions/runs/38818531599)
  runs `e8fbb5bb28214bdb0c576cf13ec7f30f554a50ed`, including the Unix fixture
  scopes, host-absolute OCI path, bounded transfer expiry and queue metric
  corrections. Windows workspace Clippy and strict rustdoc passed; portable
  tests exposed the incorrect rooted-read positive described above. Both stand configuration jobs
  passed. Console's locked checks, 397 unit tests, 69 browser tests and packaged
  browser slice also passed. Final portable/coverage outcomes remain pending.
- [Released Host contracts at e8fbb5b](https://github.com/OctaHive/OctaCity/actions/runs/38818531579)
  passed Linux AMD64, macOS ARM64 and Windows AMD64, including the aggregate
  evidence job. This run contains the final queue, OCI and expiry corrections;
  it does not qualify Factory execution on Windows.
- [Final code CI at 3d62d85](https://github.com/OctaHive/OctaCity/actions/runs/38822065162)
  includes the platform-aware private-credential assertions and exact Shadow
  read budget. Both stand jobs and Console passed; Windows strict Clippy and
  rustdoc passed. Full portable and coverage outcomes remain pending.
- [Final-code released Host matrix](https://github.com/OctaHive/OctaCity/actions/runs/38822065251)
  passed all three platforms and its aggregate evidence job.

The fresh independent packaged ordinary matrix passed again in 64.61 seconds
using the current checksummed candidates built from `0b3c48b`. The composed
packaged Factory pilot also passed in 654.33 seconds after the eight-byte Shadow
fixture correction. Their exact environments, commands, scope and retained
receipts are recorded in `verification-16.7.md`.

The push-triggered backend workflow runs released Host contracts only. It does
not establish a fresh full Linux Native/containerd/Microsandbox or performance
qualification. A later operator-dispatched or scheduled full run must qualify
the complete corrected revision. No browser dispatch is attempted by this
session, as requested by the operator.

The original runs cannot attest to the corrections. Final platform outcomes
for the corrected source must be recorded before closing the task. Optional
self-hosted Apple VF, macOS Microsandbox and Windows WHP jobs skipped by the
hosted suite are not treated as successful backend qualifications.
