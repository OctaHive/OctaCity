# Task 6.2: operator-only discussion application contracts

## Boundaries

The optional application handler and narrow memory port append plain-text
comments to an existing Issue. Manual Issue creation and execution admission
remain unchanged. Discussion has no provider, model, Context publication,
approval or execution-control dependency. A trusted existing clock supplies
server time; accepted management context supplies actual actor provenance.

Bodies preserve their original text, reject blank Unicode whitespace, disallowed
control characters and recognized credential/private-capability markers, and
enforce a 16,384 UTF-8-byte ceiling. Validated deployment settings can only lower
that ceiling and the 1,000-comment lifetime quota. Roots and replies share the
quota; replies target only a retained root of the same Issue. The lifetime
counter is independent of retained body records. No retention operation is added.

Append and dedicated body reads require separate Issue-scoped discussion rights
and current owning-Issue visibility. Metadata pages require Issue visibility but
grant no body authority. Debug output redacts bodies; generic receipts, audit,
outbox and metadata pages contain no bodies. Scoped replay keeps the original
identity, sequence, actor, server time and digest, even after limits are lowered.
Changed reuse conflicts. Body, sequence, replay and safe publication reuse the
existing Issue atomic transaction and failure boundaries.

The existing sensitive-material detector is shared with Factory observation
validation instead of copying its marker table. It remains a conservative marker
check, not a guarantee that arbitrary prose contains no secret.

PostgreSQL, concurrent database recovery, REST/no-store responses and generated
contracts remain task 6.3. Console remains task 6.5; retention/holds remain task
8.2. Metadata sequence continuation here is an application contract, not an
opaque HTTP cursor or an atomic cross-section snapshot.

## TDD evidence

Commands use `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0
CARGO_PROFILE_TEST_DEBUG=0 CARGO_BUILD_JOBS=2`, `--offline`, and
`cargo test -p octacity-server-application --lib <filter>`.

| Filter | Observed Red | Observed Green |
| --- | --- | --- |
| `issue_discussion_appends_plain_operator_data` | Public discussion command/query/body contracts were absent | Original plain text, server timestamp and actual anonymous actor persist; receipt/Debug exclude the body; dedicated read returns it without changing the Issue |
| `issue_discussion_body_enforces_utf8_bytes` | Blank body was accepted | Exactly 16,384 multi-byte UTF-8 bytes pass; an extra byte, Unicode blank text and forbidden controls reject; closed intent rejects actor/time/sequence/execution/attachment setters |
| `issue_discussion_body_rejects_shared` | Credential-shaped body was accepted | Existing recognized credential/private-capability patterns reject, including case variants; ordinary discussion remains valid |
| `issue_discussion_replies_target_only` | A nested reply was accepted | Same-Issue root replies pass; nested, foreign and unknown parents reject without mutation |
| `issue_discussion_applies_stricter` | Validated deployment bounds and handler configuration API were absent | Hard caps cannot widen, stricter byte/lifetime limits apply to roots and replies, original replay survives quota exhaustion/lowered limits, changed reuse conflicts |
| `issue_discussion_metadata_is_bounded` | Metadata page/query contracts were absent | Bounded deterministic pages contain no bodies; metadata-only authority cannot read a body |

Additional characterization tests passed for reused mechanisms: distinct verified
Alice/Bob scopes retain their actual actors and separate same-key outcomes;
current visibility rejects replay and body reads; all five common transaction
failure points roll back without consuming a sequence, audit or outbox fact.
Retry publishes once. A real associated Factory Run retains its exact snapshot,
Context and controls after agent/approval-like prose; a later explicit execution
request retains the same task/acceptance input digests without importing comments.
There are nine discussion tests in total.

The verified-actor test initially used the trusted-network-only policy. Correcting
that fixture to the existing scoped policy was not behavioral Red and did not
change the runtime policy. Existing Factory input validation and observation
identity/trace tests passed before sharing the marker detector; the observation
check also passed after extraction.

## Regression verification

| Command | Result |
| --- | --- |
| `cargo test --offline -p octacity-server-application --lib issue_` | 124 passed, including the nine discussion tests and existing card, status, execution, Source and manual-task coverage |
| `cargo test --offline -p octacity-server-application --lib management_security` | 21 passed |
| `cargo test --offline -p octacity-server-application --lib factory_codex_observation_tests` | 14 passed |
| `cargo test --offline -p octacity-server-domain -p octacity-server-factory -p octacity-server-store --lib` | Factory 296 and store 81 passed; domain compiled and has no unit tests |
| `cargo test --offline -p octacity-server-api-rest --lib --test v1_handlers` | 12 library and 137 HTTP integration tests passed |
| `cargo clippy --offline -p octacity-server-application -p octacity-server-domain -p octacity-server-factory -p octacity-server-store -p octacity-server-api-rest --all-features --all-targets -- -D warnings` | Passed |
| `cargo fmt --all -- --check` | Passed |
| `python3 tools/check_architecture.py` | Passed: 63 packages, 256 dependency edges |
| `openspec validate add-factory-work-card --strict` | Passed |
| `git diff --check` | Passed |

Documentation-only edits use specification/diff validation rather than executable
TDD. Builds disable incremental compilation/debug symbols and use two jobs. No
database container, frontend build, extra checkout or dependency was needed.
Approximately 10 GiB remained free after verification. Task 6.2 is complete:
34 of 56 tasks checked. No commit was created.
