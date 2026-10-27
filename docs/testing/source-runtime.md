# Packaged Source runtime qualification

`packaged_source_only_import_creates_one_visible_canonical_issue_without_execution`
extends the existing release harness. It starts the checksummed Server executable
with `run`, serves the checksummed Console and its packaged routing/security
includes through nginx, and forwards management through the peer-verified Unix
socket. PostgreSQL and S3 must be disposable services supplied by the caller.
The copied native `octacity-github` executable is verified against its installed
adapter manifest before dispatch. Only the approved GitHub HTTPS API is a fixture:
it has a generated CA, a loopback endpoint and an explicit `localhost` network
ceiling. No live credential, live-provider write, model or Agent is used.

The browser previews and explicitly enables Project import, then opens one
canonical Issue. Public reads verify its primary native association, ordinary
default workflow/initial status, absent acceptance and empty execution history,
Factory Configuration and Run collections. Repeated reads retain its identity.
Setup/check/grant and preview alone create no Issue; every recorded provider
request must be GET. Direct unverified management ingress is denied.

## Run

Build Server and GitHub binaries from the selected revision using the ordinary
workspace commands; build `ui/dist` with `pnpm build`. Use the existing
`tools/package_server_release.py` and `tools/package_console_release.py` to package
them with the same version/revision. Extract each archive into its own directory
(the Console archive has no enclosing directory). Installed release checksums
and manifests are validated by the harness before any process starts. Debug or
release binaries can be qualified; record which build profile was used.

Supply absolute paths and disposable endpoints:

```sh
export OCTACITY_RELEASE_SERVER_ROOT=/absolute/installation/octacity-server
export OCTACITY_RELEASE_CONSOLE_ROOT=/absolute/installation/octacity-console
export OCTACITY_RELEASE_GITHUB_BINARY=/absolute/native/octacity-github
export OCTACITY_RELEASE_UI_ROOT=/absolute/checkout/ui
export OCTACITY_RELEASE_NGINX=/absolute/path/to/nginx
# Optional; the Linux default is /etc/nginx/mime.types.
export OCTACITY_RELEASE_NGINX_MIME=/absolute/path/to/mime.types
export OCTACITY_RELEASE_EVIDENCE_DIR=/absolute/disposable/evidence
export OCTACITY_POSTGRES_URL=postgres://octacity:octacity-secret@127.0.0.1:15432/source_qualification
export OCTACITY_MINIO_ENDPOINT=http://127.0.0.1:19000
CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 \
CARGO_BUILD_JOBS=2 cargo test -p octacity-release-harness \
  --test release_vertical_slice packaged_source_only_import -j2 -- --ignored --nocapture
```

The endpoint credentials above belong only to disposable fixtures. Create the selected database and
`octacity-artifacts` bucket before running. Install Chromium for the checkout's
pinned Playwright. nginx must run under the current UID; no system service or
personal stand is started. Use a fresh evidence directory for each run. The
fixture namespaces Project names, connection names, credential references and
idempotency keys. Allocate a disposable database per suite/provider installation;
retain that same database throughout its restart barriers. Keep earlier suite
databases separately instead of mixing their still-active consumers and different
installed catalogues into a new installation. The explicit metadata-history case
qualifies pagination without depending on unrelated failed-suite residue.

The evidence root contains one unique `source-<uuid>/` directory per environment,
so an aggregate run preserves each case. Each directory contains process/browser
logs, Playwright attachments and
failure traces, plus `packaged-source.json` with verified Server/Console manifests,
native adapter SHA-256, native configuration and Source-profile SHA-256, Project
identity and provider request count. `canonical-issue.json` records the observed
Issue, native key and workflow/status pins without task bodies or credentials.
Failed journeys stop their processes as well; disposable dependency services are
owned and cleaned up by the caller.

## Shared integration setup boundary

`server/tests/octacity-release-harness/tests/release_vertical_slice/source_environment.rs`
owns the packaged environment; `source_provider.rs` reuses the native adapter's
HTTPS peer, and `source_installation.rs` owns only trusted fixture installation.
`source_setup.rs` performs the currently supported protected connection,
credential, check, activation and separate Project grant operations through nginx.
No internal application handler or alternate Server startup is substituted.

`simplify-integration-setup` qualification should reuse `SourceEnvironment::start`
and `browser` for its enrollment/archive/delete journey, replacing the current
multi-step setup with its own UI actions. It must not duplicate this provider or
start a second stand harness. That change's setup/lifecycle tasks remain pending.

## Observed TDD evidence, 2026-10-27

At production revision `f8135d29281c88ac1058100096e4a957b7059f60`, the Source-only
journey was first run with the catalog omitting `source_profiles_file`. The real
browser received 422 from import discovery and could not add an import. Adding
that existing normal-startup declaration made preview, reviewed import and the
canonical card pass. No production handler change was necessary. Archive-layout,
response-field and wide-layout fixture corrections were not counted as Red.

Re-running against retained PostgreSQL exposed fixed-key/name conflicts (409).
Namespaced fixture keys/resources made that same public journey reusable without
resetting retained data. This is harness behavior, not a new provider permission.

The local run uses macOS arm64 debug0 packages, nginx 1.31.8 and real native
GitHub adapter SHA-256
`7bad32698cdd1f6166081cad316d5fd10f6330e987f94a13e150d4ed36d35ff9`.
Disposable dependencies were PostgreSQL 18.6-bookworm (image ID
`184cd2ec0e35d940bc81a1892a48484f8619651589e07362ecb950517180ca80`)
and `octacity/minio:release-2025-10-15-9e49d5e7a648` (image ID
`97dfb46a6b0f02adb6b6b3aca6b19e5150086c2997a6cf379ef09cfa85535a2e`).
This qualifies packaged native startup and the source-only browser path; Docker
server-entrypoint retention, Linux platform execution, status push and optional
Factory execution retain their separate qualification boundaries.

## Reviewed status synchronization (task 6.2)

Run the same provisioned environment with the optional Reporter journeys:

```sh
CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 \
CARGO_BUILD_JOBS=2 cargo test -p octacity-release-harness \
  --test release_vertical_slice packaged_ -j2 \
  -- --ignored --nocapture --test-threads=1
```

Run serially when sharing one disposable database. The three cases qualify
Source-only import with no Reporter, a permitted manual status effect, and a
read-only connection with Reporter support installed. Installation alone grants
no writing authority. The same `SourceEnvironment` supplies all processes and
native fixtures; normal server/Console/adapter code is unchanged from 6.1.

The status journey uses three browser phases, each with separate logs/results:

1. `issue-source-mapping.spec.ts` authors `Editorial / 研究` through the existing
   canvas with `Arrivée / Inbox` and `Décision / Accepted`, reviews publication
   and separately selects the default. After import it publishes another default,
   verifies the Issue retains its original workflow, and reviews the native push
   mapping. Every provider request through this phase must be GET.
2. `issue-source-status.spec.ts` confirms the permitted manual move, observes
   accepted local version 2 and a delivered metadata-only projection with the
   original workflow pin. The native fixture must receive exactly one
   `PATCH /api/v3/repos/owner/tickets/issues/17` with `{"state":"closed"}`. Its
   unrelated `human-owned` label remains unchanged. There is no Factory
   Configuration, Work-backed execution or Run.
3. `issue-source-pull.spec.ts` reviews Source-owned status pull mapping native
   `closed` to `Arrivée / Inbox`. The declared reverse Source edge produces one
   Source-origin move, version 3. The supervised consumer's later echo preserves
   that revision/history. Reviewing `Not synchronized` then permits a local
   manual move to version 4 without another projection or native write. The
   Issue and all retained reporting keep the original workflow despite the newer
   Project default. No status spelling supplies authority.

The Source-only and installed-Reporter/read-only cases both verify that push is
absent from the authorized UI choices, a reviewed unsynchronized manual move
still works, and no outbound operation or provider write is created. No browser
REST interception, alternate application installer, direct store mutation or
live provider is used. Pull is qualified through its supervised consumer and
public Issue/synchronization/history reads. Manual refresh continuation timing
is not an additional completion claim for this journey.

`packaged-status.json`, `packaged-source.json` and `packaged-read-only.json`
retain separate package/profile identities and Project observations.
`issue-source-pull/browser-results/*/status-qualification.json` retains the
original/default workflow identities, Source transition and final local revision,
plus the delivered operation identity. None contains a credential or task body.
The native HTTP peer now supports consistent listing/state reads and advances
`updated_at` only on a mutation, so a GET does not invent a provider update.
Production polling/reconciliation defaults are retained.

Observed Red/Green uses the same command filtered to
`packaged_issue_status_mapping_delivers_without_execution`: missing Reporter
composition hid the UI push option; the opt-in Reporter installation and separate
credential/purposes made mapping review pass. A read-only-only provider fixture
could not deliver the accepted effect; its existing stateful native peer made the
exact PATCH/delivery pass. A Source pull rule without a permitted reverse edge
kept the original status; reviewing that Source edge made the same pull pass.
Finally, an artifact contract caught status evidence being overwritten by the
Source-only filename; separate report names preserve both observations. UI
wording/navigation corrections and scan-lease diagnostic probes are not
production Red evidence. Experimental accelerated reconciliation and debug
instrumentation were removed before the final run.

## Factory admission and configured Issue status (6.3–6.4)

`packaged_factory_source_admission_retains_canonical_issue_and_configured_status`
uses the same environment. The trusted installation additionally enables bounded
`## Acceptance` preparation and a second read-only native container,
`research/backlog` (native ID `5678`). Factory is enabled explicitly after public
Project creation, through the ordinary private profile file and normal Server
restart. Both Projects are declared in that installation. The second connection,
resource grant, import, code Repository and Factory input are then published
**without another restart**. This does not claim dynamic installation of new
executors or undeclared native resources.

The first Console phase authors a workflow permitting `pipeline` origin, imports
the card, and explicitly reviews a native push mapping. Two separately enabled
inputs admit distinct Work identities for the original native task; both resolve
the same canonical Issue and use the separately published code Repository.
HumanGate nodes deliberately retain those Runs for operator review. Neither
admission nor association grants a reviewed Issue requirements revision or causes
a business transition/provider write.

The next Console phase edits acceptance and explicitly confirms an Issue pipeline
launch against a separately published Flow containing `transition_issue`. The
ordinary installed local action moves the pinned workflow and the ordinary Issue
projection worker delivers one exact `closed` effect. Unmanaged labels survive;
there is no parallel Work Reporter producer. Editing the card again preserves the
execution's original requirements revision, task/acceptance digests and Work/Run
association.

Controlled PostgreSQL triggers interrupt only checkpoint acknowledgement **after**
canonical import/admission commits. Their SQL reads coordinate fault injection;
all business assertions use public HTTP projections. Restart preserves the original
card, Work identity and exact code/configuration subject, even after native text
changes. A native API fault then loses acknowledgement after applying the effect;
the normal Server restarts over the original journals and observes it without a
second PATCH. Input versions are separately disabled; writer withdrawal preserves
the reader choice, and subsequent reader withdrawal removes intake authority.
Manual drafting without acceptance and explicitly reviewed manual Factory admission
remain available after both grants are withdrawn.

Private Source preparation bytes, object publication and original retention/deadline
immutability remain qualified at their existing normal-runtime/store boundary
(`runtime_publishes_connection_credentials_and_project_access_without_enabling_inputs`
and `normal_issue_reporter_restart_preserves_original_effects_and_current_writer_authority`).
The public Work diagnostic intentionally does not expose private input Artifacts;
this gate does not add a diagnostic disclosure solely for tests. Those narrower
journeys also cover no-card/read-only Work, missing Source, native outage, in-flight
revocation and independent manual operation.

The Source Playwright fixture checks transient credential canaries against browser
storage and completed browser API response bodies. The harness rejects canaries
in retained evidence files, including nested public results and process logs.
Only task-owned fake credentials are used. No live user-repository mutation occurs.

Run the complete packaged subset serially against disposable PostgreSQL/S3:

```sh
CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 \
CARGO_BUILD_JOBS=2 cargo test -p octacity-release-harness \
  --test release_vertical_slice packaged_ -j2 -- --ignored --nocapture --test-threads=1
```

Within that instance directory, additional artifacts are `packaged-factory.json`,
`factory-observations.json` and
`factory-issue-execution/browser-results/*/factory-execution.json`. They retain
installation/package pins, Project/resource/code references, public Work identities,
execution digests and operation identity, without task bodies or credentials.
Each Server generation has separate logs so restart does not overwrite evidence.

The Factory execution boundary is deterministic **real native HumanGate and local
Issue action execution**. No model, Agent, code checkout or Build is substituted
or qualified by this journey. Exact code revisions are operator-approved fixture
pins; Agent/backend/model qualification retains its existing separate release gates.

Observed Red/Green increments:

- `cargo test -p octacity-server --lib file_issue_status_action_installs -j2`:
  ordinary file deployment rejected the action; explicit native installation made
  that test and all ten file-profile installation tests pass.
- The packaged workflow initially permitted only manual origin; an explicitly
  selected pipeline origin made the Console review pass.
- Factory choices initially omitted a preparation reader; explicitly installed
  preparation made that choice available. A body without acceptance admitted no
  Work; adding the reviewed acceptance section made admission pass.
- An execution targeting only HumanGate did not move the Issue; explicitly
  configuring the local action made the same browser disposition/delivery pass.
- The second native resource was initially absent. Its declaration and independent
  private reader handle made protected enrollment/import/admission possible;
  attempting to provision the first connection's credential again correctly gave 409.
- `source_evidence_rejects_credential_material_in_nested_public_results_and_logs`
  failed with a permissive evidence check, then passed with bounded file inspection.

HTTP restart responses, fixture JSON/schema errors and archive-layout corrections
are environment/test corrections, not production Red evidence. Qualification of
existing behavior at narrower seams is reported as regression coverage, not a new
feature Red/Green cycle.

The 6.3 local Server archive SHA-256 is
`041d83c991faa11eecbfc302e1d9c1f0d7306152b2becc991c8a53ee8fbd92f2`;
its executable SHA-256 is
`2257a20404c95e550bd61cf337ce012a2c20fbe570e9ba579e27aaa39441f2e1`.
The Console and GitHub executable pins above are unchanged. The Server was built
from the working tree based on `f8135d29281c88ac1058100096e4a957b7059f60`, including
the uncommitted file-action installation change. This is local qualification,
not a published release or an assertion that the base commit contains that change.

## Qualification boundaries and rollout

| Boundary | What this change qualifies | Remaining separate work |
| --- | --- | --- |
| Portable/runtime/store/REST and controlled Console fixtures | Authorization, bounded preparation, retries/deadlines, independent scopes and operator controls | Not a packaged or live-provider result by themselves |
| Packaged Server/Console/nginx + PostgreSQL/S3 + native adapter + hermetic HTTPS | Import, manual and configured pipeline status, optional admission, recovery and later resource grants | Linux/Windows execution, Agent/model/backend release gates |
| Opt-in approved live read-only qualification | Existing dedicated GitHub live-read guide and caller-supplied authorization | Not run by the default packaged suite |
| Live provider mutation | None by default | Requires a separately authorized disposable live target and its own retained evidence |

There have been no product releases. Existing metadata/checks-only test bundles
need a deliberately regenerated trusted Source/Reporter installation before these
capabilities are available. No database migration, implicit metadata-pin replacement
or compatibility branch is introduced. Preserve existing connection versions and
Work/Run history; review any new metadata/grants explicitly.

For rollback, disable the affected Project import, retained-link pull, Factory
input and outbound mapping independently through their current public controls.
Withdraw read/write grants separately when required, then restore the matching
trusted profiles/binary as one deployment. Keep retained journals and original
pins; do not rewrite accepted effects, extend deadlines, or clear history to make
readiness green. Incompatible retained profiles should remain visibly unavailable.

`simplify-integration-setup` still owns the future consolidated setup/lifecycle
journey. Autonomous selection/start of pipelines beyond these explicit controls
is not enabled by this qualification change. Archived changes remain untouched.
`enable-local-stand-runtime-mcp` remains a separate execution-installation change;
this qualification does not complete or require its pending MCP work.

The disposable nginx process runs in foreground single-process mode, retaining
its packaged routing/security configuration. A failure-drop regression initially
left the listener serving through an orphan worker; `packaged_failed_qualification_drop_releases_the_gateway_listener`
failed, then passed after removing the test gateway's unowned worker process.
This qualifies test-harness cleanup, not production nginx master/worker lifecycle.
Its command is the packaged-subset command above with that exact test-name filter.

For an older metadata-only **disposable test stand**, generate a separate root
instead of replacing retained pinned files in place:

```sh
OCTACITY_LOCAL_STAND_ROOT=/absolute/new-disposable-test-stand \
  tools/local-stand up --github-repository OWNER/REPO
```

Keep the prior root/history for rollback and use the current protected connection,
credential and Project-grant reviews in the new stand. This is explicit pre-release
test setup, not an automatic data migration.

Retained qualification databases can contain several pages of connection history.
The Console helper follows the existing **More connections** control until its
namespaced connection is visible, observing each appended page before selection.
The final Source-only journey first timed out when its connection fell beyond the
first page; pagination through the actual UI is the correction. No database reset,
REST interception, larger timeout or production pagination change is used.

The final suite uses one dedicated disposable database. Mixing 21 prior temporary
Factory installations in the earlier scratch database left a new input beyond the
20-second discovery window (no scan yet); this was environment contamination, not
production Red evidence. The older database was retained untouched. All restart
barriers still reuse the original database/configuration within the journey.

Evidence-isolation Red/Green uses the same failure-drop test filter. Two sequential
environments originally selected the same evidence directory (actual assertion
failure); unique per-instance directories preserve each case and Server generation
when all packaged cases run together. This changes only harness artifact layout.

## Section 6 local verification

The macOS arm64/debug0 qualification also passed these relevant gates:

| Command/boundary | Observed result |
| --- | --- |
| `python3 tools/check_architecture.py` | 63 packages, 260 dependency edges |
| `cargo test -p octacity-server --lib -j2` | 212 passed, 47 explicit environment/platform gates ignored |
| `cargo test -p octacity-github --tests -j2` | 123 passed |
| `cargo test -p octacity-server-api-rest --tests -j2` | 249 passed |
| PostgreSQL `issues`, `issue_execution`, `factory_discovery`, `integration_project_access`, `integration_connections`, `integration_administration`, with `--ignored --test-threads=2` | 139 real disposable-database contracts passed |
| Release harness `--lib --test harness --test release_vertical_slice` | 50 portable checks passed; platform/packaged opt-ins run separately |
| `cargo clippy -p octacity-server -p octacity-github -p octacity-release-harness --all-targets -j2 -- -D warnings` | Passed; affected harness targets rechecked after fixture corrections |
| `pnpm typecheck`, `pnpm lint`, `pnpm build` | Passed, including architecture/bundle budgets |
| `pnpm test:unit` | 36 Node checks and 830 Vitest tests passed |
| Console browser suite | 161 passed, 5 opt-ins skipped; the previously deferred pan/fit case explicitly excluded |
| Launcher/installation/package Python suites | 160 tests checked: 157 passed, 3 opt-ins skipped |
| Rustfmt, changed released-test Prettier, relative documentation links, strict OpenSpec validation and `git diff --check` | Passed |

The browser exclusion uses the exact previously deferred name:

```sh
pnpm exec playwright test --grep-invert \
  'pans the canvas with pointer and keyboard and fits every node without requests or persistence'
```

These results do not claim a Linux/Windows Agent/backend matrix, live model,
live-provider write, or clearance of the previously deferred Linux Microsandbox
zstd upstream audit branch. No audit ignore/vendor workaround is added here.

The final aggregate packaged run passed **all 5 cases in 150.39 seconds**, with
per-instance evidence isolation and explicit pagination histories for the shared
read-only/Source-only browser journey. Independent Standards and Spec reviews
found no remaining actionable findings after the cleanup, pagination and artifact
layout corrections. The task-owned PostgreSQL/MinIO services were stopped after
recording their evidence; no personal stand or retained user installation was reset.
