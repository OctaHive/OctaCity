# Static Agent vertical-slice transcript

This transcript records the server-observable sequence for the first
PostgreSQL-backed static-Agent slice. Credentials and fencing values are
redacted; identifiers are symbolic because exact values are generated for each
run. The executable integration test is
`server/app/tests/postgres_runtime.rs`, while every wire
document below has a strict golden fixture in
`shared/protocol-fixtures/coordinator/`.

| Step | Listener and operation | Durable result |
| --- | --- | --- |
| 1 | management `POST /api/v1/agent-enrollments` | one expiring, single-use credential digest bound to Pool version 1 |
| 2 | Agent `POST /api/v1/agents/register` | enrollment consumed; Agent and registration epoch 1 committed |
| 3 | management `POST /api/v1/triggers/manual` | Build, Attempt, two Jobs and one root ready-queue entry committed |
| 4 | Agent `POST /api/v1/agents/{agent_id}/leases:acquire` | root Job leased with Pool, epoch, expiry, fence verifier and signed JobSpec |
| 5 | Agent `POST /api/v1/leases/{lease_id}/events:append` | root event sequence 1 committed and acknowledged |
| 6 | Agent `POST /api/v1/leases/{lease_id}/complete` | root succeeded; dependent Job signed and moved from blocked to ready atomically |
| 7 | Agent repeats acquire, append and complete | dependent Job and aggregate Attempt/Build succeeded |
| 8 | management reads Build, Attempt, each Job and events | terminal assignment, outcome and contiguous cursor remain queryable |

The retained request/response shapes are:

- `register-request-v1.json` and `register-response-v1.json`;
- `lease-response-v1.json` plus the typed `AcquireLeaseRequest` contract;
- `events-append-request-v1.json` and `events-append-response-v1.json`;
- `complete-request-v1.json` and `complete-response-v1.json`.

Every Agent operation carries `protocol_version`, a request ID equal to the
HTTP `Idempotency-Key`, the current registration identity, and either the
enrollment/registration bearer or complete Lease fence. The server stores only
credential and fence verifiers. A lost response is replayed from durable
mutation evidence; it does not create another Agent, Lease, event, or terminal
transition.

The integration test deliberately uses the public Agent HTTP protocol as its
Agent boundary. The released Linux Native machine gate additionally requires a
verified Agent archive, a verified Octa archive, bubblewrap, a delegated
cgroup-v2 subtree, and a quota-backed work filesystem. macOS and Windows do
not substitute a weaker host-native path for that Linux isolation contract.
