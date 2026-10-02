# Bounded build and staging contexts

## Result

**PASS:** the root Docker build context is default-deny and the native staging
boundary accepts only the three declared immutable bundles and the two required
checksum sidecars. Git metadata,
Rust and Node build products, generated credentials, mutable local-stand state,
and unrelated repository or developer files remain outside both boundaries.

Verified on `2026-10-03`.

## Docker build context

The root `.dockerignore` begins with `**` and reopens only:

- workspace lockfile, manifest, pinned Rust toolchain, and license;
- production Rust manifests, build scripts, sources, and embedded PostgreSQL
  migrations below `agent/`, `server/`, and `shared/`;
- the locked UI inputs and generator script required to derive the ignored
  OpenAPI schema inside the TypeScript and Vite builder;
- repository-owned `deployment/local-stand` Dockerfiles and JSON policies.

Every reopened directory level is denied again before individual inputs are
allowed. Final rules unconditionally exclude `.git`, Cargo targets,
`node_modules`, generated UI schemas, UI distributions, pnpm stores, Python
caches, `.env` files, private keys, secret directories, IDE metadata, and
generated or durable local stand state.

The context was materialized through the OrbStack Docker engine with a scratch
`COPY . /` build. Docker exported 669 files (approximately 5.5 MiB). The
repository verifier inspected that actual output and found every required
input and no excluded or unsafe entry:

```console
python3 tools/verify_local_stand_context.py \
  --materialized-context <docker-local-output>
# validated bounded local-stand build and staging contexts
```

This real-engine check caught and corrected an earlier rule-ordering issue in
which reopening a directory also admitted unrelated files at that level.

## Native staging allowlist

`deployment/local-stand/staging-allowlist.json` accepts exactly:

- `octacity-agent-macos-arm64.tar.gz`, whose release manifest includes the
  native Agent and source plugin;
- `octa-linux-arm64-v0.4.0.tar.gz`;
- `microsandbox-darwin-aarch64.tar.gz`.

The Agent and Octa archives each require a filename-bound SHA-256 sidecar.
Microsandbox bytes must match the digest in `inputs.json`. The validator also
checks the Agent release manifest against the staged OctaCity/Octa revisions
and checks Octa runner capabilities against its pinned version, revision, and
Linux ARM64 platform. Arbitrary fixture bytes are therefore rejected.

Filenames and manifest entries are derived from and checked against
`deployment/local-stand/inputs.json`. Each role has explicit archive,
expanded-byte, and member-count limits. A staged directory must contain the
complete exact set as regular files: missing, additional, symbolic,
non-regular, oversized, checksum-mismatched, and identity-mismatched entries
fail validation.

## Automated checks

```console
python3 -m unittest \
  tools.tests.test_verify_local_stand_inputs \
  tools.tests.test_build_pinned_minio \
  tools.tests.test_verify_local_stand_context \
  tools.tests.test_package_release
# See immutable-input evidence for the current targeted test count.

python3 -m compileall -q \
  tools/verify_local_stand_context.py \
  tools/tests/test_verify_local_stand_context.py

git diff --check
```

The context-specific suite covers required source and lockfile inclusion,
every prohibited path class, broad-rule drift, manifest coupling, complete
staging acceptance, and rejection of unknown, missing, symbolic, oversized,
checksum-mismatched, and identity-mismatched native inputs. `.dockerignore`
is the sole complete rule source; the verifier retains only mandatory security
invariants and representative required/forbidden paths.
