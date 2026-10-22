# Task 4.2: PostgreSQL native links and exact association recovery

## Scope and shared mechanisms

`PostgresStore` implements the existing optional `IssueSourceStore` boundary.
Additive migrations 80–81 retain one primary link per Issue, a unique canonical
native key in the Project/security partition, and bounded immutable observation
receipts. The native key reuses the existing Source correlation helpers; it
does not include input/pipeline IDs, display names, credentials or mutable
configuration versions. Compatible installation adoption/rotation preserves the
original link. Manual creation remains independent of a provider or Factory.

Import and attachment use the existing Project lock and management
replay/audit/outbox transaction. PostgreSQL manual creation and native import
share the ordinary initial Issue policy, workflow, number allocation, revision
and history writer. Existing-link imports retain their own replay receipts and
observations without changing Issue text/status or admitting Work. Import
visibility is checked before publishing a canonical reference or replay receipt.

Each safe observation retains the exact installed Source/configuration mapping
references, verified plugin, transport/selection/snapshot digests and original
read/deadline times. It stores no native content, wire object or credentials;
replay does not renew the original provenance. Memory and PostgreSQL expose the
same scoped point read. This is persistence of read provenance, not activation
of field ownership or an external status mapping. Refresh, field/status mapping
validation/activation and HTTP/Console composition retain tasks 4.3–4.5.

The common `associate_issue_work` transaction checks the existing settled Source
preparation's native identity and original accepted Work partition against the
retained link. It does not infer identity from text, input names or current
selectors. Different inputs retain their independently admitted Work/Run pairs
while sharing one Issue. Ordinary association replay/audit/publication remains
the only association mechanism; Source preparation recovery remains with its
existing journal. Legacy Source Artifact provenance has no invented Issue
requirements revision. Manual Work without verified Source preparation keeps
its existing association contract. No worker, admission service, provider SDK,
credential mechanism, second outbox or new execution path was introduced.

## Observed TDD cycles

Cargo commands used the existing target directory with:

```sh
CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_BUILD_JOBS=2
```

PostgreSQL tests additionally used an explicitly disposable PostgreSQL URL and
`-- --ignored`. The actual database was PostgreSQL 18.6 from an already cached
image, with a 512 MiB container limit and 384 MiB data tmpfs. There was no image
download. Public store operations and original application commands are the
assertion boundaries; SQL is used for controlled faults, migrations and backend
termination, not as the source of expected task/association results.

Pre-refactoring baselines passed:

```sh
cargo test -p octacity-server-application --lib issue_tests::source
cargo test -p octacity-server-store-postgres --test issue_execution execution_reservation_is_atomic -- --ignored
```

These passed 11 application scenarios and one existing PostgreSQL manual
Issue/execution reservation scenario respectively.

| Command/filter | Observed Red before implementation | Observed Green |
| --- | --- | --- |
| `cargo test -p octacity-server-store-postgres --test issues source_attachment_matches_memory -- --ignored` | PostgreSQL attachment/link API was absent | Original manual Issue unchanged, memory parity and exact attachment replay after adapter restart |
| Same target, `source_concurrent_imports` filter | Required import operation returned `Unavailable` | Concurrent input selections resolve one canonical Issue, preserve independent replay keys and leave equal-text manual creation distinct |
| Same target, `source_observations` filter | Required safe receipt and scoped read API were absent | Both original immutable configuration/mapping references and snapshot provenance survive adapter restart without another Issue revision |
| `cargo test -p octacity-server-store-postgres --test factory_discovery source_work_cannot_attach -- --ignored` | Existing common association accepted a verified Source Work into an Issue for another native task | Wrong native association conflicts, while the correct association/replay preserves the original Work/Run and Artifact provenance |

Missing required APIs are the Red boundary for the corresponding new contracts;
Green runs assert behavior against PostgreSQL, not compilation alone. A setup
import error and an invalid error-constructor call were corrected and were not
counted as behavioral Red.

Additional characterization/recovery qualification passed through the existing
shared machinery, without adding production recovery logic:

- Compatible installed-version/configuration/resource-reference rotation,
  separate Project and accepted security partitions, hidden canonical denial,
  duplicate primary conflict and foreign-Project denial.
- Failed source import/attachment publication rolls back Issue numbers, links,
  observations and replay publication; retries converge to the original link.
- Two simultaneous Source inputs resolve one Issue while retaining distinct
  original Work/Run pairs. After scan-owner restart, existing Source admission
  replay and Issue association replay return those original identities.
- A trigger pauses the actual association transaction on an advisory lock;
  terminating its backend leaves no association. A later outbox failure also
  rolls association back without changing the Source preparation. Retry after
  restart succeeds, and retry after a discarded acknowledgement/later Issue
  edit returns the same association. A different security partition is denied.
- Upgrade from schema 79 and repeated latest migration preserve an existing
  manual Issue, its number and the original retained source receipt/replay.
- The final attachment parity check reads the same original safe observation
  through both memory and PostgreSQL ports.

## Regression and checks

All regressions passed after the implementation. The final additional receipt
parity assertion also passed in its focused PostgreSQL run.

| Command | Result |
| --- | --- |
| `cargo test -p octacity-server-store-postgres --test issues --test issue_execution --test issue_workflows --test factory_discovery -- --ignored --test-threads=2` | 84 passed: 30 Issue, 19 execution, 8 workflow and 27 Factory/Source scenarios |
| `cargo test -p octacity-server-store-postgres --test migrations -- --ignored --test-threads=1` | 6 passed, including reentrant migration, forward failure and previous-binary snapshot rollback; both new tables participate in the existing migration inventory |
| `cargo test -p octacity-server-store-postgres --test issues source_ -- --ignored --test-threads=2` | 7 passed, including six new native-link/migration contracts and existing source-ownership regression |
| `cargo test -p octacity-server-store-postgres --test factory_discovery issue_association -- --ignored --test-threads=2` | 3 passed, including real backend termination and independent input Run replay |
| `cargo test -p octacity-server-store-postgres --test issues source_attachment_matches_memory -- --ignored` | 1 passed after final safe-receipt parity assertions |
| `cargo test -p octacity-server-application --lib issue_` | 79 passed, including all 11 Source contracts, manual Issues, shared execution publication and status actions |
| `cargo test -p octacity-server-store --all-features --lib` | 81 passed |
| `cargo clippy -p octacity-server-store-postgres -p octacity-server-application --all-features --all-targets -- -D warnings` | Passed |
| `cargo fmt --all -- --check` and `git diff --check` | Passed |
| `python3 tools/check_architecture.py` | Passed: 63 workspace packages, 256 dependency edges |
| `openspec validate add-factory-work-card --strict` | Passed after marking task 4.2 complete |

The normalized fixture vocabulary is provider neutral and the existing Source
contract includes GitHub and additional fake provider names. This qualifies
store persistence/recovery, not live provider access or a native GitLab/Jira
adapter. REST/Console schemas were not changed in this task.

Disk checks stayed around 9.4–10 GiB available. Builds disabled incremental
artifacts/debug information and used two build jobs, with no additional target
directory or unrelated cleanup. The dedicated test container was removed after
qualification. Task 4.2 is complete, bringing the change to 24 of 56 tasks. No
commit was created.

Documentation and formatting add no executable behavior, so TDD does not apply
to those edits; format/diff, architecture and strict OpenSpec checks apply.
