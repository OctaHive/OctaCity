#!/usr/bin/env python3
"""Build MinIO or mc from checksum-verified, revision-pinned sources."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import platform
import re
import shutil
import subprocess
import sys
import tempfile
from typing import Any

from pinned_source_archive import SourceArchiveError as BuildError
from pinned_source_archive import download, extract_source, verify_release_ref


REPOSITORY = Path(__file__).resolve().parents[1]
DEFAULT_MANIFEST = REPOSITORY / "deployment/local-stand/inputs.json"
DOCKERFILE = REPOSITORY / "deployment/local-stand/minio.Dockerfile"
OBJECT_STORE_INITIALIZER = (
    REPOSITORY / "deployment/local-stand/object-store-init.sh"
)
RELEASE_TAG = re.compile(
    r"RELEASE\.(?P<date>[0-9]{4}-[0-9]{2}-[0-9]{2})"
    r"T(?P<time>[0-9]{2}-[0-9]{2}-[0-9]{2})Z"
)


def release_version(tag: str) -> str:
    """Convert a validated MinIO release tag to its embedded RFC 3339 version."""

    match = RELEASE_TAG.fullmatch(tag)
    if match is None:
        raise BuildError(f"invalid MinIO release tag: {tag}")
    clock = match.group("time").replace("-", ":")
    return f'{match.group("date")}T{clock}Z'


def release_year(tag: str) -> str:
    """Return the copyright year encoded by a validated release tag."""

    return release_version(tag)[:4]


def host_architecture() -> str:
    """Return the manifest architecture for the current Docker host."""

    architecture = platform.machine().lower()
    if architecture in {"arm64", "aarch64"}:
        return "arm64"
    if architecture in {"x86_64", "amd64"}:
        return "amd64"
    raise BuildError(f"unsupported Docker host architecture: {architecture}")


def local_image_name(document: dict[str, Any], target: str) -> str:
    """Return the Compose image name derived from one pinned source identity."""

    source_name = {"minio": "minio", "mc": "minio_client"}.get(target)
    if source_name is None:
        raise BuildError(f"unsupported MinIO build target: {target}")
    source = document["sources"][source_name]
    date = release_version(source["tag"]).split("T", 1)[0]
    return f"octacity/{target}:release-{date}-{source['source_revision'][:12]}"


def manifest_images(document: dict[str, Any], architecture: str) -> dict[str, str]:
    """Select the digest-qualified build images for one supported architecture."""

    image_set = "images" if architecture == "arm64" else "ci_images"
    expected_platform = f"linux/{architecture}"
    try:
        images = document[image_set]
        selected = {role: images[role] for role in ("go", "alpine")}
    except (KeyError, TypeError) as error:
        raise BuildError(f"manifest has no complete {image_set} build image set") from error
    for role, image in selected.items():
        if image.get("platform") != expected_platform:
            raise BuildError(f"{image_set}.{role} does not target {expected_platform}")
        reference = image.get("reference")
        if not isinstance(reference, str) or "@sha256:" not in reference:
            raise BuildError(f"{image_set}.{role} is not digest-qualified")
    return {role: image["reference"] for role, image in selected.items()}


def docker_arguments(
    document: dict[str, Any], target: str, image: str, architecture: str, context: Path
) -> list[str]:
    """Create the deterministic Docker build invocation for pinned sources."""

    images = manifest_images(document, architecture)
    minio = document["sources"]["minio"]
    mc = document["sources"]["minio_client"]
    arguments = [
        "docker",
        "build",
        "--platform",
        f"linux/{architecture}",
        "--file",
        str(DOCKERFILE),
        "--target",
        target,
        "--tag",
        image,
    ]
    build_arguments = {
        "GO_IMAGE": images["go"],
        "RUNTIME_IMAGE": images["alpine"],
        "MINIO_REVISION": minio["source_revision"],
        "MINIO_SHORT_REVISION": minio["source_revision"][:12],
        "MINIO_VERSION": release_version(minio["tag"]),
        "MINIO_RELEASE_TAG": minio["tag"],
        "MINIO_COPYRIGHT_YEAR": release_year(minio["tag"]),
        "MC_REVISION": mc["source_revision"],
        "MC_SHORT_REVISION": mc["source_revision"][:12],
        "MC_VERSION": release_version(mc["tag"]),
        "MC_RELEASE_TAG": mc["tag"],
        "MC_COPYRIGHT_YEAR": release_year(mc["tag"]),
    }
    for name, value in build_arguments.items():
        arguments.extend(("--build-arg", f"{name}={value}"))
    labeled_source = minio if target == "minio" else mc
    arguments.extend(
        (
            "--label",
            f"org.opencontainers.image.revision={labeled_source['source_revision']}",
            "--label",
            f"org.opencontainers.image.version={labeled_source['tag']}",
            str(context),
        )
    )
    return arguments


def image_is_current(
    document: dict[str, Any], target: str, image: str, architecture: str
) -> bool:
    """Return whether one local image has the exact pinned target identity."""

    source_name = {"minio": "minio", "mc": "minio_client"}.get(target)
    if source_name is None:
        raise BuildError(f"unsupported MinIO build target: {target}")
    try:
        inspected = subprocess.run(
            ["docker", "image", "inspect", "--format", "{{json .}}", image],
            check=True,
            capture_output=True,
            text=True,
        )
        metadata = json.loads(inspected.stdout)
    except (OSError, subprocess.CalledProcessError, json.JSONDecodeError):
        return False
    source = document["sources"][source_name]
    labels = (metadata.get("Config") or {}).get("Labels") or {}
    return (
        metadata.get("Architecture") == architecture
        and labels.get("org.opencontainers.image.revision") == source["source_revision"]
        and labels.get("org.opencontainers.image.version") == source["tag"]
    )


def build_targets(
    manifest: Path, targets: dict[str, str], architecture: str
) -> None:
    """Stage pinned sources once and build every requested target from them."""

    try:
        document = json.loads(manifest.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
        raise BuildError(f"cannot load {manifest}: {error}") from error
    with tempfile.TemporaryDirectory(prefix="octacity-minio-build-") as temporary:
        context = Path(temporary)
        shutil.copyfile(OBJECT_STORE_INITIALIZER, context / OBJECT_STORE_INITIALIZER.name)
        for role, directory in (("minio", "minio-source"), ("minio_client", "mc-source")):
            source = document["sources"][role]
            verify_release_ref(source)
            archive = context / f"{role}.tar.gz"
            download(source, archive)
            extract_source(archive, context / directory, source)
            archive.unlink()
        for target, image in targets.items():
            subprocess.run(
                docker_arguments(document, target, image, architecture, context),
                check=True,
            )


def build(manifest: Path, target: str, image: str, architecture: str) -> None:
    """Build one target through the shared multi-target staging path."""

    build_targets(manifest, {target: image}, architecture)


def parse_args() -> argparse.Namespace:
    """Parse command-line arguments."""

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("target", choices=("minio", "mc"))
    parser.add_argument("image", help="local image name and tag")
    parser.add_argument("--manifest", type=Path, default=DEFAULT_MANIFEST)
    parser.add_argument("--architecture", choices=("amd64", "arm64"))
    return parser.parse_args()


def main() -> int:
    """Build one pinned image and present failures without a traceback."""

    arguments = parse_args()
    try:
        architecture = arguments.architecture or host_architecture()
        build(arguments.manifest, arguments.target, arguments.image, architecture)
    except (BuildError, KeyError, TypeError, subprocess.CalledProcessError) as error:
        print(f"build pinned MinIO: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
