# Management REST v1: section-4 workflow

This workflow exercises the complete static-Agent slice: Project, Pipeline,
Repository, Build Configuration and Pool publication; enrollment; manual
Trigger acceptance; Agent execution; and Build diagnostics.

The management API contract is unauthenticated in this release and must only
be exposed on a trusted network. The production `octacity-server` composes
these routes with PostgreSQL-backed application handlers. Its separately bound
Agent listener requires enrollment or current-registration credentials. The
examples use `octacity.example.test:8080` for management and
`octacity.example.test:8081` for Agent traffic.

## Prerequisites

The current boundary assumes these server-owned resources already exist:

- VCS integration `55555555-5555-4555-8555-555555555555`;
- enabled manual Trigger `77777777-7777-4777-8777-777777777777`, version 1.

The Pool is created through the endpoint documented below. VCS-integration and
Trigger-definition management arrive with later tasks, so operators currently
provision those two definitions through controlled bootstrap data. In the
requests below, replace each illustrative resource identity with the
`resource.id` returned by the preceding create response:

| Resource | Illustrative identity |
| --- | --- |
| Project | `11111111-1111-4111-8111-111111111111` |
| Pipeline | `22222222-2222-4222-8222-222222222222` |
| Repository | `33333333-3333-4333-8333-333333333333` |
| Build Configuration | `44444444-4444-4444-8444-444444444444` |

Every mutation has a stable `Idempotency-Key`. Repeating an identical request
with the same key returns its original logical result; never reuse a key for
different content.

## Static Agent Pool management

Task 5.1 adds the following OpenAPI-verified operations:

| Method and path | Purpose |
| --- | --- |
| `POST /api/v1/agent-pools` | Create a stable Pool identity and version 1 |
| `GET /api/v1/agent-pools` | List current Pool versions with cursor pagination |
| `POST /api/v1/agent-pools/{pool_id}/versions` | Publish the next version using `If-Match` |
| `GET /api/v1/agent-pools/{pool_id}/versions/{version}` | Read an exact immutable version |
| `DELETE /api/v1/agent-pools/{pool_id}` | Delete only when no protected references remain |

The versioned definition contains `enabled`, `drain_state`, admission policy,
Pool-wide `concurrency_limit`, placement `fairness_policy`, and
`static_capacity_limit`. An allowlist policy
uses exact operating-system and architecture pairs. Pool deletion returns a
conflict while any Build Configuration, Agent, Lease, or Ready Job references
the Pool.

```json
{
  "name": "linux-native",
  "definition": {
    "enabled": true,
    "drain_state": "accepting",
    "admission_policy": {
      "mode": "allowlist",
      "platforms": [
        {"operating_system": "linux", "architecture": "amd64"}
      ]
    },
    "concurrency_limit": 4,
    "fairness_policy": "priority_fifo",
    "static_capacity_limit": 8
  }
}
```

## 1. Create the Project

```http
POST /api/v1/projects HTTP/1.1
Host: octacity.example.test:8080
Content-Type: application/json
Idempotency-Key: example-create-project

{
  "parent_id": null,
  "name": "Example"
}
```

The response is `201 Created`. Save `resource.id` as the Project identity.

## 2. Publish Pipeline version 1

```http
POST /api/v1/pipelines HTTP/1.1
Host: octacity.example.test:8080
Content-Type: application/json
Idempotency-Key: example-create-pipeline

{
  "project_id": "11111111-1111-4111-8111-111111111111",
  "name": "Release",
  "dag": {
    "nodes": [
      {
        "id": "build",
        "name": "Build",
        "dependency_policy": "all_succeeded",
        "required_capabilities": ["native"],
        "execution": {
          "commands": ["build"],
          "parallel": false,
          "failfast": true
        }
      }
    ],
    "edges": []
  }
}
```

The response is `201 Created`. Save `resource.id`; the initial immutable
Pipeline version is `1`.

## 3. Publish Repository version 1

```http
POST /api/v1/repositories HTTP/1.1
Host: octacity.example.test:8080
Content-Type: application/json
Idempotency-Key: example-create-repository

{
  "project_id": "11111111-1111-4111-8111-111111111111",
  "name": "Source",
  "definition": {
    "vcs_integration_id": "55555555-5555-4555-8555-555555555555",
    "repository_locator": "https://git.example.test/acme/service.git",
    "selection": {
      "allowed_references": ["refs/heads/main"],
      "default_reference": "refs/heads/main",
      "allow_exact_revision": true
    }
  }
}
```

The response is `201 Created`. Save `resource.id`; the initial immutable
Repository version is `1`.

## 4. Publish Build Configuration version 1

```http
POST /api/v1/build-configurations HTTP/1.1
Host: octacity.example.test:8080
Content-Type: application/json
Idempotency-Key: example-create-configuration

{
  "project_id": "11111111-1111-4111-8111-111111111111",
  "name": "Release",
  "definition": {
    "enabled": true,
    "job_concurrency_limit": 2,
    "repository_id": "33333333-3333-4333-8333-333333333333",
    "repository_version": 1,
    "pipeline_id": "22222222-2222-4222-8222-222222222222",
    "pipeline_version": 1,
    "parameters": {
      "parameters": {
        "profile": {
          "value_type": "string",
          "required": true,
          "default": null
        },
        "publish": {
          "value_type": "boolean",
          "required": false,
          "default": false
        }
      },
      "deny_unknown": true
    },
    "triggers": ["manual"],
    "agent_requirements": {
      "capabilities": ["native"],
      "labels": {},
      "minimum_cpu_millis": 1000,
      "minimum_memory_bytes": 1073741824,
      "minimum_disk_bytes": 10737418240
    },
    "allowed_pools": ["66666666-6666-4666-8666-666666666666"],
    "runtime": {
      "class": "native",
      "operating_system": "linux",
      "architecture": "amd64",
      "immutable_image": null,
      "cpu_millis": 1000,
      "memory_bytes": 1073741824,
      "writable_disk_bytes": 10737418240,
      "timeout_seconds": 3600,
      "network": {"mode": "disabled"},
      "workload_identity_profile": null
    },
    "cache": {
      "namespace": null,
      "read": false,
      "write": false
    },
    "artifacts": {
      "artifact_count": 0,
      "artifact_bytes": 0,
      "report_count": 0,
      "report_bytes": 0,
      "single_output_bytes": 0
    },
    "retry": {
      "max_attempts": 1,
      "retry_on": []
    }
  }
}
```

The response is `201 Created`. Save `resource.id`; the initial immutable Build
Configuration version is `1`.

## 5. Publish the initial Project policy

Send `POST /api/v1/projects/{project_id}/policy-versions` with an
`Idempotency-Key`. The body contains the complete strict policy-directive
document under `policy`. Omit `If-Match` for the initial version; subsequent
publications supply the current policy version as a strong `If-Match` value.
The server assigns the next immutable version and records the mutation, audit
fact, and outbox entry atomically.

## 6. Create a manual Trigger definition

Send `POST /api/v1/trigger-definitions/manual` with an `Idempotency-Key` and a
body containing `configuration_id`, `configuration_version`, `enabled`, and a
bounded `definition` object. The response is `201 Created`; use `resource.id`
and `resource.version` in the occurrence request. Trigger identities are
server-owned, so automation does not need a database bootstrap or a
deterministic-ID convention.

## 7. Accept one manual Trigger occurrence

The HTTP idempotency key must equal `deduplication_identity` for a manual
Trigger request.

```http
POST /api/v1/triggers/manual HTTP/1.1
Host: octacity.example.test:8080
Content-Type: application/json
Idempotency-Key: example-release-main-1

{
  "trigger_id": "77777777-7777-4777-8777-777777777777",
  "trigger_version": 1,
  "configuration_id": "44444444-4444-4444-8444-444444444444",
  "configuration_version": 1,
  "deduplication_identity": "example-release-main-1",
  "source": {
    "kind": "reference",
    "value": "refs/heads/main"
  },
  "parameters": {
    "profile": "release",
    "publish": true
  },
  "priority": 50
}
```

The response is `200 OK`. An accepted result contains the Trigger occurrence,
Build, Attempt, and initially ready Job identities. A policy-suppressed result
contains the durable Trigger occurrence identity and creates no queued work.

## 6. Issue a one-time enrollment credential

The server derives an independent 256-bit enrollment secret from protected
server credential material and the request's idempotency identity. The
response contains the only full copy of the enrollment bearer; protect it
as a credential and never write it to logs.

```shell
curl --fail-with-body -X POST http://octacity.example.test:8080/api/v1/agent-enrollments \
  -H 'Content-Type: application/json' \
  -H 'Idempotency-Key: example-enroll-linux-1' \
  --data '{
    "pool_id":"66666666-6666-4666-8666-666666666666",
    "pool_version":1,
    "expected_platform":{"operating_system":"linux","architecture":"amd64"}
  }'
```

Copy the returned `credential` into the Agent's permission-restricted
`credential_file`. On first registration the server consumes the enrollment
credential atomically and the Agent promotes it in memory to the current
registration credential.

## 7. Run the Agent and follow execution

Validate the installed release with the same service identity and cgroup used
at runtime, then start it through the platform service manager. The complete
Linux Native installation procedure is in [Agent operations](operations.md).
The protocol sequence is retained in
[the static-Agent transcript](protocols/static-agent-transcript-v1.md).

Read the Build and its latest Attempt:

```shell
curl --fail-with-body http://octacity.example.test:8080/api/v1/builds/BUILD_ID
curl --fail-with-body http://octacity.example.test:8080/api/v1/attempts/ATTEMPT_ID
```

Read one Job and follow its ordered event stream. `after` is the last durable
cursor already observed; `wait_ms` is bounded by the server.

```shell
curl --fail-with-body http://octacity.example.test:8080/api/v1/jobs/JOB_ID
curl --fail-with-body 'http://octacity.example.test:8080/api/v1/jobs/JOB_ID/events?after=0&limit=100&wait_ms=30000'
```

The Attempt response contains the complete materialized DAG. A dependent Job
remains `blocked` until its dependency policy is satisfied, then becomes
`ready` with a newly signed JobSpec. Successful completion of the final Job
makes both the Attempt and Build `succeeded`.

## 8. Cancel or retry a Build

Cancellation and retry are idempotent commands. They require a fresh
`Idempotency-Key` but no request body:

```shell
curl --fail-with-body -X POST \
  -H 'Idempotency-Key: example-cancel-build-1' \
  http://octacity.example.test:8080/api/v1/builds/BUILD_ID/cancel

curl --fail-with-body -X POST \
  -H 'Idempotency-Key: example-retry-build-1' \
  http://octacity.example.test:8080/api/v1/builds/BUILD_ID/retry
```

Cancellation makes unowned work terminal immediately and sends a fenced
cancel directive to current Lease owners. Retry is accepted only for an
eligible terminal Build and creates the next immutable Attempt from the frozen
Build inputs.

Use `/api/v1/openapi.json` as the authoritative schema inventory. Log search,
artifact publication, cache sessions, and mutable VCS revision resolution are
implemented by their later feature tasks.

## Agent inventory and Pool assignment

The management contract exposes `GET /api/v1/agents` and
`GET /api/v1/agents/{agent_id}`. Each resource has exactly one `pool_id` and
`pool_version`, a normalized status, validated inventory, a separate static
capacity projection, and `last_seen_at_unix_ms`. Inventory therefore does not
duplicate Agent identity or host capacity.

An idle Agent can be moved with `POST /api/v1/agents/{agent_id}/pool`. The body
contains only `pool_id`; `Idempotency-Key` is required and `If-Match` carries
the current Agent version. The server resolves the destination Pool's current
version atomically, checks its admission policy and static capacity, revokes
the old registration, and leaves the Agent offline until its next enrollment.
An Agent that owns an active, cancellation-requested, or drain-requested Lease
cannot be reassigned.

Drain an Agent with `POST /api/v1/agents/{agent_id}/drain`, an
`Idempotency-Key`, and the current version in `If-Match`:

```json
{"mode":"graceful"}
```

`graceful` prevents new placement and makes the current Lease heartbeat return
`drain`; `forced` makes it return `cancel`. Publishing a Pool version with
`graceful_drain` or `forced_drain` applies the same directive atomically to all
current Leases in that Pool. A draining Agent remains ineligible for placement,
including after re-registration.
