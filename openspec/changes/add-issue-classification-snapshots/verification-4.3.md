# Task 4.3 verification

## Public behavior and scope

Three native Source-worker tests cover four negative admission cases against a
disposable, migrated PostgreSQL database and the verified process-provider
fixture. They use canonical Issue commands, authorized manual execution and
classification application; they do not fabricate admissions, alter database
rows or supply a forged snapshot. Each test installs a predecessor-reader spy
that fails if Issue intake searches for a producing Run.

- Missing native correlation waits with `missing`, even when a differently
  correlated, fully classified Issue exists in the same Project.
- Editing canonical requirements after classification leaves the original
  assessment stale and waits with `stale`.
- Explicit required evidence without an independently qualifying reader waits
  with `inaccessible`. No Work inputs are written and no Run is admitted.
- A code-dependent size assessment of another immutable base waits with
  `wrong_subject`, despite an existing manually admitted Run for that Issue.
  Reassessing the selected field for the actual resolved base resumes the same
  Source preparation and admits the consumer with `size=medium` and no
  predecessor attachment.

Every wait is candidate-local: the Source scan completes without a target-wide
failure. Both the retained preparation and Source diagnostics expose the closed
reason, with no predecessor cursor. The public wait contains exactly preparation
identity, check time and reason; it carries no Issue text or evidence body. The
existing task 4.1 regression additionally qualifies that another eligible item in
the same poll progresses while an Issue is blocked.

Production changes reuse the existing wait vocabulary and common exact-subject
comparison. Required evidence has an explicit safe diagnostic instead of a
generic unavailable-reference failure. Automatic admission checks selected
classification against its independently resolved Repository/base before
constructing Incoming; the existing store admission fence remains in place.
No new schema, journal, permission, producer lookup or access bypass is added.

## Observed Red/Green

The owned disposable PostgreSQL service is selected through
`OCTACITY_POSTGRES_URL`. All Cargo commands use `CARGO_INCREMENTAL=0`, `--offline`
and `--locked`.

```sh
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server --lib issue_snapshot_required_evidence_waits \
  -- --ignored --nocapture
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server --lib issue_snapshot_incompatible_code_subject \
  -- --ignored --nocapture
```

Each independently reached a behavioral **Red**: the Source outcome was
`completed=0, deferred=1` instead of a completed scan with a retained candidate
wait. Evidence lacked a classified wait diagnostic; the incompatible subject
reached a generic store conflict. Fixture/compilation corrections are not counted
as Red. The missing-link/stale-requirements test passed its first executable run
against the existing implementation and is recorded as characterization, not an
invented failure.

After the minimal diagnostic/admission changes:

```sh
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server --lib issue_snapshots::readiness \
  -- --ignored --nocapture --test-threads=2
```

The three tests reached **Green** together (3 passed, 0 failed). The final focused
run with all assertions also passed (3/3, 7.90 seconds). Further assertions
qualify the bounded diagnostic wire shape, absence of guessed Issue correlation
and recovery after exact-subject reassessment.

## Regression commands

```sh
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

Observed regression results (all passed):

| Suite | Passed |
| --- | ---: |
| Application admission | 153 |
| Application Issues | 211 |
| Shared manual text intake | 27 |
| Ordinary Source workers | 97 |
| Native PostgreSQL Source workers | 59 |
| **Unique regression tests** | **547** |

The final native suite includes the three new tests; focused reruns are not
counted again. Test-name sets were checked for overlap across the listed filters.
Clippy passed with warnings denied for all targets/features of both affected
packages. Formatting, architecture (64 packages, 263 dependency edges), strict
OpenSpec validation, 34 local documentation links and whitespace checks passed.
The owned disposable PostgreSQL container was removed after qualification.

## Limits

This task qualifies negative evidence readiness. A required evidence declaration
is not treated as proof: without an independently qualifying reader it continues
to block admission. It does not introduce positive evidence ingestion, broaden
producer access or read private reports. Explicit automatic Issue-material
ingestion remains task 4.4. Existing provider and explicit predecessor modes keep
their original admission paths. No live provider or user credentials are used.
