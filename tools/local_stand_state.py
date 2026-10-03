#!/usr/bin/env python3
"""Shared private-filesystem primitives for the local development stand."""

from __future__ import annotations

from collections.abc import Iterator
from contextlib import contextmanager
import json
import os
from pathlib import Path
import stat
import tempfile


DEFAULT_ROOT = Path.home() / "Library/Application Support/OctaCity/local-stand"
DIRECTORY_MODE = 0o700
PRIVATE_FILE_MODE = 0o600
PUBLIC_FILE_MODE = 0o644


class StateError(RuntimeError):
    """Private local-stand state cannot be created or trusted safely."""


def current_uid() -> int:
    """Return the invoking POSIX user identity."""

    if not hasattr(os, "getuid"):
        raise StateError("local stand host state requires POSIX ownership")
    return os.getuid()


@contextmanager
def private_umask() -> Iterator[None]:
    """Prevent newly created state from inheriting group or world access."""

    previous = os.umask(0o077)
    try:
        yield
    finally:
        os.umask(previous)


def _is_relative_to(path: Path, root: Path) -> bool:
    try:
        path.relative_to(root)
    except ValueError:
        return False
    return True


def resolve_safe_root(root: Path, repository: Path, home: Path) -> Path:
    """Resolve a stand root and reject broad, repository-owned, or symbolic targets."""

    expanded = root.expanduser()
    if not expanded.is_absolute():
        raise StateError("local stand root must be an absolute path")
    if expanded.is_symlink():
        raise StateError("local stand root must not be symbolic")
    resolved = expanded.resolve(strict=False)
    repository = repository.resolve()
    home = home.resolve()
    if len(resolved.parts) < 3:
        raise StateError(f"local stand root is too broad: {resolved}")
    if _is_relative_to(resolved, repository):
        raise StateError("local stand root must be outside the repository")
    for protected in (home, repository):
        if resolved == protected or _is_relative_to(protected, resolved):
            raise StateError(f"local stand root is too broad: {resolved}")
    return resolved


def _validate_owner(metadata: os.stat_result, path: Path) -> None:
    if metadata.st_uid != current_uid():
        raise StateError(f"local stand path is not owned by the invoking user: {path}")


def validate_private_directory(path: Path) -> None:
    """Require one real, invoking-user-owned directory with mode 0700."""

    try:
        metadata = path.lstat()
    except FileNotFoundError as error:
        raise StateError(f"local stand directory is absent: {path}") from error
    if stat.S_ISLNK(metadata.st_mode):
        raise StateError(f"local stand directory must not be symbolic: {path}")
    if not stat.S_ISDIR(metadata.st_mode):
        raise StateError(f"local stand path is not a directory: {path}")
    _validate_owner(metadata, path)
    if stat.S_IMODE(metadata.st_mode) != DIRECTORY_MODE:
        raise StateError(f"local stand directory must have mode 0700: {path}")


def ensure_private_directory(path: Path) -> None:
    """Create a missing private directory and validate an existing one."""

    try:
        path.mkdir(parents=True, mode=DIRECTORY_MODE)
    except FileExistsError:
        pass
    else:
        path.chmod(DIRECTORY_MODE)
    validate_private_directory(path)


def read_regular_file(
    path: Path,
    description: str,
    *,
    max_bytes: int,
    modes: frozenset[int],
) -> bytes:
    """Open without following links, validate the handle, and read with a hard bound."""

    descriptor, metadata = open_regular_file(
        path, description, max_bytes=max_bytes, modes=modes
    )
    try:
        with os.fdopen(descriptor, "rb", closefd=False) as source:
            contents = source.read(max_bytes + 1)
        if len(contents) == 0 or len(contents) > max_bytes:
            raise StateError(f"{description} has an invalid size: {path}")
        current = os.fstat(descriptor)
        if (
            file_state(current) != file_state(metadata)
            or len(contents) != current.st_size
        ):
            raise StateError(f"{description} changed while it was read: {path}")
        return contents
    finally:
        if descriptor >= 0:
            os.close(descriptor)


def file_state(metadata: os.stat_result) -> tuple[int, int, int, int, int]:
    """Return handle metadata that changes when file content or identity changes."""

    return (
        metadata.st_dev,
        metadata.st_ino,
        metadata.st_size,
        metadata.st_mtime_ns,
        metadata.st_ctime_ns,
    )


def open_regular_file(
    path: Path,
    description: str,
    *,
    max_bytes: int,
    modes: frozenset[int],
) -> tuple[int, os.stat_result]:
    """Return one validated no-follow descriptor and its handle metadata."""

    if max_bytes <= 0:
        raise ValueError("max_bytes must be greater than zero")
    flags = os.O_RDONLY
    if hasattr(os, "O_CLOEXEC"):
        flags |= os.O_CLOEXEC
    if hasattr(os, "O_NOFOLLOW"):
        flags |= os.O_NOFOLLOW
    try:
        descriptor = os.open(path, flags)
    except FileNotFoundError as error:
        raise StateError(f"{description} is absent: {path}") from error
    except OSError as error:
        raise StateError(f"{description} cannot be opened safely: {path}: {error}") from error
    try:
        metadata = os.fstat(descriptor)
        if not stat.S_ISREG(metadata.st_mode):
            raise StateError(f"{description} must be a regular file: {path}")
        _validate_owner(metadata, path)
        if metadata.st_nlink != 1:
            raise StateError(f"{description} must not have additional hard links: {path}")
        if stat.S_IMODE(metadata.st_mode) not in modes:
            rendered = " or ".join(f"{mode:04o}" for mode in sorted(modes))
            raise StateError(f"{description} must have mode {rendered}: {path}")
        if metadata.st_size == 0 or metadata.st_size > max_bytes:
            raise StateError(f"{description} has an invalid size: {path}")
        return descriptor, metadata
    except BaseException:
        os.close(descriptor)
        raise


def load_json_object(contents: bytes, description: str) -> dict[str, object]:
    """Decode a strict top-level JSON object from already bounded bytes."""

    try:
        document = json.loads(contents)
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise StateError(f"{description} is invalid JSON") from error
    if not isinstance(document, dict):
        raise StateError(f"{description} must be a JSON object")
    return document


def publish_exclusive_private_file(path: Path, contents: bytes) -> bool:
    """Publish one private file without replacing an existing filesystem entry."""

    descriptor, temporary_name = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
    temporary = Path(temporary_name)
    try:
        os.fchmod(descriptor, PRIVATE_FILE_MODE)
        with os.fdopen(descriptor, "wb") as output:
            descriptor = -1
            output.write(contents)
            output.flush()
            os.fsync(output.fileno())
        try:
            os.link(temporary, path, follow_symlinks=False)
        except FileExistsError:
            return False
        return True
    finally:
        if descriptor >= 0:
            os.close(descriptor)
        temporary.unlink(missing_ok=True)
