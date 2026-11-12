# Task 5.4: versioned input compatibility and explicit producer-result execution

## Behavior

The existing public Issue/Factory journey now qualifies execution of a newly
authored producer-result consumer, rather than stopping after admission. Both
Flow and Configuration are published through authorized handlers with exact
producer Flow/terminal/outcome constraints. No Issue property/snapshot reader is
installed on that publication path. Pending or inaccessible producers and a
missing terminal retain their original diagnostics.

Manual execution selects one exact accepted producer Run. Automatic Source
admission resolves that same eligible result through its canonical Issue
correlation and bounded paged predecessor lookup. A different Run of the same
Flow on another code revision is not substituted. Reconstructed handlers retain
the original automatic identity. After a requirements edit, both consumers
execute the original frozen result through a replacement Factory owner and
complete their declared terminal. Work and Incoming data remain unchanged;
Issue classification and planning values stay empty. Manual publication and
automatic admission replay still return the original associations after Run
progress. A new request against stale producer requirements still rejects.

Legacy-shaped Flow, Configuration, execution Request, Run and admitted-Flow
documents are decoded and re-encoded without adding snapshot data or changing
their pins. The original documents omit the additive fields; assertions require
that absence explicitly. Incoming data is restored through its public checked
byte reader using the independently retained digest and exact Work, admitted
closure and schema. Substituting the producer Run is rejected. This is owner and
wire-contract qualification using the existing in-memory adapters, not a new
PostgreSQL restart or packaged-release claim.

## Observed Red/Green

The existing producer-result publication/admission test passed before extension
(1/1, 0.35s). Its additional execution, recovery and legacy-wire assertions passed
without changing the explicit producer-result implementation. This is
characterization of preserved behavior, not an invented Red or a decoder rewrite.

A separate compatibility test found a real regression after task 5.3's empty
dependency correction. Before that correction, a valid optional selection of an
empty dependency list was sealed as an `inapplicable` omission. A retained,
unprepared request with that exact older digest failed publication with an Issue
conflict instead of recovering its original input. The test reserves the older
receipt through the existing store port, then exercises ordinary authorized
Issue publication and admission.

**Red:** `snapshot_recovers_a_legacy_optional_empty_dependency_omission` failed at
publication with `Application(Store(Conflict { entity: Issue }))` (0.04s).

**Green:** bounded recovery now considers that one historical projection only
when the selected dependency field is optional and its current canonical value
is the empty array. It accepts it only if the complete candidate encoding matches
the already sealed digest. The test passed (1/1, 0.06s), retaining the exact old
snapshot, empty object Incoming payload and original digest through replay.

The same test then added negative cases: a different omission reason and a
required dependency selection both reject without Factory mutation. All three
cases passed (1/1, 0.08s). New requests continue to include the canonical empty
array. Nonempty claims retain their ordinary applicability checks.

Recovery reuses the existing material-omission matcher and definition-format
fallbacks. It does not add a wire field, loosen ordinary decoding, promote a
producer result into Issue classification, infer a latest Run or activate intake.
There is no unbounded subset search: the candidate is the historical all-empty
optional dependency projection, subject to the existing selection/encoding bounds
and complete digest match. Documentation-only changes have no executable behavior
requiring a separate Red/Green cycle.

```sh
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server-application --lib --all-features \
  manual_delivery_admits_the_original_issue_and_exact_triage_result -- --nocapture
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server-application --lib --all-features \
  explicit_producer_result_publication_executes_and_recovers_without_issue_classification -- --nocapture
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server-application --lib --all-features \
  snapshot_recovers_a_legacy_optional_empty_dependency_omission -- --nocapture
```

The first command is the pre-extension test name. Its replacement is the second
command (final targeted run: 1/1, 0.16s). A Rust ownership error during test setup
was corrected before that successful run and was not counted as behavioral Red.

## Regression qualification

Observed final results: **616 tests passed**, with no failures or ignored tests:

- Issue execution, snapshots, materials and compatibility: **102 passed** (2.38s).
- Admission and Source ownership/configuration/preparation/polling: **153 passed**
  (31.22s). These tests are registered under `factory::admission`; an initial
  `factory::sources` filter selected zero tests and was not counted.
- Issue-delivery resolver and frozen-route qualification: **10 passed** (17.93s).
- Core Factory contracts, wire checks and interpreter: **351 passed** (3.07s).
- Application Clippy, all targets/features with warnings denied: passed.
- Rust formatting, repository architecture, strict OpenSpec and diff whitespace:
  passed.

The final application groups were rerun after avoiding an unnecessary snapshot
clone when a mismatched older encoding has no eligible empty-dependency aliases.
The core suite had no intervening code changes and was not repeated. Test adapters
are deterministic in-memory owners with controlled bytes/revision ports. No
native services or provider writes were used for this task.

```sh
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server-application --lib --all-features \
  issues::tests::issue_tests::execution -- --quiet
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server-application --lib --all-features factory::admission -- --quiet
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server-application --lib --all-features issue_delivery_ -- --quiet
CARGO_INCREMENTAL=0 cargo test --offline --locked \
  -p octacity-server-factory --lib --all-features -- --quiet
CARGO_INCREMENTAL=0 cargo clippy --offline --locked \
  -p octacity-server-application --all-targets --all-features -- -D warnings
CARGO_INCREMENTAL=0 cargo fmt --all -- --check
python3 tools/check_architecture.py
openspec validate add-issue-classification-snapshots --strict
git diff --check
```
