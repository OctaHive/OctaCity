# Signed JobSpec execution contract v2

Status: implemented negotiated wire and placement contract. Provider execution
qualification is delivered by the platform-specific Agent Ready tasks.

Revision 2 adds provider-neutral execution intent without changing the meaning
of a revision-1 `Native` or OCI payload. The `SignedEnvelope`, signature
algorithm, source, Octa, task, cache, output, lease binding, and validity rules
remain those of [Signed JobSpec v1](signed-job-spec-v1.md). The authenticated
top-level `protocol_version` selects the decoder only after the exact payload
bytes have passed signature verification.

## Negotiation and compatibility window

An Agent registration advertises an inclusive `execution_contract` range. The
server selects the newest revision in the intersection with its own supported
range, persists that revision with the registration epoch, and uses the
persisted selection for every lease-placement decision. A new Agent currently
advertises `1..=2`; old registration documents omit the field and therefore
deserialize as `1..=1`. The registration response carries the selected
revision for v2 and omits it for v1 so strict legacy response decoders retain
their original wire shape. The Agent rejects a response that selects a revision
outside its offered range. An Agent also advertises v2 routes separately in
`executions` and must not advertise a route until its released-machine contract
passes.

Revision 1 remains accepted for every release that advertises revision 2.
Removing revision 1 requires a future major execution-contract revision, an
explicit migration release, and release notes that close this compatibility
window. During this window:

- `protocol_version: 1` is decoded only as `JobSpecV1`;
- legacy `Native` retains its existing Bubblewrap/cgroup-backed Linux meaning
  and is not evidence of direct host execution;
- legacy OCI `process` and `hypervisor` targets retain their original meaning;
- no legacy value is translated into `host`, `isolation`, or `virtualization`.

## Provider-neutral target

A revision-2 `runtime.target` contains five independent facts:

| Field | Meaning |
| --- | --- |
| `mode` | `host`, `isolation`, or `virtualization` |
| `host_platform` | OS and architecture running the Agent and provider |
| `target_platform` | OS and architecture observed by the runner |
| `required_guarantees` | Complete observable boundary required by policy |
| `immutable_image` | Optional workload or guest image identity, never a provider name |

`host` requires identical host and target platforms, no image, and no
isolation claim. `isolation` requires filesystem, process, network, and
resource isolation. Cross-platform isolation requires an immutable image.
`virtualization` adds a hardware-virtualization guarantee and always requires
an immutable guest image. Images use a digest-pinned OCI reference.

Concrete names such as `containerd`, `apple-vf-isolation`, or `microsandbox`
are forbidden from JobSpec and policy intent. They appear only in Agent
inventory and evidence:

- `ExecutionCapabilityV2` identifies a provider route and its exact semantic
  platform/mode/guarantee contract;
- `ExecutionEvidenceV2` records the provider that actually enforced a target;
- backend health binds diagnostics to the same advertised route;
- telemetry carries either the unchanged v1 runtime/isolation pair or one v2
  execution-evidence object, never both;
- `ExecutionCacheIdentityV2` combines execution evidence with a stable
  environment identity so equivalent intent implemented by different concrete
  providers cannot accidentally share physical cache entries.

## Placement and local enforcement

Placement matches v2 Jobs by host platform, target platform, mode, guarantees,
toolchain, plugins, capacity, and policy. Project policy grants an exact set of
provider-neutral `(mode, host_platform, target_platform, guarantees)` targets;
the legacy RuntimeClass allowlist cannot authorize a v2 target. Inherited policy
may only preserve or narrow this set. It does not add provider names or mode
labels to the generic capability set. Backend health must describe the same
provider route as inventory, preventing a ready diagnostic for one provider
from making another route schedulable.

The cache-session boundary accepts either the unchanged v1 Native/OCI identity
or `ExecutionCacheIdentityV2`. For v2 it derives the physical L1 runtime
identity from the complete validated provider, semantic target, immutable image
when present, and operator-owned environment identity. A provider or environment
change therefore produces a cold physical cache scope rather than a cross-route
cache hit.

The Agent verifies revision 1 or 2 before a payload reaches lifecycle code. A
valid v2 Job for which no qualified local provider is enabled fails as an
unavailable execution mode; it is never retried through a legacy Native or OCI
backend. Platform-specific tasks 9.4–9.6 add and qualify the corresponding
provider routes.
