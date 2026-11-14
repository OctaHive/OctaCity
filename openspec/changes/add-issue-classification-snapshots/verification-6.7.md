# Task 6.7 verification

## Scope and public boundary

Issue documents now use three dedicated REST operations beneath
`/api/v1/projects/{project_id}/issues/{issue_id}/input-documents`: immutable text
publication, bounded content-free publication history and exact Artifact body
reads. The transport delegates to the existing `ProjectTextIntake` owner and
its Issue document application contracts. Runtime composition shares the
existing store, immutable byte publisher/reader, authorization policy and clock;
there is no additional journal, version model, database migration or execution
path. Console document management remains task 6.9; material-selection transport
remains task 6.8.

The behavioral seam is the HTTP router with real authorized Issue and document
handlers over the existing public in-memory store and byte ports. Document
commands/queries are not stubbed. The existing mutation-evidence probe verifies
that rejected inputs allocate no publication and that accepted document
publication does not edit or execute the parent Issue.

## Red → Green

From the repository root, all Rust commands used `CARGO_INCREMENTAL=0` and
`--offline --locked` to reuse cached dependencies without incremental build
artifacts.

1. `cargo test -p octacity-server-api-rest --test v1_handlers issue_documents_http_publication -- --nocapture`
   failed with HTTP **404 instead of 201** before the Issue publication route
   existed. Adding the thin adapter, typed route and closed request schema made
   it pass. The test checks exact owner, body-free metadata, immutable Artifact
   size, no-store and unchanged parent mutation evidence.
2. `cargo test -p octacity-server-api-rest --test v1_handlers issue_documents_http_history -- --nocapture`
   failed with HTTP **405 instead of 200** before history/body routes existed.
   Wiring the existing bounded list and exact read queries made it pass. Both
   original and newer text remain separately readable, metadata pagination
   retains both publications, and invalid page bounds/cursors/unknown fields
   are rejected.
3. `cargo test -p octacity-server-api-rest --test v1_handlers issue_documents_http_ -- --nocapture`
   exposed a missing current Issue visibility check: publication succeeded
   (**201**) with Issue View denied while Issue Update was allowed. The shared
   owner now checks both before new publication or replay. Green preserves the
   existing generic **400** for a hidden parent reference and **403** for direct
   Update/Download denial. The complete focused suite passed **7 tests**.
4. `cargo test -p octacity-server-api-rest --test v1_handlers every_management_operation -- --nocapture`
   detected the three missing entries in the exhaustive authorization test
   inventory (**232 requests vs 235 operations**). Adding the typed recording
   handlers and requests restored both allow/deny checks in the full suite.

Additional behavioral coverage exercises the existing owner through the new
transport: exact replay after a newer version preserves the original actor,
Artifact and deadlines even when the caller changes; changed intent conflicts
without effects; another Issue/security scope receives a separate publication
rather than another scope's receipt. Separate Update, parent View and Artifact
Download rights, exact expiry, forged owner/actor/publication fields, missing
idempotency keys, nonblank UTF-8 byte bounds and foreign parent routes are
covered. Issue documents stay out of the Project catalog used by settings and
selectors, and cannot be read as Project inputs. Shared Project publication,
metadata and exact content still work through the unchanged contracts.

## Regression and contract checks

```sh
CARGO_INCREMENTAL=0 cargo test --offline --locked -p octacity-server-api-rest --lib --test v1_handlers --test v1_contract
CARGO_INCREMENTAL=0 cargo test --offline --locked -p octacity-server-application --lib issue_document
CARGO_INCREMENTAL=0 CARGO_NET_OFFLINE=true pnpm --dir ui api:generate
pnpm --dir ui typecheck
CARGO_INCREMENTAL=0 cargo check --offline --locked -p octacity-server
cargo fmt --all --check
git diff --check
python3 tools/check_architecture.py
openspec validate add-issue-classification-snapshots --strict
```

The final REST suites passed **13 library + 281 handler + 33 contract = 327 tests**
after the schema move, and the existing
Issue document application suite passed **8 tests**. OpenAPI generation and
client type checking passed. Contracts are closed, expose the exact Issue owner,
share existing byte/page bounds and typed authorization/visibility mappings,
and document no-store body/history/receipt responses. Generated client files
remain reproducible ignored output, as in the existing API workflow. The
existing `IssueDocumentResource` schema moved to the document schema owner
without changing its wire contract. Architecture validation passed for
64 workspace packages and 263 dependency edges; strict OpenSpec validation,
server runtime compilation, Rust formatting and whitespace checks passed.

Documentation has no executable behavior requiring Red/Green. The new
verification document passes Prettier; the new reference section was formatted
separately. Full `docs/reference/issues.md` Prettier checking still reports
pre-existing formatting, reproduced against its unchanged HEAD contents; this
task does not reformat the rest of the reference. Native PostgreSQL,
object storage, Agent/executor, provider effects and packaged release execution
were not run or claimed by these HTTP/application tests. No commit was requested;
`github.token` remains untouched and untracked.

## Review correction: exact-body error cache policy

A new HTTP regression test first failed because a denied Artifact download
returned `403` with no `Cache-Control` header. The exact-body route now applies
`no-store` around the handler, covering application errors and URL-extractor
rejections as well as success. The handler no longer duplicates the header.
Other route/error policies are unchanged.

```sh
CARGO_INCREMENTAL=0 cargo test --offline --locked -p octacity-server-api-rest --test v1_handlers issue_document_body_http_is_no_store -- --nocapture
```

Red: `download denied` returned no cache policy instead of `no-store`. The
regression checks successful exact content plus forbidden download, expired
publication, malformed identity, invalid UTF-8 path and unavailable service;
errors must retain their original statuses, use no-store and contain no private
source body.

Green: the focused regression passed. The full REST command above (library,
`v1_handlers`, `v1_contract`) then passed **13 + 288 + 33 = 334 tests**. Rust
formatting, architecture, strict OpenSpec validation and whitespace checks also
passed. No wire schema or client-generation change was needed for this header
correction.
