#!/usr/bin/env python3
"""Private process-group supervisor used by the local-stand launcher.

The long-lived supervisor remains the process-group leader until every child
has exited.  A later launcher invocation therefore asks the still-live owner
to stop its own group instead of signalling a numeric PGID after a racy PID
identity check.
"""

from __future__ import annotations

import hmac
import json
import os
from pathlib import Path
import secrets
import signal
import socket
import stat
import subprocess
import sys
import time
from typing import BinaryIO, Mapping, Sequence


CONTROL_SCHEMA_VERSION = 1
CONTROL_MODE = 0o600
MAX_CONTROL_BYTES = 1024 * 1024
MAX_SOCKET_PATH_BYTES = 103
PRECOMMIT_TIMEOUT_SECONDS = 10.0
GROUP_POLL_SECONDS = 0.05


class SupervisorError(RuntimeError):
    """The private supervisor could not safely perform an operation."""


class SpawnedSupervisor:
    """Minimal wait handle for a deliberately detached supervisor process."""

    def __init__(self, pid: int):
        self.pid = pid
        self._return_code: int | None = None

    def poll(self) -> int | None:
        """Reap an early startup failure without owning long-term lifecycle."""

        if self._return_code is not None:
            return self._return_code
        try:
            waited, status_value = os.waitpid(self.pid, os.WNOHANG)
        except ChildProcessError:
            return None
        if waited == 0:
            return None
        self._return_code = os.waitstatus_to_exitcode(status_value)
        return self._return_code

    def terminate(self) -> None:
        """Terminate only an uncommitted supervisor during failed startup."""

        try:
            os.kill(self.pid, signal.SIGTERM)
        except ProcessLookupError:
            pass


def _read_bounded(descriptor: int) -> bytes:
    chunks: list[bytes] = []
    total = 0
    while True:
        chunk = os.read(descriptor, min(64 * 1024, MAX_CONTROL_BYTES + 1 - total))
        if not chunk:
            return b"".join(chunks)
        chunks.append(chunk)
        total += len(chunk)
        if total > MAX_CONTROL_BYTES:
            raise SupervisorError("supervisor payload exceeds its size limit")


def _write_all(descriptor: int, contents: bytes) -> None:
    view = memoryview(contents)
    while view:
        written = os.write(descriptor, view)
        view = view[written:]


def _socket_path(root: Path, name: str) -> Path:
    path = root / "lifecycle" / f"{name}.sock"
    if len(os.fsencode(path)) > MAX_SOCKET_PATH_BYTES:
        raise SupervisorError(
            f"lifecycle control socket path exceeds {MAX_SOCKET_PATH_BYTES} bytes: {path}"
        )
    return path


def _validate_socket(path: Path) -> None:
    try:
        metadata = path.lstat()
    except FileNotFoundError as error:
        raise SupervisorError("supervisor control socket is absent") from error
    if not stat.S_ISSOCK(metadata.st_mode):
        raise SupervisorError("supervisor control path is not a socket")
    if metadata.st_uid != os.getuid() or stat.S_IMODE(metadata.st_mode) != CONTROL_MODE:
        raise SupervisorError("supervisor control socket is not private to the invoking user")


def request(
    path: Path,
    token: str,
    action: str,
    *,
    graceful_timeout_seconds: float = 0.0,
    forced_timeout_seconds: float = 0.0,
    timeout_seconds: float = 5.0,
) -> str:
    """Send one authenticated bounded request to a live supervisor."""

    _validate_socket(path)
    document = {
        "schema_version": CONTROL_SCHEMA_VERSION,
        "token": token,
        "action": action,
        "graceful_timeout_seconds": graceful_timeout_seconds,
        "forced_timeout_seconds": forced_timeout_seconds,
    }
    encoded = (json.dumps(document, separators=(",", ":")) + "\n").encode("utf-8")
    if len(encoded) > MAX_CONTROL_BYTES:
        raise SupervisorError("supervisor request exceeds its size limit")
    try:
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as connection:
            connection.settimeout(timeout_seconds)
            connection.connect(os.fspath(path))
            connection.sendall(encoded)
            chunks: list[bytes] = []
            total = 0
            while True:
                chunk = connection.recv(min(64 * 1024, MAX_CONTROL_BYTES + 1 - total))
                if not chunk:
                    break
                chunks.append(chunk)
                total += len(chunk)
                if total > MAX_CONTROL_BYTES:
                    raise SupervisorError("supervisor response exceeds its size limit")
                if b"\n" in chunk:
                    break
    except (OSError, TimeoutError) as error:
        raise SupervisorError(f"cannot communicate with process supervisor: {error}") from error
    try:
        response = json.loads(b"".join(chunks))
    except (UnicodeError, json.JSONDecodeError) as error:
        raise SupervisorError("process supervisor returned an invalid response") from error
    if not isinstance(response, dict) or set(response) != {"status"}:
        raise SupervisorError("process supervisor returned an invalid response shape")
    status_value = response["status"]
    if not isinstance(status_value, str):
        raise SupervisorError("process supervisor returned an invalid status")
    if status_value.startswith("error:"):
        raise SupervisorError(status_value.removeprefix("error:").strip())
    return status_value


def spawn(
    root: Path,
    name: str,
    command: Sequence[str],
    *,
    environment: Mapping[str, str] | None,
    working_directory: Path | None,
    output: BinaryIO,
) -> tuple[SpawnedSupervisor, Path, str]:
    """Start an uncommitted supervisor and wait for its control socket."""

    if not command or any(not isinstance(argument, str) or "\0" in argument for argument in command):
        raise SupervisorError("supervised command must contain non-empty NUL-free arguments")
    token = secrets.token_hex(32)
    path = _socket_path(root, name)
    payload = {
        "schema_version": CONTROL_SCHEMA_VERSION,
        "token": token,
        "command": list(command),
        "environment": None if environment is None else dict(environment),
        "working_directory": None if working_directory is None else os.fspath(working_directory),
    }
    encoded = json.dumps(payload, separators=(",", ":")).encode("utf-8")
    if len(encoded) > MAX_CONTROL_BYTES:
        raise SupervisorError("supervisor startup payload exceeds its size limit")

    read_descriptor, write_descriptor = os.pipe()
    try:
        os.set_inheritable(read_descriptor, True)
        pid = os.fork()
        if pid == 0:
            try:
                os.setsid()
                null_descriptor = os.open(os.devnull, os.O_RDONLY)
                os.dup2(null_descriptor, 0)
                os.dup2(output.fileno(), 1)
                os.dup2(output.fileno(), 2)
                os.execv(
                    sys.executable,
                    [
                        sys.executable,
                        os.fspath(Path(__file__).resolve()),
                        "_supervise",
                        os.fspath(root),
                        name,
                        str(read_descriptor),
                    ],
                )
            except BaseException:
                os._exit(127)
        process = SpawnedSupervisor(pid)
        os.close(read_descriptor)
        read_descriptor = -1
        try:
            _write_all(write_descriptor, encoded)
        finally:
            os.close(write_descriptor)
            write_descriptor = -1
    finally:
        if read_descriptor >= 0:
            os.close(read_descriptor)
        if write_descriptor >= 0:
            os.close(write_descriptor)

    deadline = time.monotonic() + PRECOMMIT_TIMEOUT_SECONDS
    while True:
        if process.poll() is not None:
            raise SupervisorError("process supervisor exited during startup; inspect its log")
        try:
            if request(path, token, "status", timeout_seconds=0.5) == "uncommitted":
                return process, path, token
        except SupervisorError:
            pass
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            process.terminate()
            raise SupervisorError("process supervisor did not become ready")
        time.sleep(min(GROUP_POLL_SECONDS, remaining))


def _group_members(process_group_id: int, supervisor_pid: int) -> list[int]:
    try:
        result = subprocess.run(
            ["/bin/ps", "-axo", "pid=,pgid=,uid="],
            check=True,
            capture_output=True,
            text=True,
            timeout=5,
            start_new_session=True,
            env={"LC_ALL": "C", "PATH": "/usr/bin:/bin"},
        )
    except (OSError, subprocess.SubprocessError) as error:
        raise SupervisorError(f"cannot inspect supervised process group: {error}") from error
    members: list[int] = []
    for line in result.stdout.splitlines():
        try:
            pid_text, group_text, uid_text = line.split()
            pid, group, uid = int(pid_text), int(group_text), int(uid_text)
        except (TypeError, ValueError):
            continue
        if group == process_group_id and uid == os.getuid() and pid != supervisor_pid:
            members.append(pid)
    return members


def _wait_for_empty_group(
    process_group_id: int,
    timeout_seconds: float,
    child: subprocess.Popen[bytes] | None = None,
) -> bool:
    deadline = time.monotonic() + timeout_seconds
    while True:
        if child is not None:
            child.poll()
        if not _group_members(process_group_id, os.getpid()):
            return True
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            return False
        time.sleep(min(GROUP_POLL_SECONDS, remaining))


def _request_graceful_group_stop(
    process_group_id: int,
    child: subprocess.Popen[bytes],
    timeout_seconds: float,
) -> bool:
    """Signal the owned group and report whether inspection proved it empty."""

    os.killpg(process_group_id, signal.SIGTERM)
    try:
        return _wait_for_empty_group(process_group_id, timeout_seconds, child)
    except SupervisorError:
        # The caller still owns a live, stable group leader and can therefore
        # safely force the group even when host process inspection failed.
        return False


def _send_response(connection: socket.socket, status_value: str) -> None:
    connection.sendall(
        (json.dumps({"status": status_value}, separators=(",", ":")) + "\n").encode("utf-8")
    )


def _decode_request(connection: socket.socket, expected_token: str) -> dict[str, object]:
    connection.settimeout(5)
    chunks: list[bytes] = []
    total = 0
    while True:
        chunk = connection.recv(min(64 * 1024, MAX_CONTROL_BYTES + 1 - total))
        if not chunk:
            break
        chunks.append(chunk)
        total += len(chunk)
        if total > MAX_CONTROL_BYTES:
            raise SupervisorError("supervisor request exceeds its size limit")
        if b"\n" in chunk:
            break
    try:
        document = json.loads(b"".join(chunks))
    except (UnicodeError, json.JSONDecodeError) as error:
        raise SupervisorError("supervisor request is invalid JSON") from error
    expected = {
        "schema_version",
        "token",
        "action",
        "graceful_timeout_seconds",
        "forced_timeout_seconds",
    }
    if not isinstance(document, dict) or set(document) != expected:
        raise SupervisorError("supervisor request has an invalid shape")
    if document["schema_version"] != CONTROL_SCHEMA_VERSION:
        raise SupervisorError("supervisor request has an unsupported schema")
    supplied_token = document["token"]
    if not isinstance(supplied_token, str) or not hmac.compare_digest(supplied_token, expected_token):
        raise SupervisorError("supervisor request is not authenticated")
    if document["action"] not in {"status", "commit", "shutdown"}:
        raise SupervisorError("supervisor request has an invalid action")
    return document


def _bounded_timeout(value: object) -> float:
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise SupervisorError("supervisor timeout is not numeric")
    converted = float(value)
    if not 0 <= converted <= 600:
        raise SupervisorError("supervisor timeout is outside its allowed bound")
    return converted


def _serve(root: Path, name: str, descriptor: int) -> int:
    try:
        payload = json.loads(_read_bounded(descriptor))
    finally:
        os.close(descriptor)
    expected = {"schema_version", "token", "command", "environment", "working_directory"}
    if not isinstance(payload, dict) or set(payload) != expected:
        raise SupervisorError("supervisor startup payload has an invalid shape")
    if payload["schema_version"] != CONTROL_SCHEMA_VERSION:
        raise SupervisorError("supervisor startup payload has an unsupported schema")
    token = payload["token"]
    command = payload["command"]
    environment = payload["environment"]
    working_directory = payload["working_directory"]
    if not isinstance(token, str) or len(token) != 64:
        raise SupervisorError("supervisor startup token is invalid")
    if not isinstance(command, list) or not command or not all(isinstance(item, str) for item in command):
        raise SupervisorError("supervisor startup command is invalid")
    if environment is not None and (
        not isinstance(environment, dict)
        or not all(isinstance(key, str) and isinstance(value, str) for key, value in environment.items())
    ):
        raise SupervisorError("supervisor startup environment is invalid")
    if working_directory is not None and not isinstance(working_directory, str):
        raise SupervisorError("supervisor startup working directory is invalid")

    path = _socket_path(root, name)
    try:
        metadata = path.lstat()
    except FileNotFoundError:
        pass
    else:
        if metadata.st_uid != os.getuid() or not stat.S_ISSOCK(metadata.st_mode):
            raise SupervisorError("refusing to replace an unsafe supervisor control path")
        path.unlink()

    child = subprocess.Popen(
        command,
        stdin=subprocess.DEVNULL,
        env=environment,
        cwd=working_directory,
    )
    # A caught handler (unlike SIG_IGN) is reset to the default by exec, so
    # children receive SIGTERM while their stable group leader stays alive.
    signal.signal(signal.SIGTERM, lambda _signal, _frame: None)
    signal.signal(signal.SIGINT, lambda _signal, _frame: None)
    process_group_id = os.getpgrp()
    committed = False
    precommit_deadline = time.monotonic() + PRECOMMIT_TIMEOUT_SECONDS
    server = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    try:
        server.bind(os.fspath(path))
        os.chmod(path, CONTROL_MODE)
        server.listen(4)
        server.settimeout(0.25)
        while True:
            if not committed and time.monotonic() >= precommit_deadline:
                if not _request_graceful_group_stop(process_group_id, child, 2):
                    os.killpg(process_group_id, signal.SIGKILL)
                return 1
            if child.poll() is not None:
                if not _request_graceful_group_stop(process_group_id, child, 2):
                    os.killpg(process_group_id, signal.SIGKILL)
                return child.returncode or 0
            try:
                connection, _ = server.accept()
            except TimeoutError:
                continue
            with connection:
                try:
                    request_document = _decode_request(connection, token)
                    action = request_document["action"]
                    if action == "status":
                        _send_response(connection, "running" if committed else "uncommitted")
                    elif action == "commit":
                        committed = True
                        _send_response(connection, "committed")
                    else:
                        graceful = _bounded_timeout(request_document["graceful_timeout_seconds"])
                        _bounded_timeout(request_document["forced_timeout_seconds"])
                        if _request_graceful_group_stop(process_group_id, child, graceful):
                            _send_response(connection, "graceful")
                            return 0
                        _send_response(connection, "forced")
                        connection.shutdown(socket.SHUT_WR)
                        time.sleep(0.01)
                        os.killpg(process_group_id, signal.SIGKILL)
                        return 1  # unreachable
                except (OSError, SupervisorError) as error:
                    try:
                        _send_response(connection, f"error: {error}")
                    except OSError:
                        pass
    finally:
        server.close()
        try:
            path.unlink()
        except FileNotFoundError:
            pass


def main(arguments: Sequence[str] | None = None) -> int:
    """Run the private subprocess entry point."""

    values = list(sys.argv[1:] if arguments is None else arguments)
    if len(values) != 4 or values[0] != "_supervise":
        raise SupervisorError("local_stand_supervisor is an internal launcher component")
    return _serve(Path(values[1]), values[2], int(values[3]))


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except SupervisorError as error:
        print(f"supervisor: {error}", file=sys.stderr)
        raise SystemExit(1) from error
