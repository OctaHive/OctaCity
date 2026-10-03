"""Contract tests for fail-closed native local-stand Agent startup."""

from __future__ import annotations

import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import unittest
from unittest import mock


REPOSITORY = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPOSITORY / "tools"))

import bootstrap_local_stand as bootstrap  # noqa: E402
import configure_local_stand_agent as configurator  # noqa: E402
import local_stand_lifecycle as lifecycle  # noqa: E402
import start_local_stand_agent as starter  # noqa: E402




def response(document: dict[str, object]) -> bootstrap.HttpResponse:
    return bootstrap.HttpResponse(200, {}, json.dumps(document).encode("utf-8"))


class AgentTransport:
    def __init__(self, pages: list[dict[str, object]]):
        self.pages = pages
        self.calls: list[tuple[str, str, dict[str, str], float]] = []

    def request(self, method, path, headers, body, timeout_seconds):  # type: ignore[no-untyped-def]
        self.calls.append((method, path, dict(headers), timeout_seconds))
        if body is not None or path != "/api/v1/agents?limit=100":
            raise AssertionError(f"unexpected request: {method} {path}")
        page = self.pages.pop(0) if len(self.pages) > 1 else self.pages[0]
        return response(page)


class FakeLifecycle:
    def __init__(
        self,
        root: Path,
        *,
        disposition: lifecycle.StartDisposition = lifecycle.StartDisposition.STARTABLE,
        launch_error: Exception | None = None,
        wait_error: Exception | None = None,
    ):
        self.root = root.resolve()
        self.disposition = disposition
        self.launch_error = launch_error
        self.wait_error = wait_error
        self.launched: tuple[list[str], dict[str, str], Path] | None = None
        self.shutdown_calls = 0
        self.record = root / "lifecycle/agent.json"

    def require_exclusive_lock(self) -> None:
        return None

    def open_log(self, name: str):  # type: ignore[no-untyped-def]
        log = self.root / "logs" / f"{name}.log"
        log.parent.mkdir(parents=True, exist_ok=True)
        return log.open("ab")

    def reconcile_before_start(self, name: str):  # type: ignore[no-untyped-def]
        return self.disposition

    def launch(self, name, command, *, environment, working_directory):  # type: ignore[no-untyped-def]
        if self.launch_error is not None:
            raise self.launch_error
        self.record.parent.mkdir(parents=True, exist_ok=True)
        self.record.write_text("owned\n", encoding="utf-8")
        self.record.chmod(0o600)
        self.launched = (list(command), dict(environment), working_directory)
        return mock.Mock(identity=mock.Mock(pid=4242))

    def wait_until_ready(self, name, probe, *, timeout_seconds, poll_interval_seconds):  # type: ignore[no-untyped-def]
        if self.wait_error is not None:
            raise self.wait_error
        if not probe(min(timeout_seconds, 1.0)):
            raise lifecycle.LifecycleError("agent did not become ready")
        return mock.Mock(identity=mock.Mock(pid=4242))

    def shutdown(self, name, **kwargs):  # type: ignore[no-untyped-def]
        self.shutdown_calls += 1
        self.record.unlink(missing_ok=True)
        return lifecycle.ShutdownDisposition.GRACEFUL


@unittest.skipUnless(os.name == "posix", "native Agent startup requires POSIX ownership")
class NativeAgentStartupTests(unittest.TestCase):
    def setUp(self) -> None:
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        temporary_root = Path(temporary.name).resolve()
        self.root = temporary_root / "stand"
        for relative in ("secrets", "agent/state", "config", "logs"):
            (self.root / relative).mkdir(parents=True, mode=0o700)
        self.root.chmod(0o700)
        self.installation = temporary_root / "native"
        self.agent = self.installation / "agent/bin/octacity-agent"
        self.msb = self.installation / "microsandbox/bin/msb"
        self.libkrunfw = self.installation / "microsandbox/lib/libkrunfw.5.dylib"
        for path in (self.agent, self.msb, self.libkrunfw):
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(b"fixture")
            path.chmod(0o755 if path != self.libkrunfw else 0o644)
        manifest = self.installation / "installation-manifest.json"
        manifest.write_text(
            json.dumps({"microsandbox_version": "0.7.6"}) + "\n",
            encoding="utf-8",
        )
        manifest.chmod(0o600)
        self.config = self.root / "config/agent.toml"
        self.config.write_text(
            "\n".join(
                (
                    'agent_id = "local-stand-agent"',
                    'enabled_runtime_modes = []',
                    'allow_native_execution = false',
                    'allow_host_execution = false',
                    'oci_engines = []',
                    'isolation_providers = []',
                    "[[virtualization_providers]]",
                    'provider = "microsandbox"',
                    'environment_identity = "microsandbox-0.7.6-linux-arm64-v1"',
                    f'executable = "{self.msb}"',
                    f'libkrunfw = "{self.libkrunfw}"',
                    "metrics_sample_interval_seconds = 1",
                    "[labels]",
                    'os = "macos"',
                    'arch = "arm64"',
                    'guest = "linux-arm64"',
                    'environment = "local-stand"',
                    "",
                )
            ),
            encoding="utf-8",
        )
        self.config.chmod(0o600)
        self.receipt = {
            "config": str(self.config),
            "state_root": str(self.root / "agent/state"),
        }
        self.online_page = {
            "items": [
                {
                    "id": "77777777-7777-4777-8777-777777777777",
                    "name": "local-stand-agent",
                    "version": 1,
                    "pool_id": "66666666-6666-4666-8666-666666666666",
                    "pool_version": 1,
                    "inventory": {
                        "host_platform": {"os": "macos", "architecture": "arm64"},
                        "labels": {
                            "os": "macos",
                            "arch": "arm64",
                            "guest": "linux-arm64",
                            "environment": "local-stand",
                        },
                    },
                    "capacity": {"virtualization_available": True},
                    "status": "online",
                    "last_seen_at_unix_ms": int(time.time() * 1000) + 60_000,
                }
            ],
            "next_cursor": None,
        }

    def start(self, owner: FakeLifecycle, transport: AgentTransport):
        with (
            mock.patch.object(
                configurator,
                "generate_agent_configuration",
                return_value=self.receipt,
            ) as generate,
            mock.patch.object(
                starter.subprocess,
                "run",
                return_value=subprocess.CompletedProcess([], 0),
            ) as run,
        ):
            result = starter.start_agent(
                self.root,
                self.installation,
                transport=transport,
                lifecycle_state=owner,
                repository=REPOSITORY,
                home=Path.home(),
                startup_timeout_seconds=2,
            )
        return result, generate, run

    def test_preflights_launches_exact_agent_and_waits_for_online_registration(self):
        owner = FakeLifecycle(self.root)
        transport = AgentTransport([self.online_page])

        result, generate, doctor = self.start(owner, transport)

        self.assertEqual(result.state, "registered")
        self.assertEqual(result.pid, 4242)
        generate.assert_called_once()
        doctor.assert_called_once()
        arguments = doctor.call_args.args[0]
        self.assertEqual(arguments, [str(self.msb), "doctor"])
        environment = doctor.call_args.kwargs["env"]
        self.assertEqual(environment["MSB_HOME"], str(self.root / "agent/state/microsandbox"))
        self.assertEqual(environment["MSB_LIBKRUNFW_PATH"], str(self.libkrunfw))
        self.assertEqual(
            owner.launched[0],
            [str(self.agent), "--log-format", "json", "run", str(self.config)],
        )
        self.assertEqual(owner.shutdown_calls, 0)
        self.assertNotIn("Authorization", transport.calls[0][2])
        self.assertFalse(any(key.endswith("_PROXY") for key in owner.launched[1]))

    def test_repeated_start_reuses_the_exact_running_agent(self):
        owner = FakeLifecycle(
            self.root,
            disposition=lifecycle.StartDisposition.ALREADY_RUNNING,
        )

        result, generate, doctor = self.start(owner, AgentTransport([self.online_page]))

        self.assertEqual(result.state, "already_running")
        self.assertEqual(result.pid, 4242)
        generate.assert_called_once()
        doctor.assert_called_once()
        self.assertIsNone(owner.launched)
        self.assertEqual(owner.shutdown_calls, 0)

    def test_fallback_configuration_is_rejected_before_doctor_or_launch(self):
        contents = self.config.read_text(encoding="utf-8").replace(
            "isolation_providers = []", 'isolation_providers = ["containerd"]'
        )
        self.config.write_text(contents, encoding="utf-8")
        owner = FakeLifecycle(self.root)

        with (
            mock.patch.object(
                configurator, "generate_agent_configuration", return_value=self.receipt
            ),
            mock.patch.object(starter.subprocess, "run") as doctor,
            self.assertRaisesRegex(starter.AgentStartupError, "fallback"),
        ):
            starter.start_agent(
                self.root,
                self.installation,
                transport=AgentTransport([self.online_page]),
                lifecycle_state=owner,
                repository=REPOSITORY,
                home=Path.home(),
            )

        doctor.assert_not_called()
        self.assertIsNone(owner.launched)
        self.assertFalse(owner.record.exists())

    def test_doctor_failure_never_creates_ownership(self):
        owner = FakeLifecycle(self.root)
        with (
            mock.patch.object(
                configurator, "generate_agent_configuration", return_value=self.receipt
            ),
            mock.patch.object(
                starter.subprocess,
                "run",
                return_value=subprocess.CompletedProcess([], 9),
            ),
            self.assertRaisesRegex(starter.AgentStartupError, "msb doctor"),
        ):
            starter.start_agent(
                self.root,
                self.installation,
                transport=AgentTransport([self.online_page]),
                lifecycle_state=owner,
                repository=REPOSITORY,
                home=Path.home(),
            )
        self.assertIsNone(owner.launched)
        self.assertFalse(owner.record.exists())

    def test_registration_failure_stops_agent_and_removes_ownership(self):
        owner = FakeLifecycle(
            self.root, wait_error=lifecycle.LifecycleError("registration timed out")
        )

        with self.assertRaisesRegex(starter.AgentStartupError, "registration timed out"):
            self.start(owner, AgentTransport([{"items": [], "next_cursor": None}]))

        self.assertEqual(owner.shutdown_calls, 1)
        self.assertFalse(owner.record.exists())

    def test_spawn_failure_leaves_no_ownership_record(self):
        owner = FakeLifecycle(
            self.root, launch_error=lifecycle.LifecycleError("spawn rejected")
        )

        with self.assertRaisesRegex(starter.AgentStartupError, "spawn rejected"):
            self.start(owner, AgentTransport([self.online_page]))

        self.assertEqual(owner.shutdown_calls, 0)
        self.assertFalse(owner.record.exists())

    def test_wrong_registered_capability_fails_and_is_cleaned_up(self):
        page = json.loads(json.dumps(self.online_page))
        page["items"][0]["inventory"]["host_platform"]["os"] = "linux"
        owner = FakeLifecycle(self.root)

        with self.assertRaisesRegex(starter.AgentStartupError, "platform"):
            self.start(owner, AgentTransport([page]))

        self.assertEqual(owner.shutdown_calls, 1)
        self.assertFalse(owner.record.exists())

    def test_stale_online_record_cannot_satisfy_new_process_readiness(self):
        page = json.loads(json.dumps(self.online_page))
        page["items"][0]["last_seen_at_unix_ms"] = 1
        owner = FakeLifecycle(self.root)

        with self.assertRaisesRegex(starter.AgentStartupError, "did not become ready"):
            self.start(owner, AgentTransport([page]))

        self.assertEqual(owner.shutdown_calls, 1)
        self.assertFalse(owner.record.exists())


if __name__ == "__main__":
    unittest.main()
