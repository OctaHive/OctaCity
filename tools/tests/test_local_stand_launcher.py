"""Contract tests for the unified hybrid local-stand launcher."""

from __future__ import annotations

from contextlib import redirect_stdout
import io
import json
import os
from pathlib import Path
import shutil
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


class LocalStandTestDependencyTests(unittest.TestCase):
    @unittest.skipUnless(os.name == "posix", "private integration bundles require POSIX permissions")
    def test_compose_boundary_is_skipped_when_docker_is_not_on_path(self):
        with tempfile.TemporaryDirectory() as empty_path:
            result = subprocess.run(
                [sys.executable, "-m", "unittest", "-v",
                 "test_local_stand_launcher.LocalStandLauncherTests."
                 "test_integration_overlay_requires_private_configuration_without_docker",
                 "test_local_stand_launcher.LocalStandLauncherTests."
                 "test_integrations_use_a_private_socket_volume_shared_only_with_the_gateway"],
                cwd=REPOSITORY,
                env={**os.environ, "PATH": empty_path,
                     "PYTHONPATH": str(REPOSITORY / "tools/tests")},
                capture_output=True,
                text=True,
                timeout=30,
            )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("Ran 2 tests", result.stderr)
        self.assertIn("OK (skipped=1)", result.stderr)


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
    def integration_configuration(self) -> Path:
        bundle = self.root / "config/integrations"
        bundle.mkdir(parents=True, mode=0o700)
        config = bundle / "server.toml"
        config.write_text('[integration_management]\npeer_uid=101\n')
        config.chmod(0o600)
        return config

    @unittest.skipUnless(os.name == "posix", "private integration bundles require POSIX permissions")
    def test_integration_overlay_requires_private_configuration_without_docker(self):
        config = self.integration_configuration()
        command = launcher._compose_command(self.root, "config", "--format", "json")
        self.assertIn(str(REPOSITORY / "deployment/local-stand/integrations.compose.yaml"), command)
        config.chmod(0o644)
        with self.assertRaises(launcher.LocalStandError):
            launcher._compose_command(self.root, "up")

    @unittest.skipUnless(os.name == "posix", "private integration bundles require POSIX permissions")
    @unittest.skipUnless(shutil.which("docker"), "Docker Compose is not installed")
    def test_integrations_use_a_private_socket_volume_shared_only_with_the_gateway(self):
        self.integration_configuration()
        command = launcher._compose_command(self.root, "config", "--format", "json")
        rendered = subprocess.run(command, env=launcher._compose_environment(self.root, 8443),
                                  check=True, capture_output=True, text=True)
        services = json.loads(rendered.stdout)["services"]
        readers = {name for name, service in services.items()
                   if any(v["target"] == "/run/octacity-management" for v in service.get("volumes", []))}
        self.assertEqual(readers, {"server", "gateway"})
        gateway_socket = next(v for v in services["gateway"]["volumes"] if v["target"] == "/run/octacity-management")
        self.assertTrue(gateway_socket["read_only"])
        self.assertEqual(services["gateway"]["ports"][0]["host_ip"], "127.0.0.1")
        self.assertEqual(services["server"]["command"], ["run", "/run/octacity-secrets/integration-server.toml"])

    @unittest.skipUnless(os.name == "posix", "private integration bundles require POSIX permissions")
    @unittest.skipUnless(shutil.which("docker"), "Docker Compose is not installed")
    def test_integration_restart_can_reapply_private_volume_permissions(self):
        self.integration_configuration()
        command = launcher._compose_command(self.root, "config", "--format", "json")
        rendered = subprocess.run(command, env=launcher._compose_environment(self.root, 8443),
                                  check=True, capture_output=True, text=True)
        server = json.loads(rendered.stdout)["services"]["server"]
        self.assertEqual(server["cap_drop"], ["ALL"])
        self.assertIn("FOWNER", server["cap_add"],
                      "startup must chmod retained directories already owned by the runtime UID")

    @unittest.skipUnless(os.name == "posix", "private integration bundles require POSIX permissions")
    @unittest.skipUnless(shutil.which("docker"), "Docker Compose is not installed")
    def test_integration_adapters_have_outbound_access_without_exposing_private_services(self):
        self.integration_configuration()
        command = launcher._compose_command(self.root, "config", "--format", "json")
        rendered = subprocess.run(command, env=launcher._compose_environment(self.root, 8443),
                                  check=True, capture_output=True, text=True)
        configuration = json.loads(rendered.stdout)
        networks = configuration["networks"]
        services = configuration["services"]
        self.assertTrue(
            any(not networks[name].get("internal", False)
                for name in services["server"]["networks"]),
            "installed adapters need outbound access to the approved provider service",
        )
        self.assertFalse(services["server"].get("ports"))
        for name in ("postgres", "minio", "minio-init"):
            self.assertTrue(all(networks[network].get("internal", False)
                                for network in services[name]["networks"]), name)
            self.assertFalse(services[name].get("ports"), name)

    def setUp(self) -> None:
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name).resolve() / "stand"

    def arguments(self, command: str, *extra: str):  # type: ignore[no-untyped-def]
        return launcher.parse_arguments((command, "--root", str(self.root), *extra))

    def test_factory_update_preflight_validates_packaged_files_offline_without_durable_mounts(self):
        import local_stand_installation_update as updates

        (self.root / "config/server").mkdir(parents=True)
        (self.root / "secrets").mkdir()
        for name in ("object-access-key", "object-secret-key", "signing-key", "agent-enrollment-key", "cache-credential-key"):
            (self.root / "secrets" / name).write_text("private-canary")
        (self.root / "config/server/postgres-url").write_text("private-database-canary")
        (self.root / "config/server/factory-profiles.json").write_text('{"version":1,"projects":[],"codex":[]}')
        with mock.patch.object(updates.subprocess, "run", return_value=subprocess.CompletedProcess([], 0, "server configuration is valid", "")) as run:
            updates.validate_factory_installation(self.root, launcher.SERVER_IMAGE)
        command = run.call_args.args[0]
        self.assertEqual(command[:5], ["docker", "run", "--rm", "--network", "none"])
        self.assertIn("--read-only", command)
        self.assertEqual(command[-3:], [launcher.SERVER_IMAGE, "validate", "/run/octacity/server.toml"])
        self.assertNotIn("--volume", command)
        self.assertNotIn("--volumes-from", command)
        self.assertNotIn("private-canary", " ".join(command))
        self.assertLessEqual(run.call_args.kwargs["timeout"], 300)
        with mock.patch.object(updates.subprocess, "run", return_value=subprocess.CompletedProcess([], 1, "", "private-canary")):
            with self.assertRaisesRegex(RuntimeError, "rejected.*installation") as error:
                updates.validate_factory_installation(self.root, launcher.SERVER_IMAGE)
        self.assertNotIn("private-canary", str(error.exception))
        with mock.patch.object(updates.subprocess, "run", side_effect=[
            subprocess.TimeoutExpired("private-canary", 120),
            subprocess.CompletedProcess([], 0, "", ""),
        ]) as run:
            with self.assertRaisesRegex(RuntimeError, "preflight timed out") as error:
                updates.validate_factory_installation(self.root, launcher.SERVER_IMAGE)
            launch, cleanup = [call.args[0] for call in run.call_args_list]
            name = launch[launch.index("--name") + 1]
            self.assertTrue(name.startswith("octacity-factory-preflight-"))
            self.assertEqual(cleanup, ["docker", "rm", "--force", name])
            self.assertNotIn("private-canary", str(error.exception))
        self.assertFalse(list(self.root.glob(".factory-preflight-*")))

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

    def test_connector_without_factory_profile_fails_before_creating_stand_state(self):
        connectors = self.root.parent / "connectors.json"
        with mock.patch.object(
            launcher.initializer, "initialize", side_effect=RuntimeError("stand state was mutated")
        ) as initialize:
            with self.assertRaisesRegex(launcher.LocalStandError, "connectors require.*profile"):
                launcher.run(self.arguments("up", "--factory-signal-connectors", str(connectors)))
        initialize.assert_not_called()
        self.assertFalse(self.root.exists())

    def test_source_only_up_installs_before_starting_without_factory_profiles_or_runtime(self):
        import factory_integrations
        events = []
        native = launcher.NativeInstallation(Path("/verified/native"), {"sources": {}})
        with (
            mock.patch.object(launcher.initializer, "initialize"),
            mock.patch.object(launcher.lifecycle, "LifecycleState", return_value=FakeOwner(self.root)),
            mock.patch.object(launcher, "_prepare_native_installation", return_value=native),
            mock.patch.object(launcher.server_configurator, "generate_server_configuration",
                              side_effect=lambda *a, **kw: events.append("configure")) as configure,
            mock.patch.object(launcher, "_build_images", side_effect=lambda *a: events.append("build")),
            mock.patch.object(factory_integrations, "install_repository",
                              side_effect=lambda *a: events.append("install-source")) as install,
            mock.patch.object(launcher, "_run_compose", side_effect=lambda *a, **kw: events.append("compose")),
            mock.patch.object(launcher.bootstrapper, "bootstrap", return_value=types.SimpleNamespace(state="enrolled")),
            mock.patch.object(launcher.agent_starter, "start_agent", return_value=types.SimpleNamespace(state="registered", pid=42)),
        ):
            receipt = launcher.run(self.arguments("up", "--github-repository", "Other/Issues"))
        self.assertEqual(receipt["state"], "running")
        self.assertEqual(events, ["configure", "build", "install-source", "compose", "compose"])
        install.assert_called_once_with(self.root, "Other/Issues")
        self.assertIsNone(configure.call_args.kwargs["factory_profiles"])
        self.assertIsNone(configure.call_args.kwargs["factory_runtime_bundle"])

    def test_explicit_factory_profiles_reach_server_configuration(self):
        profiles = self.root.parent / "profiles.json"
        connectors = self.root.parent / "connectors.json"
        delivery = self.root.parent / "delivery-installation"
        runtime = self.root.parent / "runtime-installation"
        arguments = self.arguments("up", "--factory-profiles", str(profiles),
                                   "--factory-signal-connectors", str(connectors),
                                   "--factory-delivery-bundle", str(delivery),
                                   "--factory-runtime-bundle", str(runtime))
        native = launcher.NativeInstallation(Path("/verified/native"), {"sources": {}})
        with (
            mock.patch.object(launcher.initializer, "initialize"),
            mock.patch.object(launcher.lifecycle, "LifecycleState", return_value=FakeOwner(self.root)),
            mock.patch.object(launcher, "_prepare_native_installation", return_value=native),
            mock.patch.object(launcher.server_configurator, "generate_server_configuration") as configure,
            mock.patch.object(launcher, "_build_images"),
            mock.patch.object(launcher.installation_update, "validate_factory_installation") as preflight,
            mock.patch.object(launcher, "_run_compose"),
            mock.patch.object(launcher.bootstrapper, "bootstrap", return_value=types.SimpleNamespace(state="enrolled")),
            mock.patch.object(launcher.agent_starter, "start_agent", return_value=types.SimpleNamespace(state="registered", pid=42)),
        ):
            launcher.run(arguments)
        self.assertEqual(configure.call_args.kwargs["factory_profiles"], profiles)
        self.assertEqual(configure.call_args.kwargs["factory_signal_connectors"], connectors)
        self.assertEqual(configure.call_args.kwargs["factory_delivery_bundle"], delivery)
        self.assertEqual(configure.call_args.kwargs["factory_runtime_bundle"], runtime)
        preflight.assert_called_once_with(self.root.resolve(), launcher.SERVER_IMAGE)

    def test_running_default_stand_does_not_ignore_requested_factory_installation(self):
        profiles = self.root.parent / "profiles.json"
        native = launcher.NativeInstallation(Path("/verified/native"), {"sources": {}})
        with (
            mock.patch.object(launcher.initializer, "initialize"),
            mock.patch.object(launcher.lifecycle, "LifecycleState", return_value=FakeOwner(self.root, lifecycle.StartDisposition.ALREADY_RUNNING)),
            mock.patch.object(launcher, "_prepare_native_installation", return_value=native),
            mock.patch.object(
                launcher.server_configurator, "generate_server_configuration",
                side_effect=launcher.server_configurator.ServerConfigurationError("Factory mode change requires reset"),
            ) as configure,
            mock.patch.object(launcher, "_run_compose") as compose,
            mock.patch.object(launcher, "_build_images") as build,
            mock.patch.object(launcher.agent_starter, "start_agent") as start,
        ):
            with self.assertRaisesRegex(launcher.server_configurator.ServerConfigurationError, "Factory.*reset"):
                launcher.run(self.arguments("up", "--factory-profiles", str(profiles)))
        self.assertEqual(configure.call_args.kwargs["factory_profiles"], profiles)
        compose.assert_not_called()
        build.assert_not_called()
        start.assert_not_called()

    def test_invalid_factory_agent_profile_fails_before_any_lifecycle_mutation(self):
        policy = self.root.parent / "invalid-agent-policy.toml"
        policy.write_text('version = 1\nprivate_canary = "do-not-disclose"\n')
        policy.chmod(0o600)
        with (
            mock.patch.object(launcher.initializer, "initialize") as initialize,
            mock.patch.object(launcher, "_prepare_native_installation") as native,
            mock.patch.object(launcher, "_run_compose") as compose,
            mock.patch.object(launcher.agent_starter, "start_agent") as start,
        ):
            with self.assertRaisesRegex(launcher.LocalStandError, "Factory Agent profile is invalid") as error:
                launcher.run(self.arguments("up", "--factory-agent-profile", str(policy)))
        self.assertNotIn("do-not-disclose", str(error.exception))
        initialize.assert_not_called()
        native.assert_not_called()
        compose.assert_not_called()
        start.assert_not_called()

    def test_factory_agent_preflight_failure_prevents_compose_enrollment_and_agent_start(self):
        policy = self.root.parent / "policy.toml"
        native = launcher.NativeInstallation(Path("/verified/native"), {"sources": {}})
        with (
            mock.patch.object(launcher.agent_configurator, "read_factory_agent_profile", return_value={}),
            mock.patch.object(launcher.initializer, "initialize"),
            mock.patch.object(launcher.lifecycle, "LifecycleState", return_value=FakeOwner(self.root)),
            mock.patch.object(launcher, "_prepare_native_installation", return_value=native),
            mock.patch.object(launcher.agent_configurator, "preflight_factory_agent_profile", create=True,
                              side_effect=launcher.agent_configurator.AgentConfigurationError("Factory Agent preflight failed")) as preflight,
            mock.patch.object(launcher, "_build_images") as build,
            mock.patch.object(launcher, "_run_compose") as compose,
            mock.patch.object(launcher.bootstrapper, "bootstrap") as bootstrap,
            mock.patch.object(launcher.agent_starter, "start_agent") as start,
            mock.patch.object(launcher.server_configurator, "generate_server_configuration"),
        ):
            with self.assertRaisesRegex(launcher.LocalStandError, "Factory Agent preflight failed"):
                launcher.run(self.arguments("up", "--factory-agent-profile", str(policy)))
        preflight.assert_called_once()
        build.assert_not_called()
        compose.assert_not_called()
        bootstrap.assert_not_called()
        start.assert_not_called()

    def test_installed_factory_agent_requires_explicit_policy_on_repeated_up(self):
        installed = self.root / "config/factory-agent-profile.toml"
        installed.parent.mkdir(parents=True)
        installed.write_text("version = 1\n")
        installed.chmod(0o600)
        with (
            mock.patch.object(launcher.initializer, "initialize") as initialize,
            mock.patch.object(launcher.lifecycle, "LifecycleState", return_value=FakeOwner(self.root)),
            mock.patch.object(launcher, "_prepare_native_installation"),
            mock.patch.object(launcher, "_run_compose") as compose,
            mock.patch.object(launcher.bootstrapper, "bootstrap"),
            mock.patch.object(launcher.agent_starter, "start_agent"),
            mock.patch.object(launcher.server_configurator, "generate_server_configuration"),
            mock.patch.object(launcher, "_build_images"),
        ):
            with self.assertRaisesRegex(launcher.LocalStandError, "factory-agent-profile"):
                launcher.run(self.arguments("up"))
        initialize.assert_not_called()
        compose.assert_not_called()

    @unittest.skipUnless(os.name == "posix", "private Factory bundles require POSIX ownership and modes")
    def test_all_compose_operations_reuse_only_an_installed_private_factory_bundle(self):
        profile = self.root / "config/server/factory-profiles.json"
        profile.parent.mkdir(parents=True)
        with mock.patch.object(launcher.subprocess, "run", return_value=subprocess.CompletedProcess([], 0)) as run:
            launcher._run_compose(self.root, 8443, ("status",))
        self.assertEqual(run.call_args.args[0].count("--file"), 1)
        profile.write_text('{"version":1}\n')
        profile.chmod(0o600)
        for operation in ("up", "down", "ps", "logs"):
            with self.subTest(operation=operation):
                with mock.patch.object(launcher.subprocess, "run", return_value=subprocess.CompletedProcess([], 0)) as run:
                    launcher._run_compose(self.root, 8443, (operation,))
                command = run.call_args.args[0]
                self.assertEqual(command.count("--file"), 2)
                self.assertIn(str(REPOSITORY / "deployment/local-stand/factory.compose.yaml"), command)
        profile.chmod(0o644)
        with mock.patch.object(launcher.subprocess, "run") as run:
            with self.assertRaises(launcher.LocalStandError):
                launcher._run_compose(self.root, 8443, ("up",))
        run.assert_not_called()

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

    def test_compose_state_accepts_compose_v5_json_lines(self):
        rows = [
            {"Service": "server", "State": "running", "Health": "healthy"},
            {"Service": "minio-init", "State": "exited", "Health": ""},
        ]
        output = "\n".join(json.dumps(row) for row in rows)
        with mock.patch.object(
            launcher,
            "_run_compose",
            return_value=subprocess.CompletedProcess([], 0, output, ""),
        ):
            present, running, healthy = launcher._compose_state(self.root, 8443)

        self.assertEqual(present, {"server", "minio-init"})
        self.assertEqual(running, {"server"})
        self.assertEqual(healthy, {"server"})

    def test_gateway_origin_uses_the_shared_host_and_container_port(self):
        arguments = self.arguments("up")
        self.assertEqual(
            launcher._resolved_origin(arguments), "https://octacity.localhost:8443"
        )

        with self.assertRaises(SystemExit):
            self.arguments("up", "--https-port", "9443")

        mismatch = self.arguments("up", "--origin", "https://octacity.localhost:9443")
        with self.assertRaisesRegex(launcher.LocalStandError, "must match"):
            launcher._resolved_origin(mismatch)

    @unittest.skipUnless(
        os.name == "posix", "local stand log ownership requires POSIX permissions"
    )
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

    @unittest.skipUnless(
        os.name == "posix", "local stand log ownership requires POSIX permissions"
    )
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
        microsandbox_state = self.root.parent / "microsandbox-state"
        with (
            mock.patch.object(launcher, "_compose_down") as down,
            mock.patch.object(
                launcher.agent_configurator,
                "microsandbox_state_root",
                return_value=microsandbox_state,
            ),
            redirect_stdout(output),
            self.assertRaisesRegex(launcher.LocalStandError, "nothing was deleted"),
        ):
            launcher.run(self.arguments("reset"))

        down.assert_not_called()
        targets = json.loads(output.getvalue())["destructive_reset_targets"]
        self.assertEqual(targets["host_state"], str(self.root))
        self.assertEqual(targets["microsandbox_state"], str(microsandbox_state))
        self.assertEqual(tuple(targets["named_volumes"]), launcher.NAMED_VOLUMES)

    def test_confirmed_reset_stops_then_removes_volumes_and_host_state(self):
        self.root.mkdir(mode=0o700)
        (self.root / "durable").write_text("state", encoding="utf-8")
        microsandbox_state = self.root.parent / "microsandbox-state"
        microsandbox_state.mkdir(mode=0o700)
        self.addCleanup(shutil.rmtree, microsandbox_state, True)
        owner = FakeOwner(self.root)
        events: list[object] = []
        with (
            mock.patch.object(
                launcher.agent_configurator,
                "microsandbox_state_root",
                return_value=microsandbox_state,
            ),
            mock.patch.object(launcher.state, "validate_private_directory"),
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
        self.assertFalse(microsandbox_state.exists())
        self.assertEqual(receipt["state"], "reset")

    def test_launcher_entrypoint_is_executable(self):
        entrypoint = REPOSITORY / "tools/local-stand"
        if os.name == "posix":
            mode = stat.S_IMODE(entrypoint.stat().st_mode)
            self.assertEqual(mode & 0o111, 0o111)
            return

        tracked = subprocess.run(
            ["git", "ls-files", "--stage", "--", "tools/local-stand"],
            cwd=REPOSITORY,
            check=True,
            capture_output=True,
            text=True,
        )
        self.assertTrue(tracked.stdout.startswith("100755 "), tracked.stdout)


if __name__ == "__main__":
    unittest.main()
