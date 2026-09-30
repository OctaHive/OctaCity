## 1. Management Security Vocabulary

- [ ] 1.1 Add bounded `ManagementActor`, `ManagementSecurityScope`, safe request attributes, request context, typed action/resource, visibility, grant, and denial types to `octacity-server-application`; verify constructor boundary, redacted-debug, oversize, and invalid-combination unit tests pass.
- [ ] 1.2 Add the `ManagementAuthorizationPolicy` port and explicit trusted-network policy that authorizes only the canonical anonymous management actor; verify allow-all coverage for every declared management action and fail-closed behavior for all other actor shapes.

## 2. Mandatory Application Authorization

- [ ] 2.1 Add authorized command and query decorators that decide before inner-handler dispatch and preserve safe request correlation on denial; verify spy-handler tests prove denied requests perform zero inner calls and allowed requests receive the exact grant and context.
- [ ] 2.2 Migrate management command handlers and their callers to accept the separate management request context in coherent feature families; after each family, verify its application tests compile and pass without adding actor fields to business commands.
- [ ] 2.3 Migrate management query handlers and their callers to the context-aware interface and expose only authorized management bundles from the composition root; verify no management handler can be obtained from server assembly without the policy decorator.

## 3. Complete Action and Resource Mapping

- [ ] 3.1 Declare one typed action/resource mapping beside every registered management command and query, including collection resources whose concrete entity does not yet exist; verify mapping unit tests reject missing identities, unsupported resource shapes, and stringly typed fallbacks.
- [ ] 3.2 Add a contract check comparing registered management routes, OpenAPI operation inventory, and typed action/resource mappings; verify the check fails on a fixture with a missing or duplicate mapping and passes for the production registry.

## 4. Actor-faithful Mutation Evidence

- [ ] 4.1 Introduce validated `MutationAuditContext` on authoritative management mutation inputs and translate it from the accepted application context; verify store contract fakes reject absent or invalid actor evidence before mutating state.
- [ ] 4.2 Propagate audit context through every management mutation handler and in-memory store operation; verify application and authoritative-store contracts record the supplied actor kind, optional identity, operation, target, and request identity exactly once.
- [ ] 4.3 Update PostgreSQL mutation operations to persist supplied audit context and remove infrastructure-selected management actors; verify source checks find no hard-coded management actor in mutation adapters and PostgreSQL contracts preserve equal mutation, idempotency, audit, and outbox evidence counts.

## 5. Security-scoped Idempotency

- [ ] 5.1 Extend the store idempotency key and APIs with `ManagementSecurityScope`; verify exact replay succeeds within one scope while the same caller key in a different scope neither collides with nor reveals the first outcome.
- [ ] 5.2 Add the PostgreSQL migration, bounded column, uniqueness/index changes, and stable legacy backfill; verify fresh migration, previous-schema upgrade, reentrancy, old-writer default behavior, and rollback-safety checks against PostgreSQL.
- [ ] 5.3 Update every management mutation to reserve and resolve idempotency in the accepted security scope; verify concurrent exact replay still commits one mutation/evidence set and cross-scope requests produce independent outcomes.

## 6. Authorization-safe Read Visibility

- [ ] 6.1 Inventory every management collection, search, audit, event, artifact, Agent, and other paged query and assign a query-specific typed visibility input; verify a contract test fails when any paged management query lacks an explicit scope.
- [ ] 6.2 Apply visibility in in-memory read adapters before ordering, cursor construction, pagination, counts, snippets, and freshness; verify `All`, bounded-subset, and `None` cases return pages and metadata derived only from visible resources.
- [ ] 6.3 Apply equivalent predicates in PostgreSQL read adapters before `ORDER BY`, cursor comparison, aggregation, and `LIMIT`; verify adapter contracts cover hidden rows before, within, and after page boundaries without short-page or existence leaks.
- [ ] 6.4 Pass authorization grants from query decorators through application handlers into each scoped read port and reject unsupported scope/query combinations; verify no application or REST response performs post-fetch authorization filtering.

## 7. REST and OpenAPI Integration

- [ ] 7.1 Normalize the canonical trusted-network context at REST ingress and pass it to every management operation without adding credentials or actor fields to DTOs; verify all documented successful section-four requests retain their existing status codes and bodies.
- [ ] 7.2 Map denial to the stable `403 forbidden` envelope with safe request correlation and no policy or resource details; verify end-to-end denial tests produce no handler, transaction, audit, outbox, idempotency, or transfer-capability side effects.
- [ ] 7.3 Document the forbidden response for every management OpenAPI operation and extend route/DTO drift checks; verify the generated document and registered routes remain identical on Linux, macOS, and Windows line endings.

## 8. Architecture, Documentation, and Regression Verification

- [ ] 8.1 Extend architecture checks to reject authorization in REST or infrastructure, direct undecorated management-handler assembly, raw credential fields in management context, and store-selected management actors; verify positive production and negative fixture cases pass.
- [ ] 8.2 Document the trusted-network policy, authorization boundary, visibility-before-pagination rule, audit/idempotency scoping, and explicit absence of login/RBAC/ABAC; verify public Rust items and operator-facing security assumptions are covered without exposing internal package inventories.
- [ ] 8.3 Run formatting, workspace Clippy with warnings denied, portable tests, architecture checks, OpenAPI contracts, migration and PostgreSQL store contracts, coverage gates, and released-server contracts; verify the trusted-network API remains compatible and every required suite is green.
