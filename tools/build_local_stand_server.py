#!/usr/bin/env python3
"""Build and verify the pinned local-stand server image."""

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
    octa_source_policy,
    stage_octacity_source,
    stage_octa_source,
    workspace_version,
)
from pinned_source_archive import SourceArchiveError
import verify_local_stand_inputs as inputs


REPOSITORY = Path(__file__).resolve().parents[1]
DEFAULT_MANIFEST = REPOSITORY / "deployment/local-stand/inputs.json"
DEFAULT_FIXTURE = REPOSITORY / "deployment/local-stand/fixtures/server.toml"


def docker_arguments(
    document: dict[str, Any],
    image: str,
    octa_source: Path,
    octacity_source: Path,
    revision: str,
) -> list[str]:
    """Create the deterministic BuildKit command for the server target."""

    octa = document["native"]["octa"]
    return [
        "docker",
        "build",
        "--platform",
        CONTAINER_PLATFORM,
        "--file",
        str(octacity_source / "deployment/local-stand/server.Dockerfile"),
        "--target",
        "server",
        "--tag",
        image,
        "--build-context",
        f"octa-source={octa_source.resolve()}",
        "--build-arg",
        f"RUST_IMAGE={document['images']['rust']['reference']}",
        "--build-arg",
        f"OCTACITY_VERSION={workspace_version(octacity_source)}",
        "--build-arg",
        f"OCTACITY_REVISION={revision}",
        "--build-arg",
        f"OCTA_REVISION={octa['source_revision']}",
        "--build-arg",
        f"OCTA_SOURCE_SHA256={octa['source_archive_sha256']}",
        str(octacity_source),
    ]


def validate_runtime_user(value: str) -> tuple[int, int]:
    """Require an explicit numeric uid:gid pair that cannot resolve to root."""

    fields = value.split(":")
    if len(fields) != 2 or not all(field.isascii() and field.isdigit() for field in fields):
        raise LocalStandBuildError("server image user must be an explicit numeric uid:gid pair")
    uid, gid = (int(field) for field in fields)
    if uid == 0 or gid == 0:
        raise LocalStandBuildError("server image must not run with a root uid or gid")
    return uid, gid


def verification_commands(image: str, fixture: Path) -> tuple[list[str], list[str]]:
    """Return isolated runtime checks for version and configuration parsing."""

    common = [
        "docker",
        "run",
        "--rm",
        "--platform",
        CONTAINER_PLATFORM,
        "--network",
        "none",
        "--read-only",
    ]
    version = [*common, image, "--version"]
    validate = [
        *common,
        "--mount",
        f"type=bind,src={fixture.resolve()},dst=/fixtures/server.toml,readonly",
        image,
        "validate",
        "--syntax-only",
        "/fixtures/server.toml",
    ]
    return version, validate


def verify_image(
    image: str, fixture: Path, expected_version: str, expected_revision: str
) -> None:
    """Verify non-root metadata and execute both required server commands."""

    inspected = subprocess.run(
        ["docker", "image", "inspect", "--format", "{{json .Config}}", image],
        check=True,
        capture_output=True,
        text=True,
    )
    configuration = json.loads(inspected.stdout)
    validate_runtime_user(configuration.get("User", ""))
    labels = configuration.get("Labels") or {}
    expected_labels = {
        "org.opencontainers.image.version": expected_version,
        "org.opencontainers.image.revision": expected_revision,
    }
    if any(labels.get(name) != value for name, value in expected_labels.items()):
        raise LocalStandBuildError("server image source labels are invalid")
    version, validate = verification_commands(image, fixture)
    result = subprocess.run(version, check=True, capture_output=True, text=True)
    expected = f"octacity-server {expected_version}"
    if result.stdout.strip() != expected:
        raise LocalStandBuildError(
            f"server image reported {result.stdout.strip()!r}, expected {expected!r}"
        )
    result = subprocess.run(validate, check=True, capture_output=True, text=True)
    if "server configuration is valid" not in result.stdout:
        raise LocalStandBuildError("server image did not validate the fixture configuration")


def build(image: str, manifest: Path, fixture: Path, repository: Path) -> None:
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
    verify_image(image, fixture, expected_version, revision)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("image")
    parser.add_argument("--manifest", type=Path, default=DEFAULT_MANIFEST)
    parser.add_argument("--fixture", type=Path, default=DEFAULT_FIXTURE)
    parser.add_argument("--repository", type=Path, default=REPOSITORY)
    arguments = parser.parse_args()
    try:
        build(
            arguments.image,
            arguments.manifest.resolve(),
            arguments.fixture.resolve(),
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
    print(f"built and verified local-stand server image: {arguments.image}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
