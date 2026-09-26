#!/usr/bin/env python3
"""Build a deterministic, self-verifying OctaCity server release archive."""

from __future__ import annotations

import argparse
import json
from pathlib import Path

from release_packaging import (
    SUPPORTED_PLATFORMS,
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


def stage_release(
    root: Path,
    repository: Path,
    platform: str,
    version: str,
    server: Path,
    octacity_revision: str,
) -> None:
    """Stage one immutable server bundle below ``root``."""

    contract_path, product_contract = load_release_contract(repository, "octacity-server")
    executable_name = "octacity-server.exe" if platform.startswith("windows-") else "octacity-server"
    executable = root / "bin" / executable_name
    copy_file(server, executable, executable=True)
    copy_file(contract_path, root / "release-contract.json")
    copy_file(require_file("management REST guide", repository / "docs/management-rest-v1.md"), root / "share/management-rest-v1.md")
    copy_file(require_file("server architecture guide", repository / "docs/server-architecture.md"), root / "share/server-architecture.md")
    copy_file(require_file("license", repository / "LICENSE"), root / "LICENSE")
    manifest = {
        "format_version": 1,
        "product": "octacity-server",
        "version": version,
        "platform": platform,
        "build_inputs": {"octacity_revision": octacity_revision},
        "protocols": product_contract["protocols"],
        "components": {
            "server": {"path": f"bin/{executable_name}", "sha256": sha256(executable)},
        },
    }
    write_text(root / "release-manifest.json", json.dumps(manifest, indent=2, sort_keys=True) + "\n")
    write_text(root / "SHA256SUMS", checksum_lines(root, {"SHA256SUMS"}))


def package(args: argparse.Namespace) -> Path:
    """Validate inputs and write one reproducible server release archive."""

    if args.platform not in SUPPORTED_PLATFORMS:
        raise ValueError(f"unsupported release platform: {args.platform}")
    repository = args.repository.resolve()
    server = require_file("server executable", args.server)
    version = validate_version(args.version)
    octacity_revision = validate_revision("OctaCity revision", args.octacity_revision)
    def stage(root: Path) -> None:
        stage_release(root, repository, args.platform, version, server, octacity_revision)

    return archive_release(
        args.platform,
        args.output,
        "octacity-server-release-",
        "octacity-server",
        stage,
    )


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repository", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--platform", required=True, choices=sorted(SUPPORTED_PLATFORMS))
    parser.add_argument("--version", required=True)
    parser.add_argument("--server", type=Path, required=True)
    parser.add_argument("--octacity-revision", required=True)
    parser.add_argument("--output", type=Path, required=True)
    return parser.parse_args()


if __name__ == "__main__":
    package(parse_args())
