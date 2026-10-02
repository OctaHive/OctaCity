# Immutable local-stand inputs

## Result

**PASS:** every external input needed by the hybrid local stand has one
platform-specific immutable identity in
`deployment/local-stand/inputs.json`. Repository checks reject missing,
mutable, malformed, cross-platform, or internally inconsistent image, source,
and native release metadata.

Verified on `2026-10-03`.

## Container inputs

The ARM64 service and build plane uses digest-qualified references for:

- Rust `1.99.0-bookworm`
- Node `24.21.0-bookworm-slim`
- nginx `1.30.5-alpine3.24`
- PostgreSQL `18.6-bookworm`
- Go `1.27.1-alpine3.24`
- Alpine `3.24.2`

The MinIO CI build also records separate Linux AMD64 digests for Go and
Alpine. Tags remain in every reference for operator readability, while the
digest is the authoritative image identity. The verifier ties the Rust image
to `rust-toolchain.toml` and the Node image to `ui/package.json` so those pins
cannot drift independently.

## Source and native inputs

- MinIO: `RELEASE.2025-10-15T17-29-55Z`, revision
  `9e49d5e7a648f00e26f2246f4dc28e6b07f8c84a`, checksum-pinned source archive
- mc: `RELEASE.2025-08-13T08-35-41Z`, revision
  `7394ce0dd2a80935aded936b09fa12cbb3cb8096`, checksum-pinned source archive
- Microsandbox: `0.7.6`, revision
  `09df3d4b9d832adaede1fb9a198cfc660bfab8cd`, release asset ID, platform,
  firmware name, URL, and SHA-256
- Octa: `0.4.0`, Linux ARM64 release asset SHA-256, source revision
  `9c917a987c00c61b5c2d8bf6aecdcc66483f38f3`, and a separately
  checksum-pinned source archive for the container builder
- OctaCity Agent: `0.1.0`, macOS ARM64, with an exact lowercase Git revision
  required when the native bundle is staged

MinIO and mc are built from the verified archives by
`tools/build_pinned_minio.py`. Before downloading, the builder resolves each
official release tag and requires its peeled Git revision to equal the pinned
revision. Downloads, archive members, individual files, and total expanded
bytes use manifest-owned limits. The runtime image does not depend on the
discontinued `bitnamilegacy/minio` image.

## Verification

The following checks passed:

```console
python3 -m unittest \
  tools.tests.test_verify_local_stand_inputs \
  tools.tests.test_build_pinned_minio \
  tools.tests.test_verify_local_stand_context
# Ran 36 tests ... OK

python3 tools/verify_local_stand_inputs.py \
  deployment/local-stand/inputs.json \
  --repository . \
  --agent-revision "$(git rev-parse HEAD)"
# validated immutable local-stand inputs
```

The verifier and its mutation tests cover exact key sets, required roles,
tag-plus-digest OCI syntax, platform identity, SHA-256 and Git revision
formats, release URL derivation, release-name/version agreement, and drift
against repository-owned Rust, Node, Agent, Microsandbox, and Octa versions.
The MinIO build tests additionally require digest-qualified builder/runtime
images, upstream release-tag resolution, bounded regular-file-only archive
extraction, correct target architecture, and source-specific OCI labels.

The real Linux ARM64 image was built with OrbStack and reported:

```text
minio version RELEASE.2025-10-15T17-29-55Z
commit-id=9e49d5e7a648f00e26f2246f4dc28e6b07f8c84a

mc version RELEASE.2025-08-13T08-35-41Z
commit-id=7394ce0dd2a80935aded936b09fa12cbb3cb8096
```

Its health endpoint, idempotent bucket creation, and bucket inspection also
passed against a disposable container.

## Dependency compatibility

Rust's locked workspace is current for Rust `1.99.0`; the remaining newer
transitive releases are constrained by direct dependants rather than stale
root requirements. Formatting, Clippy with warnings denied, and the complete
workspace test suite pass.

The UI uses the newest releases admitted by its supply-chain quarantine and
peer constraints. TypeScript remains at `5.9.3`: `openapi-typescript 7.13.0`
requires TypeScript `^5.x`; TypeScript `6.0.3` therefore fails the peer check,
while `7.0.2` is also outside `typescript-eslint 8.71.0`'s supported range and
causes an ESLint runtime failure. The locked UI passed peer validation,
formatting, linting, type checking, 71 unit tests, and dependency audit.
