"""Contract and opt-in persistence tests for local-stand infrastructure."""

from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import secrets
import shutil
import socket
import stat
import subprocess
import sys
import tempfile
import time
import unittest
import uuid


REPOSITORY = Path(__file__).resolve().parents[2]
COMPOSE = REPOSITORY / "compose.yaml"
MANIFEST = REPOSITORY / "deployment/local-stand/inputs.json"
RUN_INTEGRATION = os.environ.get("OCTACITY_RUN_COMPOSE_INTEGRATION") == "1"
RUN_SERVER_INTEGRATION = (
    os.environ.get("OCTACITY_RUN_COMPOSE_SERVER_INTEGRATION") == "1"
)
sys.path.insert(0, str(REPOSITORY / "tools"))
import build_pinned_minio as minio_builder  # noqa: E402
import configure_local_stand_server as server_configurator  # noqa: E402
import init_local_stand as stand_initializer  # noqa: E402

MC_AUTHENTICATION = (
    'user="$(cat /run/secrets/object-access-key)"; '
    'password="$(cat /run/secrets/object-secret-key)"; '
    'export MC_CONFIG_DIR="/tmp/.mc"; '
    'export MC_HOST_local="http://${user}:${password}@127.0.0.1:9000"; '
)


def compose_environment(root: Path) -> dict[str, str]:
    """Return an isolated Compose environment without secret values."""

    return {**os.environ, "OCTACITY_LOCAL_STAND_ROOT": str(root)}


def compose_command(project: str, *arguments: str) -> list[str]:
    """Build the one canonical Compose invocation used by the tests."""

    return [
        "docker",
        "compose",
        "--project-name",
        project,
        "--file",
        str(COMPOSE),
        *arguments,
    ]


def write_private_credential(path: Path, value: str) -> None:
    """Create one test-only credential with the production host-file mode."""

    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(descriptor, "w", encoding="ascii") as destination:
        destination.write(f"{value}\n")


class ComposeProjectMixin:
    """Own one isolated Compose project and remove all of its test state."""

    def configure_compose_project(self, root: Path, prefix: str) -> None:
        self.root = root
        self.environment = compose_environment(root)
        self.project = f"{prefix}-{uuid.uuid4().hex[:12]}"

    def tearDown(self):
        subprocess.run(
            compose_command(
                self.project, "down", "--volumes", "--remove-orphans", "--timeout", "30"
            ),
            check=False,
            capture_output=True,
            text=True,
            env=self.environment,
        )
        self.temporary.cleanup()

    def compose(
        self, *arguments: str, capture_output: bool = False
    ) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            compose_command(self.project, *arguments),
            check=True,
            capture_output=capture_output,
            text=True,
            env=self.environment,
        )

    def wait_for_successful_service(self, name: str) -> None:
        result = subprocess.run(
            compose_command(self.project, "wait", name),
            check=False,
            capture_output=True,
            text=True,
            env=self.environment,
        )
        if result.returncode != 0:
            logs = subprocess.run(
                compose_command(self.project, "logs", "--no-color", name),
                check=False,
                capture_output=True,
                text=True,
                env=self.environment,
            )
            self.fail(
                f"{name} exited unsuccessfully ({result.returncode}): "
                f"{result.stdout}{result.stderr}{logs.stdout}{logs.stderr}"
            )
        self.assertIn("status code 0", result.stdout)


class LocalStandComposeContractTests(unittest.TestCase):
    @unittest.skipUnless(shutil.which("docker"), "Docker Compose is not installed")
    def test_compose_configuration_is_valid(self):
        with tempfile.TemporaryDirectory() as temporary:
            result = subprocess.run(
                compose_command("octacity-contract", "config", "--quiet"),
                check=False,
                capture_output=True,
                text=True,
                env=compose_environment(Path(temporary).resolve()),
            )
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_service_inputs_match_the_immutable_manifest(self):
        document = json.loads(MANIFEST.read_text(encoding="utf-8"))
        compose = COMPOSE.read_text(encoding="utf-8")
        postgres = document["images"]["postgres"]["reference"]
        expected_minio = minio_builder.local_image_name(document, "minio")
        expected_mc = minio_builder.local_image_name(document, "mc")

        self.assertIn(f"image: {postgres}", compose)
        self.assertIn(f"image: {expected_minio}", compose)
        self.assertIn(f"image: {expected_mc}", compose)
        self.assertIn("image: octacity/server:local", compose)
        self.assertIn("image: octacity/gateway:local", compose)
        self.assertIn("pull_policy: never", compose)

    @unittest.skipUnless(shutil.which("docker"), "Docker Compose is not installed")
    def test_rendered_services_are_private_bounded_and_file_credentialed(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            result = subprocess.run(
                compose_command("octacity-contract", "config", "--format", "json"),
                check=True,
                capture_output=True,
                text=True,
                env=compose_environment(root),
            )
        configuration = json.loads(result.stdout)
        services = configuration["services"]

        self.assertEqual(
            set(services),
            {"gateway", "minio", "minio-init", "postgres", "server"},
        )
        self.assertEqual(set(configuration["networks"]), {"edge", "stand"})
        self.assertTrue(configuration["networks"]["stand"]["internal"])
        self.assertFalse(configuration["networks"]["edge"].get("internal", False))
        for name in ("postgres", "minio"):
            service = services[name]
            with self.subTest(service=name):
                self.assertNotIn("ports", service)
                self.assertNotIn("restart", service)
                health = service["healthcheck"]
                self.assertEqual(health["interval"], "2s")
                self.assertEqual(health["timeout"], "2s")
                self.assertEqual(health["retries"], 30)
                self.assertEqual(health["start_period"], "10s")

        self.assertEqual(
            services["postgres"]["environment"],
            {
                "POSTGRES_DB": "octacity",
                "POSTGRES_PASSWORD_FILE": "/run/secrets/postgres-password",
                "POSTGRES_USER": "octacity",
            },
        )
        self.assertEqual(
            services["minio"]["environment"],
            {
                "MINIO_BROWSER": "off",
                "MINIO_ROOT_PASSWORD_FILE": "/run/secrets/object-secret-key",
                "MINIO_ROOT_USER_FILE": "/run/secrets/object-access-key",
            },
        )
        self.assertEqual(
            {
                (mount["source"], mount["target"])
                for mount in services["postgres"]["volumes"]
            },
            {("postgres-data", "/var/lib/postgresql")},
        )
        self.assertEqual(
            {
                (mount["source"], mount["target"])
                for mount in services["minio"]["volumes"]
            },
            {("minio-data", "/data")},
        )
        self.assertEqual(
            {secret["source"] for secret in services["postgres"]["secrets"]},
            {"postgres-password"},
        )
        self.assertEqual(
            {secret["source"] for secret in services["minio"]["secrets"]},
            {"object-access-key", "object-secret-key"},
        )
        initializer = services["minio-init"]
        self.assertNotIn("ports", initializer)
        self.assertNotIn("restart", initializer)
        self.assertNotIn("healthcheck", initializer)
        self.assertTrue(initializer["read_only"])
        self.assertEqual(
            initializer["depends_on"],
            {
                "minio": {
                    "condition": "service_healthy",
                    "required": True,
                }
            },
        )
        self.assertEqual(
            {secret["source"] for secret in initializer["secrets"]},
            {"object-access-key", "object-secret-key"},
        )
        self.assertEqual(
            initializer["environment"],
            {
                "OCTACITY_OBJECT_BUCKET": "octacity-artifacts",
                "OCTACITY_OBJECT_ENDPOINT": "http://minio:9000",
                "OCTACITY_OBJECT_OPERATION_TIMEOUT_SECONDS": "10",
            },
        )

        server = services["server"]
        self.assertNotIn("ports", server)
        self.assertNotIn("restart", server)
        self.assertTrue(server["read_only"])
        self.assertEqual(server["command"], ["run", "/run/octacity/server.toml"])
        self.assertEqual(server["user"], "0:0")
        self.assertEqual(
            set(server["cap_add"]),
            {"CHOWN", "DAC_OVERRIDE", "SETGID", "SETUID"},
        )
        self.assertEqual(server["cap_drop"], ["ALL"])
        self.assertIn("/run/octacity-secrets:size=64k,mode=0700", server["tmpfs"])
        self.assertIn(
            "http://127.0.0.1:8080/health/ready",
            server["healthcheck"]["test"],
        )
        self.assertEqual(
            server["depends_on"],
            {
                "minio-init": {
                    "condition": "service_completed_successfully",
                    "required": True,
                },
                "postgres": {
                    "condition": "service_healthy",
                    "required": True,
                },
            },
        )
        self.assertEqual(server["healthcheck"]["interval"], "2s")
        self.assertEqual(server["healthcheck"]["timeout"], "5s")
        self.assertEqual(server["healthcheck"]["retries"], 30)
        self.assertEqual(server["stop_grace_period"], "30s")
        self.assertEqual(
            {(config["source"], config["target"]) for config in server["configs"]},
            {
                ("local-ca-certificate", "/etc/ssl/certs/octacity-local-ca.pem"),
                ("server-config", "/run/octacity/server.toml"),
                ("server-job-spec-policy", "/run/octacity/job-spec-policy.json"),
            },
        )
        self.assertEqual(
            {(secret["source"], secret["target"]) for secret in server["secrets"]},
            {
                ("agent-enrollment-key", "agent-enrollment-key"),
                ("cache-credential-key", "cache-credential-key"),
                ("object-access-key", "object-access-key"),
                ("object-secret-key", "object-secret-key"),
                ("postgres-url", "postgres-url"),
                ("signing-key", "signing-key"),
            },
        )

        gateway = services["gateway"]
        self.assertTrue(gateway["read_only"])
        self.assertEqual(
            gateway["depends_on"],
            {"server": {"condition": "service_healthy", "required": True}},
        )
        self.assertEqual(gateway["healthcheck"]["interval"], "2s")
        self.assertEqual(gateway["healthcheck"]["timeout"], "3s")
        self.assertEqual(gateway["healthcheck"]["retries"], 30)
        self.assertIn(
            "https://octacity.localhost/health/ready",
            gateway["healthcheck"]["test"],
        )
        self.assertEqual(gateway["stop_grace_period"], "30s")
        self.assertEqual(
            gateway["networks"]["stand"]["aliases"],
            [
                "octacity.localhost",
                "agent.localhost",
                "cache.localhost",
                "objects.localhost",
            ],
        )
        self.assertEqual(set(gateway["networks"]), {"edge", "stand"})
        for name in ("minio", "minio-init", "postgres", "server"):
            self.assertEqual(set(services[name]["networks"]), {"stand"})
        self.assertEqual(
            {(config["source"], config["target"]) for config in gateway["configs"]},
            {("local-ca-certificate", "/run/octacity/local-ca.pem")},
        )
        self.assertEqual(
            {(secret["source"], secret["target"]) for secret in gateway["secrets"]},
            {
                ("gateway-certificate", "gateway.pem"),
                ("gateway-private-key", "gateway-key.pem"),
            },
        )
        published = {
            name: service["ports"] for name, service in services.items() if "ports" in service
        }
        self.assertEqual(set(published), {"gateway"})
        self.assertEqual(len(published["gateway"]), 1)
        self.assertEqual(published["gateway"][0]["host_ip"], "127.0.0.1")
        self.assertEqual(published["gateway"][0]["target"], 443)


@unittest.skipUnless(
    RUN_INTEGRATION,
    "set OCTACITY_RUN_COMPOSE_INTEGRATION=1 to exercise real volume persistence",
)
class LocalStandComposePersistenceTests(ComposeProjectMixin, unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="octacity-compose-")
        self.root = Path(self.temporary.name).resolve()
        self.root.chmod(0o700)
        secrets_directory = self.root / "secrets"
        secrets_directory.mkdir(mode=0o700)
        write_private_credential(
            secrets_directory / "postgres-password", secrets.token_urlsafe(32)
        )
        write_private_credential(
            secrets_directory / "object-access-key",
            f"OCTA{secrets.token_hex(12).upper()}",
        )
        write_private_credential(
            secrets_directory / "object-secret-key", secrets.token_urlsafe(32)
        )
        self.configure_compose_project(self.root, "octacity-persistence")

    def minio_client(
        self, command: str, *, capture_output: bool = False
    ) -> subprocess.CompletedProcess[str]:
        return self.compose(
            "exec",
            "--no-TTY",
            "minio",
            "sh",
            "-euc",
            MC_AUTHENTICATION + command,
            capture_output=capture_output,
        )

    def start_infrastructure(self) -> None:
        self.compose("up", "--detach", "postgres", "minio", "minio-init")
        self.wait_for_successful_service("minio-init")
        self.compose(
            "up",
            "--detach",
            "--wait",
            "--wait-timeout",
            "120",
            "postgres",
            "minio",
        )

    def test_postgres_and_minio_data_survive_ordinary_down_and_up(self):
        for path in (self.root / "secrets").iterdir():
            self.assertEqual(stat.S_IMODE(path.stat().st_mode), 0o600)

        self.start_infrastructure()
        self.compose(
            "exec",
            "--no-TTY",
            "--user",
            "postgres",
            "postgres",
            "psql",
            "--username",
            "octacity",
            "--dbname",
            "octacity",
            "--set",
            "ON_ERROR_STOP=1",
            "--command",
            "CREATE TABLE persistence_probe (value text PRIMARY KEY); "
            "INSERT INTO persistence_probe VALUES ('database-survived');",
        )
        self.minio_client(
            "mc mb --ignore-existing local/persistence-probe >/dev/null; "
            + "printf object-survived | "
            + "mc pipe local/persistence-probe/value >/dev/null"
        )

        self.compose("down", "--remove-orphans", "--timeout", "30")
        self.start_infrastructure()

        database = self.compose(
            "exec",
            "--no-TTY",
            "--user",
            "postgres",
            "postgres",
            "psql",
            "--username",
            "octacity",
            "--dbname",
            "octacity",
            "--tuples-only",
            "--no-align",
            "--command",
            "SELECT value FROM persistence_probe;",
            capture_output=True,
        )
        stored_object = self.minio_client(
            "mc cat local/persistence-probe/value",
            capture_output=True,
        )

        self.assertEqual(database.stdout.strip(), "database-survived")
        self.assertEqual(stored_object.stdout, "object-survived")

    def test_bucket_initializer_is_replay_safe_and_cleans_its_probe_objects(self):
        self.start_infrastructure()

        for _ in range(2):
            self.compose("run", "--rm", "minio-init")

        bucket = self.minio_client(
            "mc stat local/octacity-artifacts >/dev/null",
            capture_output=True,
        )
        probes = self.minio_client(
            "mc ls --recursive local/octacity-artifacts/local-stand/readiness/",
            capture_output=True,
        )

        self.assertEqual(bucket.stdout, "")
        self.assertEqual(probes.stdout, "")


@unittest.skipUnless(
    RUN_SERVER_INTEGRATION,
    "set OCTACITY_RUN_COMPOSE_SERVER_INTEGRATION=1 to exercise the server gateway",
)
@unittest.skipUnless(
    shutil.which("docker") and shutil.which("curl"),
    "Docker Compose and curl are required",
)
class LocalStandComposeServerIntegrationTests(ComposeProjectMixin, unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="octacity-server-compose-")
        self.root = Path(self.temporary.name, "stand").resolve()
        initialized = stand_initializer.initialize(
            self.root,
            repository=REPOSITORY,
            home=Path.home(),
        )
        installation = Path(self.temporary.name, "native-installation")
        installation.mkdir(mode=0o700)
        policy = json.dumps(
            {
                "source": {
                    "provider": "git",
                    "plugin_version": "0.1.0",
                    "plugin_sha256": "1" * 64,
                    "repository_parameter": "url",
                },
                "octa": {
                    "version": "0.1.0",
                    "runner_sha256": "2" * 64,
                    "runner_protocol": 1,
                    "event_schema": 1,
                    "plugin_protocol": 1,
                    "plugin_digests": {},
                },
                "validity": 900,
            },
            sort_keys=True,
        ).encode("utf-8") + b"\n"
        policy_path = installation / "job-spec-policy.json"
        policy_path.write_bytes(policy)
        policy_path.chmod(0o600)
        manifest_path = installation / "installation-manifest.json"
        manifest_path.write_text(
            json.dumps(
                {
                    "schema_version": 1,
                    "files": {"job-spec-policy.json": hashlib.sha256(policy).hexdigest()},
                },
                sort_keys=True,
            )
            + "\n",
            encoding="utf-8",
        )
        manifest_path.chmod(0o600)
        server_configurator.generate_server_configuration(
            self.root,
            installation,
            repository=REPOSITORY,
            home=Path.home(),
        )
        with socket.socket() as listener:
            listener.bind(("127.0.0.1", 0))
            self.https_port = listener.getsockname()[1]
        self.configure_compose_project(self.root, "octacity-server")
        self.environment["OCTACITY_LOCAL_STAND_HTTPS_PORT"] = str(self.https_port)
        self.ca_certificate = Path(initialized["pki"]["ca_certificate"])

    def gateway_response(self, path: str) -> tuple[int, dict[str, str]]:
        body = Path(self.temporary.name, "response.json")
        result = subprocess.run(
            [
                "curl",
                "--silent",
                "--show-error",
                "--cacert",
                str(self.ca_certificate),
                "--resolve",
                f"octacity.localhost:{self.https_port}:127.0.0.1",
                "--connect-timeout",
                "1",
                "--max-time",
                "3",
                "--output",
                str(body),
                "--write-out",
                "%{http_code}",
                f"https://octacity.localhost:{self.https_port}{path}",
            ],
            check=True,
            capture_output=True,
            text=True,
            env={**os.environ, "NO_PROXY": "*", "no_proxy": "*"},
        )
        return int(result.stdout), json.loads(body.read_text(encoding="utf-8"))

    def wait_for_gateway(
        self, path: str, expected_status: int, expected_body: dict[str, str]
    ) -> None:
        deadline = time.monotonic() + 45
        last: object = None
        while time.monotonic() < deadline:
            try:
                last = self.gateway_response(path)
                if last == (expected_status, expected_body):
                    return
            except (OSError, ValueError, subprocess.CalledProcessError) as error:
                last = error
            time.sleep(0.25)
        self.fail(f"gateway {path} did not return expected response; last result: {last}")

    def test_liveness_and_readiness_propagate_through_the_only_gateway_port(self):
        self.compose("up", "--detach")

        self.wait_for_gateway("/health/live", 200, {"status": "live"})
        self.wait_for_gateway("/health/ready", 200, {"status": "ready"})

        self.compose("stop", "--timeout", "1", "minio")
        self.wait_for_gateway("/health/ready", 503, {"status": "unavailable"})
        self.wait_for_gateway("/health/live", 200, {"status": "live"})


if __name__ == "__main__":
    unittest.main()
