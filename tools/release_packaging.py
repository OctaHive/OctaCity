"""Shared deterministic release-packaging primitives.

Product packagers own their layouts and manifests. This module owns the
cross-product filesystem, validation, archive, and checksum mechanics.
"""

from __future__ import annotations

import gzip
import hashlib
import json
import re
import shutil
import stat
import tarfile
import tempfile
import zipfile
from collections.abc import Callable
from pathlib import Path


SUPPORTED_PLATFORMS = ("linux-amd64", "linux-arm64", "macos-arm64", "windows-amd64")
FIXED_ZIP_TIME = (1980, 1, 1, 0, 0, 0)
VERSION_PATTERN = re.compile(r"[0-9A-Za-z][0-9A-Za-z.+_-]*\Z")
REVISION_PATTERN = re.compile(r"[0-9a-f]{40}\Z")


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def require_file(name: str, path: Path) -> Path:
    if not path.is_file() or path.is_symlink():
        raise ValueError(f"{name} must be a regular file: {path}")
    return path.resolve()


def validate_version(version: str) -> str:
    if not VERSION_PATTERN.fullmatch(version):
        raise ValueError("version must contain only release-token characters")
    return version


def validate_revision(name: str, revision: str) -> str:
    if not REVISION_PATTERN.fullmatch(revision):
        raise ValueError(f"{name} must be a lowercase 40-character Git revision")
    return revision


def load_release_contract(repository: Path, product: str) -> tuple[Path, dict[str, object]]:
    """Load one product's canonical release contract from the repository."""

    path = require_file("OctaCity release contract", repository / "packaging/release-contract.json")
    try:
        contract = json.loads(path.read_text(encoding="utf-8"))
        product_contract = contract["products"][product]
    except (UnicodeDecodeError, json.JSONDecodeError, KeyError, TypeError) as error:
        raise ValueError(f"OctaCity release contract is invalid: {error}") from error
    if (
        not isinstance(contract, dict)
        or contract.get("format_version") != 1
        or contract.get("manifest") != "release-manifest.json"
        or contract.get("checksums") != "SHA256SUMS"
        or not isinstance(product_contract, dict)
        or not isinstance(product_contract.get("protocols"), dict)
        or not isinstance(product_contract.get("required_components"), list)
    ):
        raise ValueError("OctaCity release contract has an unsupported shape")
    for name, supported in product_contract["protocols"].items():
        if (
            not isinstance(name, str)
            or not isinstance(supported, dict)
            or set(supported) != {"min", "max"}
            or not all(isinstance(value, int) and not isinstance(value, bool) and value > 0 for value in supported.values())
            or supported["min"] > supported["max"]
        ):
            raise ValueError(f"OctaCity release contract contains an invalid '{name}' protocol range")
    return path, product_contract


def write_text(path: Path, value: str, executable: bool = False) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(value, encoding="utf-8", newline="\n")
    path.chmod(0o755 if executable else 0o644)


def copy_file(source: Path, destination: Path, executable: bool = False) -> None:
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source, destination)
    destination.chmod(0o755 if executable else 0o644)


def checksum_lines(root: Path, excluded: set[str] | None = None) -> str:
    excluded = excluded or set()
    files = sorted(path for path in root.rglob("*") if path.is_file() and path.relative_to(root).as_posix() not in excluded)
    return "".join(f"{sha256(path)}  {path.relative_to(root).as_posix()}\n" for path in files)


def archive_tar(root: Path, destination: Path) -> None:
    with destination.open("wb") as raw:
        with gzip.GzipFile(filename="", mode="wb", fileobj=raw, mtime=0) as compressed:
            with tarfile.open(fileobj=compressed, mode="w", format=tarfile.PAX_FORMAT) as archive:
                for path in sorted(root.rglob("*")):
                    relative = path.relative_to(root).as_posix()
                    info = archive.gettarinfo(str(path), arcname=relative)
                    info.uid = info.gid = 0
                    info.uname = info.gname = "root"
                    info.mtime = 0
                    if path.is_file():
                        info.mode = stat.S_IMODE(path.stat().st_mode)
                        with path.open("rb") as source:
                            archive.addfile(info, source)
                    else:
                        info.mode = 0o755
                        archive.addfile(info)


def archive_zip(root: Path, destination: Path) -> None:
    with zipfile.ZipFile(destination, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        for path in sorted(candidate for candidate in root.rglob("*") if candidate.is_file()):
            relative = path.relative_to(root).as_posix()
            info = zipfile.ZipInfo(relative, FIXED_ZIP_TIME)
            info.compress_type = zipfile.ZIP_DEFLATED
            info.external_attr = (stat.S_IMODE(path.stat().st_mode) & 0xFFFF) << 16
            archive.writestr(info, path.read_bytes(), compresslevel=9)


def archive_release(
    platform: str,
    output: Path,
    temporary_prefix: str,
    root_name: str,
    stage: Callable[[Path], None],
) -> Path:
    """Stage, archive, and externally checksum one product release."""

    if platform not in SUPPORTED_PLATFORMS:
        raise ValueError(f"unsupported release platform: {platform}")
    output = output.resolve()
    expected_suffix = ".zip" if platform.startswith("windows-") else ".tar.gz"
    if not str(output).endswith(expected_suffix):
        raise ValueError(f"{platform} output must end with {expected_suffix}")
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=temporary_prefix) as temporary:
        root = Path(temporary) / root_name
        root.mkdir()
        stage(root)
        if expected_suffix == ".zip":
            archive_zip(root, output)
        else:
            archive_tar(root, output)
    write_text(output.with_name(output.name + ".sha256"), f"{sha256(output)}  {output.name}\n")
    return output
