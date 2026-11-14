# Tasks 6.9–6.10 — Selected Project preview retention

The follow-up review reproduced one remaining P2: preview opened from an Issue's
selected Project material kept showing text after its publication expired. The
picker and Issue-owned preview already tracked deadlines, but the selected shared
preview retained only text and a display name.

## Fix

The independently authorized exact Project body response now includes its stored
publication `expires_at`. The owner still checks access and retention before and
after reading bytes. This adds response metadata, without a new endpoint, catalog
lookup, retention policy or storage migration. The OpenAPI property is required;
the Console types were regenerated from the Rust exporter.

The existing exact-source helper preserves that response and rejects a missing or
non-integer deadline, in addition to checking owner, ID, digest and encoded size.
The selected Project preview reuses the existing deadline-aware Issue document
presentation. At the deadline, it hides text and shows the localized expired
state in both current and historical Issue revisions. It does not need current
catalog membership and does not select, publish or start anything. Other helper
callers explicitly consume the response's content field; their existing editor
and publication behavior remains unchanged.

## Behavioral Red/Green

All tests use established public application/REST, generated HTTP client and
application-router UI boundaries. Rust commands below used
`CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0` to avoid
growing debug/incremental output on a nearly full disk.

```sh
cargo test --locked -p octacity-server-application --lib project_goals_can_be_read_and_revised_without_changing_an_accepted_version
cargo test --locked -p octacity-server-api-rest --test v1_handlers project_document_content_is_explicitly_read_through_the_project_owner_without_caching
pnpm --dir ui exec vitest run src/features/issues/IssueDocuments.test.tsx -t 'at its owner deadline'
pnpm --dir ui exec vitest run src/api/projectDocuments.test.ts -t 'authoritative retention deadline'
```

- Owner Red: serialized exact read had `expires_at: null` instead of the deadline
  from its accepted publication. Green: the strengthened publication/read test passed.
- REST Red: `expires_at` was absent from the required OpenAPI body contract. Green:
  HTTP returns the expected deadline and validates against the generated contract;
  the existing `Cache-Control: no-store` and read-only effects remain qualified.
- UI Red: both current/history cases kept the private heading after the deadline.
  Green: both hide it and show expired state, with no Project catalog lookup or
  mutation. These are two new regression cases.
- Client Red: an exact response lacking a deadline resolved successfully. Green:
  it is rejected as an invalid response. This is one new regression case. The
  earlier identity/digest/size regression retains a valid deadline fixture.

## Regression and required checks

```sh
CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 CARGO_NET_OFFLINE=true pnpm --dir ui api:generate
cargo test --offline --locked -p octacity-server-application --lib project_text_intake_tests
cargo test --offline --locked -p octacity-server-api-rest --test v1_handlers --test v1_contract
cargo clippy --offline --locked -p octacity-server-application -p octacity-server-api-rest --lib -- -D warnings
cargo fmt --all -- --check
pnpm --dir ui exec vitest run src/features/issues/IssueDocuments.test.tsx src/features/projects/ProjectDocuments.test.tsx src/features/factory/FlowDocuments.test.tsx src/features/factory/FlowProcess.test.tsx src/api/projectDocuments.test.ts
pnpm --dir ui test:unit
pnpm --dir ui exec playwright test tests/browser/issue-documents.spec.ts tests/browser/project-documents.spec.ts tests/browser/factory-process.spec.ts tests/browser/issue-outcomes.spec.ts --workers=1
pnpm --dir ui build
pnpm --dir ui lint
pnpm --dir ui format
python3 tools/check_architecture.py
openspec validate add-issue-classification-snapshots --strict
git diff --check
```

Passed: **27 application cases**, **33 REST contract cases**, **288 REST handler
cases**, **73 focused UI cases**, **1,246 Vitest cases** across 140 files,
**46 script tests**, and **18 browser journeys**. The original exact-read client
regression was also rerun after updating its fixture to the required response
contract. Build/type checking, library Clippy, Rust formatting, ESLint, Prettier,
architecture, strict OpenSpec and whitespace checks passed.

Production output: **1,613,480 JavaScript bytes**, **88,030 CSS bytes**,
**1,702,097 total static bytes**, **58 files**. Existing bundle budgets pass
without another allowance increase. No dependencies were added.

Operator/reference documentation explains selected-preview expiry and the required
owner-provided response deadline. Deploy the updated API contract with the
Console. Native database/object-store, Agent/executor and provider effects were
not run or claimed. Documentation has no executable behavior requiring TDD.
Earlier uncommitted work remains present; no commit was requested. `github.token`
was not read or changed.
