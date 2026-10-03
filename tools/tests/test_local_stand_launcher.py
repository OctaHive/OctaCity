"""Contract tests for the unified hybrid local-stand launcher."""

from __future__ import annotations

from contextlib import redirect_stdout
import io
import json
import os
from pathlib import Path
import stat
import subprocess
import sys
import tempfile
import types
import unittest
from unittest import mock


REPOSITORY = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPOSITORY / "tools"))

import local_stand_launcher as launcher  # noqa: E402
import local_stand_lifecycle as lifecycle  # noqa: E402


class FakeOwner:
    def __init__(
        self,
        root: Path,
        disposition: lifecycle.StartDisposition = lifecycle.StartDisposition.STARTABLE,
    ):
        self.root = root.resolve()
        self.disposition = disposition

    def __enter__(self):  # type: ignore[no-untyped-def]
        return self

    def __exit__(self, *args):  # type: ignore[no-untyped-def]
        return None

    def reconcile_before_start(self, name: str):  # type: ignore[no-untyped-def]
        return self.disposition

    def log_path(self, name: str) -> Path:
        return self.root / "logs" / f"{name}.log"


class LocalStandLauncherTests(unittest.TestCase):
    def setUp(self) -> None:
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name).resolve() / "stand"

    def arguments(self, command: str, *extra: str):  # type: ignore[no-untyped-def]
        return launcher.parse_arguments((command, "--root", str(self.root), *extra))

    def test_up_orders_complete_convergence_and_validates_compose(self):
        events: list[object] = []
        native = launcher.NativeInstallation(Path("/verified/native"), {"sources": {}})
        owner = FakeOwner(self.root)

        def initialized(*args, **kwargs):  # type: ignore[no-untyped-def]
            events.append("initialize")
            return {}

        def prepare(*args, **kwargs):  # type: ignore[no-untyped-def]
            events.append("prepare-native")
            return native

        def configure(*args, **kwargs):  # type: ignore[no-untyped-def]
            events.append("configure-server")
            return {}

        def build(*args, **kwargs):  # type: ignore[no-untyped-def]
            events.append("build-images")

        def compose(root, port, arguments, **kwargs):  # type: ignore[no-untyped-def]
            events.append(("compose", tuple(arguments)))
            return subprocess.CompletedProcess([], 0, "", "")

        def bootstrap(*args, **kwargs):  # type: ignore[no-untyped-def]
            events.append("bootstrap")
            return types.SimpleNamespace(state="enrolled")

        def start(*args, **kwargs):  # type: ignore[no-untyped-def]
            events.append("start-agent")
            return types.SimpleNamespace(state="registered", pid=4242)

        with (
            mock.patch.object(launcher.initializer, "initialize", side_effect=initialized),
            mock.patch.object(launcher.lifecycle, "LifecycleState", return_value=owner),
            mock.patch.object(launcher, "_prepare_native_installation", side_effect=prepare),
            mock.patch.object(
                launcher.server_configurator,
                "generate_server_configuration",
                side_effect=configure,
            ),
            mock.patch.object(launcher, "_build_images", side_effect=build),
            mock.patch.object(launcher, "_run_compose", side_effect=compose),
            mock.patch.object(launcher.bootstrapper, "bootstrap", side_effect=bootstrap),
            mock.patch.object(launcher.agent_starter, "start_agent", side_effect=start),
        ):
            receipt = launcher.run(self.arguments("up"))

        self.assertEqual(
            events,
            [
                "initialize",
                "prepare-native",
                "configure-server",
                "build-images",
                ("compose", ("config", "--quiet")),
                (
                    "compose",
                    (
                        "up",
                        "--build",
                        "--detach",
                        "--wait",
                        "--wait-timeout",
                        "180",
                        "--remove-orphans",
                    ),
                ),
                "bootstrap",
                "start-agent",
            ],
        )
        self.assertEqual(receipt["state"], "running")
        self.assertEqual(receipt["agent_pid"], 4242)

    def test_image_build_stages_each_shared_context_once(self):
        document = json.loads(launcher.MANIFEST.read_text(encoding="utf-8"))
        native = launcher.NativeInstallation(Path("/verified/native"), document)
        with (
            mock.patch.object(launcher, "load_build_inputs", return_value=document),
            mock.patch.object(launcher, "git_revision", return_value="a" * 40),
            mock.patch.object(launcher, "workspace_version", return_value="0.1.0"),
            mock.patch.object(
                launcher, "ui_toolchain", return_value=("24.21.0", "12.8.1")
            ),
            mock.patch.object(
                launcher.server_builder,
                "verify_image",
                side_effect=launcher.LocalStandBuildError("missing"),
            ),
            mock.patch.object(
                launcher.gateway_builder,
                "verify_image",
                side_effect=launcher.LocalStandBuildError("missing"),
            ),
            mock.patch.object(launcher, "stage_octa_source") as stage_octa,
            mock.patch.object(launcher, "stage_octacity_source") as stage_octacity,
            mock.patch.object(launcher.server_builder, "build_staged") as build_server,
            mock.patch.object(launcher.gateway_builder, "build_staged") as build_gateway,
            mock.patch.object(
                launcher.minio_builder, "image_is_current", return_value=False
            ),
            mock.patch.object(launcher.minio_builder, "build_targets") as build_minio,
        ):
            launcher._build_images(native)

        stage_octa.assert_called_once()
        stage_octacity.assert_called_once()
        build_server.assert_called_once()
        build_gateway.assert_called_once()
        self.assertEqual(build_minio.call_count, 1)
        self.assertEqual(set(build_minio.call_args.args[1]), {"minio", "mc"})

    def test_image_build_reuses_every_verified_local_image(self):
        document = json.loads(launcher.MANIFEST.read_text(encoding="utf-8"))
        native = launcher.NativeInstallation(Path("/verified/native"), document)
        with (
            mock.patch.object(launcher, "load_build_inputs", return_value=document),
            mock.patch.object(launcher, "git_revision", return_value="a" * 40),
            mock.patch.object(launcher, "workspace_version", return_value="0.1.0"),
            mock.patch.object(
                launcher, "ui_toolchain", return_value=("24.21.0", "12.8.1")
            ),
            mock.patch.object(launcher.server_builder, "verify_image"),
            mock.patch.object(launcher.gateway_builder, "verify_image"),
            mock.patch.object(
                launcher.minio_builder, "image_is_current", return_value=True
            ),
            mock.patch.object(launcher, "stage_octa_source") as stage_octa,
            mock.patch.object(launcher.minio_builder, "build_targets") as build_minio,
        ):
            launcher._build_images(native)

        stage_octa.assert_not_called()
        build_minio.assert_not_called()

    def test_failed_up_stops_both_lifecycle_halves_without_removing_volumes(self):
        self.root.mkdir(mode=0o700)
        owner = FakeOwner(self.root)
        events: list[object] = []
        native = launcher.NativeInstallation(Path("/verified/native"), {"sources": {}})

        def compose(root, port, arguments, **kwargs):  # type: ignore[no-untyped-def]
            events.append(("compose", tuple(arguments)))
            return subprocess.CompletedProcess([], 0, "", "")

        with (
            mock.patch.object(launcher.initializer, "initialize", return_value={}),
            mock.patch.object(launcher.lifecycle, "LifecycleState", return_value=owner),
            mock.patch.object(launcher, "_prepare_native_installation", return_value=native),
            mock.patch.object(
                launcher.server_configurator, "generate_server_configuration", return_value={}
            ),
            mock.patch.object(launcher, "_build_images"),
            mock.patch.object(launcher, "_run_compose", side_effect=compose),
            mock.patch.object(
                launcher.bootstrapper,
                "bootstrap",
                side_effect=launcher.bootstrapper.BootstrapError("not ready"),
            ),
            mock.patch.object(
                launcher,
                "_shutdown_agent",
                side_effect=lambda owner: events.append("stop-agent"),
            ),
            mock.patch.object(
                launcher,
                "_compose_down",
                side_effect=lambda root, port, remove_volumes: events.append(
                    ("down", remove_volumes)
                ),
            ),
            self.assertRaisesRegex(launcher.bootstrapper.BootstrapError, "not ready"),
        ):
            launcher.run(self.arguments("up"))

        self.assertEqual(events[-2:], ["stop-agent", ("down", False)])

    def test_failed_up_attempts_compose_cleanup_when_agent_shutdown_fails(self):
        self.root.mkdir(mode=0o700)
        owner = FakeOwner(self.root)
        native = launcher.NativeInstallation(Path("/verified/native"), {"sources": {}})
        compose_down = mock.Mock()
        with (
            mock.patch.object(launcher.initializer, "initialize", return_value={}),
            mock.patch.object(launcher.lifecycle, "LifecycleState", return_value=owner),
            mock.patch.object(launcher, "_prepare_native_installation", return_value=native),
            mock.patch.object(
                launcher.server_configurator, "generate_server_configuration", return_value={}
            ),
            mock.patch.object(launcher, "_build_images"),
            mock.patch.object(
                launcher,
                "_run_compose",
                return_value=subprocess.CompletedProcess([], 0, "", ""),
            ),
            mock.patch.object(
                launcher.bootstrapper,
                "bootstrap",
                side_effect=launcher.bootstrapper.BootstrapError("not ready"),
            ),
            mock.patch.object(
                launcher, "_shutdown_agent", side_effect=RuntimeError("cannot stop")
            ),
            mock.patch.object(launcher, "_compose_down", compose_down),
            self.assertRaisesRegex(launcher.LocalStandError, "cleanup also failed"),
        ):
            launcher.run(self.arguments("up"))

        compose_down.assert_called_once_with(self.root, 8443, remove_volumes=False)

    def test_down_stops_agent_before_compose_and_preserves_state(self):
        self.root.mkdir(mode=0o700)
        owner = FakeOwner(self.root)
        events: list[object] = []
        with (
            mock.patch.object(launcher, "_with_optional_lifecycle", return_value=owner),
            mock.patch.object(
                launcher,
                "_shutdown_agent",
                side_effect=lambda owner: (
                    events.append("agent"),
                    lifecycle.ShutdownDisposition.GRACEFUL,
                )[1],
            ),
            mock.patch.object(
                launcher,
                "_compose_down",
                side_effect=lambda root, port, remove_volumes: events.append(
                    ("compose", remove_volumes)
                ),
            ),
        ):
            receipt = launcher.run(self.arguments("down"))

        self.assertEqual(events, ["agent", ("compose", False)])
        self.assertTrue(receipt["durable_state_preserved"])

    def test_status_reports_container_and_native_halves(self):
        self.root.mkdir(mode=0o700)
        owner = FakeOwner(self.root, lifecycle.StartDisposition.ALREADY_RUNNING)
        with (
            mock.patch.object(
                launcher,
                "_compose_state",
                return_value=(
                    set(launcher.EXPECTED_SERVICES),
                    set(launcher.LONG_RUNNING_SERVICES),
                    set(launcher.LONG_RUNNING_SERVICES),
                ),
            ),
            mock.patch.object(launcher, "_with_optional_lifecycle", return_value=owner),
            mock.patch.object(launcher, "_agent_is_registered", return_value=True),
        ):
            receipt = launcher.run(self.arguments("status"))

        self.assertEqual(receipt["state"], "running")
        self.assertEqual(receipt["agent"], "running")
        self.assertTrue(receipt["agent_registered"])
        self.assertEqual(set(receipt["compose"]["present"]), launcher.EXPECTED_SERVICES)

    def test_status_never_reports_unhealthy_services_as_running(self):
        owner = FakeOwner(self.root, lifecycle.StartDisposition.ALREADY_RUNNING)
        with (
            mock.patch.object(
                launcher,
                "_compose_state",
                return_value=(
                    set(launcher.EXPECTED_SERVICES),
                    set(launcher.LONG_RUNNING_SERVICES),
                    set(),
                ),
            ),
            mock.patch.object(launcher, "_with_optional_lifecycle", return_value=owner),
            mock.patch.object(launcher, "_agent_is_registered", return_value=True),
        ):
            receipt = launcher.run(self.arguments("status"))

        self.assertEqual(receipt["state"], "partial")

    def test_compose_state_uses_structured_health_output(self):
        rows = [
            {"Service": "server", "State": "running", "Health": "healthy"},
            {"Service": "gateway", "State": "running", "Health": "unhealthy"},
            {"Service": "minio-init", "State": "exited", "Health": ""},
        ]
        with mock.patch.object(
            launcher,
            "_run_compose",
            return_value=subprocess.CompletedProcess([], 0, json.dumps(rows), ""),
        ):
            present, running, healthy = launcher._compose_state(self.root, 8443)

        self.assertEqual(present, {"server", "gateway", "minio-init"})
        self.assertEqual(running, {"server", "gateway"})
        self.assertEqual(healthy, {"server"})

    def test_gateway_origin_is_derived_from_the_published_port(self):
        arguments = self.arguments("up", "--https-port", "9443")
        self.assertEqual(launcher._resolved_origin(arguments), "https://octacity.localhost:9443")

        mismatch = self.arguments(
            "up",
            "--https-port",
            "9443",
            "--origin",
            "https://octacity.localhost:8443",
        )
        with self.assertRaisesRegex(launcher.LocalStandError, "must match"):
            launcher._resolved_origin(mismatch)

    def test_logs_include_bounded_compose_and_native_output(self):
        self.root.mkdir(mode=0o700)
        logs = self.root / "logs"
        logs.mkdir(mode=0o700)
        for name, contents in (("agent", "agent line\n"), ("microsandbox-preflight", "doctor line\n")):
            path = logs / f"{name}.log"
            path.write_text(contents, encoding="utf-8")
            path.chmod(0o600)
        owner = FakeOwner(self.root)
        output = io.StringIO()
        with (
            mock.patch.object(
                launcher,
                "_run_compose",
                return_value=subprocess.CompletedProcess([], 0, "compose line\n", ""),
            ) as compose,
            mock.patch.object(launcher, "_with_optional_lifecycle", return_value=owner),
            redirect_stdout(output),
        ):
            receipt = launcher.run(self.arguments("logs", "--tail", "25"))

        self.assertIn("Compose services", output.getvalue())
        self.assertIn("compose line", output.getvalue())
        self.assertIn("agent line", output.getvalue())
        self.assertIn("doctor line", output.getvalue())
        self.assertEqual(receipt["native"], ["agent", "microsandbox-preflight"])
        self.assertEqual(compose.call_args.args[2], ("logs", "--no-color", "--tail", "25"))

    def test_log_reader_returns_only_the_bounded_tail(self):
        path = self.root.parent / "agent.log"
        path.write_bytes(b"x" * (launcher.MAX_LOG_TAIL_BYTES + 17) + b"tail")
        path.chmod(0o600)

        contents = launcher._read_log_tail(path)

        self.assertEqual(len(contents.encode("utf-8")), launcher.MAX_LOG_TAIL_BYTES)
        self.assertTrue(contents.endswith("tail"))

    def test_compose_capture_rejects_oversized_output_without_buffering_it(self):
        def oversized(*args, stdout, stderr, **kwargs):  # type: ignore[no-untyped-def]
            del args, stderr, kwargs
            stdout.write(b"x" * (launcher.MAX_COMPOSE_OUTPUT_BYTES + 1))
            return subprocess.CompletedProcess([], 0)

        with (
            mock.patch.object(subprocess, "run", side_effect=oversized),
            self.assertRaisesRegex(launcher.LocalStandError, "output exceeded"),
        ):
            launcher._run_compose(
                self.root,
                8443,
                ("logs", "--tail", "1"),
                capture_output=True,
            )

    def test_reset_without_exact_confirmation_only_lists_targets(self):
        output = io.StringIO()
        with (
            mock.patch.object(launcher, "_compose_down") as down,
            redirect_stdout(output),
            self.assertRaisesRegex(launcher.LocalStandError, "nothing was deleted"),
        ):
            launcher.run(self.arguments("reset"))

        down.assert_not_called()
        targets = json.loads(output.getvalue())["destructive_reset_targets"]
        self.assertEqual(targets["host_state"], str(self.root))
        self.assertEqual(tuple(targets["named_volumes"]), launcher.NAMED_VOLUMES)

    def test_confirmed_reset_stops_then_removes_volumes_and_host_state(self):
        self.root.mkdir(mode=0o700)
        (self.root / "durable").write_text("state", encoding="utf-8")
        owner = FakeOwner(self.root)
        events: list[object] = []
        with (
            mock.patch.object(launcher, "_with_optional_lifecycle", return_value=owner),
            mock.patch.object(
                launcher,
                "_shutdown_agent",
                side_effect=lambda owner: events.append("agent"),
            ),
            mock.patch.object(
                launcher,
                "_compose_down",
                side_effect=lambda root, port, remove_volumes: events.append(
                    ("compose", remove_volumes)
                ),
            ),
            redirect_stdout(io.StringIO()),
        ):
            receipt = launcher.run(
                self.arguments("reset", "--confirm", launcher.RESET_CONFIRMATION)
            )

        self.assertEqual(events, ["agent", ("compose", True)])
        self.assertFalse(self.root.exists())
        self.assertEqual(receipt["state"], "reset")

    def test_launcher_entrypoint_is_executable(self):
        mode = stat.S_IMODE((REPOSITORY / "tools/local-stand").stat().st_mode)
        self.assertEqual(mode & 0o111, 0o111)


if __name__ == "__main__":
    unittest.main()
