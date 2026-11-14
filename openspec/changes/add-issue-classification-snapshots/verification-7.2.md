# Task 7.2 — Manual and replacement-triage Issue qualification

The subsequent CLI framing and exact-source attribution regressions are recorded
in [the tasks 7.1–7.2 review follow-up](verification-7.1-7.2-review-fixes.md).

## Qualified behavior

The packaged `factory_pilot` journey publishes one delivery Configuration and
uses its exact ID/version for two independently prepared Issues:

1. A manual Issue is created through the ordinary API with priority `high`,
   component `api`, type `feature_request` and size `small`. Its execution history
   is empty before delivery, and every frozen field has manual attribution.
2. Another published Flow uses a deterministic policy and the existing local
   classification action, rather than the original reasoning Build. Its accepted
   records and Issue updates are produced by the real Factory owner; no producer
   record or field origin is seeded. The replacement Flow needs no ordinary
   Build, demonstrating a different triage implementation.

Both deliveries omit `predecessor_run` and caller scheduling classification. They
use the original delivery publication without republishing its graph, Project
definitions or Configuration. The fixture resolves the actual published input
mode and current Issue revision, and scopes intentional execution keys to the
Issue, Configuration and reviewed revision.

Both candidates execute four fresh ordinary Builds: protected-test author,
coder/capture, original private-test validation and isolated verification. The
second Run must retain the same mandatory completion outcomes as the first.
The real HTTPS/Git peer confirms two independent PR identities, with one accepted
POST per candidate. Public Artifact reads supply the second candidate facts;
its PR head and actual remote Git file are checked against those facts. The
existing first-candidate private-test, permissions, capture and publication
assertions continue to execute.

## Staleness and restart

The replacement triage first applies its assessment to Issue revision 2.
Changing acceptance produces revision 3; restoring the original text produces
revision 4 without renewing classification. Review and execution start both
reject the stale fields with `issue_snapshot_inapplicable`. Each rejection must
leave the Issue execution history and Project Run identities unchanged.

The first delivery's accepted PR crosses the existing actual Server crash and
recovery. The replacement Issue's stale review/start are rejected again after
that restart. An explicit new execution of the replacement triage reassesses
revision 4 and produces revision 5, which can then use the same delivery target.

After its completed delivery, an ordinary manual size edit advances the second
Issue to revision 6 with `large`. A further Server/Agent restart retains the
original revision-5 snapshot, `small`, exact digest and all four pipeline field
origins. The first manual Run and its original records remain unchanged.

Temporal evidence expiry, denied private body access, unavailable bytes, exact
command/environment/criterion matching and historical semantic rebinding were
also qualified through the existing authorized application evidence tests. Those
tests use the existing in-memory ports. The packaged positive cases are feature
requests; native stale negatives check requirements applicability, rather than
claiming a new native verified-reproduction evidence path.

## Observed Red/Green

The native seam remains the existing packaged Issue/Factory HTTP journey, real
Jobs and disposable HTTPS/Git peer. The peer's own regression uses its public
HTTPS API and actual temporary Git refs.

- Multi-review peer: the second independent POST returned 409 instead of 201.
  `/tmp/octacity-72-forge-red.log` records Red. The implementation retains a
  bounded, atomically replaced review collection with stable identities;
  `/tmp/octacity-72-forge-green.log` records all three HTTP peer tests passing,
  including both earlier lost/held-acknowledgement cases and restart.
- Manual Issue: the original fixture created an unwanted triage execution.
  `/tmp/octacity-72-manual-red.log` records that behavioral failure after
  51.23 seconds. The manual-only slice then passed its actual PR/restart journey
  after 196.79 seconds in `/tmp/octacity-72-manual-green.log`.
- Reuse: the public Issue list contained only one Issue instead of the required
  manual and replacement-triage pair. `/tmp/octacity-72-reuse-red.log` records
  Red after 21.27 seconds. The complete two-Issue journey passed after
  342.22 seconds in `/tmp/octacity-72-reuse-green.log`, including all six stale
  review/start rejections, reassessment, two confirmed PRs and frozen inputs
  after restart.

Canonical command for the final native case:

```sh
CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 \
  cargo test --locked -p octacity-release-harness --test factory_pilot \
  packaged_delivery_reuses_one_configuration_for_manual_and_replacement_triage \
  -- --ignored --exact --nocapture
```

Actual local invocations first built the test target with `--no-run`, then used
the ignored `target/qualification/issue-snapshot-72/run.py` environment wrapper
to invoke the resulting `factory_pilot-*` binary with the exact case name and
`--ignored --exact --nocapture`. The wrapper supplied the previously qualified
packaged binaries/image/CLI identities and new disposable PostgreSQL/MinIO
services. No production repository, provider credential or stand data was used.

## Retained proof and regression checks

The full successful case exported a mode-0600 proof under the explicitly supplied
`OCTACITY_RELEASE_EVIDENCE_DIR`:

`target/qualification/issue-snapshot-72/evidence/issue-snapshot-reuse-6062cca2-e315-4ce1-884a-4aa74d263025.json`

It retains the original Configuration reference/version 1, execution and frozen
snapshot version 5, pipeline attribution, four ordinary Build references,
mandatory completions, captured candidate facts, two confirmed reviews and stale
diagnostics for `changed`/revision 3, `restored`/revision 4 and
`after-restart`/revision 4. Every blocked phase retains the original single triage
execution. The proof is independent of the disposable database cleanup.

```sh
CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 \
  cargo test --locked --offline -p octacity-release-harness \
  --test factory_pilot --test factory_delivery_target
# 7 harness and 3 HTTPS peer tests passed; native-dependent cases remain ignored.
# /tmp/octacity-72-harness-regressions.log

CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 \
  cargo test --locked --offline -p octacity-server-application --lib issue_classification
# 7 passed; /tmp/octacity-72-evidence-regressions.log

CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 \
  cargo clippy --locked --offline -p octacity-release-harness \
  --test factory_pilot --test factory_delivery_target -- -D warnings
# Passed; /tmp/octacity-72-clippy.log

CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 \
  cargo test --locked -p octacity-release-harness --test factory_pilot \
  packaged_issue_classification_delivers_from_its_snapshot_without_a_predecessor \
  -- --ignored --exact --nocapture
# Native 7.1 regression passed after 229.33 seconds.
# /tmp/octacity-72-native-71-regression.log

python3 tools/check_architecture.py
```

The initial filter using the evidence module's filename selected zero tests and
is not counted as verification. The corrected `issue_classification` filter
executed the seven tests above, including all four evidence-reader tests. That
application test build emits existing unused readiness-fixture warnings; this
task changes no application source or warning policy.

Targeted Rust formatting and `git diff --check` passed. Documentation changes
have no executable behavior and are checked separately from these behavioral
cycles. Broader Rust/Console regressions and rollout documentation remain tasks
7.3–7.5.
