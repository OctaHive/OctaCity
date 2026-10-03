#!/usr/bin/env python3
"""Reject local-stand logs containing credentials or private key material."""

from __future__ import annotations

import argparse
import os
from pathlib import Path
import re
import stat


MAX_LOG_BYTES = 32 * 1024 * 1024
MAX_SECRET_BYTES = 64 * 1024
MIN_SECRET_FRAGMENT_BYTES = 8
SENSITIVE_PATHS = (
    "pki/local-ca-key.pem",
    "pki/gateway-key.pem",
    "config/server/postgres-url",
    "agent/credential",
)
SENSITIVE_PATTERNS = (
    re.compile(rb"-----BEGIN (?:EC |OPENSSH |RSA )?PRIVATE KEY-----"),
    re.compile(
        rb"(?:enrollment|registration)\."
        rb"[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}\."
        rb"[A-Za-z0-9_-]{43}"
    ),
    re.compile(rb"postgres(?:ql)?://[^\s:/@]+:[^\s/@]+@[^\s]+"),
    re.compile(rb"OCTA[0-9A-F]{24}"),
)


class LogSafetyError(RuntimeError):
    """A log or secret input cannot be inspected without weakening the gate."""


def _read_regular(path: Path, description: str, maximum_bytes: int) -> bytes:
    """Read one bounded, non-symlink regular file through a stable descriptor."""

    try:
        expected = path.lstat()
    except OSError as error:
        raise LogSafetyError(f"cannot inspect {description}: {path}") from error
    if stat.S_ISLNK(expected.st_mode) or not stat.S_ISREG(expected.st_mode):
        raise LogSafetyError(f"{description} is not a bounded regular file: {path}")
    flags = os.O_RDONLY
    if hasattr(os, "O_CLOEXEC"):
        flags |= os.O_CLOEXEC
    if hasattr(os, "O_NOFOLLOW"):
        flags |= os.O_NOFOLLOW
    try:
        descriptor = os.open(path, flags)
    except OSError as error:
        raise LogSafetyError(f"cannot open {description}: {path}") from error
    try:
        metadata = os.fstat(descriptor)
        if (
            (expected.st_dev, expected.st_ino) != (metadata.st_dev, metadata.st_ino)
            or not stat.S_ISREG(metadata.st_mode)
            or metadata.st_size > maximum_bytes
        ):
            raise LogSafetyError(f"{description} is not a bounded regular file: {path}")
        chunks: list[bytes] = []
        received = 0
        while chunk := os.read(
            descriptor, min(1024 * 1024, maximum_bytes + 1 - received)
        ):
            chunks.append(chunk)
            received += len(chunk)
            if received > maximum_bytes:
                raise LogSafetyError(f"{description} exceeds its size limit: {path}")
        current = os.fstat(descriptor)
        if (metadata.st_dev, metadata.st_ino) != (current.st_dev, current.st_ino):
            raise LogSafetyError(f"{description} changed while reading: {path}")
        return b"".join(chunks)
    finally:
        os.close(descriptor)


def _secret_paths(root: Path) -> list[Path]:
    secrets = root / "secrets"
    if secrets.is_symlink() or not secrets.is_dir():
        raise LogSafetyError(f"secret directory is absent or unsafe: {secrets}")
    paths = sorted(secrets.iterdir())
    paths.extend(
        root / relative
        for relative in SENSITIVE_PATHS
        if (root / relative).exists() or (root / relative).is_symlink()
    )
    if not paths:
        raise LogSafetyError(f"secret directory is empty: {secrets}")
    return paths


def _secret_fragments(root: Path) -> tuple[bytes, ...]:
    fragments: set[bytes] = set()
    for path in _secret_paths(root):
        contents = _read_regular(path, "secret material", MAX_SECRET_BYTES)
        value = contents.strip()
        if len(value) >= MIN_SECRET_FRAGMENT_BYTES:
            fragments.add(value)
        for line in contents.splitlines():
            line = line.strip()
            if len(line) >= MIN_SECRET_FRAGMENT_BYTES:
                fragments.add(line)
    if not fragments:
        raise LogSafetyError("local stand contains no scannable secret material")
    return tuple(sorted(fragments))


def check_logs(root: Path, logs: list[Path]) -> None:
    """Fail when any bounded log contains generated or recognizable secret material."""

    if not logs:
        raise LogSafetyError("at least one log is required")
    if root.is_symlink() or not root.is_dir():
        raise LogSafetyError(f"state root is absent or unsafe: {root}")
    fragments = _secret_fragments(root.resolve())
    for log in logs:
        contents = _read_regular(log, "CI log", MAX_LOG_BYTES)
        if any(fragment in contents for fragment in fragments):
            raise LogSafetyError(f"generated secret material appears in CI log: {log}")
        if any(pattern.search(contents) for pattern in SENSITIVE_PATTERNS):
            raise LogSafetyError(f"credential-shaped material appears in CI log: {log}")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--state-root", required=True, type=Path)
    parser.add_argument("--log", required=True, action="append", type=Path)
    arguments = parser.parse_args()
    try:
        check_logs(arguments.state_root, arguments.log)
    except LogSafetyError as error:
        parser.error(str(error))
    print(f"validated {len(arguments.log)} secret-safe local-stand log(s)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
