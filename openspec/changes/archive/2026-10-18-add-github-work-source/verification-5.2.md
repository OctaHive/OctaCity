# Task 5.2 verification: protected provider-credential management

## Public interface and installed scope

`octacity-server-secrets::ProviderCredentialManager` has three write-only methods:
provision, adopt and rotate. Each selects an existing `LogicalSecretReference`;
requests cannot supply a filesystem path, resource set or credential class.
`ProviderCredentialValue` owns zeroing transient bytes, bounds input to 16 KiB,
redacts Debug, and implements neither Serialize, Clone nor Display. Rejected input
is also zeroed. Receipts serialize only the logical reference and configured state;
configured does not assert remote authentication, write readiness or Project use.
Errors are fixed safe classifications without secret values, private paths or OS
messages. The port is separate from `SecretGrantProvider` and Job delegation.
Compile-fail contracts cover accidental ordinary serialization/copy/printing and
conversion of the Job-grant provider interface into integration administration.

The native implementation is `octacity-github::GithubCredentialManager`. Its trusted
constructor binds exact references to existing private handles, credential classes
and resource aliases. These facts come from installation composition, not client
payloads. The common port has no GitHub discriminator, SDK type, resource-name
assumption or dependency on Factory. Other providers can implement that same port;
this task does not advertise native GitLab/Jira implementations.

The GitHub adapter retains the existing SHA-256 handle filenames and private JSON
format (`class`, `token`, `repositories`). Task read, task update, repository read
and Delivery retain the existing `work_source_read`, `work_reporter_write`,
`vcs_read` and `delivery_write` classes. Source/Reporter/Delivery runtime credential
parsing shares the token validator, document bounds and filename derivation with
management. Adoption and rotation require the exact installed class and resource
set; they do not reclassify material or widen its scope. Alias ordering is irrelevant
and duplicates are rejected. The native token limit remains 8192 bytes, and the
complete private document is bounded to the existing 64 KiB reader ceiling.

## Private-file guarantees

The installation owner and its approved root are trusted. Linux/macOS use
Rustix descriptor-relative operations. The root is canonicalized once and pinned
by device/inode; every operation reopens and checks the original root/directory,
owner UID and private modes. Every path component is opened with NOFOLLOW.
Relative traversal, symlinks, replaced directories, public permissions, foreign
ownership, hard-linked/nonregular files and oversized documents are refused.
NONBLOCK prevents opening a FIFO from waiting for an external writer.

New material is written to an exclusive owner-private temporary file, set to 0600,
flushed and atomically renamed in the same opened directory. Provisioning uses
NOREPLACE. Rotation first verifies existing material and then replaces the same
purpose handle; readers of the old inode retain the complete old value. Directory
state is synced after publication. Normal failure cleanup removes only the owned
temporary inode. File I/O runs in spawn_blocking, and encoded/read secret buffers
are zeroed. No provider API call is made. This session exercised macOS; unsupported
platforms fail closed, with no permission fallback. Linux code uses the same Rustix
interface but was not separately executed in this session.

The primitive reports an unknown storage result if syncing fails after rename;
it does not claim rollback of a completed rename. Administrative authorization,
stable operation identities, preparation/activation recovery, stale replay and
revocation, and bounded crash-orphan cleanup remain task 5.3. There is no secret
read-back operation or new public REST route in this slice.

No database migration or Factory authority was added. Public connection metadata,
Work, admission identities, installed profile digests and native resource identities
remain unchanged. Successful receipt/error/Debug surfaces expose no raw value.
Private file encoding is the only deliberate serialization of credential bytes;
ordinary durable serialization of the input type fails to compile. The module
emits no secret-bearing log events or audit data and issues no Job grants.

## Red/Green evidence

| Slice | Observed Red before implementation | Observed Green |
| --- | --- | --- |
| Transient input | Credential-value API absent | Boundaries, zeroing ownership and redacted Debug; compile-fail durable/copy/printing contracts |
| Provisioning | Public manager/port API absent | Existing Source consumes the generated file, receipt is safe, profile/native identities stay unchanged and no provider call occurs |
| Adoption | `adopt` method absent | Existing bytes unchanged; class/resource substitution and unregistered references refused |
| Atomic rotation | `rotate` method absent | Same logical handle/profile, complete old/new reader views, 0600 file and no temporary residue |
| Complete-document bound | A >64 KiB generated private document was incorrectly published | Rejected before any private file is written |
| Missing material | Known missing handle returned Denied instead of NotConfigured | Safe missing status; rotation creates no replacement for absent material |
| Architecture policy reconciliation | Store URL syntax parser rejected with ARCH010 | Package-specific parser allowance; HTTP clients and URL dependencies in other core packages remain rejected |

Additional public-port characterization tests cover invalid UTF-8/control/oversized
tokens, escaping paths, duplicate installed references/handles/resources, all four
private classes, concurrent create contention, unsafe files and changed nested
ancestry. They use real temporary directories and existing provider consumers,
with no sleeps, widened timeouts, user credentials or external provider mutations.
The FIFO fixture uses the platform mkfifo utility; a macOS Rustix mknodat compile
error was corrected in the test setup and was not counted as behavioral Red.

## Commands and outcomes

The baseline before implementation passed all 92 tests selected by:

```sh
cargo test --locked -p octacity-server-secrets -p octacity-server-adapter-host -p octacity-github
```

Each slice was driven with these public tests (API/method commands failed before
their implementation, then their expanded groups passed):

```sh
cargo test --locked -p octacity-server-secrets --lib provider_credentials
cargo test --locked -p octacity-github --test work_credentials managed_provisioning
cargo test --locked -p octacity-github --test work_credentials managed_adoption
cargo test --locked -p octacity-github --test work_credentials managed_rotation
cargo test --locked -p octacity-github --test work_credentials managed_provisioning_bounds
cargo test --locked -p octacity-github --test work_credentials managed_missing
python3 -m unittest tools.tests.test_check_architecture.ArchitecturePolicyTests.test_connection_endpoint_parser_is_store_specific_and_does_not_allow_transport
```

Final verification (`CARGO_INCREMENTAL=0` for the Rust commands):

```sh
cargo test --locked -p octacity-server-secrets -p octacity-github -p octacity-server-vcs -p octacity-delivery-protocol
cargo test --locked -p octacity-server-vcs
cargo test --locked -p octacity-server-secrets --doc
cargo test --locked -p octacity-server-application --lib factory_admission_tests
cargo test --locked -p octacity-server-store --lib connection_
cargo clippy --locked -p octacity-server-secrets -p octacity-github --all-targets --all-features -- -D warnings
RUSTDOCFLAGS='-D warnings' cargo doc --locked -p octacity-server-secrets -p octacity-github --no-deps
python3 -m unittest discover -s tools/tests -v
python3 tools/check_architecture.py
cargo fmt --all -- --check
openspec validate add-github-work-source --strict
git diff --check
```

The credential suite passed 22 tests, including the 10 original Source/Reporter
credential contracts. Secret domain tests passed 9 cases and its 5 compile-fail
documentation contracts passed. The provider/VCS/Delivery/secret groups account for
118 passing test cases in the final repeated four-package command (zero failures). Manual/source admission passed 60;
connection metadata passed 4. Tool tests ran 432 cases successfully (8 skipped).
Clippy, rustdoc with warnings denied, formatting and architecture checks passed.

The broad provider run passed every changed credential/provider test but the
existing VCS `preserves_all_adapter_failure_classes` process fixture timed out once
while other builds were running. A repeat of the entire VCS package, with its
normal parallel tests and unchanged timeouts, passed 11/11 in 1.38 seconds. VCS
production code/tests and their time budgets were not changed for this task.
The complete four-package command was then repeated with normal parallel tests
and passed all 118 unit/integration/documentation cases with zero failures.

The architecture check also detected the omitted policy declaration for task 5.1's
existing URL-parser dependency. Its narrow tested allowance enables only store
endpoint syntax validation; it grants no network-client capability to core. The
lockfile adds only existing workspace dependency edges, with no package/version
updates. The user's running stand and real credential files were not changed.
