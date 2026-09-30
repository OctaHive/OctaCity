# Security release gates

The scheduled `security` workflow keeps slow, network-dependent checks out of
the portable commit loop while still making them reproducible. The release
workflow calls it with fuzzing enabled and cannot package before it succeeds.
The complete trust analysis and remaining deployment assumptions are recorded
in [the server and Agent threat model](architecture/threat-model.md).

## Dependencies, licenses, and sources

The workflow audits both the production and fuzz lockfiles against RustSec:

```shell
cargo audit
cargo audit --file fuzz/Cargo.lock
```

It also applies separate checked-in policies to the production and fuzz
dependency graphs:

```shell
cargo deny --locked check licenses sources
cargo deny --locked --manifest-path fuzz/Cargo.toml --config fuzz/deny.toml check licenses sources
```

The production policy is stored in `deny.toml`; the independent fuzz workspace
uses `fuzz/deny.toml` because its libFuzzer dependency graph requires a
different reviewed license set. Both policies reject unknown registries and Git
sources and do not exempt private workspace packages from license metadata. A
new license or source requires an explicit reviewed policy change.

The workflow pins `cargo-audit`, `cargo-deny`, Gitleaks, and `cargo-fuzz`;
version changes therefore remain reviewable. Vulnerability, license, source,
and detected-secret findings fail the job. Informational RustSec warnings
remain visible and must be evaluated rather than silently ignored or converted
into permanent blanket exceptions.

## Secret scanning

Gitleaks scans both source trees used to produce a release: this repository and
the pinned Octa checkout. It uses the maintained default rules and redacts any
detected value from CI output:

```shell
gitleaks dir --config octacity/.gitleaks.toml --redact --no-banner --no-color octacity
gitleaks dir --config octacity/.gitleaks.toml --redact --no-banner --no-color octa
```

Do not add a baseline for a real credential. Revoke it, remove it from the
revision, and rerun the gate. A demonstrable false positive may receive the
narrowest possible path-and-rule exception with a reason and a regression test;
global rule disablement is not acceptable. Runtime leakage remains covered by
the security contract matrix and redaction tests because a repository scanner
cannot observe generated logs, events, artifacts, or diagnostics.

The checked-in configuration has one such exception: the pinned Octa source
contains a public test-only TLS private-key fixture used by cache protocol
tests. Only the `private-key` rule at that exact fixture path is exempted.

## Protocol fuzzing

Four fuzz targets feed arbitrary bytes to the public deserializers at each
process or network boundary:

```shell
cargo fuzz run server-agent-json
cargo fuzz run runner-json
cargo fuzz run source-plugin
cargo fuzz run server-protocols
```

`server-agent-json` covers every public coordinator request, response, nested
wire DTO, and signed JobSpec decoder,
`runner-json` covers command deserialization and the exact bounded output-frame
decoder used by the supervisor. `source-plugin` covers the shared bounded JSONL
decoder in both directions plus the operator TOML manifest. `server-protocols`
covers the exact bounded request and correlated-response decoders for artifact
transfer, VCS, webhook-provider, and agent-provisioning processes. The
server-Agent target also
constructs correctly signed arbitrary payloads so signature verification,
payload decoding, and JobSpec validation are reached. Asynchronous I/O and
lifecycle behavior retain ordinary unit and contract tests; fuzzing complements
those tests rather than replacing timeout or state-machine assertions.

Corpus and crash artifacts are intentionally untracked. Reproduce a reported
crash with the exact workflow artifact and pinned toolchain before minimizing
it; add the minimized input as a normal regression test at the owning protocol
boundary.

## Release composition

`ci.yml` is also a reusable workflow. `release.yml` requires it, this security
workflow with fuzzing enabled, and the release-qualified backend contract
workflow before any package job starts. This binds Clippy with `-D warnings`,
rustdoc with `-D missing_docs`, the architecture guard, the 80% production-line
coverage floor, security checks, and released-backend evidence to the exact
revision being packaged.
