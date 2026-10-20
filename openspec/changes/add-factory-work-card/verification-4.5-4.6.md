# Tasks 4.5–4.6: task Source management and provider-neutral conformance

## Scope and public test boundaries

REST task Source/synchronization handlers use the optional `IssueSourceHandlers`
application owner and its existing store/observation ports. Card reads use
retained metadata; explicit previews/attachment/refresh use authorized installed
read profiles. Ownership/status mapping publication reuses the immutable 4.4
contract and ordinary transition policy. Console tests exercise accessible
operator actions against the generated transport contract. Installed GitHub
fixtures and a second fake vocabulary qualify the same Source boundary.

No task form accepts credentials, endpoints, native provider payloads or
execution setters. Source choices do not grant workflow, Issue or Reporter
authority. Hidden canonical associations must remain undisclosed. Manual tasks
need no installed provider or Factory target. Reporter delivery remains the
separately planned 5.1–5.3 work.

Rust builds use the existing target with `CARGO_INCREMENTAL=0`,
`CARGO_PROFILE_DEV_DEBUG=0`, `CARGO_PROFILE_TEST_DEBUG=0` and
`CARGO_BUILD_JOBS=2`. Free disk space at the start was 15 GiB.

## TDD evidence

| Command/filter | Observed Red | Observed Green |
| --- | --- | --- |
| `cargo test --offline -p octacity-server-api-rest --test v1_handlers issue_source_http_manual_synchronization` | Optional task Source HTTP composition was absent | Manual synchronization and source-link reads return retained empty state with `no-store`, without invoking the provider |
| `pnpm exec vitest run src/features/issues/IssueSource.test.tsx` | The card had no Source/synchronization section | A local task is explicit and requirements editing remains available; the test awaits the independent retained read |
| `cargo test --offline -p octacity-server-api-rest --test v1_handlers issue_source_http_choices` | Authorized choices endpoint returned 404 | Only installed readable fields and opaque advertised states appear; an absent writer exposes no push fields |
| `cargo test --offline -p octacity-server-application --lib issue_source_choices_recheck` | An independently authorized installed writer was hidden | Current connection use and TaskUpdate expose it; denied use or revoked write hides push while preserving authorized pull. All 14 mapping tests then passed with the shared writer authorizer |
| `cargo test --offline -p octacity-server-api-rest --test v1_handlers issue_source_http_explicit_attachment` | POST attachment returned 405 | Reviewed association preserves manual requirements/ownership; exact replay returns the original link after a later edit, and the retained observation matches the closed schema |
| `cargo test --offline -p octacity-server-api-rest --test v1_handlers issue_source_http_canonical_conflicts` | A generic 409 lost the scoped canonical outcome | Existing finite association outcomes preserve a visible canonical link or null for hidden links without merging tasks; all four Source HTTP scenarios passed |
| `cargo test --offline -p octacity-server-api-rest --test v1_handlers issue_source_http_native_candidates` | Native choice preview was absent | Explicit bounded Source reads return safe identities and original continuation context without creating or attaching a task |
| `cargo test --offline -p octacity-server-api-rest --test v1_handlers issue_source_http_untrusted_continuations` | A cursor exceeding the UTF-8 byte ceiling reached the provider boundary | Attachment and preview reject it before provider I/O, reusing the existing normalized Source cursor validation |
| `cargo test --offline -p octacity-server-api-rest --test v1_handlers issue_source_http_mapping_publication` | Mapping publication was absent | Separate immutable mapping versions, exact server pins, historical reads, original replay and stale-review conflict preserve the Issue head |
| `cargo test --offline -p octacity-server-api-rest --test v1_handlers issue_source_http_explicit_refresh` | Explicit refresh endpoint was absent | A new authorized read advances retained freshness without editing unsynchronized fields |
| `cargo test --offline -p octacity-server-api-rest --test v1_handlers issue_source_http_refresh_retry` | Repeating the same HTTP request after provider outage returned unavailable instead of the original receipt | A one-shot read fingerprint and read-only probe reuse the existing refresh publisher and replay records; changed continuation conflicts before I/O |
| `cargo test --offline -p octacity-server-store-postgres --test issues source_explicit_refresh_replay -- --ignored` | PostgreSQL one-shot receipt lookup was absent | Outbox failure rolls back receipt/freshness, retry and restart recover the original receipt, and legacy observation producers retain their original separate namespace and strict observation identity |

The 38 existing Issue HTTP scenarios passed before shared transport/choice
extension. Setup-only missing type imports and asynchronous test waits are not
counted as behavioral Red.

| Command/filter | Observed Red | Observed Green |
| --- | --- | --- |
| REST `issue_source_http_pull_control` | Control endpoint absent | Original generation/replay receipts, stale generation conflict, independent pause/resume and no provider call |
| REST `issue_source_http_linked_choices` | An incompatible installed profile remained advertised after attachment | Existing installation/native-container compatibility is shared by mapping validation and choices |
| REST `issue_source_http_schema_publishes` | UTF-8 mapping/predicate limits absent from the schema | 65,536-byte mapping and 128-byte native-state bounds are exported for generated forms |
| `node --test ui/scripts/issue-source-constraints.test.mjs` | Source constraints export absent | Distinct synthetic row/UTF-8 ceilings are preserved; missing required limits reject generation |
| `pnpm exec vitest run src/features/issues/sourceApi.test.ts` | Source transport operations absent, then a finite conflict incorrectly resolved as success | All task paths/headers use the generated client; finite conflict retains only server-authorized canonical data |
| `pnpm exec vitest run src/features/issues/IssueSource.test.tsx` — attachment | Configuration/native discovery controls absent | Explicit bounded discovery and frozen ownership review precede association; no native preview storage |
| Same command — ownership | Authority fields absent | Publication uses its own mapping/pin and Issue precondition without editing a workflow |
| Same command — pull rules | Pull editor absent | Opaque advertised state/labels map to a stable workflow status ID; confirmation contains the reviewed values |
| Same command — push effects | Independent enable switch absent | Shared Reporter editor selects installed effects and stable workflow IDs; push starts disabled |
| Same command — observation controls | Retained freshness/control presentation absent | Pause and explicit refresh use separate confirmed commands; only pause uses observation generation |
| Same command — stale workflow/choices | New review remained enabled after changed review data | Draft is preserved and requires explicit reload; confirmed intent is never silently rebased |
| Same command — canonical conflict | Authorized canonical navigation absent | Only the authorized canonical task appears in the conflict confirmation |
| Same command — attachment focus | Closing successful attachment left focus on the document | Focus returns to the stable card control after the old form becomes stale |
| Same command — expiry | Expiry presentation absent, then no change at the original deadline | A receipt-specific badge updates at the original expiry without provider I/O or resetting a confirmed intent |
| Same command — changed profile | Profiles were indistinguishable, then old native rules survived selection | Exact profiles are distinguishable; explicit selection discards previous rules/ownership and disables push before new review |
| `cargo test --offline -p octacity-server --lib runtime_issue_journey_and_original_replay -- --ignored` | Default runtime returned 503 for retained synchronization | Manual task journey, retained Source metadata and original replay survive restart without any installed provider |
| `cargo test --offline -p octacity-server --lib runtime_installs_issue_sources -- --ignored` | Public independent Source installation API absent | Exact Source choices install without Factory; transient outage returns 503, absent profile returns 422, and ordinary editing remains available |
| `node --test ui/scripts/verify-architecture.test.mjs` and `pnpm build` | Measured Source workspace exceeded the old aggregate budget | Tight measured ceilings admit the feature and reject another unplanned 8,000 bytes; entry/CSS ceilings stay unchanged |

Shared refactor baselines: 19 existing Factory Source UI tests passed before
promoting exact-reference, row-key and Reporter presentation helpers to `shared`.
The full Console regression subsequently passed. The existing Issue HTTP baseline
passed before transport extension. The complete HTTP authorization matrix now
covers all nine additional routes under both allow and deny policies.
Fixture/import/type setup failures are excluded from behavioral Red evidence.

## Provider qualification

`cargo test --offline -p octacity-server --lib github_fixture_and_second_source`
passes the same task attachment/choice/observation/local-creation contract for
actual `GithubWorkProvider` output from the reused verified HTTPS Issue fixture
and for a second fake Source with opaque native identifiers/status vocabulary.
The GitHub fixture's normalized protocol page is fed through the existing
installed process fixture and production `HostedWorkSourceObservations`
normalization; no test-only SDK-to-Issue translation is added. This is fixture
qualification, not a live-provider claim. Native container/item identity remains
distinct from display IDs, and attachment does not overwrite local requirements.
Transient Source failure preserves retained association/freshness and manual
creation without GitLab/Jira installations, Factory admission or artifacts.

The reused GitHub Source and Reporter HTTPS suites separately qualify finite
managed effects, unsupported grants, partial writes and restart observation.
The existing second-provider process Source/Reporter suites qualify those common
protocol contracts and ordering/recovery. New Issue writer choices reuse the
independent managed TaskUpdate authorizer; read-only profiles advertise no push.
Reporter dispatch of retained Issue push intent is still tasks 5.1–5.3.
Native/SDK types remain in infrastructure/runtime fixtures; Issue business status
continues to use its workflow IDs and provider-neutral literal predicates.

## Final verification

All Rust commands use the environment flags above and `--offline`. PostgreSQL
commands additionally use `OCTACITY_POSTGRES_URL` pointing to the disposable
`octacity-source-console-45` container; test names/databases isolate runs. UI
commands run from `ui/`. Results below are recorded after the relevant changes.

The final Console correction used another public UI Red/Green cycle:
`pnpm exec vitest run src/features/issues/IssueSource.test.tsx` first left an old
title visible when navigating from an explicitly refreshed card to a retained
list; it then passed after refresh invalidation reused the common Project Issue
query root. The test gives the retained list an infinite freshness window so an
automatic route refetch cannot conceal missing invalidation. All 14 Source UI
scenarios passed, followed by the complete 566-test Console regression.

| Qualification command | Result |
| --- | --- |
| `cargo test --offline -p octacity-server-api-rest --lib --test v1_handlers` | 12 library tests passed; the initial route-matrix count failure identified missing test registration. After adding all nine routes and recording handlers, `cargo test --offline -p octacity-server-api-rest --test v1_handlers` passed all 130 scenarios, including allow/deny authorization and all Source HTTP scenarios |
| `cargo test --offline -p octacity-server-application --lib issue_` | 103 passed |
| `cargo test --offline -p octacity-server-store --lib` | 81 passed |
| `cargo test --offline -p octacity-server-factory --lib` | 296 passed |
| `cargo test --offline -p octacity-github --test issue_listing --test issue_reporting` | 20 Source and 18 Reporter HTTPS fixture tests passed |
| `cargo test --offline -p octacity-server-store-postgres --test issues -- --ignored --test-threads=2` | 41 passed, including one-shot refresh rollback/restart and legacy replay separation |
| `cargo test --offline -p octacity-server-store-postgres --test migrations -- --ignored --test-threads=2` | 6 PostgreSQL migration scenarios passed |
| `cargo test --offline -p octacity-server --lib runtime::issue_tests -- --ignored` | 2 composed runtime/PostgreSQL journeys passed |
| `cargo test --offline -p octacity-server --lib runtime::work_` | 78 native Source/Reporter scenarios passed, including GitHub/second-Source task conformance; 9 PostgreSQL scenarios intentionally excluded from this non-database run |
| `cargo test --offline -p octacity-server --lib runtime::work_ -- --ignored --test-threads=1` | All 9 PostgreSQL Reporter recovery/order scenarios passed |
| `cargo clippy --offline -p octacity-server -p octacity-server-application -p octacity-server-store-postgres --all-features --all-targets -- -D warnings` | Passed |
| `cargo fmt --all --check` | Passed |
| `pnpm api:generate` | Passed; task API/types/constraints derive from OpenAPI |
| `pnpm test:unit` | Final 33 Node/script checks and 566 Vitest tests passed |
| `pnpm test:browser` | All 105 browser scenarios passed; after later profile/expiry changes, Source ownership and manual-card regressions were rerun with `pnpm exec playwright test tests/browser/issue-sources.spec.ts tests/browser/issues.spec.ts`: 4 passed |
| `pnpm lint`, `pnpm format`, `pnpm build` | Final lint, complete Prettier check, TypeScript/build and bundle/source architecture checks passed |
| `python3 tools/check_architecture.py` | 63 workspace packages / 256 dependency edges, passed |
| `openspec validate add-factory-work-card --strict`, `git diff --check` | Passed |

The final bundle contains 1,068,160 JavaScript bytes, 85,477 CSS bytes,
1,154,259 total static bytes and 51 assets. The aggregate ceilings are
1,075,200 JavaScript bytes and 1,157,120 static bytes; entry/CSS ceilings remain
460,800 / 86,016 bytes. Historical duplicate budget examples were consolidated
into the measured Source workspace scenario; an additional unplanned 8,000 bytes
still fails both aggregate gates. No Console runtime dependency was added.

The disposable PostgreSQL container was stopped and removed. Its data used a
bounded tmpfs. Of this run's generated artifacts, 1.75 GiB of obsolete/unused
native test executables were removed; current library test executables and
compilation caches were retained. Final free space was approximately 8.3 GiB.
No commit was created. Documentation/format-only edits have no executable TDD
cycle and were validated by the relevant format/spec/architecture checks.
