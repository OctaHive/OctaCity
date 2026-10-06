"""Shared immutable inputs and source snapshots for local-stand builds."""

from __future__ import annotations

import json
from pathlib import Path
import subprocess
import tempfile
import tomllib
from typing import Any

from pinned_source_archive import SourceArchiveError, download, extract_source
import verify_local_stand_inputs as inputs


CONTAINER_PLATFORM = "linux/arm64"
OCTACITY_SOURCE_LIMITS = {
    "expanded_max_bytes": 1024 * 1024 * 1024,
    "file_max_bytes": 256 * 1024 * 1024,
    "member_max_count": 65536,
}


class LocalStandBuildError(RuntimeError):
    """A local-stand build input or produced image violates its contract."""


def git_revision(repository: Path) -> str:
    """Return the exact committed OctaCity revision used by every build plane."""

    result = subprocess.run(
        ["git", "-C", str(repository), "rev-parse", "HEAD"],
        check=True,
        capture_output=True,
        text=True,
    )
    return inputs.require_match(result.stdout.strip(), inputs.GIT_SHA1, "OctaCity revision")


def stage_octacity_source(repository: Path, destination: Path, revision: str) -> None:
    """Stage one bounded Git snapshot shared by container and native builds."""

    inputs.require_match(revision, inputs.GIT_SHA1, "OctaCity revision")
    with tempfile.TemporaryDirectory(prefix="octacity-source-archive-") as temporary:
        archive = Path(temporary) / "octacity.tar.gz"
        subprocess.run(
            [
                "git",
                "-C",
                str(repository),
                "archive",
                "--format=tar.gz",
                "--prefix=octacity/",
                "--output",
                str(archive),
                revision,
            ],
            check=True,
        )
        extract_source(archive, destination, OCTACITY_SOURCE_LIMITS)
    for required in (
        ".dockerignore",
        "Cargo.toml",
        "Cargo.lock",
        "rust-toolchain.toml",
    ):
        if not (destination / required).is_file():
            raise LocalStandBuildError(
                f"OctaCity snapshot omits required build input: {required}"
            )


def load_build_inputs(path: Path, repository: Path) -> dict[str, Any]:
    """Load and validate every immutable input consumed by stand images."""

    document = inputs.load_manifest(path)
    tags = inputs.validate_images(
        document.get("images"),
        "images",
        inputs.EXPECTED_IMAGE_ROLES,
        CONTAINER_PLATFORM,
    )
    native = inputs.require_object(document.get("native"), "native")
    inputs.validate_octa(inputs.require_object(native.get("octa"), "native.octa"))
    inputs.validate_repository_pins(document, tags, repository)
    return document


def octa_source_policy(document: dict[str, Any]) -> dict[str, Any]:
    """Adapt pinned Octa source metadata to the bounded archive loader."""

    octa = inputs.require_object(
        inputs.require_object(document.get("native"), "native").get("octa"),
        "native.octa",
    )
    inputs.validate_octa(octa)
    return {
        "archive_url": octa["source_archive_url"],
        "sha256": octa["source_archive_sha256"],
        "archive_max_bytes": octa["source_archive_max_bytes"],
        "expanded_max_bytes": octa["source_archive_expanded_max_bytes"],
        "file_max_bytes": octa["source_archive_file_max_bytes"],
        "member_max_count": octa["source_archive_member_max_count"],
        "tag": octa["source_revision"],
    }


def stage_octa_source(document: dict[str, Any], destination: Path) -> None:
    """Download, checksum, and safely extract the exact Octa source revision."""

    policy = octa_source_policy(document)
    with tempfile.TemporaryDirectory(prefix="octacity-octa-download-") as temporary:
        archive = Path(temporary) / "octa-source.tar.gz"
        download(policy, archive)
        extract_source(archive, destination, policy)
    for required in (
        "Cargo.toml",
        "Cargo.lock",
        "LICENSE",
        "crates/octa-runner-protocol/Cargo.toml",
    ):
        if not (destination / required).is_file():
            raise SourceArchiveError(
                f"verified Octa source omits required input: {required}"
            )


def workspace_version(repository: Path) -> str:
    """Return the release version shared by the Rust workspace."""

    workspace = tomllib.loads(
        (repository / "Cargo.toml").read_text(encoding="utf-8")
    )
    return workspace["workspace"]["package"]["version"]


def ui_toolchain(repository: Path) -> tuple[str, str]:
    """Return the Node and pnpm versions from their canonical pins."""

    ui = repository / "ui"
    package = json.loads((ui / "package.json").read_text(encoding="utf-8"))
    node_version = (ui / ".node-version").read_text(encoding="utf-8").strip()
    manager = package.get("packageManager")
    if (
        not node_version
        or not isinstance(manager, str)
        or not manager.startswith("pnpm@")
        or not manager.removeprefix("pnpm@")
    ):
        raise LocalStandBuildError("UI Node and pnpm pins are inconsistent")
    npmrc = (ui / ".npmrc").read_text(encoding="utf-8").splitlines()
    if "ignore-scripts=true" not in npmrc:
        raise LocalStandBuildError("UI dependency scripts must remain disabled")
    return node_version, manager.removeprefix("pnpm@")
