# Signed JobSpec protocol v1

Status: implemented wire contract; server-agent transport not yet implemented.

This document specifies the signed execution intent represented by the
`octacity-protocol` crate. A server creates a `JobSpecV1`, signs its exact JSON
bytes, and places it in a `SignedEnvelope`. An agent authenticates and validates
that envelope before selecting a source plugin, Octa release, or execution
backend.

The document does not define lease acquisition, heartbeat, event upload,
artifact upload, or completion endpoints. Those operations belong to the
planned server-agent transport. The current crate contains only Signed JobSpec
types, signature verification, lease-attempt binding, and semantic validation.

## Purpose and trust boundary

JobSpec is an authorization document for one job attempt. It lets the control
plane request a bounded Octa execution without giving it a general-purpose
remote shell on the agent host.

```text
OctaCity server
  -> creates exact JobSpec JSON bytes
  -> signs those bytes with an Ed25519 server key
  -> sends SignedEnvelope as part of a lease

OctaCity agent
  -> selects a locally configured verification key by key_id
  -> verifies the exact signed bytes
  -> deserializes and validates JobSpecV1
  -> binds job_id and attempt to the surrounding lease
  -> independently checks local policy and installed components
  -> materializes source and starts octa-runner
```

A valid signature proves that a configured server key authorized the payload.
It does not prove that the requested plugin, runner, backend, image, network
policy, or workload identity is safe or available. The agent must still apply
operator-owned policy and match every referenced executable to its locally
verified inventory.

JobSpec intentionally contains no host executable paths, shell commands,
arbitrary mounts, server credentials, resolved secret values, or environment
inheritance. `execution.commands` contains Octa task names, not commands to run
directly on the host.

## Signed envelope

The transport carries this strict JSON object:

```json
{
  "key_id": "server-key-2026-09",
  "algorithm": "ed25519",
  "payload": "<standard-base64-encoded-job-spec-json>",
  "signature": "<standard-base64-encoded-ed25519-signature>"
}
```

| Field | Meaning |
| --- | --- |
| `key_id` | Selects one verification key provisioned in agent configuration |
| `algorithm` | Must be exactly `ed25519` in v1 |
| `payload` | Standard RFC 4648 base64 of the exact JobSpec JSON bytes |
| `signature` | Standard base64 of the 64-byte Ed25519 signature over the decoded payload bytes |

The decoded payload is limited to 1 MiB. Envelope and JobSpec structures reject
unknown fields.

### Signing

The signed message is the serialized payload byte sequence itself:

```text
payload_bytes = serialize_json(job_spec)
signature     = ed25519_sign(signing_key, payload_bytes)

envelope.payload   = base64_standard(payload_bytes)
envelope.signature = base64_standard(signature)
```

JSON canonicalization is deliberately not required. Whitespace, object-key
order, and escaping are significant because they change `payload_bytes`.
Consumers must verify the decoded bytes from the envelope and must not parse and
re-serialize the payload before verification.

### Verification order

`verify_job_spec` performs these steps in order:

1. Require the exact `ed25519` algorithm identifier.
2. Resolve `key_id` from the agent's configured verification keys.
3. Decode the standard-base64 payload and enforce its 1 MiB limit.
4. Decode the signature and require a valid Ed25519 signature length.
5. Verify the signature over the decoded payload bytes.
6. Deserialize only the authenticated bytes as strict `JobSpecV1` JSON.
7. Validate the protocol version, lease binding, validity interval, and all
   nested sections.

No JobSpec field may influence execution before step 7 succeeds.

## Complete JobSpec shape

This is a structurally valid example. Digest and revision values are examples;
an actual job must use values resolved from installed artifacts and source.

```json
{
  "protocol_version": 1,
  "job_id": "job-42",
  "attempt": 1,
  "issued_at": 1789056000,
  "expires_at": 1789056300,
  "source": {
    "provider": "git",
    "plugin_version": "0.1.0",
    "plugin_sha256": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
    "revision": "0123456789abcdef0123456789abcdef01234567",
    "reference": "refs/heads/main",
    "parameters": {
      "url": "https://example.com/team/project.git"
    }
  },
  "octa": {
    "version": "0.3.0",
    "runner_sha256": "123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef0",
    "runner_protocol": 1,
    "event_schema": 3,
    "plugin_protocol": 1,
    "plugin_digests": {
      "shell": "23456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef01"
    }
  },
  "execution": {
    "octafile": "ci/Octafile.yml",
    "commands": ["test"],
    "variables": {
      "profile": "ci"
    },
    "arguments": [],
    "concurrency": 4,
    "parallel": true,
    "failfast": true,
    "secrets_profile": "ci/secrets.yml"
  },
  "runtime": {
    "target": {
      "mode": "oci",
      "platform": {
        "os": "linux",
        "architecture": "amd64"
      },
      "isolation": "hypervisor",
      "image": "registry.example.com/octacity/build@sha256:3456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef012"
    },
    "cpu_millis": 4000,
    "memory_bytes": 4294967296,
    "writable_disk_bytes": 10737418240,
    "timeout_seconds": 900,
    "network": {
      "restricted": {
        "allowed_hosts": ["proxy.example.com"]
      }
    }
  },
  "outputs": {
    "artifact_count": 100,
    "artifact_bytes": 1073741824,
    "report_count": 100,
    "report_bytes": 268435456
  }
}
```

All timestamps are unsigned Unix timestamps in whole seconds. The validity
interval is half-open: `issued_at <= now < expires_at`.

## Top-level identity and validity

| Field | Validation and semantics |
| --- | --- |
| `protocol_version` | Must equal `1` |
| `job_id` | Non-blank stable job identity |
| `attempt` | Positive attempt number; must match the surrounding lease binding |
| `issued_at` | First Unix second in which the specification is valid |
| `expires_at` | First Unix second in which the specification is no longer valid; must be greater than `issued_at` |

`JobBinding` is supplied out of band by the lease owner and contains
`job_id`, `attempt`, and the agent's current time. Requiring the signed identity
to equal this binding prevents a valid JobSpec from being substituted across
jobs or attempts.

`lease_id` and `fencing_token` are not part of the currently implemented
JobSpec crate. The future coordinator transport must validate them on
heartbeat, event, artifact, and completion operations. JobSpec verification
does not replace continuous lease ownership or cancellation after lease loss.

## Source requirement

`source` selects an operator-installed source plugin and an exact source
revision:

| Field | Validation and semantics |
| --- | --- |
| `provider` | Non-blank logical plugin name, for example `git` |
| `plugin_version` | Non-blank exact installed plugin release |
| `plugin_sha256` | Exactly 64 lowercase hexadecimal characters |
| `revision` | Non-blank immutable provider-native revision |
| `reference` | Optional non-blank mutable fetch hint; never the source identity |
| `parameters` | Provider-specific signed JSON values; defaults to an empty object |

The JobSpec validator checks only provider-independent rules. The agent matches
`provider`, `plugin_version`, and `plugin_sha256` against its verified registry;
the selected plugin authoritatively validates `revision`, `reference`, and
`parameters` before changing the workspace.

Source materialization must return the exact requested `revision`. A branch or
tag in `reference` can narrow the fetch but cannot replace this equality check.
See the [source-plugin v1 specification](../../crates/octacity-source-plugin/README.md)
for the process lifecycle.

## Octa release requirement

`octa` pins the execution engine and all permitted Octa plugins:

| Field | Validation and semantics |
| --- | --- |
| `version` | Non-blank exact Octa release version |
| `runner_sha256` | Lowercase SHA-256 of the `octa-runner` executable |
| `runner_protocol` | Positive required runner process protocol version |
| `event_schema` | Positive required runner event schema version |
| `plugin_protocol` | Positive required Octa task-plugin protocol version |
| `plugin_digests` | Plugin logical name to lowercase executable SHA-256; defaults to empty |

The shared validator checks shape and digest format. The agent separately
inventories its operator-installed Octa release and requires exact version,
protocol, schema, and digest compatibility before execution.

## Execution intent

`execution` describes what `octa-runner` should execute after source
materialization:

| Field | Validation and semantics |
| --- | --- |
| `octafile` | Optional normalized workspace-relative `/`-separated path |
| `commands` | One or more non-empty Octa task names |
| `variables` | String values supplied to Octa; defaults to an empty object |
| `arguments` | Task arguments; defaults to an empty array |
| `concurrency` | Optional positive concurrency limit |
| `parallel` | Enables Octa parallel execution; defaults to `false` |
| `failfast` | Stops scheduling after failure; defaults to `false` |
| `secrets_profile` | Optional normalized workspace-relative secrets profile path |

Normalized wire paths are non-empty, relative, use `/`, contain no `.` or `..`
components, backslashes, colons, empty components, or control characters. The
agent/backend resolves them within the assigned workspace; they are never host
paths.

Variables and arguments are runner inputs, not inherited process environment.
Secret values are not embedded in JobSpec. Octa resolves them inside the job
through the selected secrets profile and workload identity.

## Runtime policy

`runtime` describes the required execution boundary:

| Field | Validation and semantics |
| --- | --- |
| `target` | Tagged `native` or `oci` execution target described below |
| `cpu_millis` | Positive CPU allocation where 1000 represents one CPU |
| `memory_bytes` | Positive memory limit |
| `writable_disk_bytes` | Positive writable-workspace limit |
| `timeout_seconds` | Positive wall-clock execution timeout |
| `network` | `unrestricted`, `disabled`, or a non-empty restricted host list; the selected backend must enforce it or reject the job |
| `workload_identity_profile` | Optional non-blank operator-known identity profile name; rejected until workload identity is enabled |

Restricted network policy serializes as:

```json
{
  "restricted": {
    "allowed_hosts": ["proxy.example.com", "vault.example.com"]
  }
}
```

`target` has exactly one of these shapes:

```json
{
  "mode": "native",
  "platform": { "os": "linux", "architecture": "amd64" }
}
```

```json
{
  "mode": "oci",
  "platform": { "os": "windows", "architecture": "amd64" },
  "isolation": "hypervisor",
  "image": "registry.example.com/build/windows@sha256:3456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef012"
}
```

Supported operating-system names are `linux`, `windows`, and `macos` for
Native targets. OCI targets currently accept only `linux` and `windows`.
Architectures are `amd64` and `arm64`. OCI isolation is exactly `process` or
`hypervisor`. The tagged target makes image and isolation fields mandatory for
OCI and impossible for Native. The image is an immutable
`repository@sha256:<64 lowercase hex>` reference.

The shared validator guarantees only the rules above. The agent must reject a
runtime mode or OCI isolation tier that is disabled locally and must never fall
back between OCI tiers or from OCI to Native. The selected backend is
responsible for enforcing resource and network limits. Engine-specific numeric
representability belongs to the engine adapter rather than this wire contract.
Until workload identity provisioning is enabled,
the agent rejects a requested profile before source acquisition. Once enabled,
it maps the name to operator-owned configuration; the job can never provide raw
credentials.

## Output limits

`outputs` bounds artifacts and reports declared by Octa:

| Pair | Rule |
| --- | --- |
| `artifact_count`, `artifact_bytes` | Both zero to disable artifacts, or both positive |
| `report_count`, `report_bytes` | Both zero to disable reports, or both positive |

The protocol validator enforces pair consistency, not deployment-wide maxima.
The agent and server must additionally apply operator and project limits before
accepting or uploading output.

## Local policy remains authoritative

After `verify_job_spec` succeeds, the agent still must verify that:

- the source plugin name, version, platform, and executable digest are installed;
- the exact Octa runner and task-plugin digests are installed;
- required runner, event, and plugin protocol versions are supported;
- the requested runtime mode, guest platform, architecture, and OCI isolation
  tier are enabled and healthy;
- native execution was explicitly allowed by the operator;
- the OCI image digest is available and trusted by the selected engine;
- requested resources fit agent capacity and configured maxima;
- network hosts and workload identity profile are locally permitted;
- workspace paths remain inside the assigned execution root;
- the lease remains current throughout execution and result publication.

Signed server intent can narrow operator policy but cannot broaden it.

## Error classes

`verify_job_spec` distinguishes:

- unsupported signature algorithm;
- unknown signing-key ID;
- invalid payload or signature base64;
- oversized decoded payload;
- invalid signature length or verification failure;
- invalid authenticated JSON;
- semantic JobSpec validation failure.

Errors must not log decoded payload bytes, signatures, secret-bearing
parameters, or verification-key material. A rejected JobSpec must not partially
prepare source, credentials, workspaces, or execution backends.

## Versioning and compatibility

Protocol v1 is strict:

- all structures reject unknown fields;
- enum encodings and field meanings are part of the wire contract;
- omitted fields are allowed only where the Rust type defines a default;
- changes to required fields, validation, enum variants, path semantics,
  signing input, or verification order require an explicit protocol review and
  normally a new `protocol_version`;
- key rotation uses `key_id` and does not require a protocol-version change;
- changing installed plugin, runner, or image digests changes job content but
  not the wire version.

Producers must sign the exact bytes they transmit. Consumers may accept any JSON
serialization that deserializes to `JobSpecV1` after its exact bytes have been
authenticated; there is no preferred canonical serializer in v1.

## Implementation locations

- Wire types and verification: `crates/octacity-protocol/src/lib.rs`
- Agent configuration and verification keys: `crates/octacity-config/src/lib.rs`
- Source-plugin inventory and host: `crates/octacity-source/src/`
- Octa release inventory and supervision: `crates/octacity-runner/src/`
- Backend-neutral execution contract: `crates/octacity-execution/src/lib.rs`
- Native execution adapter: `crates/octacity-execution-native/src/lib.rs`
