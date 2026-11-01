#!/usr/bin/env python3
"""Stage a pinned released or source-built Microsandbox guest agent."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path
from typing import Callable, NamedTuple
from urllib.error import URLError
from urllib.request import Request, urlopen

from pinned_source_archive import SourceArchiveError, download, extract_source


VERSION = "0.7.6"
RELEASE_URL = (
    "https://github.com/superradcompany/microsandbox/releases/download/"
    f"v{VERSION}"
)
ASSET_API_URL = "https://api.github.com/repos/superradcompany/microsandbox/releases/assets"
REPAIR_ROOT = Path(__file__).resolve().parents[1] / "config/runtime/microsandbox-agentd-v1"


def qualified_digest(architecture: str, checksums: Path = REPAIR_ROOT / "SHA256SUMS") -> str:
    """Resolve the exact qualified guest bytes, shared with backend registration."""
    for line in checksums.read_text().splitlines():
        fields = line.split()
        if len(fields) == 2 and fields[1] == f"agentd-{architecture}":
            digest = fields[0]
            if len(digest) == 64 and all(c in "0123456789abcdef" for c in digest):
                return digest
    raise StageError(f"no qualified Microsandbox guest for {architecture}")


def stage_repaired_agentd(
    architecture: str,
    output: Path,
    *,
    policy_root: Path = REPAIR_ROOT,
    execute: Callable[..., object] = subprocess.run,
) -> Path:
    """Stage the qualified file-overlay repair without changing upstream downloads."""
    expected = qualified_digest(architecture, policy_root / "SHA256SUMS")
    if output.is_symlink():
        raise StageError("repaired Microsandbox guest path must not be a symlink")
    if output.is_file() and file_sha256(output) == expected:
        return output.resolve()
    if os.name != "posix":
        raise StageError("building the qualified guest requires a Linux or macOS build host")
    policy = json.loads((policy_root / "inputs.json").read_text())
    builder = policy["builders"][architecture]
    output = output.absolute()
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=".agentd-build-", dir=output.parent) as value:
        staging = Path(value)
        archive = staging / "source.tar.gz"
        download(policy["source"], archive)
        source = staging / "source"
        extract_source(archive, source, policy["source"])
        manifest = source / "Cargo.toml"
        text = manifest.read_text()
        members = json.dumps([
            "crates/agentd", "crates/protocol",
            "packages/microsandbox-types/macros", "packages/microsandbox-types/rust",
        ])
        text, count = re.subn(
            r"members = \[.*?\]", "members = " + members, text, count=1, flags=re.S
        )
        if count != 1:
            raise StageError("pinned Microsandbox workspace has no members")
        manifest.write_text(text)
        shutil.copyfile(policy_root / "Cargo.lock", source / "Cargo.lock")
        patch = str((policy_root / "readonly-file-mount.patch").resolve())
        execute(
            ["git", "apply", "--check", patch],
            cwd=source, check=True, capture_output=True, text=True,
        )
        execute(
            ["git", "apply", patch],
            cwd=source, check=True, capture_output=True, text=True,
        )
        execute([
            "docker", "run", "--rm", "--platform", builder["platform"],
            "--user", f"{os.getuid()}:{os.getgid()}",
            "--mount", f"type=bind,src={source.resolve()},dst=/build",
            "--workdir", "/build",
            "--env", "CARGO_HOME=/build/.cargo-cache",
            "--env", f"CARGO_TARGET_DIR=/build/target-musl-{architecture}",
            "--env", f"CARGO_BUILD_JOBS={os.environ.get('CARGO_BUILD_JOBS', '2')}",
            "--env", "CARGO_PROFILE_RELEASE_DEBUG=0",
            "--env", "RUSTFLAGS=-C strip=symbols -C link-arg=-Wl,--build-id=none",
            builder["image"], "cargo", "build", "--locked", "--release",
            "--target", builder["target"],
            "-p", "microsandbox-agentd", "--bin", "agentd",
        ], check=True)
        binary = source / f"target-musl-{architecture}" / builder["target"] / "release/agentd"
        actual = file_sha256(binary)
        if binary.is_symlink() or actual != expected:
            raise StageError(
                f"repaired guest checksum mismatch: expected {expected}, received {actual}"
            )
        binary.chmod(0o500)
        os.replace(binary, output)
    return output.resolve()


class Asset(NamedTuple):
    """Immutable identity of one upstream release asset."""

    name: str
    sha256: str
    api_id: int


ASSETS = {
    "aarch64": Asset(
        "agentd-aarch64",
        "f858a7b227308b40e06387b99c5f9a99f5fb9f1aeaa70f47080a8998b87dd2ba",
        603564489,
    ),
    "x86_64": Asset(
        "agentd-x86_64",
        "78bb21c3bf16f195068c946ce72419d242fa2b3f1bebd2c339cc8e58dd79847d",
        603564491,
    ),
}


class StageError(RuntimeError):
    """Raised when a verified guest agent cannot be staged."""


def file_sha256(path: Path) -> str:
    """Return the lowercase SHA-256 digest of a file."""
    digest = hashlib.sha256()
    with path.open("rb") as source:
        while chunk := source.read(1024 * 1024):
            digest.update(chunk)
    return digest.hexdigest()


def stage_agentd(
    architecture: str,
    output: Path,
    *,
    attempts: int = 5,
    opener: Callable[..., object] = urlopen,
    sleeper: Callable[[float], None] = time.sleep,
) -> Path:
    """Stage a checksum-verified agentd, retrying transient download failures."""
    if architecture not in ASSETS:
        raise StageError(f"unsupported Microsandbox agentd architecture: {architecture}")
    if attempts < 1:
        raise ValueError("attempts must be positive")

    asset = ASSETS[architecture]
    output = output.resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    if output.is_file() and file_sha256(output) == asset.sha256:
        return output

    partial = output.with_name(f"{output.name}.part")
    requests = (
        Request(
            f"{ASSET_API_URL}/{asset.api_id}",
            headers={
                "Accept": "application/octet-stream",
                "X-GitHub-Api-Version": "2022-11-28",
                "User-Agent": "OctaCity-CI",
            },
        ),
        Request(
            f"{RELEASE_URL}/{asset.name}", headers={"User-Agent": "OctaCity-CI"}
        ),
    )

    for attempt in range(1, attempts + 1):
        partial.unlink(missing_ok=True)
        request = requests[(attempt - 1) % len(requests)]
        try:
            digest = hashlib.sha256()
            with opener(request, timeout=60) as response, partial.open("wb") as target:
                while chunk := response.read(1024 * 1024):
                    target.write(chunk)
                    digest.update(chunk)
            actual_digest = digest.hexdigest()
            if actual_digest != asset.sha256:
                raise StageError(
                    f"checksum mismatch for {asset.name}: expected {asset.sha256}, "
                    f"received {actual_digest}"
                )
            os.replace(partial, output)
            return output
        except (OSError, URLError, StageError) as error:
            partial.unlink(missing_ok=True)
            if attempt == attempts:
                raise StageError(
                    f"failed to stage {asset.name} after {attempts} attempts: {error}"
                ) from error
            delay = min(2 ** (attempt - 1), 8)
            print(
                f"agentd download attempt {attempt}/{attempts} via "
                f"{request.host} failed: {error}; "
                f"retrying in {delay}s",
                file=sys.stderr,
            )
            sleeper(delay)

    raise AssertionError("retry loop must return or raise")


def parse_args() -> argparse.Namespace:
    """Parse command-line arguments."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--architecture", required=True, choices=sorted(ASSETS))
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--attempts", type=int, default=5)
    parser.add_argument(
        "--repaired", action="store_true",
        help="build or reuse the qualified read-only-parent file-mount repair",
    )
    return parser.parse_args()


def main() -> int:
    """Stage agentd and print its absolute path for CI environment wiring."""
    arguments = parse_args()
    try:
        if arguments.repaired:
            path = stage_repaired_agentd(arguments.architecture, arguments.output)
        else:
            path = stage_agentd(
                arguments.architecture, arguments.output, attempts=arguments.attempts
            )
    except (StageError, SourceArchiveError, subprocess.CalledProcessError, OSError, ValueError) as error:
        print(error, file=sys.stderr)
        return 1
    print(path)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
