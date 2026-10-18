# Task 3.2 verification

## Scope

The existing GitHub executable now validates independently enabled Issue Source
and Reporter scopes through private `github-work.toml`. Source requires a stored
`work_source_read` grant; Reporter observe/project requires a separate stored
`work_reporter_write` grant. Neither borrows Delivery, repository-read or Job
authority. Issue-only installations need no Delivery config, Git executable or
Artifact inputs. Shared API/TLS, private-file and grant parsing helpers preserve
the existing Delivery format and exact configuration digest.

Exact native ticket-container identity, installed alias, immutable profile and
actual API host are checked before opening a credential file. The installed
profile and private roots are rechecked per operation. A replaced profile fails
as `conflict`; unsafe files, escaped directories and invalid grants fail as
`permanent`, without raw diagnostics. Credential contents are excluded from the
profile digest, allowing private token rotation. The retained grant owns zeroing
token storage and has a redacted Debug representation.

The actual process invokes this authorization boundary. Absent Work installation
remains `unsupported`, and cancellation remains credential-free. Authorized Issue
listing/projection still returns `unsupported`: network operations, selector
loading, effect mappings and worker activation belong to tasks 3.3 and later.
No provider call or live credential was used here.

Provider-specific installation types remain inside `octacity-github`; Factory
and the Source/Reporter contracts retain opaque provider identities/resources
for GitLab, Jira and other providers. Manual Factory admission remains available.

## TDD evidence

The public boundaries are `GithubWorkProvider::load`/`prepare` and the real
`octacity-github` process. Private fixtures and a nonblocking loopback peer prove
that preparation/rejection sends no provider request. Tests inspect public
results and process frames, never private implementation state.

| Slice | Observed Red | Observed Green |
| --- | --- | --- |
| Issue-only read installation | Public Work provider API was missing (E0432) | Source read grant resolves the exact native container without Delivery/Git material |
| Separate Reporter authority | Reporter preparation returned `Unsupported` | Reporter-only installation accepts its write grant and rejects other stored classes |
| Installed profile drift and rotation | Loaded provider accepted a request after its profile file changed | Old profile fails `Conflict`; reloaded profile accepts token rotation without repinning |
| Private grant directory drift | Loaded provider accepted grants after its directory became 0755 | Both roles fail `Permanent` before protected file access |
| Process integration | Missing grant returned the placeholder `Unsupported` | Actual process returns `Permanent`; valid grants, stale profiles and malformed installations have their expected closed outcomes |
| Nested directory confinement | Replacing a parent with an outside symlink still resolved a grant | Rechecked confinement rejects the redirected directory as `Permanent` |

Focused Red commands (Green was verified by focused or complete suite runs):

```sh
cargo test --locked --jobs 2 -p octacity-github --test work_credentials issue_read -- --nocapture
cargo test --locked --jobs 2 -p octacity-github --test work_credentials reporter_grant -- --nocapture
cargo test --locked --jobs 2 -p octacity-github --test work_credentials profile_drift -- --nocapture
cargo test --locked --jobs 2 -p octacity-github --test work_credentials private_grant_directory -- --nocapture
cargo test --locked --jobs 2 -p octacity-github --test work_credentials provider_process_checks -- --nocapture
cargo test --locked --jobs 2 -p octacity-github --test work_credentials changed_nested -- --nocapture
```

Additional passing regression cases cover exact native id/alias/network/profile
ceilings before a missing grant, grant repository substitution, both directions
of credential-class substitution, private configuration/token files and roots,
symlink files, malformed/unknown grant fields, empty/control/oversized tokens,
unconfined configuration paths, duplicate/unknown role aliases, unsupported
configuration versions, insecure URLs and invalid TLS trust. Diagnostics and
Debug results contain no fixture token, handle, endpoint or raw configuration.

Before extracting the common validation helpers, existing GitHub behavioral tests
passed. After extraction the existing library/credential contracts passed again.
The final complete affected suites below also pass, including Delivery HTTPS,
credential, replay and process contracts. Documentation-only edits have no
executable behavior and were checked through strict OpenSpec/rustdoc and diff
validation.

## Regressions and checks

Passed on macOS on 2026-10-16:

```sh
cargo test --locked --jobs 2 -p octacity-work-source-protocol -p octacity-server-work-source \
  -p octacity-delivery-protocol -p octacity-server-delivery -p octacity-server-adapter-host \
  -p octacity-adapter-protocol -p octacity-github --all-features
cargo test --locked --jobs 2 -p octacity-server-application --lib factory_admission_tests -- --test-threads=2
cargo test --locked --jobs 2 -p octacity-server-application --test manual_trigger --test factory_artifact_reader -- --test-threads=2
cargo clippy --locked --jobs 2 -p octacity-work-source-protocol -p octacity-server-work-source \
  -p octacity-github -p octacity-delivery-protocol -p octacity-server-delivery \
  --all-targets --all-features -- -D warnings
RUSTDOCFLAGS='-D warnings' cargo doc --locked --jobs 2 -p octacity-github --all-features --no-deps
cargo fmt --all -- --check
python3 tools/check_architecture.py
openspec validate add-github-work-source --strict
git diff --check
```

Observed: 63 protocol/host/provider tests, 40 application admission tests,
34 manual-trigger tests and 5 Artifact-reader tests: **142 distinct tests**.
Architecture: 63 packages, 250 dependency edges. No external dependency/version
was added for task 3.2, and no shared Factory/application implementation changed.

Linux verification passed all 13 actual-process/private-installation tests in
an owned disposable container labelled `octacity.test=work-grants-32`, using
the installed Rust 1.99.0 toolchain, umask 022, read-only repository/dependency
mounts, cached registry sources and a separate target directory. Networking was
disabled except for container loopback. The successful command was:

```sh
cargo test --offline --locked --jobs 2 --target-dir /tmp/work-grants-target \
  -p octacity-github --test work_credentials --test process
```

The container's identity/ownership label and successful exit were checked before
removing it and its isolated build directory. No user stand, external provider,
paid model or user's repository was used. Only task 3.2 is completed here;
progress is 11/29. No commit was requested for this implementation.
