"""Tests for the fail-closed native local-stand Agent configuration."""

from __future__ import annotations

import hashlib
import importlib.util
import json
import os
from pathlib import Path
import platform
import shutil
import stat
import sys
import tempfile
import tomllib
import unittest
from unittest import mock


REPOSITORY = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPOSITORY / "tools"))


def load_module(name: str, path: Path):
    spec = importlib.util.spec_from_file_location(name, path)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


INITIALIZER = load_module("init_local_stand", REPOSITORY / "tools/init_local_stand.py")
CONFIGURATOR = load_module(
    "configure_local_stand_agent",
    REPOSITORY / "tools/configure_local_stand_agent.py",
)


@unittest.skipUnless(os.name == "posix", "local stand host state requires POSIX permissions")
class LocalStandAgentConfigurationTests(unittest.TestCase):
    def setUp(self):
        openssl = shutil.which("openssl")
        if openssl is None:
            self.skipTest("OpenSSL is required for local stand tests")
        self.openssl = openssl
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name) / "stand"
        self.addCleanup(
            shutil.rmtree,
            CONFIGURATOR.microsandbox_state_root(self.root),
            True,
        )
        INITIALIZER.initialize(
            self.root,
            repository=REPOSITORY,
            home=Path.home(),
            openssl=openssl,
        )
        agent_root = self.root / "agent"
        agent_root.mkdir(mode=0o700)
        self.credential = agent_root / "credential"
        self.credential.write_text("private-agent-credential\n", encoding="ascii")
        self.credential.chmod(0o600)
        self.installation = Path(self.temporary.name) / "native"
        self.installation.mkdir(mode=0o700)
        self._write_installation()

    def _executable(self, relative: str, contents: str) -> None:
        path = self.installation / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(contents, encoding="utf-8")
        path.chmod(0o755)

    def _write_manifest(self) -> None:
        files = {}
        for path in sorted(self.installation.rglob("*")):
            if path.is_file() and path.name != "installation-manifest.json":
                files[path.relative_to(self.installation).as_posix()] = hashlib.sha256(
                    path.read_bytes()
                ).hexdigest()
        manifest = {
            "schema_version": 1,
            "agent_revision": "a" * 40,
            "agent_version": "0.1.0",
            "octa_revision": "b" * 40,
            "microsandbox_version": "0.7.6",
            "inputs": {"fixture": "c" * 64},
            "files": files,
        }
        path = self.installation / "installation-manifest.json"
        path.write_text(json.dumps(manifest, sort_keys=True) + "\n", encoding="utf-8")
        path.chmod(0o600)

    def _write_installation(self, *, validator_succeeds: bool = True) -> None:
        real_agent = os.environ.get("OCTACITY_AGENT_BIN") if validator_succeeds else None
        if real_agent:
            destination = self.installation / "agent/bin/octacity-agent"
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(real_agent, destination)
            destination.chmod(0o755)
        else:
            validation = (
                'echo "configuration is valid"\nexit 0'
                if validator_succeeds
                else 'echo "configuration rejected" >&2\nexit 2'
            )
            self._executable(
                "agent/bin/octacity-agent",
                "#!/bin/sh\n"
                'if [ "$1" = "--version" ]; then echo "octacity-agent 0.1.0"; exit 0; fi\n'
                'if [ "$1" = "validate" ]; then '
                + validation.replace("\n", "; ")
                + "; fi\nexit 2\n",
            )
        self._executable(
            "microsandbox/bin/msb",
            '#!/bin/sh\necho "msb 0.7.6"\n',
        )
        firmware = self.installation / "microsandbox/lib/libkrunfw.5.dylib"
        firmware.parent.mkdir(parents=True, exist_ok=True)
        firmware.write_bytes(b"firmware")
        firmware.chmod(0o644)
        runner = self.installation / "octa/octa-runner"
        runner.parent.mkdir(parents=True, exist_ok=True)
        runner.write_bytes(b"runner")
        runner.chmod(0o755)
        octa_plugins = self.installation / "octa/plugins"
        octa_plugins.mkdir(exist_ok=True)
        codex_plugin = octa_plugins / "octa_plugin_codex"
        codex_plugin.write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
        codex_plugin.chmod(0o755)
        codex_digest = hashlib.sha256(codex_plugin.read_bytes()).hexdigest()
        (octa_plugins / "codex.plugin.yml").write_text(
            "manifest_version: 1\nname: codex\n", encoding="utf-8"
        )
        (self.installation / "octa/Octa.lock").write_text(
            "version: 1\nplugins:\n  codex:\n"
            "    version: '0.5.0'\n"
            "    protocol: 1\n"
            "    platforms: [linux-aarch64]\n"
            "    entrypoint: octa_plugin_codex\n"
            f"    sha256: {codex_digest}\n"
            "    capabilities: []\n"
            "    source: codex.plugin.yml\n",
            encoding="utf-8",
        )
        (self.installation / "octa/codex-compatibility.json").write_text(
            json.dumps(
                {
                    "format_version": 1,
                    "plugin": {
                        "name": "codex",
                        "version": "0.5.0",
                        "protocol": 1,
                        "manifest": "plugins/codex.plugin.yml",
                    },
                    "executable": {
                        "product": "codex-cli",
                        "supported_versions": ["0.130.0"],
                        "selection_environment": "OCTA_CODEX_EXECUTABLE",
                    },
                }
            ),
            encoding="utf-8",
        )
        (self.installation / "octa/octa-runner-capabilities.json").write_text(
            json.dumps(
                {
                    "type": "capabilities",
                    "octa_version": "0.5.0",
                    "runner_protocols": [1],
                    "event_schemas": [1],
                    "plugin_protocols": [1],
                    "octafile_versions": [1],
                    "platform": "linux-aarch64",
                    "features": [],
                }
            ),
            encoding="utf-8",
        )
        plugin_directory = self.installation / "agent/source-plugins/git"
        plugin_directory.mkdir(parents=True, exist_ok=True)
        plugin_executable = plugin_directory / "octacity-source-git"
        plugin_executable.write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
        plugin_executable.chmod(0o755)
        operating_system = "macos" if platform.system() == "Darwin" else platform.system().lower()
        architecture = {
            "arm64": "aarch64",
            "x86_64": "x86_64",
            "AMD64": "x86_64",
        }.get(platform.machine(), platform.machine().lower())
        plugin = plugin_directory / "plugin.toml"
        plugin.write_text(
            "\n".join(
                (
                    "manifest_version = 1",
                    'name = "git"',
                    'version = "0.1.0"',
                    "protocol_min = 1",
                    "protocol_max = 1",
                    'executable = "octacity-source-git"',
                    f'sha256 = "{hashlib.sha256(plugin_executable.read_bytes()).hexdigest()}"',
                    f'platforms = ["{operating_system}-{architecture}"]',
                    "",
                )
            ),
            encoding="utf-8",
        )
        plugin.chmod(0o644)
        self._executable(
            "agent/share/local-stand-codex-fixture",
            "#!/bin/sh\n# codex-cli 0.130.0\nexit 0\n",
        )
        self._write_manifest()

    def generate(self):
        return CONFIGURATOR.generate_agent_configuration(
            self.root,
            self.installation,
            repository=REPOSITORY,
            home=Path.home(),
            openssl=self.openssl,
            host_system="Darwin",
            host_machine="arm64",
        )

    def test_generates_only_microsandbox_with_private_distinct_bounded_state(self):
        receipt = self.generate()
        expected_keys = {
            "config",
            "credential",
            "work_root",
            "state_root",
            "cache_root",
            "log_root",
            "ca_certificate",
        }
        self.assertEqual(set(receipt), expected_keys)
        configuration_path = Path(receipt["config"])
        self.assertEqual(stat.S_IMODE(configuration_path.stat().st_mode), 0o600)
        configuration = tomllib.loads(configuration_path.read_text(encoding="utf-8"))
        self.assertEqual(
            Path(configuration["state_root"]),
            CONFIGURATOR.microsandbox_state_root(self.root),
        )

        self.assertEqual(configuration["server_url"], "https://agent.localhost:8443")
        self.assertEqual(
            configuration["cache"]["allowed_remote_origins"],
            ["https://cache.localhost:8443"],
        )
        self.assertEqual(
            configuration["allowed_upload_origins"],
            ["https://objects.localhost:8443"],
        )
        self.assertEqual(configuration["enabled_runtime_modes"], [])
        tool = configuration["tool_executables"]["codex-cli"]
        self.assertEqual(tool["version"], "0.130.0")
        self.assertEqual(tool["platform"], "linux-aarch64")
        self.assertEqual(
            Path(tool["path"]),
            self.installation.resolve() / "agent/share/local-stand-codex-fixture",
        )
        self.assertNotIn("OPENAI_API_KEY", configuration_path.read_text(encoding="utf-8"))
        self.assertIs(configuration["allow_native_execution"], False)
        self.assertIs(configuration["allow_host_execution"], False)
        self.assertEqual(configuration["oci_engines"], [])
        self.assertEqual(configuration["isolation_providers"], [])
        self.assertEqual(len(configuration["virtualization_providers"]), 1)
        provider = configuration["virtualization_providers"][0]
        self.assertEqual(provider["provider"], "microsandbox")
        self.assertEqual(
            provider["environment_identity"],
            "microsandbox-0.7.6-linux-arm64-v1",
        )
        self.assertEqual(
            configuration["server_signing_keys"]["local-stand"],
            (self.root / "pki/server-signing-public-key")
            .read_text(encoding="ascii")
            .strip(),
        )
        roots = [Path(receipt[name]).resolve() for name in ("work_root", "state_root", "cache_root", "log_root")]
        self.assertEqual(len(set(roots)), len(roots))
        for path in roots:
            self.assertTrue(path.is_dir())
            self.assertEqual(stat.S_IMODE(path.stat().st_mode), 0o700)
        self.assertLessEqual(configuration["max_workspace_bytes"], 4 * 1024**3)
        self.assertLessEqual(configuration["cache"]["capacity"]["max_bytes"], 512 * 1024**2)
        self.assertEqual(
            configuration["tls_ca_certificate_file"],
            configuration["cache"]["ca_certificate_file"],
        )

        snapshot = (configuration_path.read_bytes(), configuration_path.stat().st_mtime_ns)
        self.assertEqual(receipt, self.generate())
        self.assertEqual(snapshot, (configuration_path.read_bytes(), configuration_path.stat().st_mtime_ns))

    def test_repeated_generation_does_not_reopen_the_live_agent_state(self):
        with mock.patch.object(
            CONFIGURATOR,
            "_validate_with_agent",
            wraps=CONFIGURATOR._validate_with_agent,
        ) as validate:
            self.generate()
            self.generate()

        validate.assert_called_once()

    def test_rejects_wrong_host_or_missing_credential_before_publishing(self):
        with self.assertRaisesRegex(CONFIGURATOR.AgentConfigurationError, "Apple Silicon"):
            CONFIGURATOR.generate_agent_configuration(
                self.root,
                self.installation,
                repository=REPOSITORY,
                home=Path.home(),
                openssl=self.openssl,
                host_system="Linux",
                host_machine="aarch64",
            )
        self.credential.unlink()
        with self.assertRaisesRegex(CONFIGURATOR.AgentConfigurationError, "credential.*absent"):
            self.generate()
        self.assertFalse((self.root / "config/agent.toml").exists())

    def test_rejects_installation_or_generated_configuration_tampering(self):
        msb = self.installation / "microsandbox/bin/msb"
        msb.write_bytes(msb.read_bytes() + b"\n")
        with self.assertRaisesRegex(CONFIGURATOR.AgentConfigurationError, "inventory differs"):
            self.generate()
        self.assertFalse((self.root / "config/agent.toml").exists())

        self._write_manifest()
        receipt = self.generate()
        configuration = Path(receipt["config"])
        configuration.write_text("malformed = true\n", encoding="utf-8")
        configuration.chmod(0o600)
        with self.assertRaisesRegex(CONFIGURATOR.AgentConfigurationError, "differs"):
            self.generate()
        self.assertEqual(configuration.read_text(encoding="utf-8"), "malformed = true\n")

    def test_failed_agent_validation_never_publishes_configuration(self):
        self._write_installation(validator_succeeds=False)
        with self.assertRaisesRegex(CONFIGURATOR.AgentConfigurationError, "rejected"):
            self.generate()
        self.assertFalse((self.root / "config/agent.toml").exists())

    def test_installation_verification_enforces_allowlist_aggregate_bounds(self):
        with self.assertRaisesRegex(CONFIGURATOR.AgentConfigurationError, "too many entries"):
            CONFIGURATOR._verified_installation(
                self.installation,
                CONFIGURATOR.InstallationLimits(
                    entries=1,
                    single_file_bytes=1024**2,
                    total_bytes=1024**2,
                ),
            )
        with self.assertRaisesRegex(CONFIGURATOR.AgentConfigurationError, "invalid size"):
            CONFIGURATOR._verified_installation(
                self.installation,
                CONFIGURATOR.InstallationLimits(
                    entries=10_000,
                    single_file_bytes=1,
                    total_bytes=1024**2,
                ),
            )


if __name__ == "__main__":
    unittest.main()
