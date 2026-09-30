## Context

The management plane currently accepts requests only on a trusted network and intentionally has no operator authentication. REST adapters map directly to typed application handlers, while PostgreSQL mutation adapters currently select the `unauthenticated_management` audit actor themselves. Command and query handlers receive only their payload, and collection reads have no authorization-derived visibility scope.

That shape is adequate for the current deployment, but it places future authentication and authorization concerns at the wrong boundaries. Adding RBAC or ABAC after the Web UI is built would otherwise require changing transports, every application handler, idempotency semantics, audit persistence, and pagination at once. This change introduces those seams while preserving the current trusted-network behavior and successful API representations.

The affected path crosses `server/api`, `server/application`, core store contracts, PostgreSQL, and the `server/app` composition root. The Agent, webhook, cache, and execution data planes are outside this change.

## Goals / Non-Goals

**Goals:**

- Give every management command and query one transport-independent, bounded actor and request context.
- Authorize management operations once in the application layer using typed actions and resources.
- Keep the current trusted-network deployment working through an explicit allow-all policy for its anonymous management actor.
- Carry the accepted actor into authoritative audit facts instead of choosing it in infrastructure.
- Scope collection, search, and event reads before ordering, pagination, counts, freshness, or cursor creation.
- Scope idempotency outcomes by the caller's management security scope.
- Make authorization coverage, action/resource mappings, denial behavior, and OpenAPI responses mechanically verifiable.
- Leave a deep seam that can accept later local, LDAP, OIDC, RBAC, or ABAC adapters without changing management use cases again.

**Non-Goals:**

- User accounts, login, sessions, cookies, password storage, TOTP, LDAP, OIDC, or external identity-provider integration.
- Roles, permissions administration, policy persistence, a policy language, organizations, tenants, or multi-tenant isolation.
- Changing successful v1 REST request or response bodies.
- Authorizing Agent, webhook, cache, worker, or adapter traffic through the management policy.
- Creating an `octacity-server-auth` crate before it owns a real authentication or policy implementation.
- Retrofitting browser-specific CORS or CSRF behavior before the Web UI deployment model is selected.

## Decisions

### 1. Authorization stays in the application package

`octacity-server-application` will own a small management-security module containing the request context, typed authorization vocabulary, policy port, decision types, and handler decorators. This module is transport independent and depends only on server value objects.

No standalone auth crate is introduced in this change. With only one trusted-network policy, such a crate would be a shallow forwarding package. A future authentication or policy adapter may justify a separate package, but it will implement the application-owned port rather than redefine the seam.

REST is responsible only for normalizing already trusted ingress into a management request context. PostgreSQL and other infrastructure adapters never authenticate or authorize callers.

Alternative considered: authorize in Axum middleware. That would protect only REST, permit future transports or internal callers to bypass policy, and make resource-aware decisions depend on HTTP route details.

### 2. The management context contains verified facts, not credentials

Every management handler receives a `ManagementRequestContext` alongside its command or query. The context contains:

- a `ManagementActor` with a bounded kind and optional verified subject identity;
- an opaque, bounded `ManagementSecurityScope` used to isolate replayable results;
- the existing safe request-correlation identity; and
- a bounded set of explicitly admitted request attributes when a future policy needs them.

The current REST adapter creates one canonical trusted-network anonymous actor and one stable trusted-network security scope. Raw authorization headers, cookies, bearer tokens, certificates, passwords, provider claims, and arbitrary HTTP headers never enter this context. Debug output is redacted and bounded types reject oversized identities or attributes before dispatch.

The context is passed separately from command and query payloads. Business intent therefore remains stable when the authentication mechanism changes, and serialization of application requests cannot accidentally persist credentials.

Alternative considered: add actor fields to every command and query. This duplicates validation, lets callers disagree about actor representation, and mixes security context with business intent and idempotency fingerprints.

### 3. Every operation has one typed action and resource mapping

Management operations map to closed `ManagementAction` and `ManagementResource` values. Actions describe the application capability, not the HTTP verb or route. Resources identify the target kind plus the minimum bounded identity already known before the operation runs; collection resources identify their owning scope without fabricating an entity.

The mapping is declared next to the typed command or query and consumed by common authorized-handler decorators. A registry contract test compares registered REST operations with their command/query mappings and rejects missing or duplicate entries. Stringly typed action names in route handlers are not permitted.

This vocabulary is intentionally not a role model. A future RBAC adapter may map roles to actions, while an ABAC adapter may evaluate actor, resource, and safe attributes, without changing application handlers.

### 4. Common decorators enforce authorization before dispatch

The typed CQRS handler interface accepts `&ManagementRequestContext`. Management composition wraps command and query handlers in `AuthorizedCommandHandler` and `AuthorizedQueryHandler` decorators. Each decorator:

1. derives the typed action and resource from the request;
2. asks the `ManagementAuthorizationPolicy` for a decision;
3. returns a stable denial without calling the inner handler; or
4. passes the resulting grant and context to the inner handler.

Only wrapped management handlers are exposed by the composition root. The initial `TrustedNetworkManagementPolicy` authorizes every registered action only for the canonical trusted-network anonymous actor. Supplying any other actor to that policy is denied, which prevents an accidental claim that arbitrary identities are trusted.

Authorization happens before resource lookup where the request already carries an opaque target identity. Denial therefore does not reveal whether that target exists and cannot open a mutation transaction, produce an audit fact, reserve an idempotency key, or obtain a transfer capability.

Alternative considered: call the policy manually inside each handler. Repetition would make omissions likely and architecture checks unable to prove universal coverage.

### 5. Authorization grants carry typed read visibility

An allowed query receives a `ManagementGrant` containing a typed visibility scope appropriate to that query. Initial trusted-network grants use `All`; tests and future policies can produce `None` or a bounded project/resource scope.

Collection, search, audit, event, artifact, Agent, and other paged read ports accept this scope as part of their query input. In-memory and PostgreSQL adapters apply it in the same predicate that selects the authoritative result set, before ordering, `LIMIT`, cursor comparison, counts, snippets, or freshness computation. An empty scope returns a valid empty page. Application code must not fetch an unrestricted page and filter it afterward.

Visibility types remain query specific where the ownership dimension differs. A generic expression tree or policy language is not introduced. Unsupported scope/query combinations are rejected rather than widened to `All`.

Alternative considered: post-filter REST responses. Besides leaking counts and cursor boundaries, that approach can return short or empty pages while authorized rows exist later and makes timing dependent on hidden data.

### 6. Accepted actors become explicit mutation evidence

Application handlers translate the accepted management context into a validated `MutationAuditContext` carried by every authoritative management mutation. The audit context contains only actor kind, optional verified identity, security scope or request identity where required by the audit contract, and no credentials.

Store adapters persist this evidence atomically with the mutation, idempotency outcome, and outbox entry. They reject management mutation inputs without an audit context. Existing hard-coded `unauthenticated_management` values are removed from PostgreSQL mutation modules.

Agent, trigger, orchestrator, adapter, and worker mutations continue to obtain their actors from their own trusted application contexts. They do not pass through the management authorizer merely to share audit vocabulary.

### 7. Idempotency is isolated by management security scope

The authoritative idempotency identity becomes `(operation scope, management security scope, idempotency key)`. Fingerprints continue to represent business intent and do not include credentials or mutable authentication claims. An exact replay within one security scope returns the original logical result; another scope cannot observe or collide with it.

PostgreSQL adds a non-null bounded security-scope column and updates the uniqueness constraint and lookup indexes. Existing rows are backfilled to stable legacy scopes selected by their operation class. The trusted-network management scope remains constant, so behavior for the only currently supported management actor is unchanged.

The migration is reentrant and contract-tested against both a new database and the previous schema. During rollout, the column default permits the previous binary to write only the existing trusted scope; introduction of multiple actor scopes is prohibited until all writers use the new key. Rollback restores the old uniqueness shape only after verifying that no two rows differ solely by security scope.

Alternative considered: prefix the caller's idempotency key in REST. That leaks policy concerns into a transport and cannot protect non-REST callers or direct application tests.

### 8. REST exposes a stable forbidden outcome without adding authentication

The REST adapter maps authorization denial to HTTP `403` with the stable error code `forbidden`, the existing request ID, and a generic message. It does not distinguish missing resources from denied resources or expose policy reasons. OpenAPI documents the response for every management operation, and route/DTO drift tests cover it.

The trusted-network listener remains explicitly unauthenticated and still requires the existing external-reachability acknowledgement. This change does not advertise a login flow or imply that a browser can safely expose the listener to the public Internet.

## Risks / Trade-offs

- **Broad CQRS signature migration**: many handlers and tests call the current one-argument interface. Migrate coherent handler families in small compiling slices and use constructors that expose only authorized management bundles.
- **An accidentally permissive scope**: a new query could ignore its grant. Require every paged/search/event query port to accept its typed scope and add restricted-scope contract cases for both in-memory and PostgreSQL adapters.
- **Action-map drift**: new routes could bypass the decorator. Compare the registered route inventory, OpenAPI operations, and typed action/resource registry in one architecture test.
- **Audit or idempotency cardinality changes**: adding context could disturb the one-mutation/one-evidence invariant. Extend the authoritative store contract to assert actor fidelity and equal evidence counts under replay and denial.
- **Schema compatibility**: changing an idempotency primary key can make rollback unsafe after multiple scopes exist. Keep this release on one trusted scope, use a staged migration, and make rollback checks fail closed.
- **Premature abstraction**: an overly generic policy language would add complexity without a consumer. Keep actions, resources, and visibility as closed Rust types and defer role/policy persistence to the change that introduces it.

## Migration Plan

1. Add the bounded management context, action/resource vocabulary, policy port, trusted-network policy, and decorator tests without changing externally visible behavior.
2. Convert management command and query handler families to the context-aware interface and expose only decorated bundles from the composition root.
3. Propagate audit context through authoritative management mutations and remove infrastructure-selected management actors.
4. Migrate idempotency persistence to the management security scope and run fresh-schema, upgrade, replay, rollback-safety, and concurrency contracts.
5. Add typed visibility to every collection, search, and event query and prove filtering occurs before pagination in memory and PostgreSQL.
6. Add REST denial mapping, OpenAPI responses, route/action coverage checks, and end-to-end allow/deny contracts.
7. Run the complete workspace, architecture, migration, PostgreSQL, OpenAPI, coverage, and released-server suites before enabling the seam for Web UI work.

No feature flag or data-plane protocol transition is required. Successful trusted-network requests retain their current v1 representations throughout the migration.
