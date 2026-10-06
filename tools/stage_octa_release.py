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

import validate_octa_release_contract as release_contract


ASSETS = {
    "linux-amd64": ("octa-Linux-amd64.tar.gz", "linux-x86_64"),
    "linux-arm64": ("octa-Linux-arm64.tar.gz", "linux-aarch64"),
    "macos-arm64": ("octa-Darwin-aarch64.tar.gz", "macos-aarch64"),
    "macos-amd64": ("octa-Darwin-x86_64.tar.gz", "macos-x86_64"),
    "windows-amd64": ("octa-Windows-amd64.zip", "windows-x86_64"),
}
DOWNLOAD_ATTEMPTS = 5
MAX_ARCHIVE_BYTES = 512 * 1024 * 1024
MAX_RELEASE_METADATA_BYTES = 1024 * 1024
MAX_EXTRACTED_BYTES = 1024 * 1024 * 1024
MAX_FILES = 4096
RELEASE_API = "https://api.github.com/repos/OctaHive/octa/releases"
RELEASE_ASSET_API = f"{RELEASE_API}/assets/"


def download(
    url: str,
    *,
    headers: dict[str, str] | None = None,
    max_bytes: int = MAX_ARCHIVE_BYTES,
) -> bytes:
    """Download a bounded response with retry for transient release-host failures."""

    last: Exception | None = None
    for attempt in range(DOWNLOAD_ATTEMPTS):
        try:
            request_headers = {"User-Agent": "octacity-release-gate", **(headers or {})}
            request = urllib.request.Request(url, headers=request_headers)
            with urllib.request.urlopen(request, timeout=60) as response:
                length = response.headers.get("Content-Length")
                if length is not None and int(length) > max_bytes:
                    raise ValueError(f"download exceeds {max_bytes} bytes")
                body = response.read(max_bytes + 1)
                if len(body) > max_bytes:
                    raise ValueError(f"download exceeds {max_bytes} bytes")
                return body
        except urllib.error.HTTPError as error:
            # HTTPError owns the response stream.  Retain only its diagnostic
            # before retrying so failed release-host responses cannot leak FDs.
            last = RuntimeError(str(error))
            error.close()
        except (OSError, urllib.error.URLError, ValueError) as error:
            last = error
        if attempt + 1 < DOWNLOAD_ATTEMPTS:
            time.sleep(2**attempt)
    raise RuntimeError(f"failed to download {url} after {DOWNLOAD_ATTEMPTS} attempts: {last}")


def github_api_headers(accept: str) -> dict[str, str]:
    """Return GitHub API headers, authenticating metadata requests when available."""

    headers = {
        "Accept": accept,
        "X-GitHub-Api-Version": "2022-11-28",
    }
    token = os.environ.get("GITHUB_TOKEN")
    if token:
        headers["Authorization"] = f"Bearer {token}"
    return headers


def release_asset_urls(version: str, required_names: tuple[str, ...]) -> dict[str, str]:
    """Resolve exact uploaded assets from the requested GitHub release tag."""

    metadata = download(
        f"{RELEASE_API}/tags/v{version}",
        headers=github_api_headers("application/vnd.github+json"),
        max_bytes=MAX_RELEASE_METADATA_BYTES,
    )
    try:
        document = json.loads(metadata.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise ValueError("GitHub release metadata is not valid UTF-8 JSON") from error
    if not isinstance(document, dict) or document.get("tag_name") != f"v{version}":
        raise ValueError("GitHub release metadata does not identify the requested tag")
    assets = document.get("assets")
    if not isinstance(assets, list):
        raise ValueError("GitHub release metadata has no asset list")

    required = set(required_names)
    resolved: dict[str, str] = {}
    for candidate in assets:
        if not isinstance(candidate, dict) or candidate.get("name") not in required:
            continue
        name = candidate["name"]
        url = candidate.get("url")
        asset_id = url.removeprefix(RELEASE_ASSET_API) if isinstance(url, str) else ""
        if candidate.get("state") != "uploaded" or not asset_id.isdecimal():
            raise ValueError(f"GitHub release asset is not an exact uploaded API asset: {name}")
        if name in resolved:
            raise ValueError(f"GitHub release contains duplicate asset: {name}")
        resolved[name] = url

    missing = required - resolved.keys()
    if missing:
        raise ValueError(f"GitHub release is missing required assets: {', '.join(sorted(missing))}")
    return resolved


def download_release_assets(version: str, names: tuple[str, ...]) -> dict[str, bytes]:
    """Download exact release assets, using the API route after direct-host failure."""

    base = f"https://github.com/OctaHive/octa/releases/download/v{version}"
    fallback_urls: dict[str, str] | None = None
    payloads: dict[str, bytes] = {}
    for name in names:
        direct_failure: RuntimeError | None = None
        try:
            payloads[name] = download(f"{base}/{name}")
            continue
        except RuntimeError as error:
            direct_failure = error
            if fallback_urls is None:
                try:
                    fallback_urls = release_asset_urls(version, names)
                except (RuntimeError, ValueError) as resolution_error:
                    raise RuntimeError(
                        f"direct release download failed ({direct_failure}); "
                        f"GitHub API resolution failed ({resolution_error})"
                    ) from resolution_error
        try:
            payloads[name] = download(
                fallback_urls[name],
                headers={
                    "Accept": "application/octet-stream",
                    "X-GitHub-Api-Version": "2022-11-28",
                },
            )
        except RuntimeError as fallback_error:
            raise RuntimeError(
                f"direct release download failed ({direct_failure}); "
                f"GitHub API asset download failed ({fallback_error})"
            ) from fallback_error
    return payloads


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
    payloads = download_release_assets(version, (asset, f"{asset}.sha256"))
    payload = payloads[asset]
    checksum = payloads[f"{asset}.sha256"]
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
        contract = temporary / "octa-release-contract.json"
        release_contract.validate(contract)
        release_contract.validate_codex_compatibility(
            temporary / "codex-compatibility.json", version
        )
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
