# Task 4.4 verification

## Published policy and public boundaries

Automatic material selection reuses `FlowIssueSnapshotInput.materials` on the
exact published Flow/Configuration selected by the enabled Source input. Its
required/optional, exact/approved-selection, source-count and aggregate UTF-8 byte
bounds are the policy. Publishing an Issue material selection alone does not
enable intake or declare a consumer policy. No second Source selection schema,
input identity, body store or permission grant is introduced.

Both managed-connection and legacy-profile Source compositions install one shared
runtime reader factory. It composes the existing authorized comment-body query
and owner-aware Issue/Project document queries. `IssueSourceSnapshotReader`
passes those readers to the same snapshot builder as manual execution; its
standalone default still installs no body readers. A contract without materials
performs no material-body authorization or reads.

The native tests use the real Source worker, migrated PostgreSQL, verified
process-provider fixture and public authorized Issue/comment/material/document
commands. Immutable byte storage and authorization/time/VCS services are the
controlled external boundaries. No test fabricates an admission, writes database
rows directly or supplies caller-authored snapshot values.

## Behavioral qualification

- Required approved text can remain pending while unrelated discussion exists.
  This is a candidate-local `inaccessible` wait, with no fabricated input or Run.
- Selecting one exact comment, an Issue-owned document and a separately linked
  Project document releases the same preparation and admits one Work/Run.
  Incoming retains literal CRLF/non-ASCII text, original owner/publication
  metadata, selection attribution and the canonical Issue revision. The native
  Work association records that reviewed revision and no predecessor is attached.
- Posting text and publishing either document do not select it or start a Run.
  New publications leave earlier references pinned until explicit replacement.
  Replacing all three references after admission and restarting twice preserves
  the original complete Run snapshot, Source preparation, admission receipt,
  private documents and exact input-object manifest. Only the two intentional new
  document publications write bytes; rediscovery publishes no replacement input.
- A consumer without a material declaration admits its canonical scalar fields
  despite independently denied body access to approved comment/document sources.
  It performs zero material-body authorization checks and retains no material
  values, sources or omissions.
- Independently denied comment access blocks required material and creates no
  input/Run. Optional unavailable text admits with an explicit `unavailable`
  omission, without copying its body or substituting an empty string/array.
- Exceeding the published three-entry or 4,096-byte bound waits with
  `incompatible`, for required and optional selections. Two 2,500-byte Unicode
  texts exceed the aggregate byte bound despite containing fewer than 4,096
  characters. There is no truncation or optional-omission bypass.
- Revoking comment-body access while VCS resolution is paused after preparation
  prevents admission even though the text and intent were already retained.
  Restoring access resumes the same preparation and admits one Run.

## Observed test-first outcomes

All Cargo commands use `CARGO_INCREMENTAL=0`, `--offline` and `--locked`.
`OCTACITY_POSTGRES_URL` selects the owned disposable PostgreSQL service.

```sh
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server --lib source_required_material_policy_waits \
  -- --ignored --nocapture
```

**Red:** `completed=0, deferred=1` instead of a completed scan retaining a waiting
candidate (3.28 seconds). **Green:** 1 passed after mapping required material
unavailability to the existing candidate-local wait (3.70 seconds).

```sh
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server --lib source_approved_materials_resume_once \
  -- --ignored --nocapture
```

**Red:** no Run after explicit approval of the three real sources (expected one;
4.73 seconds). The Source reader still supplied no material readers.
**Green:** the wait and lifecycle tests passed together after installing the
shared authorized readers (2 passed, 5.65 seconds).

```sh
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server --lib source_material_policy_bounds \
  -- --ignored --nocapture
```

**Red:** oversized approved material deferred the whole scan instead of waiting
for its candidate (3.58 seconds). **Green:** all four count/byte and
required/optional cases passed after classifying the existing limit diagnostic
as `incompatible` (1 test, 14.99 seconds). Compilation/fixture corrections are
excluded from the behavioral Red observations.

No-policy and independent-denial tests passed on their first executable run
using these installed readers and existing authorization/omission rules; no
separate missing-behavior Red is claimed for them. The combined five-test run
passed (22.38 seconds). The additional final-admission revocation check exercises
the existing current-input recheck and passed on its first executable run; it
does not add another admission fence. The final focused run, including owner
metadata/association assertions and revocation recovery, passed all six tests
(22.26 seconds).

## Regression commands

```sh
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server --lib issue_snapshots::materials \
  -- --ignored --nocapture --test-threads=2
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server-application --lib factory::admission
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server-application --lib issues
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server-application --lib management::tests::project_text_intake_tests
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server --lib runtime::sources::tests::worker
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server --lib runtime::sources::tests::worker \
  -- --ignored --test-threads=2

CARGO_INCREMENTAL=0 cargo clippy --offline --locked \
  -p octacity-server-application -p octacity-server \
  --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
python3 tools/check_architecture.py
openspec validate add-issue-classification-snapshots --strict
git diff --check
```

Observed final regression results (all passed):

| Suite | Passed |
| --- | ---: |
| Application admission | 153 |
| Application Issues | 211 |
| Shared manual text intake | 27 |
| Ordinary Source workers | 97 |
| Native PostgreSQL Source workers | 65 |
| **Unique regression tests** | **553** |

The native suite includes all six new tests (284.75 seconds); focused reruns are
not counted again. Test-name sets were checked for overlap across filters.
Clippy passed with warnings denied for all targets/features of both affected
packages. Formatting, architecture (64 packages, 263 dependency edges), strict
OpenSpec validation, 46 local documentation links and whitespace checks passed.
The owned disposable PostgreSQL container was removed after qualification.

## Scope

Required evidence remains independently qualified; supplementary prose supplies
no reproduction, approval, tool or credential authority. Each step still needs
its explicit frozen Context projection. Source publication/selection does not
activate reporting or another processing cycle. No live provider, user token or
new database migration is required for this qualification.
