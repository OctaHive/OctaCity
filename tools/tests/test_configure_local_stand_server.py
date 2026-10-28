"""Tests for fail-closed local-stand server configuration generation."""

from __future__ import annotations

import hashlib
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
from unittest.mock import patch


REPOSITORY = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPOSITORY / "tools"))


import init_local_stand as INITIALIZER  # noqa: E402
import configure_local_stand_server as CONFIGURATOR  # noqa: E402


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

    def test_initial_explicit_installation_rejects_packaged_catalog_before_service_activation(self):
        import types
        import local_stand_launcher as launcher
        import local_stand_state as state

        profiles = Path(self.temporary.name) / "profiles.json"
        profiles.write_text('{"version":1,"projects":[],"codex":[],"unknown_field":true}\n')
        profiles.chmod(0o600)
        native = launcher.NativeInstallation(self.installation, {"sources": {}})
        with (
            patch.object(launcher, "_prepare_native_installation", return_value=native),
            patch.object(launcher, "_build_images"),
            patch.object(launcher.installation_update, "validate_factory_installation",
                         side_effect=state.StateError("packaged server rejected the Factory installation")) as preflight,
            patch.object(launcher, "_run_compose") as compose,
            patch.object(launcher.bootstrapper, "bootstrap", return_value=types.SimpleNamespace(state="enrolled")) as bootstrap,
            patch.object(launcher.agent_starter, "start_agent", return_value=types.SimpleNamespace(state="registered", pid=1)) as start,
        ):
            with self.assertRaisesRegex(RuntimeError, "packaged server rejected"):
                launcher.run(launcher.parse_arguments([
                    "up", "--root", str(self.root), "--factory-profiles", str(profiles),
                ]))
            preflight.assert_called_once_with(self.root.resolve(), launcher.SERVER_IMAGE)
            compose.assert_not_called()
            bootstrap.assert_not_called()
            start.assert_not_called()

    def validation_root(self, receipt: dict[str, str]) -> Path:
        root = Path(self.temporary.name) / "validation-root"
        octacity_secrets = root / "run/octacity-secrets"
        policy = root / "run/octacity"
        octacity_secrets.mkdir(parents=True, mode=0o700)
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
            destination = octacity_secrets / name
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
            "https://objects.localhost:8443",
        )
        self.assertEqual(
            configuration["cache"]["endpoint"], "https://cache.localhost:8443"
        )
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

    def test_factory_delivery_installation_is_private_immutable_and_separate_from_agent(self):
        profiles = Path(self.temporary.name) / "profiles.json"
        profiles.write_text('{"version":1,"projects":[],"codex":[]}')
        profiles.chmod(0o600)
        adapters = Path(self.temporary.name) / "operator-delivery"
        provider = adapters / "arbitrary-provider"
        provider.mkdir(parents=True, mode=0o700)
        adapters.chmod(0o700)
        (provider / "inputs").mkdir(mode=0o700)
        executable = provider / "plugin"
        executable.write_bytes(b"#!/bin/sh\nexit 0\n")
        executable.chmod(0o700)
        secret = provider / "credential.json"
        secret.write_bytes(b"private-delivery-canary")
        secret.chmod(0o600)
        arguments = dict(repository=REPOSITORY, home=Path.home(), openssl=self.openssl,
                         factory_profiles=profiles, factory_delivery_bundle=adapters)
        receipt = CONFIGURATOR.generate_server_configuration(self.root, self.installation, **arguments)
        installed = Path(receipt["factory_delivery_bundle"])
        self.assertEqual((installed / "arbitrary-provider/plugin").read_bytes(), executable.read_bytes())
        self.assertEqual(stat.S_IMODE((installed / "arbitrary-provider/plugin").stat().st_mode), 0o700)
        self.assertEqual((installed / "arbitrary-provider/credential.json").read_bytes(), secret.read_bytes())
        self.assertEqual(stat.S_IMODE((installed / "arbitrary-provider/credential.json").stat().st_mode), 0o600)
        self.assertTrue((installed / "arbitrary-provider/inputs").is_dir())
        self.assertNotIn("private-delivery-canary", json.dumps(receipt))
        self.assertNotIn(b"private-delivery-canary", Path(receipt["config"]).read_bytes())
        self.assertFalse((self.root / "config/agent.toml").exists())
        self.assertEqual(receipt, CONFIGURATOR.generate_server_configuration(self.root, self.installation, **arguments))
        secret.write_bytes(b"rotated-delivery-canary")
        with self.assertRaises(CONFIGURATOR.ServerConfigurationError):
            CONFIGURATOR.generate_server_configuration(self.root, self.installation, **arguments)
        self.assertEqual((installed / "arbitrary-provider/credential.json").read_bytes(), b"private-delivery-canary")

    def test_runtime_upgrade_preserves_stand_data_and_replaces_only_the_recognized_generated_bundle(self):
        import factory_local_stand as stand
        profiles = self.root / "factory-demo/factory-profiles.json"
        old = {"version":1,"projects":[],"codex":[]}
        stand._private_json(profiles,old)
        CONFIGURATOR.generate_server_configuration(self.root,self.installation,repository=REPOSITORY,
            home=Path.home(),openssl=self.openssl,factory_profiles=profiles)
        runtime = self.root.parent / "runtime"
        runtime.mkdir(mode=0o700)
        (runtime/"plugin").write_text("#!/bin/sh\nexit 0\n"); (runtime/"plugin").chmod(0o700)
        history = self.root / "evidence/operator-history.json"
        stand._private_json(history,{"run":"retained-operator-run"})
        new = {**old,"projects":[{"project_id":"project","role_profiles":{"codex":{}}}]}
        with patch.object(stand.launcher,"run") as restart:
            stand.publish_runtime_profiles(self.root,self.installation,profiles,new,runtime,previous=old)
        self.assertEqual(stand._read_json(profiles),new)
        self.assertEqual(stand._read_json(self.root/"config/server/factory-profiles.json"),new)
        self.assertEqual(stand._read_json(history),{"run":"retained-operator-run"})
        self.assertTrue((self.root/"config/server/factory-runtime/plugin").is_file())
        self.assertEqual(restart.call_count,1)
        with patch.object(stand.launcher,"run") as stop:
            with self.assertRaisesRegex(ValueError,"customized"):
                stand.publish_runtime_profiles(self.root,self.installation,profiles,old,runtime,previous=old)
        stop.assert_not_called()
        self.assertEqual(stand._read_json(profiles),new)

    def test_runtime_support_upgrade_preserves_operator_state_and_the_same_postgres_path(self):
        import copy
        import factory_local_stand as stand
        profiles = self.root / "factory-demo/factory-profiles.json"
        old = {"version":1,"projects":[{"project_id":"project","role_profiles":{"codex":{"ceiling":{"model_identifier":"Vendor/future"}}}}],"codex":[]}
        stand._private_json(profiles,old)
        original = CONFIGURATOR.generate_server_configuration(self.root,self.installation,repository=REPOSITORY,
            home=Path.home(),openssl=self.openssl,factory_profiles=profiles)
        postgres = Path(original['postgres_url']).read_bytes()
        runtime = self.root.parent / "runtime"
        runtime.mkdir(mode=0o700)
        (runtime/"plugin").write_text("#!/bin/sh\nexit 0\n"); (runtime/"plugin").chmod(0o700)
        history = self.root / "evidence/operator-history.json"
        stand._private_json(history,{"run":"retained-operator-run"})
        current = copy.deepcopy(old)
        current['projects'][0]['role_profiles']['codex']['settings_support'] = {'mcp_transports':['http'],'mcp_executables':{}}
        legacy = {"version":1,"projects":[],"codex":[]}
        with patch.object(stand.launcher,"run") as stop:
            stand.publish_runtime_profiles(self.root,self.installation,profiles,current,runtime,previous=legacy)
            stand.publish_runtime_profiles(self.root,self.installation,profiles,current,runtime,previous=legacy)
        self.assertEqual(stop.call_count,1)
        self.assertEqual(stand._read_json(profiles),current)
        self.assertEqual(stand._read_json(self.root/"config/server/factory-profiles.json"),current)
        self.assertEqual(Path(original['postgres_url']).read_bytes(),postgres)
        self.assertEqual(stand._read_json(history),{"run":"retained-operator-run"})
        with patch.object(stand.launcher,"run") as stop:
            with self.assertRaisesRegex(ValueError,"customized"):
                stand.publish_runtime_profiles(self.root,self.installation,profiles,old,runtime,previous=legacy)
        stop.assert_not_called()

    def test_runtime_programs_use_the_bounded_installation_without_changing_profile_content(self):
        profiles = self.root.parent / "runtime-profiles.json"
        profiles.write_bytes(b'{"version":1,"projects":[],"codex":[]}\n')
        profiles.chmod(0o600)
        runtime = self.root.parent / "runtime"
        runtime.mkdir(mode=0o700)
        plugin = runtime / "plugin"
        plugin.write_bytes(b"#!/bin/sh\nprintf catalog\n")
        plugin.chmod(0o700)
        private_home = runtime / "home/.codex"
        private_home.mkdir(parents=True, mode=0o700)
        (runtime / "home").chmod(0o700)
        auth = private_home / "auth.json"
        auth.write_text('{"auth_mode":"chatgpt","private":"canary"}')
        auth.chmod(0o600)
        receipt = CONFIGURATOR.generate_server_configuration(
            self.root, self.installation, repository=REPOSITORY, home=Path.home(),
            openssl=self.openssl, factory_profiles=profiles, factory_runtime_bundle=runtime,
        )
        installed = Path(receipt["config"]).parent / "factory-runtime/plugin"
        self.assertEqual(installed.read_bytes(), plugin.read_bytes())
        retained_auth = installed.parent / "home/.codex/auth.json"
        self.assertEqual(retained_auth.read_bytes(), auth.read_bytes())
        self.assertEqual(stat.S_IMODE(retained_auth.stat().st_mode), 0o600)
        self.assertEqual(stat.S_IMODE(installed.stat().st_mode), 0o700)
        self.assertEqual(Path(receipt["factory_profiles"]).read_bytes(), profiles.read_bytes())

    @unittest.skipUnless(os.environ.get("OCTACITY_TEST_SERVER_IMAGE"), "requires an explicitly selected local server image")
    def test_packaged_factory_preflight_accepts_valid_files_and_rejects_invalid_native_catalog(self):
        import local_stand_installation_update as updates

        profiles = Path(self.temporary.name) / "profiles.json"
        profiles.write_text('{"version":1,"projects":[{"project_id":"10000000-0000-4000-8000-000000000001","references":[],"build_configurations":[]}],"codex":[]}\n')
        profiles.chmod(0o600)
        CONFIGURATOR.generate_server_configuration(self.root, self.installation, factory_profiles=profiles)
        updates.validate_factory_installation(self.root, os.environ["OCTACITY_TEST_SERVER_IMAGE"])
        installed = self.root / "config/server/factory-profiles.json"
        installed.write_text('{"version":1,"projects":[],"codex":[],"private-canary":"secret"}')
        with self.assertRaisesRegex(RuntimeError, "packaged server rejected") as error:
            updates.validate_factory_installation(self.root, os.environ["OCTACITY_TEST_SERVER_IMAGE"])
        self.assertNotIn("private-canary", str(error.exception))
        self.assertFalse(list(self.root.glob(".factory-preflight-*")))

    def test_factory_is_explicit_and_copies_the_private_profile_without_interpreting_flow(self):
        profiles = Path(self.temporary.name) / "operator-profiles.json"
        contents = b'{"version":1,"projects":[],"codex":[],"signals":[]}\n'
        profiles.write_bytes(contents)
        profiles.chmod(0o600)
        receipt = CONFIGURATOR.generate_server_configuration(
            self.root,
            self.installation,
            repository=REPOSITORY,
            home=Path.home(),
            openssl=self.openssl,
            factory_profiles=profiles,
        )
        configuration = tomllib.loads(Path(receipt["config"]).read_text())
        self.assertEqual(
            configuration["factory_profiles_file"],
            "/run/octacity-secrets/factory-profiles.json",
        )
        self.assertEqual(configuration["factory"]["concurrency"], 2)
        copied = Path(receipt["factory_profiles"])
        self.assertEqual(copied.read_bytes(), contents)
        self.assertEqual(stat.S_IMODE(copied.stat().st_mode), 0o600)
        if shutil.which("docker"):
            rendered = subprocess.run(
                ["docker", "compose", "--file", str(REPOSITORY / "compose.yaml"),
                 "--file", str(REPOSITORY / "deployment/local-stand/factory.compose.yaml"),
                 "config", "--format", "json"],
                env={**os.environ, "OCTACITY_LOCAL_STAND_ROOT": str(self.root)},
                capture_output=True, text=True, check=False,
            )
            self.assertEqual(rendered.returncode, 0, rendered.stderr)
            compose = json.loads(rendered.stdout)
            runtime_mounts = compose["services"]["server"]["tmpfs"]
            self.assertIn("/run/octacity-delivery:exec,size=512m,mode=0700", runtime_mounts)
            self.assertIn("/run/octacity-secrets:size=320m,mode=0700", runtime_mounts)
            self.assertEqual(sum(mount.startswith("/run/octacity-secrets:") for mount in runtime_mounts), 1)
            self.assertEqual(set(compose["services"]), {"server", "gateway", "postgres", "minio", "minio-init"})
            self.assertEqual(Path(compose["secrets"]["factory-profiles"]["file"]).resolve(), copied.resolve())
            self.assertIn(
                {"source": "factory-profiles", "target": "factory-profiles.json"},
                compose["services"]["server"]["secrets"],
            )
        self.assertEqual(
            CONFIGURATOR.generate_server_configuration(
                self.root, self.installation, repository=REPOSITORY,
                home=Path.home(), openssl=self.openssl, factory_profiles=profiles,
            ),
            receipt,
        )
        profiles.write_bytes(b'{"version":1,"projects":[{}]}\n')
        with self.assertRaises(CONFIGURATOR.ServerConfigurationError):
            CONFIGURATOR.generate_server_configuration(
                self.root, self.installation, repository=REPOSITORY,
                home=Path.home(), openssl=self.openssl, factory_profiles=profiles,
            )

    def test_factory_opt_in_cannot_silently_replace_an_existing_default_bundle(self):
        ordinary = self.generate()
        original = Path(ordinary["config"]).read_bytes()
        profiles = Path(self.temporary.name) / "profiles.json"
        profiles.write_text('{"version":1,"projects":[],"codex":[]}\n')
        profiles.chmod(0o600)
        with self.assertRaisesRegex(CONFIGURATOR.ServerConfigurationError, "Factory.*reset"):
            CONFIGURATOR.generate_server_configuration(
                self.root, self.installation, repository=REPOSITORY,
                home=Path.home(), openssl=self.openssl, factory_profiles=profiles,
            )
        self.assertEqual(Path(ordinary["config"]).read_bytes(), original)
        self.assertFalse((Path(ordinary["config"]).parent / "factory-profiles.json").exists())

    def test_separate_connector_bundle_copies_opaque_config_and_private_credentials(self):
        directory = Path(self.temporary.name)
        profiles = directory / "profiles.json"
        profiles.write_text('{"version":1,"projects":[],"codex":[],"signals":[]}\n')
        configuration = directory / "provider-settings.json"
        opaque = b'{"adapter_owned":"private-endpoint-canary"}\n'
        configuration.write_bytes(opaque)
        credential = directory / "provider-key"
        secret = b"private-provider-credential-canary\n"
        credential.write_bytes(secret)
        connectors = directory / "connectors.json"
        connectors.write_text(json.dumps({"version": 1, "connectors": [{
            "kind": "operator-adapter", "credential_profile": "routing-key", "configuration_file": str(configuration),
            "credential_file": str(credential),
        }]}))
        for path in (profiles, configuration, credential, connectors):
            path.chmod(0o600)
        arguments = dict(
            repository=REPOSITORY, home=Path.home(), openssl=self.openssl,
            factory_profiles=profiles, factory_signal_connectors=connectors,
        )
        receipt = CONFIGURATOR.generate_server_configuration(self.root, self.installation, **arguments)
        registry = Path(receipt["factory_signal_connectors"])
        copied = json.loads(registry.read_bytes())
        self.assertEqual(copied, {"version": 1, "connectors": [{
            "kind": "operator-adapter",
            "credential_profile": "routing-key",
            "configuration_file": "/run/octacity-secrets/factory-signal-0000-configuration",
            "credential_file": "/run/octacity-secrets/factory-signal-0000-credential",
        }]})
        self.assertEqual((registry.parent / "factory-signal-0000-configuration").read_bytes(), opaque)
        self.assertEqual((registry.parent / "factory-signal-0000-credential").read_bytes(), secret)
        toml = Path(receipt["config"]).read_text()
        self.assertEqual(tomllib.loads(toml)["factory_signal_connectors_file"],
                         "/run/octacity-secrets/factory-signal-connectors.json")
        self.assertNotIn("canary", toml)
        self.assertNotIn("canary", json.dumps(receipt))
        if shutil.which("docker"):
            rendered = subprocess.run([
                "docker", "compose", "--file", str(REPOSITORY / "compose.yaml"),
                "--file", str(REPOSITORY / "deployment/local-stand/factory.compose.yaml"),
                "config", "--format", "json",
            ], env={**os.environ, "OCTACITY_LOCAL_STAND_ROOT": str(self.root)},
               capture_output=True, text=True, check=False)
            self.assertEqual(rendered.returncode, 0, rendered.stderr)
            volumes = json.loads(rendered.stdout)["services"]["server"]["volumes"]
            mounted = [volume for volume in volumes if volume["target"] == "/run/octacity-factory-connectors"]
            self.assertEqual(len(mounted), 1)
            self.assertEqual(Path(mounted[0]["source"]).resolve(), registry.parent.resolve())
            self.assertIs(mounted[0]["read_only"], True)
        for path in registry.parent.iterdir():
            self.assertEqual(stat.S_IMODE(path.stat().st_mode), 0o600)
        self.assertEqual(CONFIGURATOR.generate_server_configuration(self.root, self.installation, **arguments), receipt)
        cli = subprocess.run([
            sys.executable, str(REPOSITORY / "tools/configure_local_stand_server.py"),
            "--root", str(self.root), "--native-installation", str(self.installation),
            "--openssl", self.openssl, "--factory-profiles", str(profiles),
            "--factory-signal-connectors", str(connectors),
        ], capture_output=True, text=True, check=False)
        self.assertEqual(cli.returncode, 0, cli.stderr)
        self.assertEqual(json.loads(cli.stdout), receipt)
        self.assertNotIn("canary", cli.stdout)
        credential.write_bytes(b"changed-private-credential-canary\n")
        with self.assertRaises(CONFIGURATOR.ServerConfigurationError):
            CONFIGURATOR.generate_server_configuration(self.root, self.installation, **arguments)
        self.assertEqual((registry.parent / "factory-signal-0000-credential").read_bytes(), secret)

    def test_connector_rejects_unsafe_private_inputs_without_publishing_a_bundle(self):
        directory = Path(self.temporary.name)
        profiles = directory / "profiles.json"
        profiles.write_text('{"version":1,"projects":[],"codex":[],"signals":[]}\n')
        profiles.chmod(0o600)
        configuration = directory / "adapter-settings"
        credential = directory / "private-key"
        connectors = directory / "connectors.json"
        connectors.write_text(json.dumps({"version": 1, "connectors": [{
            "kind": "operator-adapter", "credential_profile": "routing-key", "configuration_file": str(configuration),
            "credential_file": str(credential),
        }]}))
        connectors.chmod(0o600)
        for case in ("public-key", "symlink-key", "oversized-key", "oversized-config"):
            with self.subTest(case=case):
                credential.unlink(missing_ok=True)
                credential.write_bytes(b"private-credential-canary")
                configuration.write_bytes(b"private-configuration-canary")
                credential.chmod(0o600)
                configuration.chmod(0o600)
                if case == "public-key":
                    credential.chmod(0o644)
                elif case == "symlink-key":
                    credential.unlink()
                    credential.symlink_to(configuration)
                elif case == "oversized-key":
                    credential.write_bytes(b"x" * (65536 + 1))
                else:
                    configuration.write_bytes(b"x" * (4194304 + 1))
                with self.assertRaises(CONFIGURATOR.ServerConfigurationError) as error:
                    CONFIGURATOR.generate_server_configuration(
                        self.root, self.installation, repository=REPOSITORY,
                        home=Path.home(), openssl=self.openssl, factory_profiles=profiles,
                        factory_signal_connectors=connectors,
                    )
                self.assertEqual(str(error.exception), "Factory connector inputs are invalid")
                self.assertFalse((self.root / "config/server").exists())

    def test_factory_rejects_legacy_or_malformed_version_without_disclosing_body(self):
        profiles = Path(self.temporary.name) / "profiles.json"
        for contents in (
            '{"schema_version":1,"private_canary":"do-not-disclose"}',
            '{"version":true,"private_canary":"do-not-disclose"}',
            '{"version":2,"private_canary":"do-not-disclose"}',
            'do-not-disclose-invalid-json',
        ):
            with self.subTest(contents=contents):
                profiles.write_text(contents)
                profiles.chmod(0o600)
                with self.assertRaises(CONFIGURATOR.ServerConfigurationError) as error:
                    CONFIGURATOR.generate_server_configuration(
                        self.root, self.installation, repository=REPOSITORY,
                        home=Path.home(), openssl=self.openssl, factory_profiles=profiles,
                    )
                self.assertNotIn("do-not-disclose", str(error.exception))
                self.assertFalse((self.root / "config/server").exists())

    def test_real_server_loads_generated_source_preparation_and_reporter_without_provider_or_database(self):
        from factory_integrations import configure_integrations
        plugin = Path(os.environ.get("OCTACITY_GITHUB_BIN", REPOSITORY / "target/debug/octacity-github"))
        if not plugin.is_file():
            self.skipTest("build octacity-github to validate its exact installation")
        receipt = self.generate()
        bundle = configure_integrations(self.root, "Example/Tickets", native_repository_id="1234",
                                        adapter_digest=hashlib.sha256(plugin.read_bytes()).hexdigest())
        catalog = json.loads((bundle / "catalog.json").read_text())
        files = self.validation_root(receipt)
        registry = files / "adapters"
        provider = registry / "github"
        for directory in (registry, provider, provider / "credentials"):
            directory.mkdir(mode=0o700)
        shutil.copyfile(plugin, provider / "plugin")
        (provider / "plugin").chmod(0o700)
        for name in ("adapter.toml", "github-work.toml"):
            shutil.copyfile(bundle / name, provider / name)
            (provider / name).chmod(0o600)
        for name in ("selections", "mappings"):
            shutil.copytree(bundle / name, provider / name)
        profiles = files / "source-profiles.json"
        shutil.copyfile(bundle / profiles.name, profiles)
        profiles.chmod(0o600)
        catalog["registry"] = str(registry)
        catalog["source_profiles_file"] = str(profiles)
        catalog["credential_publishers"][0]["root"] = str(provider)
        catalog_file = files / "catalog.json"
        catalog_file.write_text(json.dumps(catalog))
        catalog_file.chmod(0o600)
        config = files / "server.toml"
        source = (bundle / "server.toml").read_text()
        original_catalog = tomllib.loads(source)["integration_management"]["catalog_file"]
        config.write_text(source.replace(original_catalog, str(catalog_file)))
        valid = self.validate_with_real_server(config, files)
        self.assertEqual(valid.returncode, 0, valid.stderr)
        self.assertIn("server configuration is valid", valid.stdout)
        read = catalog["checks"][0]
        credential = read["resource"]["credential"]
        credential_file = provider / "credentials" / (hashlib.sha256(credential["handle"].encode()).hexdigest() + ".json")
        credential_file.write_text(json.dumps({"class": "work_source_read", "token": "synthetic-fixture-token",
                                                "repositories": ["Example/Tickets"]}))
        credential_file.chmod(0o600)
        authorization = {
            "family": "source", "message": {"protocol_version": 1, "request_id": "stand-read-scope",
                "command": {"operation": "authorize", "payload": {
                    "scan_id": "a" * 64, "configuration_digest": read["provider_configuration"],
                    "resource": read["resource"], "network_hosts": read["network_hosts"],
                }}},
        }
        authorized = subprocess.run([str(provider / "plugin")], cwd=provider, env={},
                                    input=json.dumps(authorization) + "\n", capture_output=True, text=True, timeout=10)
        self.assertEqual(authorized.returncode, 0, authorized.stderr)
        self.assertEqual(json.loads(authorized.stdout)["message"]["outcome"], {
            "status": "authorized", "payload": {"scan_id": "a" * 64, "resource_id": "1234",
            "configuration_digest": read["provider_configuration"]},
        })
        self.assertNotIn("synthetic-fixture-token", authorized.stdout + authorized.stderr)
        document = json.loads(profiles.read_text())
        document["readers"][0]["polling"]["max_pages"] = 0
        profiles.write_text(json.dumps(document))
        invalid = self.validate_with_real_server(config, files)
        self.assertNotEqual(invalid.returncode, 0)

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
        (missing_root / "run/octacity-secrets/cache-credential-key").unlink()
        missing = self.validate_with_real_server(config, missing_root)
        self.assertNotEqual(missing.returncode, 0)
        self.assertIn("cache credential", missing.stdout + missing.stderr)

        shutil.rmtree(missing_root)
        malformed_root = self.validation_root(receipt)
        malformed = malformed_root / "run/octacity-secrets/signing-key"
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
