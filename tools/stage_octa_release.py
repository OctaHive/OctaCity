#!/usr/bin/env python3
"""Download, verify, and safely extract one pinned Octa release bundle."""

from __future__ import annotations

import argparse
import hashlib
import io
import json
import os
from pathlib import Path, PurePosixPath
import shutil
import stat
import tarfile
import tempfile
import time
import urllib.error
import urllib.request
import zipfile


ASSETS = {
    "linux-amd64": ("octa-Linux-amd64.tar.gz", "linux-x86_64"),
    "linux-arm64": ("octa-Linux-arm64.tar.gz", "linux-aarch64"),
    "macos-arm64": ("octa-Darwin-aarch64.tar.gz", "macos-aarch64"),
    "macos-amd64": ("octa-Darwin-x86_64.tar.gz", "macos-x86_64"),
    "windows-amd64": ("octa-Windows-amd64.zip", "windows-x86_64"),
}
DOWNLOAD_ATTEMPTS = 5
MAX_ARCHIVE_BYTES = 512 * 1024 * 1024
MAX_EXTRACTED_BYTES = 1024 * 1024 * 1024
MAX_FILES = 4096


def download(url: str) -> bytes:
    """Download a bounded response with retry for transient release-host failures."""

    last: Exception | None = None
    for attempt in range(DOWNLOAD_ATTEMPTS):
        try:
            request = urllib.request.Request(url, headers={"User-Agent": "octacity-release-gate"})
            with urllib.request.urlopen(request, timeout=60) as response:
                length = response.headers.get("Content-Length")
                if length is not None and int(length) > MAX_ARCHIVE_BYTES:
                    raise ValueError(f"download exceeds {MAX_ARCHIVE_BYTES} bytes")
                body = response.read(MAX_ARCHIVE_BYTES + 1)
                if len(body) > MAX_ARCHIVE_BYTES:
                    raise ValueError(f"download exceeds {MAX_ARCHIVE_BYTES} bytes")
                return body
        except (OSError, urllib.error.HTTPError, urllib.error.URLError, ValueError) as error:
            last = error
            if attempt + 1 < DOWNLOAD_ATTEMPTS:
                time.sleep(2**attempt)
    raise RuntimeError(f"failed to download {url} after {DOWNLOAD_ATTEMPTS} attempts: {last}")


def expected_digest(document: bytes, asset: str) -> str:
    """Parse the single canonical checksum entry accompanying an Octa asset."""

    try:
        fields = document.decode("utf-8").strip().split()
    except UnicodeDecodeError as error:
        raise ValueError("Octa checksum is not UTF-8") from error
    if len(fields) != 2 or fields[1].lstrip("*") != asset:
        raise ValueError("Octa checksum does not identify the requested asset")
    digest = fields[0]
    if len(digest) != 64 or any(character not in "0123456789abcdef" for character in digest):
        raise ValueError("Octa checksum is not a lowercase SHA-256 digest")
    return digest


def checked_relative(name: str) -> Path:
    """Return an archive member path only when it stays below the destination."""

    path = PurePosixPath(name.replace("\\", "/"))
    if path.is_absolute() or any(part in {"", ".", ".."} for part in path.parts):
        raise ValueError(f"unsafe Octa archive member: {name!r}")
    return Path(*path.parts)


def validate_extracted_size(sizes: list[int]) -> None:
    """Reject archives whose declared regular-file payload exceeds the staging budget."""

    total = 0
    for size in sizes:
        total += size
        if total > MAX_EXTRACTED_BYTES:
            raise ValueError(f"Octa archive expands beyond {MAX_EXTRACTED_BYTES} bytes")


def extract_tar(payload: bytes, destination: Path) -> None:
    """Extract only regular files and directories from a gzip tar bundle."""

    with tarfile.open(fileobj=io.BytesIO(payload), mode="r:gz") as archive:
        members = archive.getmembers()
        if len(members) > MAX_FILES:
            raise ValueError("Octa archive contains too many entries")
        validate_extracted_size([member.size for member in members if member.isfile()])
        for member in members:
            relative = checked_relative(member.name)
            target = destination / relative
            if member.isdir():
                target.mkdir(parents=True, exist_ok=True)
                continue
            if not member.isfile():
                raise ValueError(f"unsupported Octa archive member type: {member.name!r}")
            target.parent.mkdir(parents=True, exist_ok=True)
            source = archive.extractfile(member)
            if source is None:
                raise ValueError(f"Octa archive member has no body: {member.name!r}")
            with source, target.open("wb") as output:
                shutil.copyfileobj(source, output)
            os.chmod(target, member.mode & 0o777)


def extract_zip(payload: bytes, destination: Path) -> None:
    """Extract regular Windows release files without trusting archive paths."""

    with zipfile.ZipFile(io.BytesIO(payload)) as archive:
        members = archive.infolist()
        if len(members) > MAX_FILES:
            raise ValueError("Octa archive contains too many entries")
        validate_extracted_size([member.file_size for member in members if not member.is_dir()])
        for member in members:
            relative = checked_relative(member.filename)
            target = destination / relative
            mode = member.external_attr >> 16
            if stat.S_ISLNK(mode):
                raise ValueError(f"Octa archive contains a symbolic link: {member.filename!r}")
            if member.is_dir():
                target.mkdir(parents=True, exist_ok=True)
                continue
            target.parent.mkdir(parents=True, exist_ok=True)
            with archive.open(member) as source, target.open("wb") as output:
                shutil.copyfileobj(source, output)


def stage(version: str, platform: str, revision: str, destination: Path) -> Path:
    """Stage one verified immutable Octa release below a new destination."""

    asset, runtime_platform = ASSETS[platform]
    base = f"https://github.com/OctaHive/octa/releases/download/v{version}"
    payload = download(f"{base}/{asset}")
    checksum = download(f"{base}/{asset}.sha256")
    actual = hashlib.sha256(payload).hexdigest()
    expected = expected_digest(checksum, asset)
    if actual != expected:
        raise ValueError(f"Octa release checksum mismatch: expected {expected}, got {actual}")
    if destination.exists():
        raise ValueError(f"Octa release destination already exists: {destination}")
    destination.parent.mkdir(parents=True, exist_ok=True)
    temporary = Path(tempfile.mkdtemp(prefix="octa-release-", dir=destination.parent))
    try:
        if asset.endswith(".zip"):
            extract_zip(payload, temporary)
        else:
            extract_tar(payload, temporary)
        capabilities = temporary / "octa-runner-capabilities.json"
        if not capabilities.is_file():
            raise ValueError("Octa release has no runner capability manifest")
        # The release harness performs the full manifest and checksum audit.
        # Keep the staging boundary limited to archive identity and exact source.
        document = json.loads(capabilities.read_text(encoding="utf-8"))
        expected_metadata = {
            "octa_version": version,
            "build_commit": revision,
            "platform": runtime_platform,
        }
        actual_metadata = {name: document.get(name) for name in expected_metadata}
        if actual_metadata != expected_metadata:
            raise ValueError(
                f"Octa release metadata mismatch: expected {expected_metadata}, got {actual_metadata}"
            )
        temporary.replace(destination)
    except Exception:
        shutil.rmtree(temporary, ignore_errors=True)
        raise
    return destination.resolve()


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--version", required=True)
    parser.add_argument("--revision", required=True)
    parser.add_argument("--platform", required=True, choices=sorted(ASSETS))
    parser.add_argument("--output", required=True, type=Path)
    return parser.parse_args()


if __name__ == "__main__":
    arguments = parse_args()
    print(stage(arguments.version, arguments.platform, arguments.revision, arguments.output))
