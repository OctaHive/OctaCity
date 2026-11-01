# Microsandbox guest file-mount repair

This directory pins the common guest runtime used for selected-source Jobs.
It is independent of Factory business definitions and does not change source
selection, permissions, JobSpec signing or the host Microsandbox version.

## Reason and scope

Microsandbox 0.7.6 opens an existing file bind target for writing before mounting
the supplied file. An individually writable file inside a read-only source
tree therefore fails during guest initialization with `EROFS`.
`readonly-file-mount.patch` leaves existing regular targets unopened, creates
only missing targets and rejects symlinks/non-regular targets. Mount ordering
and flags remain upstream behavior. The parent directory gains no write access.

The upstream source is Microsandbox v0.7.6, commit
`09df3d4b9d832adaede1fb9a198cfc660bfab8cd`, under its Apache-2.0 license.
The downloaded source retains its license and notices. `inputs.json` records
the bounded source archive SHA-256 and platform-specific builder image digests.
The guest-only `Cargo.lock` retains upstream package versions, sources and
checksums; unrelated workspace members are excluded from this guest build.
The OctaCity workspace lockfile and upstream release downloads are unchanged.

## Build and install

From the repository root, on Linux or macOS with Python 3, Git and Docker:

```sh
python3 tools/stage_microsandbox_agentd.py --repaired \
  --architecture aarch64 --output target/microsandbox-agentd/agentd-aarch64
```

Use `--architecture x86_64` and a distinct output for Linux x86-64. Cross builds
need Docker's corresponding platform support. The builder uses static musl,
`cargo build --locked`, a pinned compiler image and caller-owned temporary
files. `CARGO_BUILD_JOBS` defaults to two. Source and Cargo build directories are
removed on completion/failure. Existing output with the exact qualified digest
is reused without downloads or compilation. Unqualified output is replaced
only after a successful, checksum-verified build.

`SHA256SUMS` is the shared artifact identity for staging and production Agent
registration. An output mismatch is an error, not a reason to update a pin
automatically. Updating a source, patch, lockfile or builder requires rebuilding,
reviewing the output and rerunning the physical backend contracts before
changing the qualified pins.

The common local-stand launcher builds/reuses this payload, installs it in a
separate immutable native installation and retains its digest in the existing
installation manifest. The generated execution environment identity includes
the full digest, partitioning execution/cache identity from the released guest.
Startup validates the retained bytes and explicitly supplies `MSB_AGENTD_PATH`
to preflight and the Agent process; it does not trust an ambient override.
Older installations remain independently addressable.

## Qualification and rollback

The Microsandbox adapter checks the actual SDK-selected guest bytes against
the architecture-specific pin before advertising `source_visibility_v1` and
again before starting a selected-source VM. Missing or changed payloads do not
qualify. Protocol version support alone does not grant selected visibility.
Containerd and Windows qualification is separate.

With the existing backend-contract environment and local guest image prepared:

```sh
export MSB_AGENTD_PATH="$PWD/target/microsandbox-agentd/agentd-aarch64"
cargo test --locked -p octacity-job --test backend_contract selected_ -- \
  --ignored --test-threads=1
cargo test --locked -p octacity-job --test backend_contract \
  source_free_ordinary_job_runs_retries_and_cancels_on_microsandbox -- --ignored
cargo test --locked -p octacity-agent --bin octacity-agent \
  qualified_guest_enables_selected_source_in_production_registration -- --ignored
```

These contracts exercise hidden paths/lines, read-only access, separately
writable directories/files, retained authorized writes, retry, cancellation
and cleanup. Both Linux and macOS Microsandbox CI jobs select the repair and
run the selected-source contracts as required checks. Local Apple Silicon VM
execution and both architecture builds were verified during implementation;
an x86-64 cross build is not evidence of local x86-64 VM execution.

For rollback, select the prior verified native installation through the
existing installation lifecycle. Do not copy a released guest over a pinned
installation or grant write access to a parent directory. An older Agent or
runtime without this qualification remains ineligible for selected-source
Jobs; retained signed scopes are not rewritten.
