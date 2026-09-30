## Why

The trusted-network management plane currently has no transport-independent actor or authorization context, and several authoritative mutations construct the anonymous audit actor inside PostgreSQL adapters. Building the Web control plane on that shape would make later RBAC or ABAC adoption a cross-cutting rewrite of REST handlers, application handlers, query pagination, and audit persistence.

## What Changes

- Introduce a bounded management request context carrying the verified actor, request identity, and safe request attributes independently of HTTP.
- Define a typed management action and resource vocabulary plus one application-layer authorization interface used by every management command and query.
- Preserve current behavior through an explicit trusted-network policy that authorizes the anonymous management actor for all existing management actions; this change does not add accounts, login, sessions, roles, policies, RBAC, ABAC, organizations, or multi-tenancy.
- Propagate the accepted actor into authoritative mutation audit facts instead of constructing `unauthenticated_management` inside store adapters.
- Make collection, search, and event queries accept an authorization-derived visibility scope before pagination so later policies cannot leak resource existence through pages, cursors, counts, freshness, or timing-dependent post-filtering.
- Add a stable forbidden outcome and OpenAPI response contract while preserving the existing unauthenticated trusted-network deployment mode.
- Add architecture and contract checks that every registered management operation declares an action/resource mapping and crosses authorization before its application handler.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `server/rest-control-plane`: Require normalized management actor context, application-layer authorization of every operation, pre-pagination visibility scoping, and a stable forbidden response without enabling operator authentication in the trusted-network release.
- `server/operations`: Require authoritative mutations and audit facts to use the accepted management actor supplied by the application context rather than an infrastructure-selected identity.

## Impact

- Changes the internal command and query dispatch interface in `server/application` and the management composition assembled by `server/app`.
- Extends the REST adapter and OpenAPI contract with authorization mapping and stable denial behavior, while successful v1 request and response bodies remain compatible.
- Extends authoritative store operation inputs and PostgreSQL audit writes with validated actor context; no authentication tables, role bindings, policy language, or external identity dependencies are introduced.
- Agent, webhook, cache data-plane, execution backends, scheduling semantics, JobSpec, and public Agent protocols are unchanged.
