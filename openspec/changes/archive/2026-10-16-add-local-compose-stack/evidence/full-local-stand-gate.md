# Full local-stand verification gate

## Result

**PASS:** the complete hybrid local stand was built and exercised on the target
Apple Silicon Mac. A native macOS ARM64 Agent executed a Linux ARM64 job through
Microsandbox, PostgreSQL retained its metadata, MinIO retained and returned the
artifact, repeated convergence did not duplicate resources, and ordinary
restart preserved the exact build and object.

Verified on `2026-10-03`. The executable gate used a disposable Git snapshot of
the implementation working tree; evidence-only Markdown and task-state updates
were added afterward.

## Quality and dependency gates

The following checks passed:

- `cargo fmt --all -- --check`;
- workspace Clippy for all targets and features with warnings denied;
- workspace Rustdoc with missing documentation denied;
- the complete Rust workspace test suite with all features;
- locked Cargo metadata for the workspace and fuzz workspace;
- Cargo license/source policy and dependency audit for both workspaces (the
  repository's existing, documented `bincode 2.0.1` advisory allowance remains
  transitively required by Microsandbox);
- locked UI installation, formatting, ESLint, type checking, production build,
  all 71 UI tests, and dependency audit in the pinned Node image;
- all 253 Python tool and policy tests (`3` platform-specific skips);
- architecture policy (`53` workspace packages and `185` dependency edges);
- bounded Docker/native contexts, immutable input policy, real container image
  contracts, ARM64 Compose rendering, and `docker compose config --quiet`;
- `openspec validate add-local-compose-stack --strict` and `git diff --check`.

The Python suite must run outside the assistant filesystem sandbox because two
lifecycle tests create real process groups and Unix sockets. The same suite
passes without failures in the normal host execution environment.

## Hybrid vertical slice

The target used macOS ARM64, OrbStack's Docker-compatible ARM64 engine, Compose
v5, and the pinned native Microsandbox `0.7.6` runtime. The clean `up` gate
reported PostgreSQL, MinIO, server, and gateway healthy and exactly one online
Agent with the required Linux ARM64 Microsandbox execution target.

The real job produced and re-downloaded this object through the public TLS
gateway:

```text
agent_id:       b657659e-0dc5-5bdb-ac8b-e2ec9e2f7fce
pool_id:        c408f583-6788-45a3-851c-f6497656a6df
project_id:     4054de3c-5a05-4f17-8e34-035c6f497ca8
build_id:       1acdd96f-6122-5f1d-99ba-25359247a23d
job_id:         ca13f199-1d75-59ac-944d-9b48722683b9
artifact_id:    0fee09d4-c2e2-4518-ae42-d06f5020450d
artifact_bytes: 1048576
artifact_sha256: ed320945d94992e79e7c98efb6898a71e40c065a7940e1191f8af63c7721f355
```

A second `up` returned `already_running` with the original Agent PID and did
not create another Pool or Agent. After a complete `down` and `up`, observation
returned the same Pool, Build, Artifact, byte count, and SHA-256. Final `down`
reported no running or present Compose service and no owned Agent process.
Confirmed `reset` then removed only the reported private host state, shortened
Microsandbox state root, and the two named data volumes.

The bounded combined runtime log was checked after the real artifact transfer:

```text
validated 1 secret-safe local-stand log(s)
```

This check exposed and corrected an Nginx diagnostic path that could include a
complete presigned object URL even when the access-log format omitted query
strings. Credential-bearing cache and object hosts now retain safe access logs
and restrict their unformattable internal diagnostics to process-critical
events; the regression is enforced by the gateway policy tests.

## Claim boundary

This evidence qualifies only the local development and integration deployment
defined by this change. It does not claim release qualification, production
hardening, high availability, backup coverage, remotely safe exposure, or any
replacement for the release Agent Ready matrix. The operations guide and delta
spec state these limitations explicitly.
