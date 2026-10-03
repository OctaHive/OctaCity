#!/usr/bin/env python3
"""Build and verify the pinned local-stand UI gateway image."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import subprocess
import tempfile
from typing import Any

from local_stand_build_inputs import (
    CONTAINER_PLATFORM,
    LocalStandBuildError,
    git_revision,
    load_build_inputs,
    stage_octacity_source,
    stage_octa_source,
    ui_toolchain,
    workspace_version,
)
from pinned_source_archive import SourceArchiveError
import verify_local_stand_inputs as inputs


REPOSITORY = Path(__file__).resolve().parents[1]
DEFAULT_MANIFEST = REPOSITORY / "deployment/local-stand/inputs.json"
DOCKERFILE = REPOSITORY / "deployment/local-stand/server.Dockerfile"
STATIC_ROOT = "/srv/octacity-ui"


def docker_arguments(
    document: dict[str, Any],
    image: str,
    octa_source: Path,
    octacity_source: Path,
    revision: str,
) -> list[str]:
    """Create the deterministic BuildKit command for the gateway target."""

    node_version, pnpm_version = ui_toolchain(octacity_source)
    node_image = document["images"]["node"]["reference"]
    nginx_image = document["images"]["nginx"]["reference"]
    return [
        "docker",
        "build",
        "--platform",
        CONTAINER_PLATFORM,
        "--file",
        str(octacity_source / "deployment/local-stand/server.Dockerfile"),
        "--target",
        "gateway",
        "--tag",
        image,
        "--build-context",
        f"octa-source={octa_source.resolve()}",
        "--build-arg",
        f"RUST_IMAGE={document['images']['rust']['reference']}",
        "--build-arg",
        f"NODE_IMAGE={node_image}",
        "--build-arg",
        f"NGINX_IMAGE={nginx_image}",
        "--build-arg",
        f"OCTACITY_VERSION={workspace_version(octacity_source)}",
        "--build-arg",
        f"OCTACITY_REVISION={revision}",
        "--build-arg",
        f"NODE_VERSION={node_version}",
        "--build-arg",
        f"PNPM_VERSION={pnpm_version}",
        str(octacity_source),
    ]


def runtime_contract_command(image: str) -> list[str]:
    """Return the isolated runtime filesystem and toolchain check."""

    contract = f"""
test -f {STATIC_ROOT}/index.html
test -d {STATIC_ROOT}/assets
test ! -e /workspace
test ! -e /pnpm
test ! -e /root/.cache/node/corepack
test ! -e /root/.local/share/pnpm
test ! -e {STATIC_ROOT}/src
test ! -e {STATIC_ROOT}/package.json
test ! -e {STATIC_ROOT}/pnpm-lock.yaml
! command -v node
! command -v pnpm
! command -v vite
nginx
attempt=0
until wget -q -O /tmp/index.html http://127.0.0.1:8080/; do
  attempt=$((attempt + 1))
  test "$attempt" -lt 20
  sleep 0.1
done
cmp /tmp/index.html {STATIC_ROOT}/index.html
if wget -q -O /tmp/api-response http://127.0.0.1:8080/api/v1/unknown; then
  exit 1
fi
if wget -q -O /tmp/health-response http://127.0.0.1:8080/health/unknown; then
  exit 1
fi
nginx -s quit
""".strip()
    return [
        "docker",
        "run",
        "--rm",
        "--platform",
        CONTAINER_PLATFORM,
        "--network",
        "none",
        "--read-only",
        "--tmpfs",
        "/tmp:rw,noexec,nosuid,nodev,size=16m",
        "--entrypoint",
        "/bin/sh",
        image,
        "-ec",
        contract,
    ]


def verify_image(
    image: str,
    expected_version: str,
    expected_revision: str,
    node_version: str,
    pnpm_version: str,
) -> None:
    """Verify pinned metadata and the minimal static-only runtime contents."""

    inspected = subprocess.run(
        ["docker", "image", "inspect", "--format", "{{json .Config}}", image],
        check=True,
        capture_output=True,
        text=True,
    )
    configuration = json.loads(inspected.stdout)
    if configuration.get("User") != "101:101":
        raise LocalStandBuildError("gateway image must run as nginx uid:gid 101:101")
    labels = configuration.get("Labels") or {}
    expected = {
        "org.opencontainers.image.version": expected_version,
        "org.opencontainers.image.revision": expected_revision,
        "dev.octacity.node.version": node_version,
        "dev.octacity.pnpm.version": pnpm_version,
    }
    if any(labels.get(name) != value for name, value in expected.items()):
        raise LocalStandBuildError("gateway image toolchain labels are invalid")
    subprocess.run(runtime_contract_command(image), check=True)


def build(image: str, manifest: Path, repository: Path) -> None:
    """Stage verified source, build the image, and run its contract checks."""

    document = load_build_inputs(manifest, repository)
    revision = git_revision(repository)
    with tempfile.TemporaryDirectory(prefix="octacity-build-sources-") as temporary:
        root = Path(temporary)
        octa_source = root / "octa"
        octacity_source = root / "octacity"
        stage_octa_source(document, octa_source)
        stage_octacity_source(repository, octacity_source, revision)
        expected_version = workspace_version(octacity_source)
        node_version, pnpm_version = ui_toolchain(octacity_source)
        subprocess.run(
            docker_arguments(
                document,
                image,
                octa_source,
                octacity_source,
                revision,
            ),
            check=True,
        )
    verify_image(
        image,
        expected_version,
        revision,
        node_version,
        pnpm_version,
    )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("image")
    parser.add_argument("--manifest", type=Path, default=DEFAULT_MANIFEST)
    parser.add_argument("--repository", type=Path, default=REPOSITORY)
    arguments = parser.parse_args()
    try:
        build(
            arguments.image,
            arguments.manifest.resolve(),
            arguments.repository.resolve(),
        )
    except (
        OSError,
        KeyError,
        TypeError,
        json.JSONDecodeError,
        inputs.InputError,
        LocalStandBuildError,
        SourceArchiveError,
        subprocess.CalledProcessError,
    ) as error:
        parser.error(str(error))
    print(f"built and verified local-stand gateway image: {arguments.image}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
