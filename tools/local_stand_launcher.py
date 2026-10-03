#!/usr/bin/env python3
"""Own the complete container and native lifecycle of the local stand."""

from __future__ import annotations

import argparse
from dataclasses import dataclass
import json
import os
from pathlib import Path
import shutil
import stat
import subprocess
import sys
import tempfile
import time
from typing import Any, Callable, Sequence

import bootstrap_local_stand as bootstrapper
import build_local_stand_gateway as gateway_builder
import build_local_stand_server as server_builder
import build_pinned_minio as minio_builder
import configure_local_stand_agent as agent_configurator
import configure_local_stand_server as server_configurator
import init_local_stand as initializer
import local_stand_lifecycle as lifecycle
import local_stand_state as state
import stage_local_stand_native as native_stager
import start_local_stand_agent as agent_starter
from local_stand_build_inputs import (
    LocalStandBuildError,
    git_revision,
    load_build_inputs,
    stage_octacity_source,
    stage_octa_source,
    ui_toolchain,
    workspace_version,
)
from pinned_source_archive import SourceArchiveError


REPOSITORY = Path(__file__).resolve().parents[1]
COMPOSE_FILE = REPOSITORY / "compose.yaml"
MANIFEST = REPOSITORY / "deployment/local-stand/inputs.json"
ALLOWLIST = REPOSITORY / "deployment/local-stand/staging-allowlist.json"
AGENT_FIXTURE = REPOSITORY / "deployment/local-stand/fixtures/agent.toml.in"
SERVER_FIXTURE = REPOSITORY / "deployment/local-stand/fixtures/server.toml"
PROJECT_NAME = "octacity-local"
SERVER_IMAGE = "octacity/server:local"
GATEWAY_IMAGE = "octacity/gateway:local"
EXPECTED_SERVICES = frozenset({"postgres", "minio", "minio-init", "server", "gateway"})
LONG_RUNNING_SERVICES = EXPECTED_SERVICES - {"minio-init"}
NAMED_VOLUMES = ("octacity-local_postgres-data", "octacity-local_minio-data")
RESET_CONFIRMATION = "DELETE-OCTACITY-LOCAL-STAND"
DEFAULT_HTTPS_PORT = initializer.GATEWAY_HTTPS_PORT
DEFAULT_COMPOSE_WAIT_SECONDS = 180
DEFAULT_COMMAND_TIMEOUT_SECONDS = 300
MAX_LOG_LINES = 10_000
MAX_LOG_TAIL_BYTES = 1024 * 1024
MAX_COMPOSE_OUTPUT_BYTES = 8 * 1024 * 1024
SHUTDOWN_GRACE_SECONDS = 30
SHUTDOWN_FORCE_SECONDS = 10


class LocalStandError(RuntimeError):
    """The complete stand could not be converged without weakening safety."""


@dataclass(frozen=True)
class NativeInstallation:
    """The verified native input set selected for this repository revision."""

    path: Path
    manifest: dict[str, Any]


def _validated_port(value: int) -> int:
    if isinstance(value, bool) or value != DEFAULT_HTTPS_PORT:
        raise LocalStandError(
            f"gateway HTTPS port must equal the shared service port {DEFAULT_HTTPS_PORT}"
        )
    return value


def _gateway_origin(https_port: int) -> str:
    _validated_port(https_port)
    return initializer.gateway_origin("octacity.localhost")


def _resolved_origin(arguments: argparse.Namespace) -> str:
    expected = _gateway_origin(arguments.https_port)
    if arguments.origin is not None and arguments.origin != expected:
        raise LocalStandError(
            f"management origin must match the published gateway origin {expected}"
        )
    return expected


def _compose_environment(root: Path, https_port: int) -> dict[str, str]:
    return {
        **os.environ,
        "OCTACITY_LOCAL_STAND_ROOT": str(root),
        "OCTACITY_LOCAL_STAND_HTTPS_PORT": str(_validated_port(https_port)),
    }


def _compose_command(*arguments: str) -> list[str]:
    return [
        "docker",
        "compose",
        "--project-name",
        PROJECT_NAME,
        "--file",
        str(COMPOSE_FILE),
        *arguments,
    ]


def _run_compose(
    root: Path,
    https_port: int,
    arguments: Sequence[str],
    *,
    capture_output: bool = False,
    check: bool = True,
    timeout_seconds: float = DEFAULT_COMMAND_TIMEOUT_SECONDS,
) -> subprocess.CompletedProcess[str]:
    try:
        command = _compose_command(*arguments)
        if not capture_output:
            return subprocess.run(
                command,
                check=check,
                text=True,
                env=_compose_environment(root, https_port),
                timeout=timeout_seconds,
            )
        with tempfile.TemporaryFile() as stdout, tempfile.TemporaryFile() as stderr:
            result = subprocess.run(
                command,
                check=False,
                stdout=stdout,
                stderr=stderr,
                env=_compose_environment(root, https_port),
                timeout=timeout_seconds,
            )
            for stream in (stdout, stderr):
                if stream.tell() > MAX_COMPOSE_OUTPUT_BYTES:
                    raise LocalStandError(
                        f"Compose output exceeded {MAX_COMPOSE_OUTPUT_BYTES} bytes"
                    )
                stream.seek(0)
            completed = subprocess.CompletedProcess(
                result.args,
                result.returncode,
                stdout.read().decode("utf-8", errors="replace"),
                stderr.read().decode("utf-8", errors="replace"),
            )
            if check and completed.returncode != 0:
                raise subprocess.CalledProcessError(
                    completed.returncode,
                    completed.args,
                    completed.stdout,
                    completed.stderr,
                )
            return completed
    except FileNotFoundError as cause:
        raise LocalStandError(
            "Docker-compatible CLI is unavailable; start OrbStack and install its docker CLI"
        ) from cause
    except subprocess.TimeoutExpired as cause:
        raise LocalStandError(f"Compose command exceeded {timeout_seconds:g} seconds") from cause


def _prepare_native_installation(root: Path) -> NativeInstallation:
    native_stager.require_native_host()
    revision = git_revision(REPOSITORY)
    document, entries = native_stager.load_policy(
        MANIFEST,
        ALLOWLIST,
        REPOSITORY,
        revision,
    )
    cache_root = root / "native-inputs"
    install_root = root / "native-installations"
    with state.private_umask():
        state.ensure_private_directory(cache_root)
        state.ensure_private_directory(install_root)
    inputs = cache_root / native_stager.input_set_name(document, revision)
    native_stager.prepare_inputs(inputs, document, entries, revision, REPOSITORY)
    installation = install_root / native_stager.installation_name(document, revision)
    native_stager.install_native_bundle(
        inputs,
        installation,
        document,
        entries,
        revision,
        AGENT_FIXTURE,
    )
    return NativeInstallation(installation, document)


def _image_contract_holds(check: Callable[[], None]) -> bool:
    try:
        check()
    except (OSError, subprocess.CalledProcessError, LocalStandBuildError):
        return False
    return True


def _build_images(native: NativeInstallation) -> None:
    """Reuse verified images or stage each shared source context exactly once."""

    document = load_build_inputs(MANIFEST, REPOSITORY)
    revision = git_revision(REPOSITORY)
    version = workspace_version(REPOSITORY)
    node_version, pnpm_version = ui_toolchain(REPOSITORY)
    server_current = _image_contract_holds(
        lambda: server_builder.verify_image(
            SERVER_IMAGE, SERVER_FIXTURE, version, revision
        )
    )
    gateway_current = _image_contract_holds(
        lambda: gateway_builder.verify_image(
            GATEWAY_IMAGE, version, revision, node_version, pnpm_version
        )
    )
    if not server_current or not gateway_current:
        with tempfile.TemporaryDirectory(prefix="octacity-build-sources-") as temporary:
            sources = Path(temporary)
            octa_source = sources / "octa"
            octacity_source = sources / "octacity"
            stage_octa_source(document, octa_source)
            stage_octacity_source(REPOSITORY, octacity_source, revision)
            if not server_current:
                server_builder.build_staged(
                    SERVER_IMAGE,
                    document,
                    SERVER_FIXTURE,
                    octa_source,
                    octacity_source,
                    revision,
                )
            if not gateway_current:
                gateway_builder.build_staged(
                    GATEWAY_IMAGE,
                    document,
                    octa_source,
                    octacity_source,
                    revision,
                )

    minio_images = {
        target: minio_builder.local_image_name(native.manifest, target)
        for target in ("minio", "mc")
    }
    missing = {
        target: image
        for target, image in minio_images.items()
        if not minio_builder.image_is_current(document, target, image, "arm64")
    }
    if missing:
        minio_builder.build_targets(MANIFEST, missing, "arm64")


def _shutdown_agent(owner: lifecycle.LifecycleState) -> lifecycle.ShutdownDisposition:
    return owner.shutdown(
        agent_starter.AGENT_PROCESS_NAME,
        graceful_timeout_seconds=SHUTDOWN_GRACE_SECONDS,
        forced_timeout_seconds=SHUTDOWN_FORCE_SECONDS,
    )


def _compose_down(root: Path, https_port: int, *, remove_volumes: bool) -> None:
    arguments = ["down"]
    if remove_volumes:
        arguments.append("--volumes")
    arguments.extend(("--remove-orphans", "--timeout", str(SHUTDOWN_GRACE_SECONDS)))
    _run_compose(root, https_port, arguments, timeout_seconds=SHUTDOWN_GRACE_SECONDS + 30)


def _stop_halves(
    owner: lifecycle.LifecycleState | None,
    root: Path,
    https_port: int,
    *,
    remove_volumes: bool,
) -> lifecycle.ShutdownDisposition:
    """Attempt both lifecycle shutdowns and report every failure together."""

    disposition = lifecycle.ShutdownDisposition.ALREADY_STOPPED
    errors: list[str] = []
    if owner is not None:
        try:
            disposition = _shutdown_agent(owner)
        except Exception as cause:
            errors.append(f"Agent shutdown failed: {cause}")
    try:
        _compose_down(root, https_port, remove_volumes=remove_volumes)
    except Exception as cause:
        errors.append(f"Compose shutdown failed: {cause}")
    if errors:
        raise LocalStandError("; ".join(errors))
    return disposition


def _up(arguments: argparse.Namespace) -> dict[str, object]:
    root = state.resolve_safe_root(arguments.root, REPOSITORY, Path.home())
    origin = _resolved_origin(arguments)
    initializer.initialize(root, repository=REPOSITORY, openssl=arguments.openssl)
    with lifecycle.LifecycleState(root) as owner:
        native = _prepare_native_installation(root)
        server_configurator.generate_server_configuration(
            root,
            native.path,
            repository=REPOSITORY,
            openssl=arguments.openssl,
        )
        _build_images(native)
        _run_compose(root, arguments.https_port, ("config", "--quiet"))
        try:
            _run_compose(
                root,
                arguments.https_port,
                (
                    "up",
                    "--build",
                    "--detach",
                    "--wait",
                    "--wait-timeout",
                    str(arguments.compose_wait_seconds),
                    "--remove-orphans",
                ),
                timeout_seconds=arguments.compose_wait_seconds + 60,
            )
            bootstrap_result = bootstrapper.bootstrap(
                root,
                origin=origin,
                lifecycle_state=owner,
                repository=REPOSITORY,
            )
            agent_result = agent_starter.start_agent(
                root,
                native.path,
                origin=origin,
                lifecycle_state=owner,
                repository=REPOSITORY,
                openssl=arguments.openssl,
                startup_timeout_seconds=arguments.agent_wait_seconds,
            )
        except BaseException as cause:
            try:
                _stop_halves(
                    owner, root, arguments.https_port, remove_volumes=False
                )
            except LocalStandError as cleanup_error:
                raise LocalStandError(
                    f"stand startup failed ({cause}); cleanup also failed: {cleanup_error}"
                ) from cause
            raise
    return {
        "schema_version": 1,
        "state": "running",
        "root": str(root),
        "url": origin,
        "bootstrap": bootstrap_result.state,
        "agent": agent_result.state,
        "agent_pid": agent_result.pid,
    }


def _with_optional_lifecycle(root: Path) -> lifecycle.LifecycleState | None:
    if not root.exists() and not root.is_symlink():
        return None
    state.validate_private_directory(root)
    return lifecycle.LifecycleState(root)


def _down(arguments: argparse.Namespace) -> dict[str, object]:
    root = state.resolve_safe_root(arguments.root, REPOSITORY, Path.home())
    owner = _with_optional_lifecycle(root)
    if owner is None:
        disposition = _stop_halves(
            None, root, arguments.https_port, remove_volumes=False
        )
    else:
        with owner:
            disposition = _stop_halves(
                owner, root, arguments.https_port, remove_volumes=False
            )
    return {
        "schema_version": 1,
        "state": "stopped",
        "agent": disposition.value,
        "durable_state_preserved": True,
    }


def _compose_state(root: Path, https_port: int) -> tuple[set[str], set[str], set[str]]:
    result = _run_compose(
        root, https_port, ("ps", "--all", "--format", "json"), capture_output=True
    )
    output = result.stdout
    if not output.strip():
        return set(), set(), set()
    decoder = json.JSONDecoder()
    documents: list[object] = []
    cursor = 0
    try:
        while cursor < len(output):
            while cursor < len(output) and output[cursor].isspace():
                cursor += 1
            if cursor == len(output):
                break
            document, cursor = decoder.raw_decode(output, cursor)
            documents.append(document)
    except json.JSONDecodeError as cause:
        raise LocalStandError("Compose returned malformed service state") from cause
    rows = (
        documents[0]
        if len(documents) == 1 and isinstance(documents[0], list)
        else documents
    )
    if any(not isinstance(row, dict) for row in rows):
        raise LocalStandError("Compose returned malformed service state")
    present = {str(row.get("Service", "")) for row in rows if row.get("Service")}
    running = {
        str(row["Service"])
        for row in rows
        if row.get("Service") and row.get("State") == "running"
    }
    healthy = {
        str(row["Service"])
        for row in rows
        if row.get("Service") and row.get("Health") == "healthy"
    }
    return present, running, healthy


def _agent_is_registered(root: Path, https_port: int) -> bool:
    client = bootstrapper.management_client(
        root,
        origin=_gateway_origin(https_port),
        retry=bootstrapper.RetryPolicy(
            attempts=1,
            timeout_seconds=agent_starter.REGISTRATION_REQUEST_TIMEOUT_SECONDS,
            initial_delay_seconds=0,
            maximum_delay_seconds=0,
        ),
    )
    return agent_starter.RegistrationObserver(client, None).registered(
        agent_starter.REGISTRATION_REQUEST_TIMEOUT_SECONDS
    )


def _status(arguments: argparse.Namespace) -> dict[str, object]:
    root = state.resolve_safe_root(arguments.root, REPOSITORY, Path.home())
    owner = _with_optional_lifecycle(root)
    if owner is None:
        present, running, healthy = _compose_state(root, arguments.https_port)
        agent_state = "stopped"
        registered = False
    else:
        with owner:
            present, running, healthy = _compose_state(root, arguments.https_port)
            disposition = owner.reconcile_before_start(agent_starter.AGENT_PROCESS_NAME)
            agent_state = (
                "running"
                if disposition is lifecycle.StartDisposition.ALREADY_RUNNING
                else "stopped"
            )
            registered = agent_state == "running" and _agent_is_registered(
                root, arguments.https_port
            )
    complete = (
        EXPECTED_SERVICES <= present
        and LONG_RUNNING_SERVICES <= running
        and LONG_RUNNING_SERVICES <= healthy
    )
    if complete and agent_state == "running" and registered:
        overall = "running"
    elif not present and not running and agent_state == "stopped":
        overall = "stopped"
    else:
        overall = "partial"
    return {
        "schema_version": 1,
        "state": overall,
        "agent": agent_state,
        "agent_registered": registered,
        "compose": {
            "present": sorted(present),
            "running": sorted(running),
            "healthy": sorted(healthy),
        },
    }


def _read_log_tail(path: Path) -> str:
    flags = os.O_RDONLY
    if hasattr(os, "O_CLOEXEC"):
        flags |= os.O_CLOEXEC
    if hasattr(os, "O_NOFOLLOW"):
        flags |= os.O_NOFOLLOW
    try:
        descriptor = os.open(path, flags)
    except FileNotFoundError:
        return ""
    try:
        metadata = os.fstat(descriptor)
        if (
            not stat.S_ISREG(metadata.st_mode)
            or metadata.st_uid != state.current_uid()
            or metadata.st_nlink != 1
            or stat.S_IMODE(metadata.st_mode) != state.PRIVATE_FILE_MODE
        ):
            raise LocalStandError(f"unsafe local stand log: {path}")
        offset = max(0, metadata.st_size - MAX_LOG_TAIL_BYTES)
        os.lseek(descriptor, offset, os.SEEK_SET)
        contents = os.read(descriptor, MAX_LOG_TAIL_BYTES)
        current = os.fstat(descriptor)
        if (metadata.st_dev, metadata.st_ino) != (current.st_dev, current.st_ino):
            raise LocalStandError(f"local stand log identity changed while reading: {path}")
        return contents.decode("utf-8", errors="replace")
    finally:
        os.close(descriptor)


def _logs(arguments: argparse.Namespace) -> dict[str, object]:
    if not 1 <= arguments.tail <= MAX_LOG_LINES:
        raise LocalStandError(f"log tail must be between 1 and {MAX_LOG_LINES}")
    root = state.resolve_safe_root(arguments.root, REPOSITORY, Path.home())
    owner = _with_optional_lifecycle(root)
    native_logs: list[str] = []
    native_contents: list[tuple[str, str]] = []

    def collect() -> subprocess.CompletedProcess[str]:
        result = _run_compose(
            root,
            arguments.https_port,
            ("logs", "--no-color", "--tail", str(arguments.tail)),
            capture_output=True,
            check=False,
            timeout_seconds=60,
        )
        if owner is not None:
            for name in (agent_starter.AGENT_PROCESS_NAME, "microsandbox-preflight"):
                contents = _read_log_tail(owner.log_path(name))
                if contents:
                    contents = "".join(contents.splitlines(keepends=True)[-arguments.tail :])
                    native_logs.append(name)
                    native_contents.append((name, contents))
        return result

    sys.stdout.write("== Compose services ==\n")
    if owner is None:
        result = collect()
    else:
        with owner:
            result = collect()
    sys.stdout.write(result.stdout)
    if result.stderr:
        sys.stderr.write(result.stderr)
    for name, contents in native_contents:
        sys.stdout.write(f"\n== Native {name} ==\n{contents}")
        if not contents.endswith("\n"):
            sys.stdout.write("\n")
    if result.returncode != 0:
        raise LocalStandError(f"Compose logs failed with status {result.returncode}")
    return {
        "schema_version": 1,
        "compose": "shown",
        "native": native_logs,
        "tail": arguments.tail,
    }


def _reset_targets(root: Path) -> dict[str, object]:
    return {
        "host_state": str(root),
        "microsandbox_state": str(agent_configurator.microsandbox_state_root(root)),
        "named_volumes": list(NAMED_VOLUMES),
    }


def _remove_microsandbox_state(root: Path) -> None:
    runtime = agent_configurator.microsandbox_state_root(root)
    if not runtime.exists() and not runtime.is_symlink():
        return
    state.validate_private_directory(runtime)
    tombstone = runtime.with_name(
        f".{runtime.name}.reset-{os.getpid()}-{time.time_ns()}"
    )
    if tombstone.exists() or tombstone.is_symlink():
        raise LocalStandError(f"reset staging path already exists: {tombstone}")
    runtime.rename(tombstone)
    shutil.rmtree(tombstone)


def _reset(arguments: argparse.Namespace) -> dict[str, object]:
    root = state.resolve_safe_root(arguments.root, REPOSITORY, Path.home())
    targets = _reset_targets(root)
    print(json.dumps({"destructive_reset_targets": targets}, sort_keys=True))
    if arguments.confirm != RESET_CONFIRMATION:
        raise LocalStandError(
            f"reset requires --confirm {RESET_CONFIRMATION}; nothing was deleted"
        )

    tombstone: Path | None = None
    owner = _with_optional_lifecycle(root)
    if owner is None:
        _stop_halves(None, root, arguments.https_port, remove_volumes=True)
    else:
        with owner:
            _stop_halves(owner, root, arguments.https_port, remove_volumes=True)
            tombstone = root.with_name(
                f".{root.name}.reset-{os.getpid()}-{time.time_ns()}"
            )
            if tombstone.exists() or tombstone.is_symlink():
                raise LocalStandError(f"reset staging path already exists: {tombstone}")
            root.rename(tombstone)
    if tombstone is not None:
        shutil.rmtree(tombstone)
    _remove_microsandbox_state(root)
    return {"schema_version": 1, "state": "reset", "removed": targets}


def _positive_bounded_seconds(value: str) -> int:
    parsed = int(value)
    if not 1 <= parsed <= lifecycle.MAX_WAIT_SECONDS:
        raise argparse.ArgumentTypeError(
            f"timeout must be between 1 and {lifecycle.MAX_WAIT_SECONDS} seconds"
        )
    return parsed


def _port(value: str) -> int:
    parsed = int(value)
    if parsed != DEFAULT_HTTPS_PORT:
        raise argparse.ArgumentTypeError(
            f"port must equal the shared service port {DEFAULT_HTTPS_PORT}"
        )
    return parsed


def _add_common_arguments(parser: argparse.ArgumentParser) -> None:
    parser.add_argument(
        "--root",
        type=Path,
        default=Path(os.environ.get("OCTACITY_LOCAL_STAND_ROOT", state.DEFAULT_ROOT)),
        help="private host state root",
    )
    parser.add_argument(
        "--https-port",
        type=_port,
        default=os.environ.get(
            "OCTACITY_LOCAL_STAND_HTTPS_PORT", str(DEFAULT_HTTPS_PORT)
        ),
        help="shared host and container HTTPS port; only %(default)s is supported",
    )


def parse_arguments(arguments: Sequence[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    up = commands.add_parser("up", help="build and converge the complete stand")
    _add_common_arguments(up)
    up.add_argument(
        "--origin",
        default=os.environ.get("OCTACITY_LOCAL_STAND_ORIGIN"),
        help="must match the gateway origin derived from --https-port",
    )
    up.add_argument("--openssl", default="openssl")
    up.add_argument(
        "--compose-wait-seconds",
        type=_positive_bounded_seconds,
        default=DEFAULT_COMPOSE_WAIT_SECONDS,
    )
    up.add_argument(
        "--agent-wait-seconds",
        type=_positive_bounded_seconds,
        default=agent_starter.DEFAULT_STARTUP_TIMEOUT_SECONDS,
    )
    for name, help_text in (
        ("down", "stop the stand while preserving durable state"),
        ("status", "show container and native lifecycle state"),
        ("logs", "show bounded container and native logs"),
        ("reset", "destroy all stand-owned durable state"),
    ):
        command = commands.add_parser(name, help=help_text)
        _add_common_arguments(command)
        if name == "logs":
            command.add_argument("--tail", type=int, default=200)
        if name == "reset":
            command.add_argument("--confirm")
    return parser.parse_args(arguments)


def run(arguments: argparse.Namespace) -> dict[str, object]:
    handlers = {
        "up": _up,
        "down": _down,
        "status": _status,
        "logs": _logs,
        "reset": _reset,
    }
    return handlers[arguments.command](arguments)


def main(arguments: Sequence[str] | None = None) -> int:
    parsed = parse_arguments(arguments)
    try:
        receipt = run(parsed)
    except (
        LocalStandError,
        LocalStandBuildError,
        SourceArchiveError,
        agent_starter.AgentStartupError,
        bootstrapper.BootstrapError,
        lifecycle.LifecycleError,
        native_stager.NativeStageError,
        server_configurator.ServerConfigurationError,
        state.StateError,
        subprocess.CalledProcessError,
        OSError,
        KeyError,
        TypeError,
        ValueError,
    ) as cause:
        print(f"local stand {parsed.command} failed: {cause}", file=sys.stderr)
        return 1
    print(json.dumps(receipt, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
