# Task 6.14 verification — subject-bound Checks and Artifacts

Status: implementation complete; final regression/check results are recorded below.
Console panes remain task 6.15. No commit or release is part of this task.

## Scope and existing authorities

- Factory Checks read the original Issue/Work/Run association, frozen verification
  policy and retained node inputs, attempts, stops and qualified report data.
  Required missing checks remain visible. A completed Build alone is indeterminate;
  only a typed observation bound to its exact plan/input establishes an outcome.
  Head/candidate, producer, record and original timing are not replaced with a
  current provider head. These facts grant no acceptance or Issue transition.
- Artifacts reuse frozen task/acceptance/specification references and verified
  Build outputs in the common Flow journal. Project/Source inputs keep their real
  origin without invented Build provenance. Produced references retain their
  actual record, candidate when known, and Build/Attempt/Job producer.
- `FactoryArtifactReader` now supplies metadata without bytes through the same
  existing Project/Source publication and ordinary Artifact authorities. Explicit
  content expansion reuses its Project/digest/size/publication/retention checks.
  No store, uploader, provider poller, execution owner or migration was added.
- Both Factory panels share opaque cursor handling, independent visibility and
  Run-version fencing. Scope is checked before version; membership is checked
  only within the same version, so a newly retained output produces `stale`
  rather than a false cursor error. Metadata expands only for the selected page.
- External CI is retained as an optional backward-compatible field in the
  existing atomic PR/MR observation. Attach/refresh reads at most one bounded
  installed-reader page under the original total deadline. Partial/unsupported
  details and unknown required-check policy remain explicit; provider URLs and
  cursors are omitted. The separate CI GET reuses original-head history paging
  and repository visibility without provider dispatch. Summary/history metadata
  reads do not eagerly expand CI details. Legacy observations keep their old
  serialization/receipt identity when the optional field is absent.
- Four optional REST GETs compose these public application queries. Content is
  independently View/Download-authorized and served with no-store, nosniff,
  sandbox CSP and explicit inline/attachment disposition. Text is forced
  plaintext; PNG/JPEG/WebP is decoded and normalized to PNG, with 64 KiB encoded
  and output ceilings, 4096-pixel dimensions and a 64 MiB allocation budget.
  Downloads keep the existing 64 MiB reader ceiling. HTML/SVG/other executable
  preview modes, arbitrary fetch URLs and expired-byte resurrection are rejected.
- Repository/Build/Job/Artifact visibility is checked before inventory membership
  and rechecked before expanded metadata/content is returned. Retention is
  rechecked after content read/normalization. Manual Issue creation and existing
  status/Source/Reporter flows remain independent of these optional reads.

The pure in-memory `image` decoder has only `png`, `jpeg` and `webp` features;
its types do not cross application contracts. A package-specific architecture
allowance is qualified by a policy test that continues to reject HTTP clients in
application and `image` in other application packages. This is data validation,
not a provider or transport dependency.

## TDD evidence

Each slice started at a public CQRS, retained-reader or HTTP boundary. The table
uses the exact test-name filter supplied to Cargo. Unless another target is
shown, the command was:

```sh
cargo test --locked -p octacity-server-application --lib <filter>
```

Builds used an owned temporary `CARGO_TARGET_DIR`, `CARGO_BUILD_JOBS=2`,
`CARGO_PROFILE_DEV_DEBUG=0`, `CARGO_PROFILE_TEST_DEBUG=0` and
`CARGO_INCREMENTAL=0`. The new decoder graph was resolved once, then subsequent
Cargo commands used `--locked`. Rejected fixture setup and environment failures
were repaired before implementation and are not counted as behavioral Red.

| Filter | Observed Red → Green |
| --- | --- |
| `issue_checks_are_bound_to_the_original_visible_candidate_without_execution_effects` | Required public Checks API absent → exact original subject/association and unchanged Issue/Run. |
| `issue_checks_show_required_unproven_checks_without_using_completion_as_a_pass` | Required declared check absent → required unqualified completion remains indeterminate. |
| `issue_artifacts_retain_original_input_references_without_fabricating_build_provenance` | Required Artifact API absent → original input references remain unavailable without invented producers or byte reads. |
| `issue_artifact_pages_bind_original_execution_visibility_and_run_version` | Missing continuation → bounded distinct original pages, canonical cursor rejection and stale-version handling. A later full regression found changed membership incorrectly invalidated the old cursor; focused Red reproduced it and Green returns stale/empty/no continuation. |
| `issue_artifact_reader_describes_project_input_origin_and_expiry_without_byte_or_build_reads` | Existing reader lacked the metadata API → real Project text publication metadata/expiry without Build or byte access; expired content rejected. |
| `issue_artifact_content_requires_exact_original_membership_and_independent_download_authority` | Required explicit content API absent → original membership, independently protected Artifact access, and plaintext expansion. |
| `issue_artifact_preview_decodes_raster_and_rejects_executable_or_disguised_bytes` | Valid PNG rejected by the text-only path → decoded PNG succeeds; HTML/SVG and disguised raster bytes fail. |
| `issue_artifacts_include_retained_reports_with_exact_execution_producer` | Retained report missing → exact report reference, record and original producer remain even when bytes are unavailable. |
| `issue_checks_qualify_typed_outcomes_and_preserve_original_producer_and_head` | Qualified pass remained indeterminate → exact typed pass/fail/indeterminate with original producer, record and time. |
| `issue_artifact_content_expiring_during_read_is_not_returned` | Bytes returned after the reader advanced past expiry → expired bytes are discarded. |
| `issue_external_checks_are_bounded_original_head_observations_and_never_factory_acceptance` | Normalized detail rejected as an unknown field → bounded safe detail retained; foreign head and duplicate/oversized detail rejected. |
| `installed_review_observation_retains_bounded_external_checks_for_its_original_head` (`-p octacity-server --lib`) | Explicit installed observation had no CI detail → real process reader retains original-head skipped/unknown facts and partial coverage, without provider content URL. |
| `issue_artifacts_discard_a_page_when_the_run_changes_during_metadata_reads` | Torn page marked partial → stale/empty/no cursor after an actual retained Run transition during metadata lookup. |
| `issue_result_panels_http_translate_exact_bounded_reads_and_safe_content` (`-p octacity-server-api-rest --test v1_handlers`) | Required HTTP composition absent → exact bounded selectors, closed grammar, safe binary/text headers and no dispatch for invalid selectors. |
| `issue_external_checks_http_pages_retained_heads_without_provider_dispatch_or_acceptance` (same REST target) | GET returned 404 → real Issue/review handlers return old/new heads, blocked read authority and retained skipped CI without provider dispatch or eager summary expansion. |
| `issue_artifact_inventory_expands_metadata_only_for_the_requested_page` | Two metadata lookups for a one-item page → one lookup. |
| `runtime_installs_review_http` (`-p octacity-server --lib`, `-- --ignored`) | Real runtime returned unconfigured-service 503 instead of selected-Work 404 → composed optional result readers; PostgreSQL restart retains CI head/skipped/blocked and unchanged manual Issue. |
| `issue_checks_retain_missing_and_active_check_lifecycle_without_acceptance` | Valid retained second check remained pending after its Node Attempt started → missing/running/confirmed technical failure follow the common journal. |
| `issue_artifact_inventory_discards_metadata_when_visibility_is_revoked_during_expansion` | Revoked metadata returned as partial → unavailable/empty/no continuation. |
| `issue_artifacts_preserve_the_original_producing_input_candidate_without_discovery_facts` | Known original Context candidate missing → retained producer input candidate preserved without relabeling. |
| `issue_artifact_provenance_respects_repository_visibility_before_inventory_membership` | Protected candidate report appeared in inventory → hidden before membership/metadata/cursors. |

`issue_artifact_content_openapi_declares_unsupported_preview_capability`
(REST target): OpenAPI lacked the existing unsupported-preview 422 response →
422 is explicitly documented alongside only plaintext, normalized PNG and
attachment binary media. Runtime behavior was unchanged.

Additional public contract Red → Green:

- `python3 -m unittest tools.tests.test_check_architecture.ArchitecturePolicyTests.test_artifact_raster_validation_is_application_specific_without_transport_authority`:
  decoder rejected by policy → only the intended pure decoder package allowance;
  other application packages and application HTTP clients remain rejected.
- `pnpm --dir ui typecheck`: generated component/path APIs were absent → generated
  client accepts all four exact retained-read contracts. Behavioral
  `vitest run src/api/issueResultContracts.test.ts` verifies original selectors,
  same-origin GETs, explicit blob expansion and absence of mutation headers.

## Final checks

- Initial full application run: 644 passed; one cursor/membership regression was
  reproduced and fixed test-first as recorded above. Subsequent final relevant
  application regressions: `cargo test --locked -p octacity-server-application
  --lib issue_` — 162 passed; `--lib artifact` — 36 passed.
- `cargo test --locked -p octacity-server-api-rest --test v1_handlers` — 159 passed,
  including every-operation allow/deny-before-dispatch inventory, closed schemas,
  binary response media contracts and no dangling OpenAPI references.
- `python3 -m unittest tools.tests.test_check_architecture` — 33 passed;
  `python3 tools/check_architecture.py` — 63 packages, 256 edges, no violations.
- `cargo audit --file Cargo.lock` — zero vulnerabilities; existing informational
  `bincode` and `unsafe-libyaml` maintenance warnings remain unchanged.
- `cargo deny check licenses sources` — passed.
- `cargo metadata --locked --offline --manifest-path fuzz/Cargo.toml --format-version 1`
  and `python3 -m unittest tools.tests.test_security_matrix` — passed; fuzz lock
  needs no change for this application-only decoder dependency.
- `cargo clippy --locked -p octacity-server-application -p octacity-server-api-rest
  -p octacity-server -p octacity-server-store -p octacity-server-store-postgres
  --all-targets -- -D warnings` — passed.
- `cargo test --locked -p octacity-server --lib review_reader` — 6 passed;
  `cargo test --locked -p octacity-server --lib runtime_installs_review_http -- --ignored`
  with the disposable PostgreSQL URL — 1 passed, including outage and restart.
- `OCTACITY_OPENAPI_SCHEMA_FILE=/tmp/octacity-614-openapi.json pnpm --dir ui api:generate`,
  `pnpm --dir ui typecheck` and
  `pnpm --dir ui exec vitest run src/api/issueResultContracts.test.ts` — passed
  (one explicit generated-client behavioral test).
- `pnpm --dir ui test:unit` — 33 Node tests and 631 Vitest tests passed;
  `pnpm --dir ui lint`, `pnpm --dir ui format` and `pnpm --dir ui build` — passed.
  Production bundle remains identical to task 6.13: 53 files, 1,133,649 JavaScript
  bytes and 1,220,733 total static bytes. Console panes remain task 6.15.
- `cargo fmt --all -- --check` and `git diff --check` — passed.

## Qualification and resource boundary

The installed-reader tests use a hermetic MR vocabulary through the actual
existing JSONL process host and managed repository grants. Runtime HTTP uses a
real disposable PostgreSQL 18.6 instance, including restart; no live forge or
object-storage service qualification is claimed. Existing native reader/protocol
qualification remains in task 6.11. These projections add no native GitLab/Jira
implementation or provider content downloader.

Disk space was checked before and during builds. Only a task-owned temporary
Cargo target and disposable tmpfs PostgreSQL container were created; shared
Cargo/Docker caches, user checkouts and unrelated work were not removed. After checks, the task-owned 3.4 GiB Cargo target was removed
and the disposable
`octacity-614-pg-20261021` container was stopped (automatic container removal).
Final free space: approximately 53 GiB. OpenSpec progress: 46/56 complete;
6.15 remains pending.
