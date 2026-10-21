# 6.12 — review attachment and retained REST observations

Date: 2026-10-21. Scope: task 6.12 only. Existing uncommitted 6.11 work is
preserved; Console review controls remain task 6.13 and typed check/artifact
projections remain task 6.14.

## Boundaries and Red → Green

Rust commands ran from the repository root through `/tmp/octacity-612-cargo`,
which selects one owned temporary Cargo target, two jobs, zero dev/test debug
information and disabled incremental compilation. Public HTTP tests compose the
real Issue/review application handlers and memory store. Runtime tests use a
bounded disposable PostgreSQL 18.6 instance and an explicitly installed observation
port. No live forge or credential is required.

| Command after the Cargo wrapper | Observed Red | Observed Green |
| --- | --- | --- |
| `test --locked -p octacity-server-api-rest --test v1_handlers review_http_attachment` | The required review HTTP composition API did not exist. | An explicit attachment calls the independent reader once, retains explicit provenance and leaves manual Issue revision/status unchanged. |
| `test --locked -p octacity-server-api-rest --test v1_handlers review_http_reads_retained` | The GET returned `405`. Registration also enforced the missing typed visibility declaration. | The shared authorized query returns retained metadata and separate current availability without a provider read, including blocked read authority. |
| `test --locked -p octacity-server-api-rest --test v1_handlers review_http_refresh` | Explicit refresh returned `404`. | Refresh/history preserve original heads, timestamps, association and replay; provider outage leaves retained observations intact. |
| `test --locked -p octacity-server-api-rest --test v1_handlers review_http_rejects_stale` | Stale Issue requirements conflicted only after spending one provider read. | The existing application owner rejects a stale new attachment before the read, after original replay lookup. The final atomic storage precondition remains in place for concurrent edits. |
| `test --locked -p octacity-server --lib runtime_installs_review_http -- --ignored` | The required independent runtime installation API did not exist. | Actual server HTTP composition supports empty/manual startup, explicit reader installation, provider outage, restart and revoked reads with retained visible metadata. |

For generated contracts, `node ui/node_modules/typescript/bin/tsc --noEmit --project
ui/tsconfig.json` first failed because the old generated schema had no review
components or paths. Exporting the actual new OpenAPI document and running the
existing `ui/scripts/generate-api.mjs` produced Green type checking and generated
client request tests. No handwritten client or Console control was introduced.

## Shared mechanisms and scope

- Four additive routes reuse `IssueCodeReviewHandlers`, existing authorized command
  and query handlers, current Issue/repository visibility, scoped receipts and the
  installed observation port. The new adapter performs only typed HTTP translation.
- The existing accepted security partition scopes replay keys. Reusing a key for a
  different Issue/selection conflicts before another provider read; separate
  attachments use distinct keys. This namespace was preserved, not redefined by
  the transport.
- Listing/history reuse the common opaque observation codec, Issue history page
  bounds and visibility before original membership/watermarks/paging. Continuation
  cannot cross Issues, associations, security partitions or changed repository
  metadata visibility. No new pagination or replay mechanism was added.
- Request schemas are closed. Clients cannot supply credentials, endpoints,
  provenance, actor, status/acceptance or execution setters. Optional lineage is
  verified through the existing execution/candidate reader. Public attachment
  cannot claim trusted Delivery provenance or forge write authority.
- GETs serve retained nullable facts with original observation time and
  `available`/`blocked` current read authority; availability makes no freshness or
  provider-connectivity claim. Refresh is explicit. Dependency errors are safe
  existing HTTP errors and do not clear history.
- `IssueRuntimeIntegrations` composes independent optional Source/review ports.
  Existing Source startup delegates to the same path, and default/manual startup
  needs neither a provider nor Factory. No token mechanism, SDK in the Issue
  domain, poller, SQL migration or dependency update was added.
- REST authorization inventory tests include all four operations and qualify
  allow/deny before application/protected effects. Actual response documents are
  checked against the published component schemas. Generated client tests verify
  paths, bodies, optimistic/idempotency headers and omitted browser credentials.

## Final verification

All selected checks passed:

- `cargo test --locked -p octacity-server-api-rest --lib --test v1_handlers`:
  12 library and 156 HTTP/contract tests passed.
- `cargo test --locked -p octacity-server-application --lib issue_tests`:
  126 Issue regressions passed; the final focused `--lib issue_code_review` run
  also passed all six selected review tests.
- `cargo test --locked -p octacity-server --lib`: 179 passed, 30 environment-gated
  tests ignored. The separate PostgreSQL run `--lib runtime_installs_review_http
  -- --ignored`, together with the existing manual and Source restart journeys,
  passed all three selected tests against the disposable database.
- `cargo clippy --locked -p octacity-server-application
  -p octacity-server-api-rest -p octacity-server --all-targets -- -D warnings`:
  passed.
- Actual OpenAPI export and the existing generated API script, full UI TypeScript
  checking and three selected generated client contract tests passed. ESLint and
  Prettier passed for the new client contract test.
- Architecture verification retained 63 workspace packages and 256 internal
  edges; all 32 architecture checker tests passed.
- `cargo fmt --all --check`, `git diff --check`, local documentation link checks
  and `openspec validate add-factory-work-card --strict` passed.

Documentation/verification edits describe these public contracts and require no
separate executable Red/Green cycle. A pre-existing macOS linker unwind-table
size warning was also present in the runtime baseline; all selected runtime tests
completed successfully.

## Resource use

One owned Cargo target and a disposable PostgreSQL container with a 512 MiB tmpfs
and memory limit were used. Free disk was checked repeatedly (about 8.6 GiB at
start and above 5 GiB during checks). The owned Cargo target and PostgreSQL
container were removed after validation, returning free space to about 8.5 GiB.
Existing checkouts/caches and 6.11 changes remain intact. No commit or GitHub
publication is part of this task.
