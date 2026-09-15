#!/usr/bin/env python3
"""Build a deterministic, self-describing OctaCity agent release archive.

The runtime never installs or updates itself. This build-time tool owns the
distribution layout so CI and local release rehearsals produce the same files.
It uses only the Python standard library and never executes staged binaries,
which also allows an archive to be assembled for a non-native target.
"""

from __future__ import annotations

import argparse
import gzip
import hashlib
import json
import os
import re
import shutil
import stat
import tarfile
import tempfile
import zipfile
from pathlib import Path
from typing import NamedTuple


class PlatformLayout(NamedTuple):
    """Release paths and source-plugin identity for one build target."""

    source_platform: str
    agent_name: str
    plugin_name: str
    service_asset: str
    config_asset: str


PLATFORMS = {
    "linux-amd64": PlatformLayout(
        "linux-x86_64", "octacity-agent", "octacity-source-git", "systemd/octacity-agent.service", "agent.example.toml"
    ),
    "linux-arm64": PlatformLayout(
        "linux-aarch64",
        "octacity-agent",
        "octacity-source-git",
        "systemd/octacity-agent.service",
        "agent.linux-arm64.example.toml",
    ),
    "windows-amd64": PlatformLayout(
        "windows-x86_64",
        "octacity-agent.exe",
        "octacity-source-git.exe",
        "windows/install-service.ps1",
        "agent.windows.example.toml",
    ),
    "macos-arm64": PlatformLayout(
        "macos-aarch64",
        "octacity-agent",
        "octacity-source-git",
        "launchd/com.octahive.octacity-agent.plist",
        "agent.macos.example.toml",
    ),
}
FIXED_ZIP_TIME = (1980, 1, 1, 0, 0, 0)
VERSION_PATTERN = re.compile(r"[0-9A-Za-z][0-9A-Za-z.+_-]*\Z")
REVISION_PATTERN = re.compile(r"[0-9a-f]{40}\Z")
LINUX_RUNTIME_ASSETS = ("native-runtime.conf", "containerd-runtime.conf", "microsandbox-runtime.conf")


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
    # The value is embedded in TOML and JSON metadata. A deliberately narrow
    # release-token alphabet avoids format-specific escaping and path-like
    # values while still accepting SemVer and CI pre-release identifiers.
    if not VERSION_PATTERN.fullmatch(version):
        raise ValueError("version must contain only release-token characters")
    return version


def validate_revision(name: str, revision: str) -> str:
    if not REVISION_PATTERN.fullmatch(revision):
        raise ValueError(f"{name} must be a lowercase 40-character Git revision")
    return revision


def load_source_metadata(path: Path, expected_version: str, expected_platform: str) -> dict[str, object]:
    source = require_file("Git source-plugin package metadata", path)
    try:
        metadata = json.loads(source.read_text(encoding="utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise ValueError(f"Git source-plugin package metadata is invalid: {error}") from error
    required = {"manifest_version", "name", "version", "protocol_min", "protocol_max", "platform"}
    if not isinstance(metadata, dict) or set(metadata) != required:
        raise ValueError("Git source-plugin package metadata has an unexpected shape")
    integers = (metadata["manifest_version"], metadata["protocol_min"], metadata["protocol_max"])
    if any(not isinstance(value, int) or isinstance(value, bool) or value <= 0 for value in integers):
        raise ValueError("Git source-plugin package metadata contains an invalid protocol version")
    if metadata["name"] != "git" or metadata["version"] != expected_version:
        raise ValueError("Git source-plugin package metadata does not match the release version")
    if metadata["platform"] != expected_platform:
        raise ValueError("Git source-plugin package metadata does not match the release platform")
    if metadata["protocol_min"] > metadata["protocol_max"]:
        raise ValueError("Git source-plugin protocol range is invalid")
    return metadata


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


def stage_release(
    root: Path,
    repository: Path,
    platform: str,
    version: str,
    agent: Path,
    source_git: Path,
    source_metadata: dict[str, object],
    octacity_revision: str,
    octa_revision: str,
) -> None:
    layout = PLATFORMS[platform]
    source_platform = layout.source_platform
    agent_name = layout.agent_name
    plugin_name = layout.plugin_name
    copy_file(agent, root / "bin" / agent_name, executable=True)
    plugin_destination = root / "source-plugins" / "git" / plugin_name
    copy_file(source_git, plugin_destination, executable=True)
    plugin_digest = sha256(plugin_destination)
    write_text(
        plugin_destination.parent / "plugin.toml",
        "\n".join(
            [
                f'manifest_version = {source_metadata["manifest_version"]}',
                'name = "git"',
                f'version = "{version}"',
                f'protocol_min = {source_metadata["protocol_min"]}',
                f'protocol_max = {source_metadata["protocol_max"]}',
                f'executable = "{plugin_name}"',
                f'sha256 = "{plugin_digest}"',
                f'platforms = ["{source_platform}"]',
                "",
            ]
        ),
    )
    service_source = require_file("service asset", repository / "packaging" / layout.service_asset)
    service_destination = root / "share" / layout.service_asset
    copy_file(service_source, service_destination, executable=service_source.suffix == ".ps1")
    if platform.startswith("linux-"):
        for asset in LINUX_RUNTIME_ASSETS:
            source = require_file("systemd runtime drop-in", repository / "packaging/systemd" / asset)
            copy_file(source, root / "share/systemd" / asset)
    if platform == "windows-amd64":
        uninstall = require_file("Windows service removal asset", repository / "packaging/windows/uninstall-service.ps1")
        copy_file(uninstall, root / "share/windows/uninstall-service.ps1", executable=True)
    config_source = require_file("example configuration", repository / "docs" / layout.config_asset)
    copy_file(config_source, root / "share/agent.example.toml")
    copy_file(require_file("operations guide", repository / "docs/operations.md"), root / "share/operations.md")
    copy_file(require_file("license", repository / "LICENSE"), root / "LICENSE")
    manifest = {
        "format_version": 1,
        "product": "octacity-agent",
        "version": version,
        "platform": platform,
        "build_inputs": {
            "octacity_revision": octacity_revision,
            "octa_revision": octa_revision,
        },
        "components": {
            "agent": {"path": f"bin/{agent_name}", "sha256": sha256(root / "bin" / agent_name)},
            "source_git": {"path": f"source-plugins/git/{plugin_name}", "sha256": plugin_digest},
        },
    }
    write_text(root / "release-manifest.json", json.dumps(manifest, indent=2, sort_keys=True) + "\n")
    write_text(root / "SHA256SUMS", checksum_lines(root, {"SHA256SUMS"}))


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


def package(args: argparse.Namespace) -> Path:
    if args.platform not in PLATFORMS:
        raise ValueError(f"unsupported release platform: {args.platform}")
    repository = args.repository.resolve()
    agent = require_file("agent executable", args.agent)
    source_git = require_file("Git source plugin", args.source_git)
    version = validate_version(args.version)
    octacity_revision = validate_revision("OctaCity revision", args.octacity_revision)
    octa_revision = validate_revision("Octa revision", args.octa_revision)
    source_platform = PLATFORMS[args.platform].source_platform
    source_metadata = load_source_metadata(args.source_metadata, version, source_platform)
    output = args.output.resolve()
    expected_suffix = ".zip" if args.platform.startswith("windows-") else ".tar.gz"
    if not str(output).endswith(expected_suffix):
        raise ValueError(f"{args.platform} output must end with {expected_suffix}")
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="octacity-release-") as temporary:
        root = Path(temporary) / "octacity"
        root.mkdir()
        stage_release(
            root,
            repository,
            args.platform,
            version,
            agent,
            source_git,
            source_metadata,
            octacity_revision,
            octa_revision,
        )
        if expected_suffix == ".zip":
            archive_zip(root, output)
        else:
            archive_tar(root, output)
    checksum = output.with_name(output.name + ".sha256")
    write_text(checksum, f"{sha256(output)}  {output.name}\n")
    return output


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repository", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--platform", required=True, choices=sorted(PLATFORMS))
    parser.add_argument("--version", required=True)
    parser.add_argument("--agent", type=Path, required=True)
    parser.add_argument("--source-git", type=Path, required=True)
    parser.add_argument("--source-metadata", type=Path, required=True)
    parser.add_argument("--octacity-revision", required=True)
    parser.add_argument("--octa-revision", required=True)
    parser.add_argument("--output", type=Path, required=True)
    return parser.parse_args()


if __name__ == "__main__":
    package(parse_args())
