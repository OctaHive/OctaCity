# OctaCity source-plugin protocol

`octacity-source-plugin` defines the versioned process protocol used by an
OctaCity agent to materialize a job workspace before Octa starts. The crate
contains shared Rust wire types, validation helpers, and bounded JSONL framing.
It does not implement VCS operations or execute plugins by itself.

The current wire version is `1`.

## Why this is a separate protocol

The agent must obtain the repository before it can read an Octafile. An Octa
task plugin therefore cannot perform primary source acquisition: the task DAG
and its plugins do not exist yet.

Source plugins and Octa task plugins have different responsibilities:

| Source plugin | Octa task plugin |
| --- | --- |
| Runs before the Octafile is loaded | Runs as part of an Octa task |
| Materializes one immutable source revision | Executes build or automation logic |
| Is installed and trusted by the agent operator | Is selected from the verified Octa release |
| Uses the OctaCity source protocol | Uses the Octa plugin protocol |

The first source plugin is `octacity-source-git`. Other VCS providers can use
the same contract without adding provider-specific behavior to the agent.

## Components and ownership

```text
signed JobSpec                 operator-managed source_plugins_dir
     |                                      |
     | provider, version, digest            | manifest, binary, settings
     v                                      v
OctaCity agent ---- verifies and selects source plugin
     |
     | stdin:  SourceCommand JSONL
     | stdout: SourceMessage JSONL
     | stderr: bounded process diagnostics
     v
source-plugin process ---- writes only to assigned destination
```

The responsibilities are deliberately split:

- The control plane is responsible for resolving a mutable VCS ref to an
  immutable provider-native revision and signing that revision in the JobSpec.
- The operator installs the plugin binary and owns its settings.
- The agent verifies the manifest, binary, signed requirement, lifecycle,
  timeouts, message bounds, and terminal revision.
- The plugin validates provider-specific parameters, materializes the source,
  and proves which immutable revision it produced.
- Octa starts only after source materialization succeeds.

A source plugin is trusted host code, not a sandbox. The protocol prevents a
job from choosing an executable path and bounds communication, but it cannot
make a malicious operator-installed executable safe.

## Installation and discovery

The agent discovers plugins from the configured `source_plugins_dir`. Every
immediate child is one plugin and its directory name must equal the manifest's
logical `name`:

```text
source_plugins_dir/
`-- git/
    |-- plugin.toml
    `-- octacity-source-git
```

Example manifest:

```toml
manifest_version = 1
name = "git"
version = "0.1.0"
protocol_min = 1
protocol_max = 1
executable = "octacity-source-git"
sha256 = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
platforms = ["linux-x86_64", "linux-aarch64"]

[settings]
git_path = "/usr/bin/git"
allow_file = false
max_diagnostic_bytes = 65536
```

`settings` is provider-specific, operator-controlled configuration. It is read
from the verified manifest and copied into `MaterializeRequest`; a repository
or signed job cannot override it.

At startup the current agent verifies that:

- the registry entries are real directories with safe permissions;
- `plugin.toml` is a regular, size-bounded file with no unknown fields;
- `manifest_version` is supported;
- the logical name matches the plugin directory;
- the supported protocol range contains the agent's protocol version;
- the current `os-arch` value appears in `platforms`;
- `executable` is a normalized relative path contained by the plugin directory;
- the executable is a regular executable file with safe permissions;
- the executable's lowercase SHA-256 equals `sha256`.

Before each job the agent also requires the installed plugin `name`, `version`,
and `sha256` to exactly match the signed source requirement. Job data never
supplies a host path or installs a plugin.

The manifest is the agent's inventory source. Protocol v1 has no dynamic
registration or capabilities RPC. A plugin may expose operator-facing CLI
commands, but they are outside this process protocol and the agent does not use
them during materialization.

## Process model

Protocol v1 uses one fresh process for one materialization request. There is no
request multiplexing and no reusable daemon mode.

The agent starts the verified executable:

- with no protocol data in command-line arguments;
- with the plugin directory as its working directory;
- with a cleared environment;
- with piped stdin, stdout, and stderr;
- in a separate process group on Unix so cancellation can terminate
  descendants as well as the plugin process.

stdin carries `SourceCommand` frames from the agent. stdout carries only
`SourceMessage` frames from the plugin. Human logs must not be written to
stdout because they would corrupt the protocol. stderr is reserved for bounded
process diagnostics and must never contain credentials.

Every frame is one UTF-8 JSON object followed by `\n`. The newline is part of
the maximum frame size. Both directions reject frames larger than 1 MiB,
unterminated frames, malformed JSON, unknown fields, and unknown message
variants.

## Lifecycle

The normal exchange is:

```text
Agent                                      Plugin
  |                                          |
  |                         hello            | process starts
  |<-----------------------------------------|
  | validates protocol, name, and version    |
  |                                          |
  | materialize                              |
  |----------------------------------------->|
  |                                accepted  |
  |<-----------------------------------------|
  |                                progress  | zero or more
  |<-----------------------------------------|
  |                              diagnostic  | zero or more
  |<-----------------------------------------|
  | finished | cancelled | error             | exactly one terminal message
  |<-----------------------------------------|
  |                                          | exits successfully
  |<-----------------------------------------|
```

The legal states are:

```text
Spawned
  -> Hello
  -> MaterializeSent
  -> Accepted
  -> Progress | Diagnostic   (zero or more, only after Accepted)
  -> Finished | Cancelled | Error
  -> ProcessExited
```

`Error` is also allowed before `Accepted` when the plugin cannot accept the
request. All request-scoped messages must carry the exact `request_id` chosen
by the agent. A process-level `Error` may use `request_id: null`.

The host rejects duplicate, out-of-order, uncorrelated, or unexpected
messages. In particular, `Progress`, `Diagnostic`, `Finished`, and `Cancelled`
are invalid before `Accepted`. Once a terminal message is emitted, the plugin
must stop producing protocol output and exit successfully within the agent's
grace period.

A provider failure is represented by `SourceMessage::Error` followed by a
successful process exit. A non-zero exit, premature EOF, invalid frame, or a
process that remains alive after its terminal message is a plugin process or
protocol failure rather than a normal provider error.

### Startup handshake

The plugin must emit `Hello` immediately after startup:

```json
{"type":"hello","protocol_version":1,"plugin_name":"git","plugin_version":"0.1.0"}
```

The current agent allows five seconds for this message and requires all three
values to match its supported protocol and the verified manifest. The agent
sends no request before the handshake succeeds.

### Materialization

After the handshake, the agent sends exactly one `Materialize` command:

```json
{
  "type": "materialize",
  "protocol_version": 1,
  "request_id": "job-42/attempt-1",
  "request": {
    "destination": "/var/lib/octacity/work/job-42/source",
    "revision": "0123456789abcdef0123456789abcdef01234567",
    "reference": "refs/heads/main",
    "parameters": {"url": "https://example.com/team/project.git"},
    "settings": {"git_path": "/usr/bin/git", "allow_file": false},
    "credential_files": {"git_config": "/run/octacity/credentials/git.config"},
    "max_workspace_bytes": 10737418240
  }
}
```

The fields have distinct trust owners:

| Field | Owner | Meaning |
| --- | --- | --- |
| `destination` | Agent | Absolute, initially empty workspace directory assigned to this request |
| `revision` | Signed job | Immutable provider-native identity that must be materialized exactly |
| `reference` | Signed job | Optional fetch hint; never a substitute for validating `revision` |
| `parameters` | Signed job | Provider-specific request such as a remote URL |
| `settings` | Operator manifest | Provider configuration unavailable for job overrides |
| `credential_files` | Agent/operator | Named paths to restricted credential files, interpreted by the provider |
| `max_workspace_bytes` | Agent/job policy | Maximum permitted materialized workspace size |

The plugin must validate the common request with `MaterializeRequest::validate`
and then validate every provider-specific parameter, setting, credential name,
and revision format. It must not inherit missing configuration from the user
profile, process environment, or repository.

Protocol v1 represents provider-specific data as strict-at-the-provider maps;
the shared manifest does not yet publish JSON Schemas for them. Consequently,
the plugin is the authoritative semantic validator. The signature protects
parameters from modification in transit, but does not make an invalid provider
parameter valid. Adding schema metadata to a future manifest format will
require a manifest-version change and will not replace plugin-side validation.

If it accepts the work, it emits:

```json
{"type":"accepted","request_id":"job-42/attempt-1"}
```

`Accepted` confirms protocol ownership of the request; it does not indicate
that source acquisition has completed.

### Progress and diagnostics

After `Accepted`, a plugin may emit structured status messages:

```jsonl
{"type":"progress","request_id":"job-42/attempt-1","message":"fetching revision"}
{"type":"diagnostic","request_id":"job-42/attempt-1","message":"remote responded slowly"}
```

`Progress` describes normal lifecycle advancement. `Diagnostic` records a
non-terminal warning or troubleshooting detail. Neither message changes the
state of the request.

In addition to the per-frame limit, the current agent accepts at most 4096
progress/diagnostic messages with at most 1 MiB of message text in aggregate.
Plugins should emit semantic milestones rather than forwarding raw VCS output.

### Successful completion and provenance

The terminal success message is:

```json
{
  "type": "finished",
  "request_id": "job-42/attempt-1",
  "revision": "0123456789abcdef0123456789abcdef01234567",
  "provenance": {
    "provider": "git",
    "revision": "0123456789abcdef0123456789abcdef01234567"
  }
}
```

The returned `revision` must exactly equal the requested immutable revision.
The agent checks this equality independently. `provenance` is provider-defined
audit metadata; it must contain no credentials or other secrets and must not be
treated as a replacement for the verified terminal revision.

The destination must be complete before `Finished` is sent. After sending it,
the plugin exits successfully and performs no more writes.

### Provider errors

A request-specific provider or validation failure is terminal:

```json
{"type":"error","request_id":"job-42/attempt-1","message":"requested revision was not found"}
```

When a failure cannot be correlated to a request, `request_id` may be `null`:

```json
{"type":"error","request_id":null,"message":"failed to initialize provider"}
```

Error messages are safe diagnostics, not arbitrary VCS output. They must be
bounded, sanitized, and free of URLs containing credentials, tokens, file
contents, and secret environment values.

## Cancellation and timeout

The agent may send cancellation after `Materialize`:

```json
{"type":"cancel","request_id":"job-42/attempt-1"}
```

The plugin must propagate cancellation to every VCS child process, stop
workspace mutation, emit `Accepted` if it has not done so already, and then
emit:

```json
{"type":"cancelled","request_id":"job-42/attempt-1"}
```

It must then exit successfully. Cancellation must be idempotent internally,
although the current agent sends it at most once per process.

The same command is used when the operation timeout expires. The agent retains
the original reason, so a late `Finished` cannot turn an externally cancelled
or timed-out operation into success. If the plugin does not stop within the
configured cancellation grace period, the agent force-kills the complete
plugin process group.

## Credentials and sensitive data

`credential_files` contains handles, not secret values. Names and file formats
are provider-specific; a plugin must reject unknown credential names. Paths
must be absolute, and plugins should require regular files with restrictive
permissions rather than following symlinks.

The component that provisions a credential file owns its cleanup. The wire
crate only transports the handle. Plugins must close credential files before
returning and must never copy credentials into the workspace, protocol
messages, stderr, process arguments, URLs, provenance, or caches.

The agent clears the plugin environment. A plugin should likewise start VCS
children with an explicit environment and disable user/system configuration,
interactive credential prompts, hooks, filters, or helper processes unless the
operator explicitly enabled them.

## Versioning and compatibility

Manifest and process protocol versions are independent:

- `manifest_version` versions the structure and interpretation of
  `plugin.toml`.
- `protocol_min` and `protocol_max` state which process protocol versions the
  binary implements.
- `protocol_version` in `Hello` and `Materialize` identifies the active wire
  protocol.
- `version` is the plugin release version. Jobs pin it together with the
  executable SHA-256.

Version 1 performs no dynamic feature negotiation. The agent uses its current
protocol only when it falls inside the installed plugin's declared range.
Because wire types reject unknown fields and variants, changing message shape,
ordering, field meaning, or terminal semantics requires a new protocol
version. Adding provider-specific entries inside `parameters`, `settings`, or
`provenance` does not change the protocol version, but may require a new plugin
release.

## Implementing a plugin

A conforming implementation should:

1. Depend on `octacity-source-plugin` for the shared wire types and limits.
2. Emit `Hello` and flush stdout before waiting for the first command.
3. Require the first command to be `Materialize` with a supported version.
4. Validate `request_id`, `MaterializeRequest`, and every provider-specific
   field before changing the destination.
5. Emit `Accepted`, then only correlated progress, diagnostic, and terminal
   messages.
6. Continue reading stdin while provider work runs so `Cancel` is observable.
7. Terminate all child processes and stop filesystem writes on cancellation.
8. Verify the exact immutable revision after acquisition rather than trusting
   a mutable ref or the provider command's exit status alone.
9. Send exactly one terminal message, flush it, and exit successfully.
10. Keep stdout protocol-only and keep all outputs free of secrets.

`octacity-source-git` is the executable reference implementation. Registry
discovery, digest verification, and process supervision are implemented by the
`octacity-source` crate.
