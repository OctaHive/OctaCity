"""Tests for fail-closed local-stand server configuration generation."""

from __future__ import annotations

import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import stat
import subprocess
import sys
import tempfile
import tomllib
import unittest


REPOSITORY = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPOSITORY / "tools"))


def load_module(name: str, path: Path):
    spec = importlib.util.spec_from_file_location(name, path)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


INITIALIZER = load_module(
    "init_local_stand", REPOSITORY / "tools/init_local_stand.py"
)
CONFIGURATOR = load_module(
    "configure_local_stand_server",
    REPOSITORY / "tools/configure_local_stand_server.py",
)


@unittest.skipUnless(os.name == "posix", "local stand host state requires POSIX permissions")
class LocalStandServerConfigurationTests(unittest.TestCase):
    def setUp(self):
        openssl = shutil.which("openssl")
        if openssl is None:
            self.skipTest("OpenSSL is required for local stand tests")
        self.openssl = openssl
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name) / "stand"
        INITIALIZER.initialize(
            self.root,
            repository=REPOSITORY,
            home=Path.home(),
            openssl=self.openssl,
        )
        self.installation = Path(self.temporary.name) / "native-installation"
        self.installation.mkdir(mode=0o700)
        self.policy = self.installation / "job-spec-policy.json"
        self.policy_document = {
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
        }
        self.write_policy(self.policy_document)

    def write_policy(self, document) -> None:
        contents = json.dumps(document, sort_keys=True).encode("utf-8") + b"\n"
        self.policy.write_bytes(contents)
        self.policy.chmod(0o600)
        manifest = {
            "schema_version": 1,
            "files": {
                "job-spec-policy.json": hashlib.sha256(contents).hexdigest(),
            },
        }
        manifest_path = self.installation / "installation-manifest.json"
        manifest_path.write_text(
            json.dumps(manifest, sort_keys=True) + "\n", encoding="utf-8"
        )
        manifest_path.chmod(0o600)

    def generate(self):
        return CONFIGURATOR.generate_server_configuration(
            self.root,
            self.installation,
            repository=REPOSITORY,
            home=Path.home(),
            openssl=self.openssl,
        )

    def validation_root(self, receipt: dict[str, str]) -> Path:
        root = Path(self.temporary.name) / "validation-root"
        secrets = root / "run/secrets"
        policy = root / "run/octacity"
        secrets.mkdir(parents=True, mode=0o700)
        policy.mkdir(parents=True, mode=0o700)
        sources = {
            "postgres-url": Path(receipt["postgres_url"]),
            "object-access-key": self.root / "secrets/object-access-key",
            "object-secret-key": self.root / "secrets/object-secret-key",
            "signing-key": self.root / "secrets/signing-key",
            "agent-enrollment-key": self.root / "secrets/agent-enrollment-key",
            "cache-credential-key": self.root / "secrets/cache-credential-key",
        }
        for name, source in sources.items():
            destination = secrets / name
            shutil.copyfile(source, destination)
            destination.chmod(0o600)
        destination = policy / "job-spec-policy.json"
        shutil.copyfile(receipt["job_spec_policy"], destination)
        destination.chmod(0o600)
        return root

    def validate_with_real_server(self, config: Path, root: Path) -> subprocess.CompletedProcess[str]:
        configured = os.environ.get("OCTACITY_SERVER_BIN")
        server = Path(configured) if configured else REPOSITORY / "target/debug/octacity-server"
        if not server.is_file():
            if configured:
                self.fail(f"OCTACITY_SERVER_BIN does not name a file: {server}")
            self.skipTest("build octacity-server to exercise its real validator")
        return subprocess.run(
            [str(server), "validate", "--file-root", str(root), str(config)],
            check=False,
            capture_output=True,
            text=True,
        )

    def test_generates_private_complete_configuration_and_is_idempotent(self):
        first = self.generate()
        bundle = self.root / "config/server"
        self.assertEqual(set(first), {"config", "postgres_url", "job_spec_policy"})
        self.assertEqual(stat.S_IMODE(bundle.stat().st_mode), 0o700)

        files = {name: Path(path) for name, path in first.items()}
        for path in files.values():
            metadata = path.lstat()
            self.assertTrue(stat.S_ISREG(metadata.st_mode))
            self.assertEqual(stat.S_IMODE(metadata.st_mode), 0o600)
            self.assertEqual(metadata.st_uid, os.getuid())

        configuration = tomllib.loads(files["config"].read_text(encoding="utf-8"))
        self.assertEqual(configuration["management_bind"], "0.0.0.0:8080")
        self.assertEqual(configuration["agent_bind"], "0.0.0.0:8081")
        self.assertEqual(configuration["cache_bind"], "0.0.0.0:8082")
        self.assertIs(configuration["acknowledge_unauthenticated_management"], True)
        self.assertEqual(
            configuration["object_storage"]["endpoint"],
            "https://objects.localhost",
        )
        self.assertEqual(configuration["cache"]["endpoint"], "https://cache.localhost")
        self.assertEqual(
            configuration["job_spec"]["policy_file"],
            "/run/octacity/job-spec-policy.json",
        )
        self.assertEqual(files["job_spec_policy"].read_bytes(), self.policy.read_bytes())
        self.assertNotIn(
            (self.root / "secrets/postgres-password")
            .read_text(encoding="ascii")
            .strip(),
            files["config"].read_text(encoding="utf-8"),
        )

        snapshot = {
            path: (path.read_bytes(), path.stat().st_mtime_ns) for path in files.values()
        }
        second = self.generate()
        self.assertEqual(first, second)
        self.assertEqual(
            snapshot,
            {
                path: (path.read_bytes(), path.stat().st_mtime_ns)
                for path in files.values()
            },
        )

    def test_real_server_accepts_generated_config_and_rejects_removed_acknowledgement(self):
        receipt = self.generate()
        config = Path(receipt["config"])
        validation_root = self.validation_root(receipt)
        valid = self.validate_with_real_server(config, validation_root)
        self.assertEqual(valid.returncode, 0, valid.stderr)
        self.assertIn("server configuration is valid", valid.stdout)

        malformed = Path(self.temporary.name) / "malformed-server.toml"
        malformed.write_text(
            config.read_text(encoding="utf-8").replace(
                "acknowledge_unauthenticated_management = true",
                "acknowledge_unauthenticated_management = false",
            ),
            encoding="utf-8",
        )
        rejected = self.validate_with_real_server(malformed, validation_root)
        self.assertNotEqual(rejected.returncode, 0)
        self.assertIn(
            "acknowledge_unauthenticated_management",
            rejected.stdout + rejected.stderr,
        )

    def test_real_server_rejects_missing_and_malformed_runtime_files(self):
        receipt = self.generate()
        config = Path(receipt["config"])

        missing_root = self.validation_root(receipt)
        (missing_root / "run/secrets/cache-credential-key").unlink()
        missing = self.validate_with_real_server(config, missing_root)
        self.assertNotEqual(missing.returncode, 0)
        self.assertIn("cache credential", missing.stdout + missing.stderr)

        shutil.rmtree(missing_root)
        malformed_root = self.validation_root(receipt)
        malformed = malformed_root / "run/secrets/signing-key"
        malformed.write_text("not-base64\n", encoding="ascii")
        malformed.chmod(0o600)
        rejected = self.validate_with_real_server(config, malformed_root)
        self.assertNotEqual(rejected.returncode, 0)
        self.assertIn("JobSpec signing key", rejected.stdout + rejected.stderr)

    def test_cli_emits_only_generated_paths(self):
        result = subprocess.run(
            [
                sys.executable,
                str(REPOSITORY / "tools/configure_local_stand_server.py"),
                "--root",
                str(self.root),
                "--native-installation",
                str(self.installation),
                "--openssl",
                self.openssl,
            ],
            check=True,
            capture_output=True,
            text=True,
        )
        receipt = json.loads(result.stdout)
        self.assertEqual(set(receipt), {"config", "postgres_url", "job_spec_policy"})
        output = result.stdout + result.stderr
        for name in (
            "database_password",
            "object_access_key",
            "object_secret_key",
            "signing_key",
            "agent_enrollment_key",
            "cache_credential_key",
        ):
            secret = (
                self.root
                / "secrets"
                / INITIALIZER.CREDENTIAL_FILES[name]
            ).read_text(encoding="ascii").strip()
            self.assertNotIn(secret, output)

    def test_missing_or_malformed_secret_never_publishes_a_bundle(self):
        password = self.root / "secrets/postgres-password"
        password.unlink()
        with self.assertRaisesRegex(CONFIGURATOR.ServerConfigurationError, "absent"):
            self.generate()
        self.assertFalse((self.root / "config/server").exists())

        password.write_text("invalid password\n", encoding="ascii")
        password.chmod(0o600)
        with self.assertRaisesRegex(CONFIGURATOR.ServerConfigurationError, "invalid format"):
            self.generate()
        self.assertFalse((self.root / "config/server").exists())

    def test_invalid_or_unverified_policy_never_publishes_a_bundle(self):
        self.write_policy(["not", "an", "object"])
        with self.assertRaisesRegex(CONFIGURATOR.ServerConfigurationError, "JSON object"):
            self.generate()
        self.assertFalse((self.root / "config/server").exists())

        self.write_policy({"source": [], "octa": {}, "validity": 900})
        with self.assertRaisesRegex(CONFIGURATOR.ServerConfigurationError, "structure"):
            self.generate()
        self.assertFalse((self.root / "config/server").exists())

        self.write_policy(self.policy_document)
        self.policy.write_bytes(self.policy.read_bytes() + b"\n")
        with self.assertRaisesRegex(CONFIGURATOR.ServerConfigurationError, "manifest digest"):
            self.generate()
        self.assertFalse((self.root / "config/server").exists())

    def test_rejects_tampered_generated_bundle_instead_of_overwriting_it(self):
        receipt = self.generate()
        config = Path(receipt["config"])
        config.write_text("malformed = true\n", encoding="utf-8")
        config.chmod(0o600)

        with self.assertRaisesRegex(CONFIGURATOR.ServerConfigurationError, "differs"):
            self.generate()
        self.assertEqual(config.read_text(encoding="utf-8"), "malformed = true\n")


if __name__ == "__main__":
    unittest.main()
