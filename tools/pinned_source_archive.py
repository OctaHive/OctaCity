"""Download and safely extract checksum-pinned source archives."""

from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import shutil
import subprocess
import sys
import tarfile
import time
from typing import Any
from urllib.error import HTTPError, URLError
from urllib.request import Request, urlopen


class SourceArchiveError(RuntimeError):
    """A pinned source archive cannot be staged safely."""


def verify_release_ref(source: dict[str, Any], *, attempts: int = 5) -> None:
    """Require a transient-tolerant Git tag lookup to match one pinned revision."""

    reference = f"refs/tags/{source['tag']}"
    result = None
    for attempt in range(1, attempts + 1):
        try:
            result = subprocess.run(
                [
                    "git",
                    "ls-remote",
                    source["repository"],
                    reference,
                    f"{reference}^{{}}",
                ],
                check=True,
                capture_output=True,
                text=True,
                timeout=60,
            )
            break
        except (
            OSError,
            subprocess.CalledProcessError,
            subprocess.TimeoutExpired,
        ) as error:
            stderr = getattr(error, "stderr", None)
            detail = (
                stderr.strip()
                if isinstance(stderr, str) and stderr.strip()
                else str(error)
            )
            if attempt == attempts:
                raise SourceArchiveError(
                    f"cannot resolve release tag {source['tag']} after "
                    f"{attempts} attempts: {detail}"
                ) from error
            delay = min(2 ** (attempt - 1), 8)
            print(
                f"release tag lookup attempt {attempt}/{attempts} failed: "
                f"{detail}; retrying in {delay}s",
                file=sys.stderr,
            )
            time.sleep(delay)
    if result is None:
        raise SourceArchiveError(f"cannot resolve release tag {source['tag']}")
    resolved: dict[str, str] = {}
    for line in result.stdout.splitlines():
        fields = line.split()
        if len(fields) == 2:
            resolved[fields[1]] = fields[0]
    actual = resolved.get(f"{reference}^{{}}", resolved.get(reference))
    if actual != source["source_revision"]:
        raise SourceArchiveError(
            f"release tag {source['tag']} resolves to {actual or 'nothing'}, "
            f"not {source['source_revision']}"
        )


def verify_github_release_asset(
    source: dict[str, Any],
    *,
    attempts: int = 5,
    maximum_bytes: int = 1024 * 1024,
    github_token: str | None = None,
) -> None:
    """Verify one GitHub release asset ID, name, and download URL."""

    repository = source["repository"].removeprefix("https://github.com/")
    url = f"https://api.github.com/repos/{repository}/releases/tags/{source['tag']}"
    request = Request(
        url,
        headers={
            "Accept": "application/vnd.github+json",
            **github_authorization_header(github_token),
            "User-Agent": "OctaCity-CI",
            "X-GitHub-Api-Version": "2022-11-28",
        },
    )
    for attempt in range(1, attempts + 1):
        try:
            with urlopen(request, timeout=60) as response:
                payload = response.read(maximum_bytes + 1)
            document = json.loads(payload)
        except (OSError, URLError, UnicodeDecodeError, json.JSONDecodeError) as error:
            if isinstance(error, HTTPError):
                error.close()
            if attempt == attempts:
                raise SourceArchiveError(
                    f"cannot verify GitHub release asset after {attempts} attempts: {error}"
                ) from error
            delay = min(2 ** (attempt - 1), 8)
            print(
                f"release asset lookup attempt {attempt}/{attempts} failed: {error}; "
                f"retrying in {delay}s",
                file=sys.stderr,
            )
            time.sleep(delay)
            continue

        if len(payload) > maximum_bytes:
            raise SourceArchiveError("GitHub release metadata exceeds its size limit")
        if not isinstance(document, dict) or document.get("tag_name") != source["tag"]:
            raise SourceArchiveError("GitHub release metadata has an unexpected identity")
        assets = document.get("assets")
        if not isinstance(assets, list):
            raise SourceArchiveError("GitHub release metadata omits its assets")
        expected = {
            "id": source["asset_id"],
            "name": source["asset"],
            "browser_download_url": source["release_url"],
        }
        matches = [
            asset
            for asset in assets
            if isinstance(asset, dict)
            and {key: asset.get(key) for key in expected} == expected
        ]
        if len(matches) != 1:
            raise SourceArchiveError(
                "GitHub release metadata does not identify the pinned asset"
            )
        return


def github_authorization_header(token: str | None = None) -> dict[str, str]:
    """Authenticate GitHub API requests when Actions supplies its scoped token."""

    token = token or os.environ.get("GITHUB_TOKEN")
    return {"Authorization": f"Bearer {token}"} if token else {}


def download(source: dict[str, Any], destination: Path, *, attempts: int = 5) -> None:
    """Download an archive within its size bound and verify its SHA-256 digest."""

    expected = source["sha256"]
    maximum = source["archive_max_bytes"]
    label = source["tag"]
    request = Request(source["archive_url"], headers={"User-Agent": "OctaCity-CI"})
    for attempt in range(1, attempts + 1):
        destination.unlink(missing_ok=True)
        try:
            digest = hashlib.sha256()
            with urlopen(request, timeout=60) as response, destination.open("wb") as target:
                length = response.headers.get("Content-Length")
                if length is not None:
                    try:
                        advertised = int(length)
                    except ValueError as error:
                        raise SourceArchiveError(
                            f"archive for {label} has an invalid Content-Length"
                        ) from error
                    if advertised < 0:
                        raise SourceArchiveError(
                            f"archive for {label} has an invalid Content-Length"
                        )
                    if advertised > maximum:
                        raise SourceArchiveError(
                            f"archive for {label} exceeds {maximum} bytes"
                        )
                received = 0
                while chunk := response.read(1024 * 1024):
                    received += len(chunk)
                    if received > maximum:
                        raise SourceArchiveError(
                            f"archive for {label} exceeds {maximum} bytes"
                        )
                    target.write(chunk)
                    digest.update(chunk)
            actual = digest.hexdigest()
            if actual != expected:
                raise SourceArchiveError(
                    f"checksum mismatch for {label}: expected {expected}, received {actual}"
                )
            return
        except SourceArchiveError:
            destination.unlink(missing_ok=True)
            raise
        except OSError as error:
            destination.unlink(missing_ok=True)
            if attempt == attempts:
                raise SourceArchiveError(
                    f"failed to stage {label} after {attempts} attempts: {error}"
                ) from error
            delay = min(2 ** (attempt - 1), 8)
            print(
                f"source download attempt {attempt}/{attempts} failed: {error}; "
                f"retrying in {delay}s",
                file=sys.stderr,
            )
            time.sleep(delay)


def extract_source(archive: Path, destination: Path, source: dict[str, Any]) -> None:
    """Extract a single-root source archive below its root directory."""

    extract_tar_archive(
        archive,
        destination,
        max_expanded_bytes=source["expanded_max_bytes"],
        max_file_bytes=source["file_max_bytes"],
        max_members=source["member_max_count"],
        strip_single_root=True,
    )


def extract_tar_archive(
    archive: Path,
    destination: Path,
    *,
    max_expanded_bytes: int,
    max_file_bytes: int,
    max_members: int,
    strip_single_root: bool = False,
) -> None:
    """Extract a bounded regular-file tar archive without trusting its paths."""

    try:
        destination.mkdir(mode=0o700)
        with tarfile.open(archive, mode="r:*") as bundle:
            root: str | None = None
            seen: set[PurePosixPath] = set()
            members = 0
            expanded_bytes = 0
            for member in bundle:
                members += 1
                if members > max_members:
                    raise SourceArchiveError(
                        f"source archive has too many members: {archive}"
                    )
                if member.name.rstrip("/") == "." and member.isdir():
                    continue
                path = PurePosixPath(member.name)
                if path.is_absolute() or ".." in path.parts or not path.parts:
                    raise SourceArchiveError(
                        f"unsafe source archive member: {member.name}"
                    )
                if strip_single_root:
                    if root is None:
                        root = path.parts[0]
                    elif path.parts[0] != root:
                        raise SourceArchiveError(
                            f"source archive must contain one root directory: {archive}"
                        )
                    relative = PurePosixPath(*path.parts[1:])
                else:
                    relative = path
                if not relative.parts:
                    continue
                if relative in seen:
                    raise SourceArchiveError(
                        f"duplicate source archive member: {member.name}"
                    )
                seen.add(relative)
                target = destination.joinpath(*relative.parts)
                if member.isdir():
                    target.mkdir(parents=True, exist_ok=True)
                    continue
                if not member.isfile():
                    raise SourceArchiveError(
                        f"unsupported source archive member: {member.name}"
                    )
                if member.size > max_file_bytes:
                    raise SourceArchiveError(
                        f"source archive member is too large: {member.name}"
                    )
                expanded_bytes += member.size
                if expanded_bytes > max_expanded_bytes:
                    raise SourceArchiveError(
                        f"source archive expands beyond its limit: {archive}"
                    )
                target.parent.mkdir(parents=True, exist_ok=True)
                extracted = bundle.extractfile(member)
                if extracted is None:
                    raise SourceArchiveError(
                        f"cannot read source archive member: {member.name}"
                    )
                with extracted, target.open("wb") as output:
                    shutil.copyfileobj(extracted, output)
                if target.stat().st_size != member.size:
                    raise SourceArchiveError(
                        f"source archive member was truncated: {member.name}"
                    )
                target.chmod(0o755 if member.mode & 0o111 else 0o644)
            if members == 0:
                raise SourceArchiveError(f"source archive is empty: {archive}")
    except SourceArchiveError:
        raise
    except (OSError, tarfile.TarError) as error:
        raise SourceArchiveError(
            f"cannot extract verified source archive {archive}: {error}"
        ) from error
