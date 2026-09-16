## Purpose

Defines the released-machine evidence required before the server and agent are considered ready for production-oriented self-hosted CI use.

## ADDED Requirements

### Requirement: Tests use released artifacts
The Agent Ready matrix SHALL install self-verifying released OctaCity and Octa bundles on clean machines and SHALL NOT execute binaries or configuration from repository build-tree paths.

#### Scenario: Release candidate is tested
- **WHEN** the matrix starts for a release candidate
- **THEN** every tested agent verifies and installs the exact packaged checksums and capability manifests for that candidate

### Requirement: Initial runtime matrix
The initial gate SHALL cover Linux Native, Linux OCI process isolation, and Linux OCI hypervisor execution on supported Linux hosts, plus the supported Linux hypervisor guest path on Apple Silicon macOS.

#### Scenario: Unsupported pair cross-compiles
- **WHEN** a platform/isolation pair compiles but has no passing released-machine contract
- **THEN** the server does not advertise that pair as release-qualified

### Requirement: Complete functional matrix
The matrix SHALL cover successful and failed tasks, invalid signatures and digests, stale fencing, source and runner failures, output and cache behavior, full-text and literal build-log search, Vault identity lifecycle, cancellation at each phase, resource accounting, bounded backpressure, and repeated idempotent requests.

#### Scenario: Two agents share remote cache
- **WHEN** agent A publishes a cacheable result and agent B starts with an empty L1 in the same authorized namespace
- **THEN** agent B restores from L2 without task execution and both agents retain verified L1 copies

### Requirement: Failure and restart matrix
The matrix SHALL inject agent crashes, server disconnects and restarts, host reboots, storage outages, corrupt remote content, search-index outage and loss, exhausted disk, full event spool, and sampling failures while verifying fencing, replay, requeue, index rebuild, and bounded cleanup.

#### Scenario: Server restarts during event delivery
- **WHEN** the server restarts after durably storing a prefix but before returning its acknowledgement
- **THEN** agent replay produces one ordered event stream and one terminal completion

#### Scenario: Search index is rebuilt
- **WHEN** retained logs are reindexed after projection loss
- **THEN** full-text and literal queries return the same redacted logical matches and event context without changing the completed Build Result

### Requirement: Operational lifecycle matrix
Each release-qualified host SHALL complete documented install, validation, service start, restart, atomic upgrade, server-directed drain, rollback where supported, and removal with dedicated unprivileged identities.

#### Scenario: Agent is upgraded
- **WHEN** an operator follows the documented drain, stop, switch, validate, and start sequence
- **THEN** the new agent registers once, completes a job, and leaves the previous release available for rollback without mixed-version files

### Requirement: Bounded performance and resources
The matrix SHALL retain raw measurements for idle CPU/RSS, lease latency, backend startup/destruction, event throughput, log-index lag and query latency, outage replay, cancellation, artifact transfer, and 100 sequential jobs, and SHALL fail on configured leaks or unexplained release regressions.

#### Scenario: Sequential-job soak completes
- **WHEN** one agent completes 100 bounded jobs
- **THEN** no job process, VM, descriptor, credential, workspace, or unbounded queue remains and resource use stays within the release thresholds
