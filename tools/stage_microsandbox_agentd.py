#!/usr/bin/env python3
"""Download and verify the pinned Microsandbox guest agent for Cargo builds."""

from __future__ import annotations

import argparse
import hashlib
import os
import sys
import time
from pathlib import Path
from typing import Callable
from urllib.error import URLError
from urllib.request import Request, urlopen


VERSION = "0.6.18"
RELEASE_URL = (
    "https://github.com/superradcompany/microsandbox/releases/download/"
    f"v{VERSION}"
)
ASSETS = {
    "aarch64": (
        "agentd-aarch64",
        "446efc7c97fd4c17233c500f1162a19b546562222d8088889207f45091468f29",
    ),
    "x86_64": (
        "agentd-x86_64",
        "ba7f7a719bacb4afdfa211b5e42da9f75cea2fcadc0b1a58a686cd5a727ecdf7",
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

    asset, expected_digest = ASSETS[architecture]
    output = output.resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    if output.is_file() and file_sha256(output) == expected_digest:
        return output

    url = f"{RELEASE_URL}/{asset}"
    partial = output.with_name(f"{output.name}.part")
    request = Request(url, headers={"User-Agent": "OctaCity-CI"})

    for attempt in range(1, attempts + 1):
        partial.unlink(missing_ok=True)
        try:
            digest = hashlib.sha256()
            with opener(request, timeout=60) as response, partial.open("wb") as target:
                while chunk := response.read(1024 * 1024):
                    target.write(chunk)
                    digest.update(chunk)
            actual_digest = digest.hexdigest()
            if actual_digest != expected_digest:
                raise StageError(
                    f"checksum mismatch for {asset}: expected {expected_digest}, "
                    f"received {actual_digest}"
                )
            os.replace(partial, output)
            return output
        except (OSError, URLError, StageError) as error:
            partial.unlink(missing_ok=True)
            if attempt == attempts:
                raise StageError(
                    f"failed to stage {asset} after {attempts} attempts: {error}"
                ) from error
            delay = min(2 ** (attempt - 1), 8)
            print(
                f"agentd download attempt {attempt}/{attempts} failed: {error}; "
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
    return parser.parse_args()


def main() -> int:
    """Stage agentd and print its absolute path for CI environment wiring."""
    arguments = parse_args()
    try:
        path = stage_agentd(
            arguments.architecture,
            arguments.output,
            attempts=arguments.attempts,
        )
    except (StageError, ValueError) as error:
        print(error, file=sys.stderr)
        return 1
    print(path)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
