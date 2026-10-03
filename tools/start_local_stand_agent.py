#!/usr/bin/env python3
"""Preflight, start, and observe the native local-stand Microsandbox Agent."""

from __future__ import annotations

import argparse
from dataclasses import dataclass
import json
import os
from pathlib import Path
import subprocess
import time

import bootstrap_local_stand as bootstrap
import configure_local_stand_agent as configurator
import local_stand_lifecycle as lifecycle
import local_stand_state as state


AGENT_PROCESS_NAME = "agent"
AGENT_NAME = configurator.LOCAL_AGENT_NAME
AGENTS_PATH = "/api/v1/agents?limit=100"
DOCTOR_TIMEOUT_SECONDS = 30
DEFAULT_STARTUP_TIMEOUT_SECONDS = 60
REGISTRATION_REQUEST_TIMEOUT_SECONDS = 2
EXISTING_REGISTRATION_MAX_AGE_MS = (
    3 * configurator.LOCAL_AGENT_LIMITS["heartbeat_interval_seconds"] * 1000
)
SHUTDOWN_GRACE_SECONDS = 10
SHUTDOWN_FORCE_SECONDS = 5


class AgentStartupError(RuntimeError):
    """The native Agent could not be started without weakening its contract."""


@dataclass(frozen=True)
class AgentStartResult:
    """Secret-free native Agent startup result."""

    state: str
    pid: int
    configuration: Path

    def receipt(self) -> dict[str, object]:
        return {
            "schema_version": 1,
            "state": self.state,
            "pid": self.pid,
            "configuration": str(self.configuration),
        }


def _child_environment() -> dict[str, str]:
    """Return the small non-secret environment needed by native executables."""

    environment: dict[str, str] = {
        "PATH": os.environ.get("PATH", "/usr/bin:/bin"),
        "HOME": str(Path.home()),
    }
    for name in ("LANG", "LC_ALL", "TMPDIR"):
        value = os.environ.get(name)
        if value and "\0" not in value:
            environment[name] = value
    return environment


def _run_doctor(
    owner: lifecycle.LifecycleState,
    msb: Path,
    libkrunfw: Path,
    state_root: Path,
) -> None:
    microsandbox_home = state_root / "microsandbox"
    try:
        with state.private_umask():
            state.ensure_private_directory(microsandbox_home)
        environment = _child_environment()
        environment.update(
            {
                "MSB_HOME": str(microsandbox_home),
                "MSB_LIBKRUNFW_PATH": str(libkrunfw),
            }
        )
        with owner.open_log("microsandbox-preflight") as output:
            result = subprocess.run(
                [str(msb), "doctor"],
                check=False,
                stdin=subprocess.DEVNULL,
                stdout=output,
                stderr=subprocess.STDOUT,
                cwd=owner.root,
                env=environment,
                timeout=DOCTOR_TIMEOUT_SECONDS,
                start_new_session=True,
            )
    except (OSError, subprocess.SubprocessError, state.StateError) as cause:
        raise AgentStartupError(f"cannot run pinned msb doctor: {cause}") from cause
    if result.returncode != 0:
        raise AgentStartupError(
            "pinned msb doctor rejected this host; inspect logs/microsandbox-preflight.log"
        )


class RegistrationObserver:
    """Recognize only the expected online native virtualization Agent."""

    def __init__(self, client: bootstrap.BootstrapClient, not_before_unix_ms: int):
        self._client = client
        self._not_before_unix_ms = not_before_unix_ms

    def registered(self, remaining_seconds: float) -> bool:
        try:
            page = self._client.get(
                AGENTS_PATH,
                timeout_seconds=max(
                    0.001,
                    min(remaining_seconds, REGISTRATION_REQUEST_TIMEOUT_SECONDS),
                ),
            )
        except bootstrap.BootstrapError:
            return False
        if set(page) != {"items", "next_cursor"} or not isinstance(page["items"], list):
            raise AgentStartupError("Agent registration response has an unexpected shape")
        if page["next_cursor"] is not None:
            raise AgentStartupError("Agent registration response exceeds the local stand bound")
        matches = [
            item
            for item in page["items"]
            if isinstance(item, dict) and item.get("name") == AGENT_NAME
        ]
        if len(matches) > 1:
            raise AgentStartupError("server reported duplicate local stand Agents")
        if not matches or matches[0].get("status") != "online":
            return False
        item = matches[0]
        last_seen = item.get("last_seen_at_unix_ms")
        if (
            not isinstance(last_seen, int)
            or isinstance(last_seen, bool)
            or last_seen < self._not_before_unix_ms
        ):
            return False
        inventory = item.get("inventory")
        capacity = item.get("capacity")
        if (
            not isinstance(inventory, dict)
            or inventory.get("host_platform") != configurator.LOCAL_HOST_PLATFORM
        ):
            raise AgentStartupError("registered Agent advertises an unexpected host platform")
        if inventory.get("labels") != configurator.LOCAL_AGENT_LABELS:
            raise AgentStartupError("registered Agent advertises an unexpected execution identity")
        if not isinstance(capacity, dict) or capacity.get("virtualization_available") is not True:
            raise AgentStartupError("registered Agent does not report virtualization availability")
        return True


def _cleanup_failed_start(owner: lifecycle.LifecycleState, cause: BaseException) -> None:
    try:
        owner.shutdown(
            AGENT_PROCESS_NAME,
            graceful_timeout_seconds=SHUTDOWN_GRACE_SECONDS,
            forced_timeout_seconds=SHUTDOWN_FORCE_SECONDS,
        )
    except lifecycle.LifecycleError as cleanup_error:
        raise AgentStartupError(
            f"Agent startup failed ({cause}); cleanup also failed: {cleanup_error}"
        ) from cause


def _start_locked(
    root: Path,
    native_installation: Path,
    *,
    origin: str,
    transport: bootstrap.Transport | None,
    owner: lifecycle.LifecycleState,
    repository: Path,
    home: Path,
    openssl: str,
    startup_timeout_seconds: float,
) -> AgentStartResult:
    owner.require_exclusive_lock()
    if owner.root != root:
        raise AgentStartupError("launcher lock belongs to a different local stand root")
    try:
        receipt = configurator.generate_agent_configuration(
            root,
            native_installation,
            repository=repository,
            home=home,
            openssl=openssl,
        )
        plan = configurator.validated_launch_plan(root, native_installation, receipt)
        client = bootstrap.management_client(
            root,
            origin=origin,
            transport=transport,
            retry=bootstrap.RetryPolicy(
                attempts=1,
                timeout_seconds=REGISTRATION_REQUEST_TIMEOUT_SECONDS,
                initial_delay_seconds=0,
                maximum_delay_seconds=0,
            ),
        )
        _run_doctor(owner, plan.msb, plan.libkrunfw, plan.state_root)
    except (configurator.AgentConfigurationError, bootstrap.BootstrapError, lifecycle.LifecycleError) as cause:
        raise AgentStartupError(str(cause)) from cause

    launched = False
    try:
        now_unix_ms = int(time.time() * 1000)
        disposition = owner.reconcile_before_start(AGENT_PROCESS_NAME)
        if disposition is lifecycle.StartDisposition.ALREADY_RUNNING:
            registration_not_before = now_unix_ms - EXISTING_REGISTRATION_MAX_AGE_MS
            result_state = "already_running"
        else:
            registration_not_before = now_unix_ms
            owner.launch(
                AGENT_PROCESS_NAME,
                [
                    str(plan.agent),
                    "--log-format",
                    "json",
                    "run",
                    str(plan.configuration),
                ],
                environment=_child_environment(),
                working_directory=root,
            )
            launched = True
            result_state = "registered"
        record = owner.wait_until_ready(
            AGENT_PROCESS_NAME,
            RegistrationObserver(client, registration_not_before).registered,
            timeout_seconds=startup_timeout_seconds,
            poll_interval_seconds=0.25,
        )
    except (AgentStartupError, lifecycle.LifecycleError) as cause:
        if launched:
            _cleanup_failed_start(owner, cause)
        raise AgentStartupError(str(cause)) from cause
    return AgentStartResult(result_state, record.identity.pid, plan.configuration)


def start_agent(
    root: Path,
    native_installation: Path,
    *,
    origin: str = bootstrap.DEFAULT_ORIGIN,
    transport: bootstrap.Transport | None = None,
    lifecycle_state: lifecycle.LifecycleState | None = None,
    repository: Path = configurator.REPOSITORY,
    home: Path = Path.home(),
    openssl: str = "openssl",
    startup_timeout_seconds: float = DEFAULT_STARTUP_TIMEOUT_SECONDS,
) -> AgentStartResult:
    """Start or reuse one verified Microsandbox-only Agent under the shared lock."""

    if not 0 < startup_timeout_seconds <= lifecycle.MAX_WAIT_SECONDS:
        raise AgentStartupError("Agent startup timeout is outside the supported bound")
    try:
        root = state.resolve_safe_root(root, repository, home)
        state.validate_private_directory(root)
    except state.StateError as cause:
        raise AgentStartupError(str(cause)) from cause
    if lifecycle_state is None:
        try:
            with lifecycle.LifecycleState(root) as owner:
                return _start_locked(
                    root,
                    native_installation,
                    origin=origin,
                    transport=transport,
                    owner=owner,
                    repository=repository,
                    home=home,
                    openssl=openssl,
                    startup_timeout_seconds=startup_timeout_seconds,
                )
        except lifecycle.LifecycleError as cause:
            raise AgentStartupError(str(cause)) from cause
    return _start_locked(
        root,
        native_installation,
        origin=origin,
        transport=transport,
        owner=lifecycle_state,
        repository=repository,
        home=home,
        openssl=openssl,
        startup_timeout_seconds=startup_timeout_seconds,
    )


def parse_arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--root",
        type=Path,
        default=Path(
            os.environ.get("OCTACITY_LOCAL_STAND_ROOT", state.DEFAULT_ROOT)
        ),
    )
    parser.add_argument("--native-installation", required=True, type=Path)
    parser.add_argument(
        "--origin",
        default=os.environ.get("OCTACITY_LOCAL_STAND_ORIGIN", bootstrap.DEFAULT_ORIGIN),
    )
    parser.add_argument(
        "--startup-timeout-seconds",
        type=float,
        default=DEFAULT_STARTUP_TIMEOUT_SECONDS,
    )
    parser.add_argument("--openssl", default="openssl")
    return parser.parse_args()


def main() -> int:
    arguments = parse_arguments()
    try:
        result = start_agent(
            arguments.root,
            arguments.native_installation,
            origin=arguments.origin,
            openssl=arguments.openssl,
            startup_timeout_seconds=arguments.startup_timeout_seconds,
        )
    except (AgentStartupError, OSError, ValueError) as cause:
        raise SystemExit(f"local stand Agent startup failed: {cause}") from cause
    print(json.dumps(result.receipt(), sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
