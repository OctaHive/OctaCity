# Review fixes for tasks 4.4–4.6

## Scope

Attachment replay recovers the original scoped receipt before provider access.
The existing transactional publisher still rechecks the same immutable intent,
including when concurrent requests both miss the read-only probe. PostgreSQL
attachment and mapping probes share the existing mutation identity/receipt helper;
no additional receipt table, queue or publication implementation was introduced.

Paged attachment retains the preview's original scan ID and opaque cursor.
Explicit refresh reuses the existing Source read journal, bounded driver, claim,
checkpoint, retry schedule and refresh publisher. A pending slice returns
`scanning`; Console continuation reuses the common frozen confirmed command and
original key. The runtime installs explicit read/lease policy without requiring a
Factory target. Existing background observation replay keeps its namespace.
Manual creation and provider-neutral native identities remain unchanged.

Pull predicates and the common Reporter effect editor enforce the generated
per-row label ceiling while preserving deselection. The effect ceiling derives
from `WorkSourceStatusEffects`; the advertised capability vocabulary may be
larger than one rule's effect set. No literal provider statuses or new UI runtime
dependency were added.

The stale future-task statement in `docs/reference/issues.md` was replaced with
the implemented HTTP/Console behavior. The same reference now describes original
pagination context, bounded refresh continuation and attachment replay. Prose and
format-only changes have no executable TDD cycle.

## Observed TDD cycles

Rust commands use `--offline`, `CARGO_INCREMENTAL=0`,
`CARGO_PROFILE_DEV_DEBUG=0`, `CARGO_PROFILE_TEST_DEBUG=0` and
`CARGO_BUILD_JOBS=2`. UI commands run from `ui/`. Tests use public HTTP, store and
Console boundaries. Fixture/environment failures are excluded from Red evidence.

| Command / filter | Observed Red | Observed Green |
| --- | --- | --- |
| `cargo test -p octacity-server-api-rest --test v1_handlers issue_source_http_attachment_replay` | Exact successful attachment retry returned 503 during provider outage | Original link/time is recovered with replay disposition; changed intent conflicts before provider I/O |
| `cargo test -p octacity-server-store-postgres --test issues source_attachment_matches_memory -- --ignored` | Required read-only attachment replay API was absent | Memory/PostgreSQL agree on absent/original receipt; original publication replay survives reconstruction |
| `cargo test -p octacity-server-api-rest --test v1_handlers issue_source_http_attachment_preserves` | Second-page attachment returned 400 after dropping preview scan ID | Original scan ID and opaque cursor attach the selected native task |
| `cargo test -p octacity-server-api-rest --test v1_handlers issue_source_http_refresh_resumes` | Linked item beyond the first page returned 422 | First bounded slice returns scanning; continuation reaches the task; exact completed retry returns the same receipt |
| `pnpm exec vitest run src/features/issues/IssueSource.test.tsx` | Pull/push allowed a 65th label; second-page attachment omitted scan ID; pending refresh had no continuation | All 18 Source scenarios pass, including deselection, original preview context and unchanged refresh body/key |
| `node --test scripts/issue-source-constraints.test.mjs` | Generated effect ceiling was missing | Common effect ceiling is derived from the published schema; both script tests pass |
| `cargo test -p octacity-server-api-rest --test v1_handlers issue_source_http_paused_pull` | A new refresh of a paused link returned 200/scanning | It preserves the existing 422 refusal before a provider read |

Before replacing the duplicated mapping probe SQL,
`cargo test -p octacity-server-api-rest --test v1_handlers issue_source_http_mapping_publication`
passed. Before changing the common Reporter form,
`pnpm exec vitest run src/features/factory/SourceInputEditor.test.tsx` passed all
17 existing scenarios. Focused Green qualification then passed 37 Console
scenarios across Issue Source, Source input and Flow Source update.

The HTTP concurrency qualification releases two identical attachments together
after both read-only probes. Exactly one returns 201, the other returns the
original 200 replay, and both links match. It exercises the existing atomic
publisher rather than adding an application lock or another idempotency journal.

## Final checks

Results are recorded below after completion. Disposable PostgreSQL uses the
`octacity-source-review-fixes` container with bounded tmpfs storage. Builds use
the existing target directory. No commit is created by this task.

| Qualification | Result |
| --- | --- |
| `cargo test --offline -p octacity-server-api-rest --lib --test v1_handlers` | 12 library and 135 HTTP tests passed, including concurrent attachment, original replay, paged attachment/refresh and paused-pull refusal |
| `cargo test --offline -p octacity-server-store-postgres --test issues -- --ignored --test-threads=2` | 41 PostgreSQL tests passed, including rollback/restart, mapping competition, Source recovery and attachment replay |
| `cargo test --offline -p octacity-server --lib runtime::issue_tests -- --ignored --test-threads=1` | 2 composed runtime/PostgreSQL journeys passed |
| `cargo test --offline -p octacity-server --lib runtime::work_` | 78 native Source/Reporter tests passed; 9 separate database scenarios intentionally excluded. Actual GitHub fixture output and a second opaque provider share the same task contract |
| `cargo clippy --offline -p octacity-server -p octacity-server-application -p octacity-server-store-postgres --all-features --all-targets -- -D warnings` | Passed |
| `pnpm api:generate`, `pnpm test:unit` | Contract generation passed; 33 Node checks and all 570 Console tests passed |
| `pnpm test:browser` | All 105 browser scenarios passed |
| `pnpm lint`, `pnpm format`, `pnpm build` | Passed, including TypeScript and unchanged bundle/architecture limits |
| `cargo fmt --all --check`, `python3 tools/check_architecture.py`, `openspec validate add-factory-work-card --strict`, `git diff --check` | Passed |

The resulting bundle has 1,068,921 JavaScript bytes, 85,477 CSS bytes,
1,155,020 total static bytes and 51 assets. Existing limits were not raised.
The native test linker emitted the macOS large unwind-section performance
warning; the runtime tests passed, and Clippy with warnings denied passed.

The complete shared regression also passed:
`cargo test --offline -p octacity-server-application -p octacity-server-store -p octacity-server-factory --lib`
completed all 585 Application, 296 Factory and 81 store tests. The longest
Application journeys completed successfully; no failed or interrupted scenario
was excluded from this result.

The disposable PostgreSQL container was stopped and removed after database
checks. Final available disk space was approximately 8.3 GiB. No commit or staged
change was created.
