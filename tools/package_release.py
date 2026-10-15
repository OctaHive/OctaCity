#!/usr/bin/env python3
"""Build a deterministic, self-describing OctaCity agent release archive.

The runtime never installs or updates itself. This build-time tool owns the
distribution layout so CI and local release rehearsals produce the same files.
It uses only the Python standard library and never executes staged binaries,
which also allows an archive to be assembled for a non-native target.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import NamedTuple

from release_packaging import (
    archive_release,
    checksum_lines,
    copy_file,
    load_release_contract,
    require_file,
    sha256,
    validate_revision,
    validate_version,
    write_text,
)


class PlatformLayout(NamedTuple):
    """Release paths and source-plugin identity for one build target."""

    source_platform: str
    agent_name: str
    plugin_name: str
    service_asset: str
    config_asset: str
    git_executable: str


PLATFORMS = {
    "linux-amd64": PlatformLayout(
        "linux-x86_64",
        "octacity-agent",
        "octacity-source-git",
        "systemd/octacity-agent.service",
        "agent.example.toml",
        "/usr/bin/git",
    ),
    "linux-arm64": PlatformLayout(
        "linux-aarch64",
        "octacity-agent",
        "octacity-source-git",
        "systemd/octacity-agent.service",
        "agent.linux-arm64.example.toml",
        "/usr/bin/git",
    ),
    "windows-amd64": PlatformLayout(
        "windows-x86_64",
        "octacity-agent.exe",
        "octacity-source-git.exe",
        "windows/install-service.ps1",
        "agent.windows.example.toml",
        r"C:\Program Files\Git\cmd\git.exe",
    ),
    "macos-arm64": PlatformLayout(
        "macos-aarch64",
        "octacity-agent",
        "octacity-source-git",
        "launchd/com.octahive.octacity-agent.plist",
        "agent.macos.example.toml",
        "/usr/bin/git",
    ),
}
LINUX_RUNTIME_ASSETS = ("native-runtime.conf", "containerd-runtime.conf", "microsandbox-runtime.conf")
PROTECTED_READER_PLATFORMS = ("linux-x86_64", "linux-aarch64")


def protected_reader_path(platform: str) -> str:
    """Keep the standalone worker in its own guest-platform distribution root."""

    return f"workers/protected-reader/{platform}/octacity-protected-reader"


def release_executable_files(
    platform: str, *, include_codex_fixture: bool, protected_reader_platform: str | None = None
) -> frozenset[str]:
    """Returns the portable executable-mode inventory for one Agent archive."""

    layout = PLATFORMS[platform]
    files = {
        f"bin/{layout.agent_name}",
        f"source-plugins/git/{layout.plugin_name}",
    }
    if layout.service_asset.endswith(".ps1"):
        files.add(f"share/{layout.service_asset}")
    if platform == "windows-amd64":
        files.add("share/windows/uninstall-service.ps1")
    if include_codex_fixture:
        files.add("share/local-stand-codex-fixture")
    if protected_reader_platform is not None:
        files.add(protected_reader_path(protected_reader_platform))
    return frozenset(files)


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

def stage_release(
    root: Path,
    repository: Path,
    platform: str,
    version: str,
    agent: Path,
    source_git: Path,
    source_metadata: dict[str, object],
    source_allow_file: bool,
    octacity_revision: str,
    octa_revision: str,
    codex_fixture: Path | None = None,
    protected_reader: Path | None = None,
    protected_reader_platform: str | None = None,
) -> None:
    layout = PLATFORMS[platform]
    executable_files = release_executable_files(
        platform, include_codex_fixture=codex_fixture is not None,
        protected_reader_platform=protected_reader_platform,
    )
    contract_path, product_contract = load_release_contract(repository, "octacity-agent")
    source_protocol = product_contract["protocols"]["source_plugin"]
    if source_metadata["protocol_max"] < source_protocol["min"] or source_metadata["protocol_min"] > source_protocol["max"]:
        raise ValueError("Git source-plugin protocol range is incompatible with the Agent release contract")
    source_platform = layout.source_platform
    agent_name = layout.agent_name
    plugin_name = layout.plugin_name
    copy_file(
        agent,
        root / "bin" / agent_name,
        executable=f"bin/{agent_name}" in executable_files,
    )
    plugin_destination = root / "source-plugins" / "git" / plugin_name
    copy_file(
        source_git,
        plugin_destination,
        executable=f"source-plugins/git/{plugin_name}" in executable_files,
    )
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
                "[settings]",
                f"git_path = {json.dumps(layout.git_executable)}",
                f"allow_file = {str(source_allow_file).lower()}",
                "max_diagnostic_bytes = 65536",
                "",
            ]
        ),
    )
    service_source = require_file("service asset", repository / "packaging" / layout.service_asset)
    service_destination = root / "share" / layout.service_asset
    copy_file(
        service_source,
        service_destination,
        executable=f"share/{layout.service_asset}" in executable_files,
    )
    if platform.startswith("linux-"):
        for asset in LINUX_RUNTIME_ASSETS:
            source = require_file("systemd runtime drop-in", repository / "packaging/systemd" / asset)
            copy_file(source, root / "share/systemd" / asset)
    if platform == "windows-amd64":
        uninstall = require_file("Windows service removal asset", repository / "packaging/windows/uninstall-service.ps1")
        copy_file(
            uninstall,
            root / "share/windows/uninstall-service.ps1",
            executable="share/windows/uninstall-service.ps1" in executable_files,
        )
    config_source = require_file("example configuration", repository / "docs" / "reference" / "examples" / layout.config_asset)
    copy_file(config_source, root / "share/agent.example.toml")
    copy_file(require_file("operations guide", repository / "docs/operations/agent.md"), root / "share/operations.md")
    copy_file(require_file("license", repository / "LICENSE"), root / "LICENSE")
    copy_file(contract_path, root / "release-contract.json")
    if codex_fixture is not None:
        copy_file(
            require_file("local-stand Codex fixture", codex_fixture),
            root / "share/local-stand-codex-fixture",
            executable="share/local-stand-codex-fixture" in executable_files,
        )
    manifest = {
        "format_version": 1,
        "product": "octacity-agent",
        "version": version,
        "platform": platform,
        "build_inputs": {
            "octacity_revision": octacity_revision,
            "octa_revision": octa_revision,
        },
        "protocols": product_contract["protocols"],
        "components": {
            "agent": {"path": f"bin/{agent_name}", "sha256": sha256(root / "bin" / agent_name)},
            "source_git": {"path": f"source-plugins/git/{plugin_name}", "sha256": plugin_digest},
        },
    }
    if protected_reader is not None:
        worker_path = protected_reader_path(protected_reader_platform)
        copy_file(
            protected_reader,
            root / worker_path,
            executable=worker_path in executable_files,
        )
        worker_manifest = {
            "format_version": 1,
            "executable": "octacity-protected-reader",
            "platform": protected_reader_platform,
            "sha256": sha256(root / worker_path),
        }
        write_text(
            (root / worker_path).parent / "manifest.json",
            json.dumps(worker_manifest, indent=2, sort_keys=True) + "\n",
        )
    write_text(root / "release-manifest.json", json.dumps(manifest, indent=2, sort_keys=True) + "\n")
    write_text(root / "SHA256SUMS", checksum_lines(root, {"SHA256SUMS"}))


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
    codex_fixture = getattr(args, "codex_fixture", None)
    protected_reader = getattr(args, "protected_reader", None)
    protected_reader_platform = getattr(args, "protected_reader_platform", None)
    if (protected_reader is None) != (protected_reader_platform is None):
        raise ValueError("protected reader executable and guest platform must be supplied together")
    if protected_reader is not None:
        if protected_reader_platform not in PROTECTED_READER_PLATFORMS:
            raise ValueError("unsupported protected reader platform")
        protected_reader = require_file("protected reader executable", protected_reader)
    def stage(root: Path) -> None:
        stage_release(
            root,
            repository,
            args.platform,
            version,
            agent,
            source_git,
            source_metadata,
            args.source_allow_file,
            octacity_revision,
            octa_revision,
            codex_fixture,
            protected_reader,
            protected_reader_platform,
        )

    return archive_release(
        args.platform,
        args.output,
        "octacity-release-",
        "octacity",
        stage,
        executable_files=release_executable_files(
            args.platform, include_codex_fixture=codex_fixture is not None,
            protected_reader_platform=protected_reader_platform,
        ),
    )


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repository", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--platform", required=True, choices=sorted(PLATFORMS))
    parser.add_argument("--version", required=True)
    parser.add_argument("--agent", type=Path, required=True)
    parser.add_argument("--source-git", type=Path, required=True)
    parser.add_argument("--source-metadata", type=Path, required=True)
    parser.add_argument(
        "--source-allow-file",
        action="store_true",
        help="Allow local Git sources in this candidate's immutable plugin policy",
    )
    parser.add_argument("--octacity-revision", required=True)
    parser.add_argument("--octa-revision", required=True)
    parser.add_argument("--codex-fixture", type=Path)
    parser.add_argument("--protected-reader", type=Path)
    parser.add_argument("--protected-reader-platform", choices=PROTECTED_READER_PLATFORMS)
    parser.add_argument("--output", type=Path, required=True)
    return parser.parse_args()


if __name__ == "__main__":
    package(parse_args())
