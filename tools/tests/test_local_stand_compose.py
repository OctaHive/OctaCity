"""Contract and opt-in persistence tests for local-stand infrastructure."""

from __future__ import annotations

import json
import os
from pathlib import Path
import secrets
import shutil
import stat
import subprocess
import tempfile
import unittest
import uuid


REPOSITORY = Path(__file__).resolve().parents[2]
COMPOSE = REPOSITORY / "compose.yaml"
MANIFEST = REPOSITORY / "deployment/local-stand/inputs.json"
RUN_INTEGRATION = os.environ.get("OCTACITY_RUN_COMPOSE_INTEGRATION") == "1"
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


class LocalStandComposeContractTests(unittest.TestCase):
    def test_service_inputs_match_the_immutable_manifest(self):
        document = json.loads(MANIFEST.read_text(encoding="utf-8"))
        compose = COMPOSE.read_text(encoding="utf-8")
        postgres = document["images"]["postgres"]["reference"]
        minio = document["sources"]["minio"]
        mc = document["sources"]["minio_client"]
        expected_minio = (
            "octacity/minio:release-"
            f"{minio['tag'].removeprefix('RELEASE.').split('T', 1)[0]}-"
            f"{minio['source_revision'][:12]}"
        )
        expected_mc = (
            "octacity/mc:release-"
            f"{mc['tag'].removeprefix('RELEASE.').split('T', 1)[0]}-"
            f"{mc['source_revision'][:12]}"
        )

        self.assertIn(f"image: {postgres}", compose)
        self.assertIn(f"image: {expected_minio}", compose)
        self.assertIn(f"image: {expected_mc}", compose)
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

        self.assertTrue({"postgres", "minio"}.issubset(services))
        self.assertTrue(configuration["networks"]["stand"]["internal"])
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


@unittest.skipUnless(
    RUN_INTEGRATION,
    "set OCTACITY_RUN_COMPOSE_INTEGRATION=1 to exercise real volume persistence",
)
class LocalStandComposePersistenceTests(unittest.TestCase):
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
        self.environment = compose_environment(self.root)
        self.project = f"octacity-persistence-{uuid.uuid4().hex[:12]}"

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
        self.compose("up", "--detach")
        initializer = self.compose("wait", "minio-init", capture_output=True)
        self.assertIn("status code 0", initializer.stdout)
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


if __name__ == "__main__":
    unittest.main()
