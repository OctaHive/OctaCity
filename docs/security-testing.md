# Dependency and protocol security checks

The scheduled `security` workflow keeps slow, network-dependent checks out of
the portable test loop while still making them reproducible. It audits both
the production and fuzz lockfiles against RustSec:

```shell
cargo audit
cargo audit --file fuzz/Cargo.lock
```

The workflow pins `cargo-audit` and `cargo-fuzz`; version changes therefore
remain reviewable. Vulnerability findings fail the job. Informational RustSec
warnings remain visible and should be evaluated rather than being silently
ignored or converted into permanent blanket exceptions.

Three fuzz targets feed arbitrary bytes to the public deserializers at each
process or network boundary:

```shell
cargo fuzz run server-agent-json
cargo fuzz run runner-json
cargo fuzz run source-plugin
```

`server-agent-json` covers every public coordinator request, response, nested
wire DTO, and signed JobSpec decoder,
`runner-json` covers command deserialization and the exact bounded output-frame
decoder used by the supervisor. `source-plugin` covers the shared bounded JSONL
decoder in both directions plus the operator TOML manifest. The server target
also constructs correctly signed arbitrary payloads so signature verification,
payload decoding, and JobSpec validation are reached. Asynchronous I/O and
lifecycle behavior retain ordinary unit and contract tests; fuzzing complements
those tests rather than replacing timeout or state-machine assertions.

Corpus and crash artifacts are intentionally untracked. Reproduce a reported
crash with the exact workflow artifact and pinned toolchain before minimizing
it; add the minimized input as a normal regression test at the owning protocol
boundary.
