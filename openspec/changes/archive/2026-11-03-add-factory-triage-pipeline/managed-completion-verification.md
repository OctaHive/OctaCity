# Task 3.2: managed completion publication and admission

## Delivered boundary

The installed Source worker can now read an explicitly selected same-Project
private completion document using the common `FactoryProjectInputReader`.
Both managed connection discovery and explicitly installed Factory Source
composition supply this reader. Reviewed inline preset drafts use the same
preparer without requiring a previous Build or document upload.

Before document or publication I/O, the existing Source preparation record
retains the exact bounded serialized preparation settings. These include the
document reference or reviewed versioned bytes, Repository source policy,
classification and mapping limits. The original snapshot and target
Configuration/Repository versions were already frozen in this record.
Recovery selects these original settings; a different policy cannot replace
them after a failed read. The existing materialization digest remains the
digest of these settings.

This is an additive optional field in the existing private preparation document,
not another reservation table, submission owner or application journal.
Historical records deserialize without inventing settings. Memory and PostgreSQL
use the same bounded transition under their existing current-selection/scan
fences. Private policy bytes expire with snapshot cleanup; exact publication
metadata and ordinary Work references keep their existing lifecycle.

Publication still uses `FactoryInputObject`, `ArtifactBytePublisher` and the
Source materialization owner. It produces the ordinary task/acceptance pair,
then stores the complete intent before ordinary Source admission. Process
completion has explicit purpose and exact document/publication provenance.
Original missing ticket body/acceptance is preserved; no Issue mutation or
implementation acceptance is generated.

## Authorization and integrity

Runtime preparation requires current Project execution authority, visibility of
the pinned Configuration and Repository, and visibility of the exact selected
Project document. These checks precede private reads. A document read is followed
by another authorization check before publication. Ordinary prepared admission
also rechecks the original document, rather than allowing its newly published
Source copy to bypass revoked visibility. Existing current Source/resource
authorization remains unconditional.

The common input reader and mapping retain Project ownership, exact artifact
identity, SHA-256, byte length, media type, publication time and retention checks.
Oversized or substituted inputs fail without truncation. A successful byte read
cannot grant visibility, renew retention or choose a different Configuration.

## TDD evidence

Rust commands below use `CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2` and the committed lockfile.

| Public boundary / command filter | Observed Red | Observed Green |
| --- | --- | --- |
| Application Source preparation: `completion_policy_is_frozen_before_document_io` | After the original document was unavailable, a replacement draft was accepted. | Replacement is rejected; the original exact settings and snapshot remain retained with no published objects. |
| Installed worker: `installed_completion_document_prepares` | Missing explicit Project-reader composition API. | A title-only ticket receives the selected exact completion bytes and enters ordinary Work admission. |
| Application preparation: `preparation_requires_current_visibility` | Missing preparation authorization API. | Hidden Project, Configuration or Repository prevents policy retention and publication. |
| Application preparation: `completion_document_read_rechecks_visibility` | Revocation during the asynchronous read still allowed publication. | Hidden input is never fetched; revocation during a successful read leaves no materialization or input bytes. |
| Ordinary Source admission: `prepared_completion_cannot_be_admitted` | A hidden original Project document could be admitted through its visible Source copy. | Admission rejects before VCS resolution and records no Work association. |

Application filters run with `cargo test --locked -p octacity-server-application
--lib <filter> -j2`; the installed-worker filter uses `-p octacity-server`.
Initial test-fixture import/trait errors were corrected before recording the
behavioral or required-new-API Reds above. A broad artifact-denial assertion did
not establish the admission Red; the retained assertion hides only the original
Project document, while leaving the published Source pair visible.

The PostgreSQL contract extends the existing memory/parity test with exact policy
retention, conflicting replacement, stale/foreign/expired/oversized requests,
legacy JSON, reconstruction with a new store instance and private cleanup.
The composed worker test uses the same operations over memory and real
PostgreSQL publication/admission.

The transfer test signs an ordinary managed V3 Job template containing the exact
Source task/acceptance references, verifies its signature and checks their
unchanged identities in the manifest. It exercises the Source entries through
the existing `ArtifactHandlers::authorize_protected_inputs`: verified retained
bytes, lease-bounded read grants, substituted-digest rejection and expiry.
Compiler fixture documents are outside this Source-entry transfer assertion.
This is signing/transfer contract evidence, not an Agent execution claim.

## Final verification

| Command | Result |
| --- | --- |
| `cargo test --locked -p octacity-server-application --lib -j2` | 781 passed, no failures or ignored tests; includes all Source mapping/preparation, admission, retained readers, ordinary signing/materialization and existing generic Factory/Project-input behavior. |
| `cargo test --locked -p octacity-server --lib runtime::work_source_tests -j2 -- --include-ignored --test-threads=2` | 130 passed, no failures or ignored tests; includes real PostgreSQL Source/Reporter qualification and the new completion contracts. |
| `cargo test --locked -p octacity-server-store-postgres --test factory_discovery source_publication_is_atomic_owned -j2 -- --ignored` | 1 passed; extended shared memory/PostgreSQL contract including exact settings, legacy representation, fences and cleanup. |
| `cargo test --locked -p octacity-server --lib runtime::work_source_tests::completion -j2 -- --include-ignored --test-threads=2` | 2 passed after replacing duplicate fixture-module declarations with existing shared modules; includes signing and Source-entry transfer assertions. |
| `cargo clippy --locked -p octacity-server-application -p octacity-server -p octacity-server-store -p octacity-server-store-postgres --lib -j2 -- -D warnings` | Passed. |
| `cargo clippy --locked -p octacity-server --lib --tests -j2 -- -D warnings` | Passed after reusing existing shared fixture modules, without suppressing the duplicate-module lint. |
| `python3 tools/check_architecture.py` | Passed: 63 packages, 261 dependency edges. |
| `cargo fmt --all -- --check`, `openspec validate add-factory-triage-pipeline --strict`, `git diff --check` | Passed. |

PostgreSQL commands use `OCTACITY_POSTGRES_URL` pointing to the disposable service.
The first broad runtime invocation used the default test-thread count while the
full application suite and database contracts ran concurrently: 127 passed and
three existing installed Reporter scenarios exceeded their delivery/readiness
windows. The complete same runtime suite passed with two test threads, including
all three previously failing scenarios. No Reporter behavior, deadline or
assertion was changed to obtain that result. The initial failure remains recorded
in `/tmp/octacity-triage-3-2-source-runtime-regression.log`; final bounded results
are in `/tmp/octacity-triage-3-2-source-runtime-bounded.log`. Other local evidence
uses the `/tmp/octacity-triage-3-2-` prefix.

## Environment and remaining scope

Tests use a separate task-owned PostgreSQL 18.6 container bound only to an
allocated loopback port. Its disposable databases and public fixture credentials
are independent of the personal stand. Object bytes and provider responses are
finite contract peers; no live GitHub, paid model or VM/Agent is involved.
The task-owned container and its anonymous data volume were removed after checks.
Available disk space stayed about 15–16 GiB, above the 10 GB cleanup threshold.
No commit was created.

Crash/replacement/concurrent-owner fault qualification is recorded under
[task 3.3](completion-recovery-verification.md).
Preset resolution, operator UI and the two reasoning stages remain later tasks.
The previously deferred native GitHub CI timeout is unchanged.
