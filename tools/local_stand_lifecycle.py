#!/usr/bin/env python3
"""Safe host-process lifecycle state primitives for the local stand."""

from __future__ import annotations

import ctypes
from dataclasses import dataclass
from enum import Enum
import errno
import json
import math
import os
from pathlib import Path
import re
import stat
import subprocess
import sys
import time
from typing import BinaryIO, Callable, Mapping, Sequence

import local_stand_supervisor as supervisor
import local_stand_state as state

try:
    import fcntl
except ImportError:  # pragma: no cover - exercised by Windows import checks.
    fcntl = None


PRIVATE_FILE_MODE = state.PRIVATE_FILE_MODE
RECORD_SCHEMA_VERSION = 2
MAX_RECORD_BYTES = 16 * 1024
MAX_START_IDENTITY_LENGTH = 512
CONTROL_TOKEN_LENGTH = 64
PROCESS_NAME_PATTERN = re.compile(r"^[a-z][a-z0-9-]{0,31}$")
POLL_INTERVAL_SECONDS = 0.05
MAX_WAIT_SECONDS = 600
MAX_POLL_INTERVAL_SECONDS = 5
DARWIN_PROC_PIDTBSDINFO = 3
DARWIN_ZOMBIE_STATUS = 5


class LifecycleError(RuntimeError):
    """Lifecycle state is unsafe, inconsistent, or cannot converge in time."""


class LockUnavailable(LifecycleError):
    """Another launcher invocation currently owns the lifecycle lock."""


@dataclass(frozen=True)
class ProcessIdentity:
    """Kernel-observed identity used to distinguish PID reuse."""

    pid: int
    process_group_id: int
    uid: int
    start_identity: str


@dataclass(frozen=True)
class ProcessRecord:
    """Persisted ownership record for one launcher-owned process group."""

    schema_version: int
    name: str
    identity: ProcessIdentity
    control_socket: Path
    control_token: str


@dataclass(frozen=True)
class _StoredRecord:
    record: ProcessRecord
    device: int
    inode: int


class _DarwinProcessBsdInfo(ctypes.Structure):
    _fields_ = [
        ("pbi_flags", ctypes.c_uint32),
        ("pbi_status", ctypes.c_uint32),
        ("pbi_xstatus", ctypes.c_uint32),
        ("pbi_pid", ctypes.c_uint32),
        ("pbi_ppid", ctypes.c_uint32),
        ("pbi_uid", ctypes.c_uint32),
        ("pbi_gid", ctypes.c_uint32),
        ("pbi_ruid", ctypes.c_uint32),
        ("pbi_rgid", ctypes.c_uint32),
        ("pbi_svuid", ctypes.c_uint32),
        ("pbi_svgid", ctypes.c_uint32),
        ("rfu_1", ctypes.c_uint32),
        ("pbi_comm", ctypes.c_char * 16),
        ("pbi_name", ctypes.c_char * 32),
        ("pbi_nfiles", ctypes.c_uint32),
        ("pbi_pgid", ctypes.c_uint32),
        ("pbi_pjobc", ctypes.c_uint32),
        ("e_tdev", ctypes.c_uint32),
        ("e_tpgid", ctypes.c_uint32),
        ("pbi_nice", ctypes.c_int32),
        ("pbi_start_tvsec", ctypes.c_uint64),
        ("pbi_start_tvusec", ctypes.c_uint64),
    ]


class StartDisposition(Enum):
    """Result of reconciling process state before startup."""

    STARTABLE = "startable"
    STALE_REPAIRED = "stale_repaired"
    ALREADY_RUNNING = "already_running"


class ShutdownDisposition(Enum):
    """How a requested process shutdown converged."""

    ALREADY_STOPPED = "already_stopped"
    STALE_REPAIRED = "stale_repaired"
    GRACEFUL = "graceful"
    FORCED = "forced"


def _process_name(value: str) -> str:
    if not PROCESS_NAME_PATTERN.fullmatch(value):
        raise LifecycleError(f"invalid lifecycle process name: {value!r}")
    return value


def _darwin_process_identity(pid: int) -> ProcessIdentity | None:
    try:
        library = ctypes.CDLL("/usr/lib/libproc.dylib", use_errno=True)
    except OSError as error:
        raise LifecycleError(f"cannot load macOS process inspection API: {error}") from error
    library.proc_pidinfo.argtypes = [
        ctypes.c_int,
        ctypes.c_int,
        ctypes.c_uint64,
        ctypes.c_void_p,
        ctypes.c_int,
    ]
    library.proc_pidinfo.restype = ctypes.c_int
    information = _DarwinProcessBsdInfo()
    size = ctypes.sizeof(information)
    result = library.proc_pidinfo(
        pid,
        DARWIN_PROC_PIDTBSDINFO,
        0,
        ctypes.byref(information),
        size,
    )
    if result == 0:
        error = ctypes.get_errno()
        if error in {0, errno.ESRCH}:
            return None
        raise LifecycleError(f"cannot inspect process {pid}: {os.strerror(error)}")
    if result != size:
        raise LifecycleError(f"macOS returned a truncated process identity for {pid}")
    if information.pbi_status == DARWIN_ZOMBIE_STATUS:
        return None
    return ProcessIdentity(
        pid,
        information.pbi_pgid,
        information.pbi_uid,
        f"darwin:{information.pbi_start_tvsec}:{information.pbi_start_tvusec}",
    )


def _process_identity(pid: int) -> ProcessIdentity | None:
    if pid <= 1:
        return None
    if sys.platform == "darwin":
        return _darwin_process_identity(pid)
    if Path("/proc").is_dir():
        try:
            stat_fields = Path(f"/proc/{pid}/stat").read_text(encoding="ascii")
            closing = stat_fields.rfind(")")
            fields = stat_fields[closing + 2 :].split()
            if fields[0] == "Z":
                return None
            process_group_id = int(fields[2])
            start_ticks = fields[19]
            uid = Path(f"/proc/{pid}").stat().st_uid
            boot_id = Path("/proc/sys/kernel/random/boot_id").read_text(encoding="ascii").strip()
        except (FileNotFoundError, ProcessLookupError):
            return None
        except (IndexError, OSError, UnicodeError, ValueError) as error:
            raise LifecycleError(f"cannot inspect process {pid}: {error}") from error
        return ProcessIdentity(pid, process_group_id, uid, f"linux:{boot_id}:{start_ticks}")

    try:
        result = subprocess.run(
            [
                "/bin/ps",
                "-o",
                "uid=",
                "-o",
                "pgid=",
                "-o",
                "state=",
                "-o",
                "lstart=",
                "-p",
                str(pid),
            ],
            check=False,
            capture_output=True,
            text=True,
            timeout=5,
            env={"LC_ALL": "C", "PATH": "/usr/bin:/bin"},
        )
    except (OSError, subprocess.TimeoutExpired) as error:
        raise LifecycleError(f"cannot inspect process {pid}: {error}") from error
    if result.returncode != 0 or not result.stdout.strip():
        return None
    try:
        uid_text, group_text, process_state, started = result.stdout.strip().split(maxsplit=3)
        if process_state.startswith("Z"):
            return None
        return ProcessIdentity(pid, int(group_text), int(uid_text), f"posix:{started}")
    except (TypeError, ValueError) as error:
        raise LifecycleError(f"cannot parse process identity for {pid}") from error


def capture_process_identity(pid: int) -> ProcessIdentity:
    """Capture an owned process-group leader or fail closed."""

    identity = _process_identity(pid)
    if identity is None:
        raise LifecycleError(f"process {pid} is not running")
    if identity.uid != state.current_uid():
        raise LifecycleError(f"process {pid} is not owned by the invoking user")
    if identity.process_group_id != identity.pid:
        raise LifecycleError(f"process {pid} is not a process-group leader")
    return identity


class LifecycleState:
    """Exclusive owner of process records and logs below one private stand root."""

    def __init__(self, root: Path):
        state.validate_private_directory(root)
        self.root = root.resolve()
        self.lifecycle_root = self.root / "lifecycle"
        self.log_root = self.root / "logs"
        with state.private_umask():
            state.ensure_private_directory(self.lifecycle_root)
            state.ensure_private_directory(self.log_root)
        self._lock_descriptor: int | None = None

    def __enter__(self) -> LifecycleState:
        if fcntl is None:
            raise LifecycleError("local-stand lifecycle locking requires POSIX flock")
        if self._lock_descriptor is not None:
            raise LifecycleError("lifecycle lock is already held by this instance")
        path = self.lifecycle_root / "launcher.lock"
        flags = os.O_RDWR | os.O_CREAT
        if hasattr(os, "O_NOFOLLOW"):
            flags |= os.O_NOFOLLOW
        descriptor = os.open(path, flags, PRIVATE_FILE_MODE)
        try:
            metadata = os.fstat(descriptor)
            self._validate_owned_private_file(metadata, "lifecycle lock")
            try:
                fcntl.flock(descriptor, fcntl.LOCK_EX | fcntl.LOCK_NB)
            except BlockingIOError as error:
                raise LockUnavailable("another local-stand lifecycle command is running") from error
        except BaseException:
            os.close(descriptor)
            raise
        self._lock_descriptor = descriptor
        return self

    def __exit__(self, _type, _value, _traceback) -> None:
        if self._lock_descriptor is not None:
            assert fcntl is not None
            fcntl.flock(self._lock_descriptor, fcntl.LOCK_UN)
            os.close(self._lock_descriptor)
            self._lock_descriptor = None

    @staticmethod
    def _validate_owned_private_file(metadata: os.stat_result, description: str) -> None:
        if not stat.S_ISREG(metadata.st_mode):
            raise LifecycleError(f"{description} must be a regular file")
        if metadata.st_uid != state.current_uid():
            raise LifecycleError(f"{description} is not owned by the invoking user")
        if metadata.st_nlink != 1 or stat.S_IMODE(metadata.st_mode) != PRIVATE_FILE_MODE:
            raise LifecycleError(f"{description} must have one link and mode 0600")

    def _require_lock(self) -> None:
        if self._lock_descriptor is None:
            raise LifecycleError("lifecycle operation requires the exclusive lock")

    def require_exclusive_lock(self) -> None:
        """Prove this instance currently owns the shared launcher lock."""

        self._require_lock()

    def _record_path(self, name: str) -> Path:
        return self.lifecycle_root / f"{_process_name(name)}.json"

    def log_path(self, name: str) -> Path:
        """Return the bounded launcher-owned log path for a component."""

        return self.log_root / f"{_process_name(name)}.log"

    def open_log(self, name: str) -> BinaryIO:
        """Open a non-link private append-only log suitable for child stdio."""

        path = self.log_path(name)
        flags = os.O_WRONLY | os.O_APPEND | os.O_CREAT
        if hasattr(os, "O_NOFOLLOW"):
            flags |= os.O_NOFOLLOW
        descriptor = os.open(path, flags, PRIVATE_FILE_MODE)
        try:
            self._validate_owned_private_file(os.fstat(descriptor), f"{name} log")
            return os.fdopen(descriptor, "ab", buffering=0)
        except BaseException:
            os.close(descriptor)
            raise

    def _load_record(self, name: str) -> _StoredRecord | None:
        path = self._record_path(name)
        try:
            descriptor, metadata = state.open_regular_file(
                path,
                f"{name} process record",
                max_bytes=MAX_RECORD_BYTES,
                modes=frozenset({PRIVATE_FILE_MODE}),
            )
        except state.StateError as error:
            if not path.exists() and not path.is_symlink():
                return None
            raise LifecycleError(str(error)) from error
        try:
            source = os.fdopen(descriptor, "rb", closefd=False)
            with source:
                contents = source.read(MAX_RECORD_BYTES + 1)
            current = os.fstat(descriptor)
            if (
                state.file_state(current) != state.file_state(metadata)
                or len(contents) != current.st_size
            ):
                raise LifecycleError(f"{name} process record changed while it was read")
        finally:
            if descriptor >= 0:
                os.close(descriptor)
        try:
            document = json.loads(contents)
        except (UnicodeError, json.JSONDecodeError) as error:
            raise LifecycleError(f"{name} process record is invalid JSON") from error
        expected = {
            "schema_version",
            "name",
            "pid",
            "process_group_id",
            "uid",
            "start_identity",
            "control_socket",
            "control_token",
        }
        if not isinstance(document, dict) or set(document) != expected:
            raise LifecycleError(f"{name} process record has an invalid shape")
        numeric = (document["pid"], document["process_group_id"], document["uid"])
        if any(not isinstance(value, int) or isinstance(value, bool) for value in numeric):
            raise LifecycleError(f"{name} process record has invalid numeric fields")
        start_identity = document["start_identity"]
        control_socket = document["control_socket"]
        control_token = document["control_token"]
        expected_socket = self.lifecycle_root / f"{name}.sock"
        if (
            document["schema_version"] != RECORD_SCHEMA_VERSION
            or document["name"] != name
            or document["pid"] <= 1
            or document["process_group_id"] != document["pid"]
            or document["uid"] != state.current_uid()
            or not isinstance(start_identity, str)
            or not start_identity
            or len(start_identity) > MAX_START_IDENTITY_LENGTH
            or any(ord(character) < 32 or ord(character) == 127 for character in start_identity)
            or not isinstance(control_socket, str)
            or Path(control_socket) != expected_socket
            or not isinstance(control_token, str)
            or len(control_token) != CONTROL_TOKEN_LENGTH
            or any(character not in "0123456789abcdef" for character in control_token)
        ):
            raise LifecycleError(f"{name} process record contains invalid ownership data")
        record = ProcessRecord(
            schema_version=RECORD_SCHEMA_VERSION,
            name=name,
            identity=ProcessIdentity(
                document["pid"],
                document["process_group_id"],
                document["uid"],
                start_identity,
            ),
            control_socket=expected_socket,
            control_token=control_token,
        )
        return _StoredRecord(record, metadata.st_dev, metadata.st_ino)

    @staticmethod
    def _is_exact_process(record: ProcessRecord) -> bool:
        return _process_identity(record.identity.pid) == record.identity

    def _write_process_record(
        self,
        name: str,
        pid: int,
        control_socket: Path,
        control_token: str,
    ) -> ProcessRecord:
        """Capture and atomically publish an owned supervisor identity."""

        self._require_lock()
        name = _process_name(name)
        identity = capture_process_identity(pid)
        expected_socket = self.lifecycle_root / f"{name}.sock"
        if control_socket != expected_socket or len(control_token) != CONTROL_TOKEN_LENGTH:
            raise LifecycleError("supervisor control identity is invalid")
        record = ProcessRecord(
            RECORD_SCHEMA_VERSION,
            name,
            identity,
            control_socket,
            control_token,
        )
        contents = (
            json.dumps(
                {
                    "schema_version": RECORD_SCHEMA_VERSION,
                    "name": name,
                    "pid": identity.pid,
                    "process_group_id": identity.process_group_id,
                    "uid": identity.uid,
                    "start_identity": identity.start_identity,
                    "control_socket": os.fspath(control_socket),
                    "control_token": control_token,
                },
                sort_keys=True,
                separators=(",", ":"),
            )
            + "\n"
        ).encode("utf-8")
        destination = self._record_path(name)
        if not state.publish_exclusive_private_file(destination, contents):
            raise LifecycleError(f"{name} process record already exists")
        return record

    def launch(
        self,
        name: str,
        command: Sequence[str],
        *,
        environment: Mapping[str, str] | None = None,
        working_directory: Path | None = None,
    ) -> ProcessRecord:
        """Launch a command behind a stable, authenticated group supervisor."""

        self._require_lock()
        name = _process_name(name)
        disposition = self.reconcile_before_start(name)
        if disposition is StartDisposition.ALREADY_RUNNING:
            raise LifecycleError(f"{name} is already running")
        control_socket: Path | None = None
        control_token: str | None = None
        record_written = False
        with self.open_log(name) as output:
            try:
                process, control_socket, control_token = supervisor.spawn(
                    self.root,
                    name,
                    command,
                    environment=environment,
                    working_directory=working_directory,
                    output=output,
                )
                record = self._write_process_record(
                    name,
                    process.pid,
                    control_socket,
                    control_token,
                )
                record_written = True
                if supervisor.request(control_socket, control_token, "commit") != "committed":
                    raise LifecycleError(f"{name} supervisor did not commit startup")
                return record
            except (OSError, supervisor.SupervisorError, LifecycleError) as error:
                cleanup_error: BaseException | None = None
                try:
                    if record_written:
                        self.shutdown(
                            name,
                            graceful_timeout_seconds=2,
                            forced_timeout_seconds=2,
                        )
                    elif control_socket is not None and control_token is not None:
                        supervisor.request(
                            control_socket,
                            control_token,
                            "shutdown",
                            graceful_timeout_seconds=2,
                            forced_timeout_seconds=2,
                            timeout_seconds=5,
                        )
                except (LifecycleError, supervisor.SupervisorError) as rollback_error:
                    cleanup_error = rollback_error
                if cleanup_error is not None:
                    raise LifecycleError(
                        f"cannot launch {name}: {error}; startup rollback failed: {cleanup_error}"
                    ) from error
                raise LifecycleError(f"cannot launch {name}: {error}") from error

    def _remove_record(self, stored: _StoredRecord) -> None:
        self._require_lock()
        path = self._record_path(stored.record.name)
        try:
            current = path.lstat()
        except FileNotFoundError:
            return
        if (current.st_dev, current.st_ino) != (stored.device, stored.inode):
            raise LifecycleError(
                f"{stored.record.name} process record changed during reconciliation"
            )
        path.unlink()
        try:
            os.waitpid(stored.record.identity.pid, os.WNOHANG)
        except ChildProcessError:
            pass

    def reconcile_before_start(self, name: str) -> StartDisposition:
        """Detect an existing process or repair a stale ownership record."""

        self._require_lock()
        stored = self._load_record(name)
        if stored is None:
            return StartDisposition.STARTABLE
        if self._is_exact_process(stored.record):
            return StartDisposition.ALREADY_RUNNING
        self._remove_record(stored)
        return StartDisposition.STALE_REPAIRED

    @staticmethod
    def _validate_wait(timeout_seconds: float, poll_interval_seconds: float) -> None:
        values = (timeout_seconds, poll_interval_seconds)
        if not all(
            isinstance(value, (int, float)) and math.isfinite(value)
            for value in values
        ):
            raise LifecycleError("lifecycle waits must use finite numeric values")
        if (
            timeout_seconds < 0
            or timeout_seconds > MAX_WAIT_SECONDS
            or poll_interval_seconds <= 0
            or poll_interval_seconds > MAX_POLL_INTERVAL_SECONDS
        ):
            raise LifecycleError(
                "lifecycle waits must use bounded timeout and poll interval values"
            )

    @staticmethod
    def _wait_for_exit(
        record: ProcessRecord,
        timeout_seconds: float,
        poll_interval_seconds: float,
    ) -> bool:
        deadline = time.monotonic() + timeout_seconds
        while LifecycleState._is_exact_process(record):
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                return False
            time.sleep(min(poll_interval_seconds, remaining))
        return True

    def wait_until_ready(
        self,
        name: str,
        probe: Callable[[float], bool],
        *,
        timeout_seconds: float,
        poll_interval_seconds: float = POLL_INTERVAL_SECONDS,
    ) -> ProcessRecord:
        """Poll readiness while the exact recorded process remains alive.

        The probe receives the remaining overall deadline and must bound its
        own I/O to no more than that value.
        """

        self._require_lock()
        self._validate_wait(timeout_seconds, poll_interval_seconds)
        stored = self._load_record(name)
        if stored is None:
            raise LifecycleError(f"{name} has no process record")
        deadline = time.monotonic() + timeout_seconds
        while True:
            if not self._is_exact_process(stored.record):
                self._remove_record(stored)
                raise LifecycleError(f"{name} exited or its PID identity changed before readiness")
            remaining = max(0.0, deadline - time.monotonic())
            if probe(remaining):
                return stored.record
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise LifecycleError(
                    f"{name} did not become ready within {timeout_seconds} seconds"
                )
            time.sleep(min(poll_interval_seconds, remaining))

    def shutdown(
        self,
        name: str,
        *,
        graceful_timeout_seconds: float,
        forced_timeout_seconds: float,
        poll_interval_seconds: float = POLL_INTERVAL_SECONDS,
    ) -> ShutdownDisposition:
        """Ask the stable group owner to stop all of its descendants."""

        self._require_lock()
        self._validate_wait(graceful_timeout_seconds, poll_interval_seconds)
        self._validate_wait(forced_timeout_seconds, poll_interval_seconds)
        stored = self._load_record(name)
        if stored is None:
            return ShutdownDisposition.ALREADY_STOPPED
        if not self._is_exact_process(stored.record):
            self._remove_record(stored)
            return ShutdownDisposition.STALE_REPAIRED

        try:
            outcome = supervisor.request(
                stored.record.control_socket,
                stored.record.control_token,
                "shutdown",
                graceful_timeout_seconds=graceful_timeout_seconds,
                forced_timeout_seconds=forced_timeout_seconds,
                timeout_seconds=min(
                    MAX_WAIT_SECONDS,
                    graceful_timeout_seconds + forced_timeout_seconds + 5,
                ),
            )
        except supervisor.SupervisorError as error:
            # Never fall back to kill(2) with a persisted numeric PID. If the
            # recorded supervisor vanished, its identity is stale; if it is
            # still alive, losing the authenticated channel is an inconsistency
            # that requires operator inspection rather than a racy signal.
            if not self._is_exact_process(stored.record):
                self._remove_record(stored)
                return ShutdownDisposition.STALE_REPAIRED
            raise LifecycleError(f"cannot stop {name} safely: {error}") from error
        if outcome not in {"graceful", "forced"}:
            raise LifecycleError(f"{name} supervisor returned an invalid shutdown outcome")
        if not self._wait_for_exit(stored.record, forced_timeout_seconds, poll_interval_seconds):
            raise LifecycleError(f"{name} did not exit after forced shutdown")
        self._remove_record(stored)
        if outcome == "forced":
            return ShutdownDisposition.FORCED
        return ShutdownDisposition.GRACEFUL
