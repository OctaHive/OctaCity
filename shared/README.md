# Shared contracts

`shared/` is a product boundary, not a general-purpose utilities directory. A
crate belongs here only when it defines a stable, dependency-light contract
owned by at least two of `agent`, `server`, and `cli`.

Every active shared Cargo package declares its contract and product consumers
in `package.metadata.octacity.shared`. The architecture check derives actual
consumers from Cargo dependency edges and rejects missing, stale, or
single-product ownership as well as unapproved implementation dependencies.

## Inventory

| Path | Contract | Product consumers | Status |
| --- | --- | --- | --- |
| `octacity-protocol` | Versioned agent/server wire messages and signed execution intent | Agent, server | Active |
| `octacity-observability` | Stable metric, trace, redaction, and cardinality vocabulary | Agent, server | Scaffolded; implemented by task 8.2 |
| `protocol-fixtures` | Language-neutral golden documents for `octacity-protocol` | Agent, server | Active test data |

The observability crate is explicitly marked `scaffold = true` and therefore
must remain dependency-free and unused. Its declared ownership reserves the
planned shared contract; task 8.2 must remove the marker and add the vocabulary
and both product consumers together. Server-only HTTP telemetry stays in the
REST adapter until that contract exists.

## What stays outside

- Management REST and future GraphQL DTOs belong to their API adapters.
- Application commands and query projections belong to the application layer.
- Projects, builds, attempts, pipelines, jobs, and other server entities belong
  to server core crates.
- SQL rows and object-store records belong to infrastructure adapters.
- GitHub, Gerrit, and other provider payloads belong to provider adapters and
  are translated into backend-neutral protocol types at their boundary.

If code has only one product consumer, keep it with that product until a real
cross-product contract exists.
