# Task 3.1: explicit process-completion input contracts

## Delivered contracts

`SourceAcceptanceMapping::ProcessCompletion` explicitly selects a reviewed
process-completion document. It never becomes a fallback for field, body or
Markdown-heading acceptance. A title-only ticket can be prepared when that
explicit policy allows a missing body; its task document preserves `body: null`
and the original normalized fields. The original ticket and canonical Issue
acceptance are not replaced with process criteria.

`SourceCompletionDocument` supports two exact choices:

- `Draft`: owning Project, immutable preset identity/version/content digest and
  exact UTF-8 criteria. Review preserves whitespace and line endings, verifies
  the digest and enforces byte/control bounds. Debug formatting redacts content.
- `ProjectInput`: an exact already published private Project text Artifact with
  its original ID, digest and byte size. The selected Project must match the
  retained Source input; missing, foreign and changed references fail closed.

The private Work acceptance-role Artifact identifies `purpose: process_completion`
and the selected document separately from native ticket provenance. Its ordinary
Artifact role remains unchanged, so no alternate Work/Agent input contract or
source-authored implementation acceptance is fabricated.

`config/factory/presets/triage/completion-v1.json` contains the reviewed versioned
criteria for Project → Systems analyst triage. It is embedded in the application
as `TRIAGE_COMPLETION_PRESET`, making the draft available in a packaged build
without a repository checkout or earlier document upload. The bounded pure
`SourceCompletionDocument::review_preset` contract binds it to a Project without
publishing anything or creating a Build. Future shared preset resolution can
reuse these ordinary preparation choices.

## Shared ownership and retention

`FactoryProjectInputReader` is a narrow Project-text reader using the existing
`ProjectInputQueryStore` and immutable byte port. Both it and the broader
`FactoryArtifactReader` call the same private publication/ownership/byte/retention
checks. It requires no producing Build and does not accept arbitrary Build output
as a private Project document. No new journal, upload service, provider client,
credential owner or database schema was added.

`FactoryWorkSourcePreparer::with_project_inputs` installs that reader for explicit
references. Preset drafts need no previously published document. Source
preparation continues to publish the existing ordinary task/acceptance pair and
retain its intent through the Source journal.

The reader exposes verified retention coverage. Referenced-document reads use
the current fenced scan window, rather than a stale snapshot reservation time.
Before publication, preparation checks the actual proposed Artifact lifetime
using the store's existing `ReserveSourceArtifacts::retain` calculation, then
rechecks the returned authoritative reservation. This covers delayed preparation
and avoids copying criteria beyond their original retention without duplicating
the expiry formula.

## TDD evidence

All checks were run on 2026-10-27 with `CARGO_PROFILE_DEV_DEBUG=0
CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2`.

Before changes, `cargo test --locked -p octacity-server-application --lib
installed_sources::mapping -j2` passed all 18 tests. The existing Project reader
case `project_text_intake_inputs_are_readable_without_build_provenance` also
passed before extracting its common private read implementation.

| Behavioral boundary / test filter | Observed Red | Observed Green |
| --- | --- | --- |
| `reviewed_process_completion_maps` | Existing mapping rejected unknown `process_completion` | Explicit reviewed criteria prepare a title-only ticket while preserving absent source body/acceptance and exact purpose/document provenance |
| `source_preparation_binds_an_exact_project` | The narrow Project reader and preparation binding API were missing | Existing same-Project text is read and copied through ordinary preparation, with no producing Build |
| `completion_preset_review` | The read-only bounded preset review API was missing | Exact Project/version/digest/content binding succeeds; substituted content fails and debug output omits criteria |
| `packaged_triage_completion_prepares` | The required versioned release document was absent; the next packaging step separately failed for the missing embedded draft API | Embedded release criteria prepare a title-only Source input without an earlier Build or separate upload; exactly the two ordinary input objects are published |
| The exact-Project preparation case with a short-lived document | A document covering the old snapshot time was accepted despite not covering the new Work Artifact lifetime | Insufficient coverage fails before Artifact reservation/publication; a sufficiently retained exact reference succeeds |
| The exact-Project preparation case resumed two days after snapshot reservation | A valid newly selected document was rejected using an already elapsed old read window | A reclaimed current scan window allows preparation while preserving the original retained snapshot |

Each filter above was run with `cargo test --locked -p octacity-server-application
--lib <filter> -j2`. Additional regression cases verify missing field/body/heading
acceptance remains rejected, and foreign Projects, missing documents, changed
digests, empty/control-bearing or oversized criteria produce no fabricated
acceptance. Failed reference attempts leave only the pre-existing Project pair.

## Final verification and scope

- `cargo test --locked -p octacity-server-application --lib installed_sources -j2`:
  55 passed, none ignored; includes the new contracts and existing mapping,
  preparation, polling, replay, staged admission and private Source-reader cases.
- `cargo test --locked -p octacity-server-application --lib project_text_intake -j2`:
  8 passed, none ignored.
- `cargo test --locked -p octacity-server-application --lib factory_protected_test_context_tests -j2`:
  1 passed; existing retained Build/context behavior remains intact.
- `cargo test --locked -p octacity-server-application --lib factory_protected_validation_input_source_tests -j2`:
  3 passed, including original input restoration and real read-window coverage.
- `cargo clippy --locked -p octacity-server-application -p octacity-server --lib
  -j2 -- -D warnings`: passed.
- Architecture validation, workspace formatting, strict OpenSpec validation and
  `git diff --check` passed. Persistence representation is unchanged; no migration
  or PostgreSQL parity result is claimed by this task.

This task qualifies mapping/preparation and preset-draft contracts. Installing
the reader into the managed runtime, freezing reviewed settings/document/source
before I/O, durable publication/admission parity and signed transfer qualification
were the scope of task 3.2, now recorded in
[managed completion verification](managed-completion-verification.md).
Restart/fencing fault qualification is recorded under
[task 3.3](completion-recovery-verification.md). The full shared
preset resolver and guided setup remain later tasks; the two-agent preset is not
yet claimed operational.

The previously reported native GitHub `TimedOut` CI failure was deferred at the
user's request before 3.1. Its temporary diagnostic padding was removed and the
ordinary 27 MiB local executable restored. No timeout fix is included here.

Disk space stayed about 14 GiB during 3.1 checks, above the 10 GB cleanup threshold.
Debug/incremental artifacts stayed disabled and Cargo concurrency was two. No
personal stand, provider/model endpoint or durable data was changed. No commit
was created.
