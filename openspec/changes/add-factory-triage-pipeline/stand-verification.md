# Factory stand implementation checkpoints

The default command now installs the actual disabled two-stage triage pipeline.
Current-owner publication and the assembled native/gateway/Console installation
passed, including immutable document edits and restart without republishing.
This qualifies the installation boundary, not live model entitlement or paid
model/GitHub-write behavior. See the [operating procedure](../../../docs/operations/local-stand.md#default-two-stage-triage).

## Credential and output boundaries

The pinned native Codex 0.161.0 recognizes the selected ChatGPT login in an
offline Linux container. No live model call or model entitlement is claimed.
An offline Octa 0.8.0 probe with a fake native client demonstrated that a token
stored only in `auth.json` could enter native result and trace files. Existing
event redaction alone did not protect those retained artifacts.

The common output freezer now refuses frozen credential-bearing files, reports
and archives before upload authorization, preserving original evidence bytes.
The Job lifecycle invokes that check before Uploading. Supervision observes
native file refreshes, and the final guard includes old and new tokens.
An explicitly selected Agent-owned installation retains native updates under
exclusive ownership; ordinary credential copies have no write-back authority.

Observed Red/Green cycles:

- `cargo test --locked -j2 -p octacity-runner --test private_credential_files`:
  missing scoped-file and persistent-file contracts failed first; scoped values,
  private copies, refreshed values and serialized retention subsequently passed.
- `cargo test --locked -j2 -p octacity-job selected_native_login`:
  a refreshed token initially escaped in terminal results; after filtering it
  remained redacted. A subsequent Red showed the source credential still held
  the old token; explicit native update retention made the same check pass.
- `cargo test --locked -j2 -p octacity-lifecycle --lib credential_bearing_output`:
  the unsafe file initially reached successful publication. Green refuses it
  before Uploading and still permits ordinary output. Both paths clean Job data.
- `cargo test --locked -j2 -p octacity-config native_login_profile` and
  `python3 -m unittest discover -s tools/tests -p test_configure_local_stand_agent.py -k native_login_file`:
  scoped fields and explicit update retention were rejected before implementing
  their configuration contracts; both then passed without copying token bytes
  into generated configuration.

Native refresh persistence follows the native client's
[documented CI pattern](https://learn.chatgpt.com/docs/auth/ci-cd-auth).
The code does not implement OAuth or guarantee indefinite session validity.

## Installation and document publication

`python3 -m unittest discover -s tools/tests -p test_factory_stand_triage.py`
has twelve passing tests. Meaningful Red/Green cycles covered the missing preset
publisher, dependency publication order, ignored explicit-catalog token flag,
Project document publication, disabled Source input publication, and missing
Compose MCP listener and separate role/profile publication. The latter verifies
stable replay identities, the same selected model and removal of investigation
MCP bindings and secret mappings from the analyst. Tests use real HTTP peers, effective Compose configuration
and a native nginx process; they do not start model inference or provider writes.

Initial goals and rules are repository templates published through the existing
Project document owner. The shared Console reader/editor publishes immutable
revisions and keeps accepted references stable. Source input publication pins
the actual published Configuration and remains disabled.

The Factory overlay exposes its authenticated MCP-only HTTPS listener through
loopback, with an explicit host-port override. Management paths return 404 on
that listener. MCP credential installation and atomic updates reuse the normal
server configurator, entrypoint and installation updater.

The listener now uses HTTPS: the live Runtime owner correctly rejected a bearer
over plain HTTP. The ordinary PKI owner can add the supported microVM peer to
the existing gateway leaf certificate without replacing its CA or private key;
ordinary roots continue to validate, and unrelated SANs remain rejected.
A real nginx/TLS test verifies the selected MCP route, peer hostname and local
CA, plus denial of management paths and removal of forged identity headers.
Bootstrap staging now copies only the initializer's core credentials and PKI,
excluding unselected native model and investigation credentials.

The new installer publishes dedicated ordinary worker resources with reviewed
network/resource/retention data in `deployment/local-stand/factory-triage.json`.
It publishes no demonstration Build, Trigger or Work. Initial Source selection
pins current connection/access revisions, requires supported open/unlabeled
narrowing and leaves intake disabled. A supplied code revision is retained
separately from process-completion criteria. Reporter writes remain separately
reviewed and are not inferred from the model's classification.

Private MCP installation declarations may provide `identity`, `version`,
`server`, optional `integration` and `authorization` instead of a precomputed
`reference`. The existing domain owner constructs and validates that exact pin;
the launcher does not duplicate Rust serialization. Previously pinned entries
remain accepted and strictly validated. A substituted retained pin cannot fall
back to a fresh declaration. This is private installation input, not a new
management endpoint or credential owner. The catalog test observed rejection
before adding declaration support; the ten related deployment tests then passed.

## Completed regression checkpoints

Before the final native-update slice, the five Agent libraries had 211 passing
tests and scoped all-target/all-feature Clippy passed with `-D warnings`.
The six installer modules had 117 passing tests with four explicit skips.
After native update retention and its failure-path characterization, the Job
library had 55 passing tests. Further final gates must be recorded after the
remaining integration is implemented; earlier gates are not a substitute.

Subscription accounting subsequently passed 327 Factory domain tests, 29 native
result-observation tests and 16 Console summary/diagnostic tests. Observed Red
cases covered unknown cost decoding, native results without monetary counters,
the API-key/subscription authentication mismatch, REST representation, Console
rendering and rejection of unpriced declared reservations. Existing measured
costs retain their numeric encoding; unknown totals preserve any known subtotal.
UI type checking and lint passed. Scoped server all-target/all-feature Clippy
also passed before the last reservation/schema adjustment; final gates must
cover the assembled installation.

The runtime installer can now install executable bytes without importing an
operator home for server discovery. Its separate native-login installer seeds
only a private Agent-owned `auth.json` and preserves later native refreshes.
The runtime module's four tests passed with one explicit platform skip.

After the MCP declaration and role-publication slices, all Server library tests
passed (226 passed, 54 explicitly ignored), scoped Server Clippy passed with
`-D warnings`, and the five affected Agent libraries passed 212 tests. The five
private-file integration tests also passed, including refusal to overwrite an
installation changed by another owner. The seven installer/runtime modules ran
122 tests with five explicit skips and no failures. `openspec validate
add-factory-triage-pipeline --strict` and `git diff --check` passed. These scoped
checks do not establish completion of the default launcher or packaged journey.

Current-owner qualification:

```sh
OCTACITY_TEST_TRIAGE_SERVER="$PWD/target/debug/octacity-server" \
OCTACITY_TEST_TRIAGE_ENV=/tmp/oc84-environment.json \
OCTACITY_TEST_TRIAGE_POSTGRES=octacity-84-postgres \
PYTHONPATH=tools/tests:tools python3 -m unittest test_factory_triage_setup_rest
```

This passed after meaningful failures for the HTTP bearer transport, missing
native MCP evidence verifier declaration, and the management client's 128 KiB
bootstrap response limit. The transport now explicitly selects a finite 4 MiB
management limit while ordinary bootstrap keeps its original bound; a real TLS
test covers both the unchanged default and rejection above the selected limit.
The qualification creates and drops its own database, uses actual retained S3
documents, qualifies both profiles, publishes the two-stage Flow and disabled
Configuration, and verifies no Run was created. It deliberately dispatches no
Agent or inference and does not substitute for native packaged execution.

## Final assembled installation and review

Normal package producers archive committed Git sources. To respect the explicit
no-commit instruction, qualification supplied a separately packaged development
source tree, using the normal source extractor, Docker builders, release packager,
strict native stager and image contract checks. Only artifact selection was
injected into the opt-in test: handlers, authorization, Source/Reporter adapters,
PostgreSQL/S3, private installers, registration and native Agent remained real.
The worktree controller scripts ran directly; no runtime permissions or handlers
were replaced. No commit was created and the real Git index was unchanged.

- Base commit: `8d206b943d9073631d5a27d612bca855b0d85b2f`.
- Explicit development source tree: `6f2d2c0048d75842386f386abb9f54670cf802df`.
- Server: `octacity/server:triage84-6f2d2c0048d7`, image
  `sha256:b8ca833511ab29d77a5dd34f9a7fe5002d7a24b0b5e587685b668f8ca32af329`.
- Gateway: `octacity/gateway:triage84-6f2d2c0048d7`, image
  `sha256:a1632d668a1a02a7e338f50c4782fa268c534a79c516c052e97752f55ae30a74`.
- Controller SHA-256 at that packaged checkpoint: `tools/factory_local_stand.py`
  `001a2366222631733c5c0ca96027508f9bc95a4696686652523a3e406817b702`;
  `tools/factory_triage_setup.py`
  `0f396aaa337b0e81211f50bc9bfe778715237aa4aed8aba7aaf5527649a14638`.
  These include the later Project-bound completion correction and reference-only
  receipt; native application behavior is the packaged tree above.

The opt-in `test_factory_stand_native` consumes a private
`OCTACITY_TEST_FACTORY_STAND_INPUT` JSON file containing `native_installation`,
`source_tree`, `server_image`, `gateway_image`, and `github_token_file` paths/refs.
It requires the shared Compose project to have no existing containers or volumes.
It runs the actual default command, verifies two published profiles, disabled
Configuration/open-unlabeled Source input, separate credentials and no Runs,
then runs `ui/tests/released/factory-stand-documents.spec.ts` against that gateway.
The EN/RU narrow keyboard tests read the initial goals and publish new immutable
revisions through real management APIs without REST interception. Node requests
trust the supplied local CA; Chromium trusts only the selected gateway public-key
SPKI. No workstation trust settings are changed. Normal TLS identity/CA and
management-path denial are independently tested by the real nginx/TLS test.

The ordinary down/default-start recovery preserves receipt references, both
old/new document content, stable Agent identity, registration credential secret,
native model/MCP credential bytes and an operator's connection revocation.
Reconnect may advance the registration ID through the
existing protocol; it is not evidence of a new Agent or a new credential secret.
No model inference or remote business mutation is requested. The GitHub check
performs an authorized native read; native write success remains unverified.

Additional observed Red/Green and characterization:

- The default command initially reached the Agent validator and failed because
  permission arrays were not canonically ordered. The existing V3 projection
  now orders the same grants without widening them; the real Agent accepts it.
- Source discovery returned 503 with `installed Source profiles unavailable`:
  raw bundled completion content lacked the typed draft kind and owning Project.
  `test_factory_source_installation` first failed for the missing required Project
  argument. Its twelve tests then passed and the real Source choices/default
  installation succeeded with the normal Project-bound draft contract.
- Gateway Docker build failed because a test-support module was excluded from
  the production build context. Existing 34 `manual_trigger` tests passed before
  and after moving that shared support into `src/testing`; the real Gateway
  build then passed without broadening Docker context rules.
- The existing demonstration generators were moved into test-only support.
  The previous 162-test baseline and subsequent scoped 60-test refactor gate
  passed (five explicit skips each). The default command owns no demo branch.
- Environment/fixture failures (archive prefix, long macOS socket path, missing
  browser CA trust, incorrect response shape or ordering assumptions) are not
  production Red evidence. PostgreSQL snapshot budgets are addressed by their
  current projection, not by assuming digest-ordered history ends with the newest
  row. The resulting actual-store regression preserves unknown cost and known
  monetary subtotal after restart.

## Final scoped gates

Cargo commands use `CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2` and `--locked -j2`.

| Command / boundary | Observed result |
| --- | --- |
| `python3 tools/check_architecture.py` | Passed: 64 packages, 263 dependency edges |
| `cargo fmt --all -- --check` | Passed after formatting |
| `cargo clippy -p octacity-server -p octacity-server-api-rest -p octacity-server-store-postgres -p octacity-agent -p octacity-config -p octacity-job -p octacity-lifecycle -p octacity-runner -p octacity-output --all-targets --all-features -- -D warnings` | Passed |
| `cargo test -p octacity-server-factory` | 373 passed including integration/doc tests |
| `cargo test -p octacity-server-api-rest --test v1_handlers` | 235 passed |
| `cargo test -p octacity-server-store-postgres --test factory_discovery subscription_usage_preserves_unknown_cost_and_known_subtotal_after_restart -- --ignored` with the explicitly disposable PostgreSQL | 1 passed |
| `cargo test -p octacity-server-store-postgres --test issue_execution standalone_project_documents_use_the_existing_input_journal_without_admission -- --ignored` | 1 passed |
| `PYTHONPATH=tools/tests:tools python3 -m unittest test_factory_stand_triage test_factory_local_stand test_factory_local_runtime test_factory_source_installation test_configure_local_stand_server test_configure_local_stand_agent test_bootstrap_local_stand test_init_local_stand test_local_stand_entrypoint test_local_stand_launcher` | 168 tests, five explicit opt-in/platform skips, no failures |
| `pnpm run typecheck`, `lint`, `format`, `build`, `check:architecture` in `ui` | Passed, including bundle budgets |
| `pnpm exec vitest run src/features/factory/FactoryTriageSetup.test.tsx src/shared/FactoryRunSummary.test.tsx src/shared/FactoryDiagnosticRecord.test.tsx` | 30 passed |
| `pnpm exec playwright test tests/browser/factory-triage.spec.ts` | 2 passed (EN/RU narrow keyboard) |
| `OCTACITY_TEST_FACTORY_STAND_INPUT=<private descriptor> PYTHONPATH=tools/tests:tools python3 -m unittest test_factory_stand_native` | Actual default installation, 2 real Console tests and restart passed |
| `openspec validate add-factory-triage-pipeline --strict`; `git diff --check` | Passed |

Architecture review kept current owners: native authentication/refresh in the
client and Agent credential installation, accounting in the common budget model,
Project content in the existing document journal, execution state/results in
Server and provider delivery in the existing ordered Reporter. No OAuth broker,
new mutable Project-state owner, second executor or demo dispatch was added.
Reviewed adjustable defaults live in the installation policy file. The small
Agent wire adapter only projects the existing V3 shape/ordering; Server and Agent
independently validate it. Immutable local receipts retain references, not Flow
graphs or step results. Public and private contracts/comments describe ownership,
privacy, replay and failure behavior; operating guidance covers renewal, retention,
label-scope updates, completion versus implementation and rollback.

Disk observations stayed above the 10 GiB reserve, approximately 11 GiB while a
full disposable stand existed and 12 GiB after cleanup. Cleanup removed only the
old task-independent Rust incremental cache and eleven unused, older test
executables after verifying no active build/process used them; source files and
user stand data were preserved. Builds disable incremental compilation. Task-owned
disposable Compose services/volumes and roots are removed by qualification.

No paid inference, live GitHub effect or compatibility with an uninstalled
ready-made external MCP server is claimed. Their optional qualification remains
explicit in the runbook. Normal production launch still builds committed sources;
this report's development-tree evidence must not be labelled a committed release.

## Post-review recovery corrections

Two public default-command regressions were corrected through separate TDD cycles:

1. `PYTHONPATH=tools/tests:tools python3 -m unittest test_factory_stand_recovery.FactoryStandRecoveryTests.test_default_resume_uses_the_retained_custom_code_repository`
   first failed with `the triage installation has another selected code source`.
   Green retains the saved code source when the flag is omitted, applying the
   public default only on first installation. Explicit conflicting selection
   remains rejected without replacing the original receipt.
2. `PYTHONPATH=tools/tests:tools python3 -m unittest test_factory_stand_recovery.FactoryStandRecoveryTests.test_default_resume_keeps_the_current_reviewed_installation_after_an_explicit_update`
   first failed with `generated server server.toml differs from the verified inputs`.
   Green resumes after real atomic updater transactions renew the MCP token/expiry
   and replace the Agent policy. Generated configuration and the Agent credential
   remain byte-for-byte unchanged during resume.

Both checks invoke the actual default CLI and existing configurators/updater.
Only package/image production, HTTP enrollment and process startup use finite
test adapters; no handler or configuration validator is replaced. These are
private installation/recovery checks, not a new packaged model/provider pilot.
An initial fixture used ordinary body-acceptance mapping instead of triage's
Project-bound completion mapping; its rejection was a fixture correction, not
additional production Red evidence.

The existing installation owner now supplies a single `installed_factory_inputs`
projection reused by update and resume. It selects current catalog, Agent policy,
Runtime/Delivery bundles and exports the existing Signal/MCP references into a
private temporary directory. It creates no new persistent state and keeps file
encoding/validation in the original configurators. Completed triage resume skips
seed publication, preserves accepted references and does not restore revoked
connections or refresh expired credentials.

Before modifying the common owner, its configurator/launcher baseline passed
83 tests with one explicit skip. The focused recovery module then passed all
three cases. The final regression and validation results are recorded below.

```sh
PYTHONPATH=tools/tests:tools python3 -m unittest \
  test_factory_stand_recovery test_factory_stand_triage test_factory_local_stand \
  test_factory_local_runtime test_factory_source_installation \
  test_configure_local_stand_server test_configure_local_stand_agent \
  test_bootstrap_local_stand test_init_local_stand test_local_stand_entrypoint \
  test_local_stand_launcher test_factory_triage_pipeline
python3 tools/check_architecture.py
openspec validate add-factory-triage-pipeline --strict
git diff --check
```

Observed: 180 tests, ten explicit opt-in/platform skips, no failures;
architecture passed (64 packages, 263 edges), strict OpenSpec validation and
diff whitespace checks passed. The older CLI dispatch test was updated to supply
two independently selected repositories rather than assert an internal eager
default; its obsolete assertion failure is not additional production Red evidence.
No Rust or Console implementation changed in these corrections, so their
unchanged build/Clippy/UI gates above were not repeated. Documentation edits
were validated rather than assigned fabricated Red/Green evidence. Approximately
11–12 GiB remained free. No commits were created.
