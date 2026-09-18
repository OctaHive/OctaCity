# Management REST v1: section-4 workflow

This workflow exercises the management operations implemented by section 4:
Project creation, Pipeline publication, Repository publication, Build
Configuration publication, and manual Trigger acceptance. It intentionally
stops after Trigger acceptance. Agent Pools, enrollment, placement, execution,
and Build diagnostics are added by their later vertical-slice tasks.

The management API contract is unauthenticated in this release and must only
be exposed on a trusted network. The section-4 routes are currently exercised
by the running contract-test server and are available for composition, but the
production `octacity-server` binary intentionally still wires only health and
operational metadata. Task 5.9 wires the complete PostgreSQL-backed server,
Agent placement/execution lifecycle, and these mutation routes into one
headless production process. The examples use `octacity.example.test:8080` as
the future trusted-listener address.

## Prerequisites

The current section-4 boundary assumes these server-owned resources already
exist:

- VCS integration `55555555-5555-4555-8555-555555555555`;
- accepting Agent Pool `66666666-6666-4666-8666-666666666666`;
- enabled manual Trigger `77777777-7777-4777-8777-777777777777`, version 1.

Their management operations arrive with the later integration and Agent Pool
tasks. In the requests below, replace each illustrative resource identity with
the `resource.id` returned by the preceding create response:

| Resource | Illustrative identity |
| --- | --- |
| Project | `11111111-1111-4111-8111-111111111111` |
| Pipeline | `22222222-2222-4222-8222-222222222222` |
| Repository | `33333333-3333-4333-8333-333333333333` |
| Build Configuration | `44444444-4444-4444-8444-444444444444` |

Every mutation has a stable `Idempotency-Key`. Repeating an identical request
with the same key returns its original logical result; never reuse a key for
different content.

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

## 5. Accept one manual Trigger occurrence

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

Use `/api/v1/openapi.json` as the authoritative schema inventory. Later tasks
extend this workflow with Pool and Agent setup, execution, Job-event following,
Build diagnostics, logs, and outputs.
