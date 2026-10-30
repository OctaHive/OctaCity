"""Tests for the fail-closed native local-stand Agent configuration."""

from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import stat
import sys
import tempfile
import tomllib
import types
import unittest
from unittest import mock


REPOSITORY = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPOSITORY / "tools"))


import init_local_stand as INITIALIZER  # noqa: E402
import configure_local_stand_agent as CONFIGURATOR  # noqa: E402


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
            "    version: '0.6.0'\n"
            "    protocol: 2\n"
            "    platforms: [linux-aarch64]\n"
            "    entrypoint: octa_plugin_codex\n"
            f"    sha256: {codex_digest}\n"
            "    capabilities: [codex.blocking-pre-tool-authorization.v1, public-command-file-observations.v1]\n"
            "    source: codex.plugin.yml\n",
            encoding="utf-8",
        )
        (self.installation / "octa/codex-compatibility.json").write_text(
            json.dumps(
                {
                    "format_version": 2,
                    "plugin": {
                        "name": "codex",
                        "version": "0.6.0",
                        "protocol": 2,
                        "manifest": "plugins/codex.plugin.yml",
                        "capabilities": [
                            "codex.blocking-pre-tool-authorization.v1",
                            "public-command-file-observations.v1",
                        ],
                    },
                    "executable": {
                        "product": "codex-cli",
                        "supported_versions": ["0.161.0"],
                        "selection_environment": "OCTA_CODEX_EXECUTABLE",
                    },
                    "tool_authorization": {
                        "mode": "blocking_pre_tool_use",
                        "capability": "codex.blocking-pre-tool-authorization.v1",
                        "selection_environment": "OCTA_CODEX_TOOL_AUTHORIZER",
                        "hook_event": "PreToolUse",
                    },
                }
            ),
            encoding="utf-8",
        )
        (self.installation / "octa/octa-runner-capabilities.json").write_text(
            json.dumps(
                {
                    "type": "capabilities",
                    "octa_version": "0.6.0",
                    "runner_protocols": [1],
                    "event_schemas": [1],
                    "plugin_protocols": [2],
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
            "#!/bin/sh\n# codex-cli 0.161.0\nexit 0\n",
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
        self.assertEqual(tool["version"], "0.161.0")
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

    def _factory_profile(self):
        permissions = json.loads((REPOSITORY / "shared/protocol-fixtures/job-spec/job-spec-v3.json").read_text())["permissions"]
        tool = self.installation / "agent/share/local-stand-codex-fixture"
        digest = hashlib.sha256(tool.read_bytes()).hexdigest()
        permissions["executables"][0]["sha256"] = digest
        permissions["executables"][0]["version"] = "0.161.0"
        permissions["commands"][0]["executable"] = permissions["executables"][0].copy()
        policy = Path(self.temporary.name) / "operator-agent-policy.toml"
        executable = f'{{ identity = "codex-cli", version = "0.161.0", sha256 = "{digest}" }}'
        rendered = (
            'version = 1\nallowed_network_hosts = ["api.openai.com"]\n'
            f'tool_executables = {{ "codex-cli" = {{ path = {json.dumps(str(tool))}, '
            f'version = "0.161.0", platform = "linux-aarch64", sha256 = "{digest}" }} }}\n'
            '[factory_permissions]\n'
            'plugins = [{ identity = "codex", version = "0.5.0", sha256 = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef" }]\n'
            f'executables = [{executable}]\n'
            'tools = []\n'
            f'commands = [{{ executable = {executable}, arguments = [{{ kind = "exact", value = "--json" }}] }}]\n'
            'max_descendants = 4\n'
            'mounts = [{ root = "/octacity/protected", mode = "read_only" }, { root = "/workspace/output", mode = "read_write" }, { root = "/workspace/scratch", mode = "read_write" }, { root = "/workspace/source", mode = "read_write" }]\n'
            'network_hosts = ["api.openai.com"]\nsecret_profiles = ["model-coding"]\nworkload_identity_profiles = []\n'
            '[factory_permissions.resources]\ncpu_millis = 1000\nmemory_bytes = 536870912\ndisk_bytes = 1073741824\nprocess_count = 8\nelapsed_millis = 600000\n'
            '[factory_permissions.outputs]\nkinds = ["codex-result"]\nmax_artifact_count = 10\nmax_artifact_bytes = 1024\nmax_report_count = 10\nmax_report_bytes = 1024\n'
        )
        policy.write_text(rendered)
        policy.chmod(0o600)
        return policy, permissions, digest

    def test_factory_uses_only_the_independently_supplied_local_agent_policy(self):
        policy, permissions, digest = self._factory_profile()
        receipt = CONFIGURATOR.generate_agent_configuration(
            self.root, self.installation, repository=REPOSITORY, home=Path.home(), openssl=self.openssl,
            host_system="Darwin", host_machine="arm64", factory_agent_profile=policy,
        )
        configuration = tomllib.loads(Path(receipt["config"]).read_text())
        self.assertEqual(configuration["factory_permissions"], permissions)
        self.assertNotIn("change_set_capture", configuration)
        self.assertEqual(configuration["allowed_network_hosts"], ["api.openai.com"])
        self.assertEqual(configuration["tool_executables"]["codex-cli"]["sha256"], digest)
        self.assertEqual(len(configuration["virtualization_providers"]), 1)
        CONFIGURATOR.validated_launch_plan(self.root.resolve(), self.installation, receipt)

    def test_stages_updated_factory_policy_without_replacing_identity_or_live_configuration(self):
        policy, _, _ = self._factory_profile()
        original = CONFIGURATOR.generate_agent_configuration(
            self.root, self.installation, repository=REPOSITORY, home=Path.home(), openssl=self.openssl,
            host_system="Darwin", host_machine="arm64", factory_agent_profile=policy,
        )
        configuration = Path(original["config"])
        installed_policy = Path(original["factory_agent_profile"])
        before = (configuration.read_bytes(), installed_policy.read_bytes(), self.credential.read_bytes())
        policy.write_text(policy.read_text().replace('["api.openai.com"]', '["api.openai.com", "mcp.example"]'))
        with tempfile.TemporaryDirectory(prefix=".factory-update-", dir=self.root / "config") as temporary:
            staged = CONFIGURATOR.generate_agent_configuration(
                self.root, self.installation, repository=REPOSITORY, home=Path.home(), openssl=self.openssl,
                host_system="Darwin", host_machine="arm64", factory_agent_profile=policy,
                staging_directory=Path(temporary),
            )
            candidate = tomllib.loads(Path(staged["config"]).read_text())
            self.assertEqual(candidate["allowed_network_hosts"], ["api.openai.com", "mcp.example"])
            self.assertEqual(candidate["credential_file"], str(self.credential.resolve()))
            self.assertEqual(candidate["work_root"], original["work_root"])
            self.assertEqual(Path(staged["config"]).parent, Path(temporary).resolve())
            self.assertEqual(stat.S_IMODE(Path(staged["config"]).stat().st_mode), 0o600)
        self.assertEqual((configuration.read_bytes(), installed_policy.read_bytes(), self.credential.read_bytes()), before)

    def _factory_update_inputs(self):
        import configure_local_stand_server as server

        job_policy = self.installation / "job-spec-policy.json"
        job_policy.write_text(json.dumps({"source": {}, "octa": {}, "validity": 900}))
        job_policy.chmod(0o600)
        self._write_manifest()
        policy, _, _ = self._factory_profile()
        CONFIGURATOR.generate_agent_configuration(
            self.root, self.installation, openssl=self.openssl,
            host_system="Darwin", host_machine="arm64", factory_agent_profile=policy,
        )
        profiles = Path(self.temporary.name) / "profiles.json"
        catalog = {"version": 1, "projects": [{
            "project_id": "10000000-0000-4000-8000-000000000001",
            "references": [], "build_configurations": [],
        }], "codex": []}
        profiles.write_text(json.dumps(catalog))
        profiles.chmod(0o600)
        server.generate_server_configuration(self.root, self.installation, factory_profiles=profiles)
        policy.write_text(policy.read_text().replace('["api.openai.com"]', '["api.openai.com", "mcp.example"]'))
        catalog["commands"] = []
        profiles.write_text(json.dumps(catalog))
        return profiles, policy

    def test_staging_validates_without_claiming_the_running_agents_cache(self):
        import fcntl
        self.generate()
        self._executable("agent/bin/octacity-agent", f'''#!{sys.executable}
import fcntl, pathlib, sys, tomllib
if sys.argv[1] == '--version':
    print('octacity-agent 0.1.0')
else:
    document = tomllib.loads(pathlib.Path(sys.argv[2]).read_text())
    cache = pathlib.Path(document['cache']['root'])
    cache.mkdir(mode=0o700, parents=True, exist_ok=True)
    with (cache / 'ownership.lock').open('a') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        print('configuration is valid')
''')
        self._write_manifest()
        cache = self.root / "agent/cache"
        with (cache / "ownership.lock").open("a") as running, tempfile.TemporaryDirectory(dir=self.root) as staging:
            fcntl.flock(running, fcntl.LOCK_EX | fcntl.LOCK_NB)
            receipt = CONFIGURATOR.generate_agent_configuration(self.root, self.installation,
                openssl=self.openssl, host_system="Darwin", host_machine="arm64", staging_directory=Path(staging))
            candidate = tomllib.loads(Path(receipt["config"]).read_text())
            self.assertEqual(candidate["cache"]["root"], str(cache.resolve()), "published paths must still target the live installation")
            self.assertEqual(candidate["credential_file"], str(self.credential.resolve()))

    def test_updates_factory_installation_and_reuses_it_without_touching_retained_state(self):
        import local_stand_installation_update as updates
        import local_stand_lifecycle as lifecycle

        profiles, policy = self._factory_update_inputs()
        retained = {self.credential: self.credential.read_bytes()}
        # These files belong to other owners, not to the generated installation.
        for name in ("config/integrations/settings.json", "evidence/factory-demo.json", "agent/cache/receipt"):
            path = self.root / name
            path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
            path.write_bytes(b"retained operator data\n")
            path.chmod(0o600)
            retained[path] = path.read_bytes()
        events = []

        def preflight(candidate):
            self.assertEqual(json.loads((candidate / "config/server/factory-profiles.json").read_bytes())["commands"], [])
            self.assertNotIn(b"mcp.example", (self.root / "config/agent.toml").read_bytes())
            events.append("preflight")

        with lifecycle.LifecycleState(self.root) as owner, mock.patch.object(CONFIGURATOR, "_require_apple_silicon"):
            with updates.factory_installation_update(
                self.root, self.installation, owner=owner, factory_profiles=profiles,
                factory_agent_profile=policy, preflight=preflight, stop=lambda: events.append("stop"),
            ):
                self.assertEqual(events, ["preflight", "stop"])
                self.assertIn(b"mcp.example", (self.root / "config/agent.toml").read_bytes())
            snapshot = (self.root / "config/agent.toml").stat().st_mtime_ns
            with updates.factory_installation_update(
                self.root, self.installation, owner=owner, factory_profiles=profiles,
                factory_agent_profile=policy, preflight=lambda _: None,
                stop=lambda: self.fail("unchanged installation must not be stopped"),
            ):
                self.assertEqual((self.root / "config/agent.toml").stat().st_mtime_ns, snapshot)
        for path, contents in retained.items():
            self.assertEqual(path.read_bytes(), contents)
        self.assertFalse(list(self.root.glob(".factory-update-*")))

    def test_restores_usable_factory_installation_when_candidate_startup_fails(self):
        import local_stand_installation_update as updates
        import local_stand_lifecycle as lifecycle

        profiles, policy = self._factory_update_inputs()
        directory = Path(self.temporary.name)
        reader_token, readers = directory / "scope-token", directory / "readers.json"
        reader_token.write_text("ocmcp.triage-issues." + "ab" * 32)
        readers.write_text(json.dumps({"version": 1, "readers": [{"id": "triage-issues",
            "project_id": "10000000-0000-4000-8000-000000000001", "expires_at_unix_ms": 2_000_000_000_000,
            "credential_file": str(reader_token)}]}))
        for path in (reader_token, readers):
            path.chmod(0o600)
        original = {path.relative_to(self.root): path.read_bytes()
                    for path in (self.root / "config").rglob("*") if path.is_file()}
        credential = self.credential.read_bytes()
        with lifecycle.LifecycleState(self.root) as owner, mock.patch.object(CONFIGURATOR, "_require_apple_silicon"):
            with self.assertRaisesRegex(RuntimeError, "registration failed"):
                with updates.factory_installation_update(
                    self.root, self.installation, owner=owner, factory_profiles=profiles,
                    factory_agent_profile=policy, factory_issue_mcp_readers=readers,
                    preflight=lambda _: None, stop=lambda: None,
                ):
                    self.assertIn(b"mcp.example", (self.root / "config/agent.toml").read_bytes())
                    installed = tomllib.loads((self.root / "config/server/server.toml").read_text())
                    self.assertEqual(installed["issue_mcp_readers"][0]["project_id"], "10000000-0000-4000-8000-000000000001")
                    raise RuntimeError("registration failed")
        self.assertEqual({path.relative_to(self.root): path.read_bytes()
                          for path in (self.root / "config").rglob("*") if path.is_file()}, original)
        self.assertEqual(self.credential.read_bytes(), credential)

    def test_factory_update_does_not_retain_executable_contents_during_preflight(self):
        import tracemalloc
        import local_stand_installation_update as updates
        import local_stand_lifecycle as lifecycle

        profiles, policy = self._factory_update_inputs()
        program = self.root / "config/retained-program"
        with program.open("wb") as output:
            for _ in range(256):
                output.write(b"reviewed-program".ljust(64 * 1024, b"."))
        program.chmod(0o700)
        with program.open("rb") as source:
            expected = hashlib.file_digest(source, "sha256").hexdigest()

        def preflight(candidate):
            retained_bytes, _ = tracemalloc.get_traced_memory()
            self.assertLess(retained_bytes - baseline, 4 * 1024 * 1024,
                            "preflight must retain metadata rather than the 16 MiB executable")
            with (candidate / "config/retained-program").open("rb") as source:
                self.assertEqual(hashlib.file_digest(source, "sha256").hexdigest(), expected)

        with lifecycle.LifecycleState(self.root) as owner, mock.patch.object(CONFIGURATOR, "_require_apple_silicon"):
            already_tracing = tracemalloc.is_tracing()
            tracemalloc.start()
            baseline, _ = tracemalloc.get_traced_memory()
            try:
                with updates.factory_installation_update(
                    self.root, self.installation, owner=owner, factory_profiles=profiles,
                    factory_agent_profile=policy, preflight=preflight, stop=lambda: None,
                ):
                    pass
            finally:
                if not already_tracing:
                    tracemalloc.stop()
        with program.open("rb") as source:
            self.assertEqual(hashlib.file_digest(source, "sha256").hexdigest(), expected)

    def test_failed_factory_preflight_keeps_the_usable_installation_running(self):
        import local_stand_installation_update as updates
        import local_stand_lifecycle as lifecycle

        profiles, policy = self._factory_update_inputs()
        original = {path.relative_to(self.root): (path.read_bytes(), path.stat().st_mtime_ns)
                    for path in (self.root / "config").rglob("*") if path.is_file()}

        def reject(candidate):
            self.assertIn(b"mcp.example", (candidate / "config/agent.toml").read_bytes())
            raise RuntimeError("packaged preflight rejected the candidate")

        with lifecycle.LifecycleState(self.root) as owner, mock.patch.object(CONFIGURATOR, "_require_apple_silicon"):
            with self.assertRaisesRegex(RuntimeError, "preflight rejected"):
                with updates.factory_installation_update(
                    self.root, self.installation, owner=owner, factory_profiles=profiles,
                    factory_agent_profile=policy, preflight=reject,
                    stop=lambda: self.fail("preflight failure must not stop the stand"),
                ):
                    self.fail("invalid candidate must not reach startup")
        self.assertEqual({path.relative_to(self.root): (path.read_bytes(), path.stat().st_mtime_ns)
                          for path in (self.root / "config").rglob("*") if path.is_file()}, original)
        self.assertFalse(list(self.root.glob(".factory-update-*")))

    def test_factory_update_refuses_concurrent_operator_edits_before_stopping(self):
        import local_stand_installation_update as updates
        import local_stand_lifecycle as lifecycle
        import local_stand_state as state

        profiles, policy = self._factory_update_inputs()
        configuration = self.root / "config/agent.toml"
        reviewed = configuration.read_bytes() + b"\n# operator edited during preflight\n"

        def edit(_):
            configuration.write_bytes(reviewed)

        with lifecycle.LifecycleState(self.root) as owner, mock.patch.object(CONFIGURATOR, "_require_apple_silicon"):
            with self.assertRaisesRegex(state.StateError, "installation changed during preflight"):
                with updates.factory_installation_update(
                    self.root, self.installation, owner=owner, factory_profiles=profiles,
                    factory_agent_profile=policy, preflight=edit,
                    stop=lambda: self.fail("concurrent edits must fail before shutdown"),
                ):
                    self.fail("changed live inputs must not activate")
        self.assertEqual(configuration.read_bytes(), reviewed)

    def test_factory_update_refreshes_generated_integration_config_and_retains_its_authority(self):
        import local_stand_installation_update as updates
        import local_stand_lifecycle as lifecycle

        profiles, policy = self._factory_update_inputs()
        bundle = self.root / "config/integrations"
        bundle.mkdir(mode=0o700)
        suffix = b'\n[integration_management]\nsocket_path="/run/octacity-management/management.sock"\npeer_uid=101\nprincipal="local-stand-gateway"\ncatalog_file="/var/lib/octacity-integrations/retained/catalog.json"\n'
        integrated = bundle / "server.toml"
        integrated.write_bytes((self.root / "config/server/server.toml").read_bytes() + suffix)
        integrated.chmod(0o600)
        connectors = Path(self.temporary.name) / "connectors.json"
        connectors.write_text('{"version":1,"connectors":[]}\n')
        connectors.chmod(0o600)
        with lifecycle.LifecycleState(self.root) as owner, mock.patch.object(CONFIGURATOR, "_require_apple_silicon"):
            with updates.factory_installation_update(
                self.root, self.installation, owner=owner, factory_profiles=profiles,
                factory_agent_profile=policy, factory_signal_connectors=connectors,
                preflight=lambda _: None, stop=lambda: None,
            ):
                actual = tomllib.loads(integrated.read_text())
                self.assertEqual(actual["factory_signal_connectors_file"], "/run/octacity-secrets/factory-signal-connectors.json")
                self.assertTrue(integrated.read_bytes().endswith(suffix))

    def test_factory_update_preserves_installed_connector_inputs_when_omitted(self):
        import configure_local_stand_server as server
        import local_stand_installation_update as updates
        import local_stand_lifecycle as lifecycle

        profiles, policy = self._factory_update_inputs()
        directory = Path(self.temporary.name)
        configuration, credential = directory / "adapter.json", directory / "adapter.key"
        configuration.write_bytes(b'{"reviewed":"adapter configuration"}\n')
        credential.write_bytes(b"private-adapter-credential\n")
        connectors = directory / "connectors.json"
        connectors.write_text(json.dumps({"version": 1, "connectors": [{
            "kind": "operator-adapter", "credential_profile": "adapter-profile",
            "configuration_file": str(configuration), "credential_file": str(credential),
        }]}))
        for path in (configuration, credential, connectors):
            path.chmod(0o600)
        issue_credential, readers = directory / "issue-reader.key", directory / "issue-readers.json"
        issue_credential.write_text("ocmcp.triage-issues." + "ab" * 32)
        readers.write_text(json.dumps({"version": 1, "readers": [{"id": "triage-issues",
            "project_id": "11111111-1111-4111-8111-111111111111", "expires_at_unix_ms": 2_000_000_000_000,
            "credential_file": str(issue_credential)}]}))
        for path in (issue_credential, readers):
            path.chmod(0o600)
        shutil.rmtree(self.root / "config/server")
        server.generate_server_configuration(self.root, self.installation,
            factory_profiles=profiles, factory_signal_connectors=connectors, factory_issue_mcp_readers=readers)
        installed = self.root / "config/server"
        retained = {path.name: path.read_bytes() for pattern in ("factory-signal-*", "factory-issue-mcp-*") for path in installed.glob(pattern)}
        with lifecycle.LifecycleState(self.root) as owner, mock.patch.object(CONFIGURATOR, "_require_apple_silicon"):
            with updates.factory_installation_update(
                self.root, self.installation, owner=owner, factory_profiles=profiles,
                factory_agent_profile=policy, preflight=lambda _: None, stop=lambda: None,
            ):
                pass
        self.assertEqual({path.name: path.read_bytes() for pattern in ("factory-signal-*", "factory-issue-mcp-*") for path in installed.glob(pattern)}, retained)
        self.assertTrue(all(stat.S_IMODE((installed / name).stat().st_mode) == 0o600 for name in retained))

    def test_factory_update_retains_runtime_support_files_at_the_existing_installation_depth_limit(self):
        import local_stand_installation_update as updates
        import local_stand_lifecycle as lifecycle
        import configure_local_stand_server as server

        profiles, policy = self._factory_update_inputs()
        runtime = Path(self.temporary.name) / "runtime"
        support = runtime / "one/two/three/four/support.json"
        support.parent.mkdir(mode=0o700, parents=True)
        for directory in (runtime, *support.parents):
            if directory.is_relative_to(runtime):
                directory.chmod(0o700)
        support.write_text('{"reviewed":"retained runtime support"}')
        support.chmod(0o600)
        shutil.rmtree(self.root / "config/server")
        server.generate_server_configuration(self.root, self.installation, factory_profiles=profiles, factory_runtime_bundle=runtime)
        with lifecycle.LifecycleState(self.root) as owner, mock.patch.object(CONFIGURATOR, "_require_apple_silicon"):
            with updates.factory_installation_update(
                self.root, self.installation, owner=owner, factory_profiles=profiles,
                factory_agent_profile=policy, preflight=lambda _: None, stop=lambda: None,
            ):
                retained = self.root / "config/server/factory-runtime/one/two/three/four/support.json"
                self.assertEqual(retained.read_bytes(), support.read_bytes())

    def test_ordinary_launcher_applies_explicit_update_then_restarts_with_the_same_identity(self):
        import local_stand_launcher as launcher

        profiles, policy = self._factory_update_inputs()
        credential = self.credential.read_bytes()
        with (
            mock.patch.object(CONFIGURATOR, "_require_apple_silicon"),
            mock.patch.object(launcher, "_prepare_native_installation", return_value=launcher.NativeInstallation(self.installation, {})),
            mock.patch.object(launcher, "_build_images"),
            mock.patch.object(launcher.installation_update, "validate_factory_installation"),
            mock.patch.object(launcher, "_run_compose"),
            mock.patch.object(launcher, "_stop_halves") as stop,
            mock.patch.object(launcher.bootstrapper, "bootstrap", return_value=types.SimpleNamespace(state="already_enrolled")),
            mock.patch.object(launcher.agent_starter, "start_agent", side_effect=[
                types.SimpleNamespace(state="registered", pid=4242),
                types.SimpleNamespace(state="already_running", pid=4242),
            ]),
        ):
            arguments = ["up", "--root", str(self.root), "--factory-profiles", str(profiles),
                         "--factory-agent-profile", str(policy)]
            updated = launcher.run(launcher.parse_arguments([*arguments, "--update-factory-installation"]))
            self.assertIn(b"mcp.example", (self.root / "config/agent.toml").read_bytes())
            restarted = launcher.run(launcher.parse_arguments(arguments))
            self.assertEqual(updated["state"], "running")
            self.assertEqual(updated["agent"], "registered")
            self.assertEqual(restarted["state"], "running")
            self.assertEqual(restarted["agent"], "already_running")
            self.assertEqual(restarted["bootstrap"], "already_enrolled")
            self.assertEqual(restarted["agent_pid"], 4242)
            self.assertEqual(stop.call_count, 1)
            self.assertFalse(stop.call_args.kwargs["remove_volumes"])
        self.assertEqual(self.credential.read_bytes(), credential)

    def test_explicit_standalone_mcp_inventory_preserves_independent_workload_grants(self):
        policy, permissions, _ = self._factory_profile()
        program = Path(self.temporary.name) / "issue-reader"
        # Inventory validation inspects native format/platform; no guest program runs here.
        elf = bytearray(64)
        elf[:7] = b"\x7fELF\x02\x01\x01"  # ELF64, little endian, version 1.
        elf[16:24] = b"\x02\x00\xb7\x00\x01\x00\x00\x00"  # Executable, AArch64, version 1.
        elf[52:54] = (64).to_bytes(2, "little")
        program.write_bytes(elf)
        program.chmod(0o755)
        entry = (
            f'"issue-reader" = {{ path = {json.dumps(str(program))}, version = "1.2.0", '
            f'platform = "linux-aarch64", sha256 = "{hashlib.sha256(program.read_bytes()).hexdigest()}", '
            'standalone = true }, '
        )
        policy.write_text(policy.read_text().replace("tool_executables = {", "tool_executables = { " + entry, 1))
        receipt = CONFIGURATOR.generate_agent_configuration(
            self.root, self.installation, repository=REPOSITORY, home=Path.home(), openssl=self.openssl,
            host_system="Darwin", host_machine="arm64", factory_agent_profile=policy,
        )
        configuration = tomllib.loads(Path(receipt["config"]).read_text())
        self.assertTrue(configuration["tool_executables"]["issue-reader"]["standalone"])
        self.assertEqual(configuration["factory_permissions"], permissions)
        self.assertEqual(configuration["allowed_network_hosts"], ["api.openai.com"])

    def test_native_login_file_reference_is_preserved_without_copying_the_host_home(self):
        policy, _, _ = self._factory_profile()
        private = Path(self.temporary.name) / "private-native-login"
        private.mkdir(mode=0o700)
        credential = private / "auth.json"
        credential.write_text('{"auth_mode":"chatgpt","tokens":{"access_token":"native-file-canary"}}')
        credential.chmod(0o600)
        (private / "config.toml").write_text("unselected MCP settings")
        references = {"model-login": {"path": str(credential), "destination": "codex/auth.json", "sensitive_fields": ["/tokens"], "persist_updates": True}}
        policy.write_text(
            f'factory_credential_profiles = {CONFIGURATOR._toml_value(references)}\n' + policy.read_text()
        )
        CONFIGURATOR.preflight_factory_agent_profile(self.root, self.installation, policy)
        receipt = CONFIGURATOR.generate_agent_configuration(
            self.root, self.installation, repository=REPOSITORY, home=Path.home(), openssl=self.openssl,
            host_system="Darwin", host_machine="arm64", factory_agent_profile=policy,
        )
        contents = Path(receipt["config"]).read_bytes()
        self.assertEqual(tomllib.loads(contents.decode())["factory_credential_profiles"], references)
        self.assertNotIn(b"native-file-canary", contents)
        self.assertFalse((Path(receipt["config"]).parent / "auth.json").exists())
        CONFIGURATOR.validated_launch_plan(self.root.resolve(), self.installation, receipt)
        CONFIGURATOR.preflight_factory_agent_profile(self.root, self.installation, policy)

    def test_factory_credential_references_remain_agent_owned_across_restart(self):
        policy, _, _ = self._factory_profile()
        private = Path(self.temporary.name) / "private-model"
        private.mkdir(mode=0o700)
        credential = private / "model.json"
        credential.write_text('{"OPENAI_API_KEY":"private-credential-canary"}')
        credential.chmod(0o600)
        references = {"model-coding": str(credential)}
        policy.write_text(
            f'factory_credential_profiles = {CONFIGURATOR._toml_value(references)}\n'
            + policy.read_text()
        )
        CONFIGURATOR.preflight_factory_agent_profile(self.root, self.installation, policy)
        receipt = CONFIGURATOR.generate_agent_configuration(
            self.root, self.installation, repository=REPOSITORY, home=Path.home(), openssl=self.openssl,
            host_system="Darwin", host_machine="arm64", factory_agent_profile=policy,
        )
        configuration = Path(receipt["config"])
        original = configuration.read_bytes()
        self.assertEqual(tomllib.loads(original.decode())["factory_credential_profiles"], references)
        self.assertNotIn(b"private-credential-canary", original)
        CONFIGURATOR.validated_launch_plan(self.root.resolve(), self.installation, receipt)
        CONFIGURATOR.preflight_factory_agent_profile(self.root, self.installation, policy)
        replacement = private / "other.json"
        replacement.write_bytes(credential.read_bytes())
        replacement.chmod(0o600)
        policy.write_text(policy.read_text().replace(str(credential), str(replacement)))
        with self.assertRaisesRegex(CONFIGURATOR.AgentConfigurationError, "reset"):
            CONFIGURATOR.preflight_factory_agent_profile(self.root, self.installation, policy)
        self.assertEqual(configuration.read_bytes(), original)

    def test_factory_protected_reader_is_separate_from_model_tools_and_immutable(self):
        policy, _, _ = self._factory_profile()
        release = Path(self.temporary.name) / "reader-release"
        release.mkdir(mode=0o700)
        binary = release / "bounded-reader"
        binary.write_bytes(b"reader fixture")
        binary.chmod(0o500)
        reader = {
            "release_root": str(release), "entrypoint": "bounded-reader", "platform": "linux-aarch64",
            "tool": {"identity": "source-reader", "version": "1", "sha256": "a" * 64},
            "executable": {"identity": "bounded-reader", "version": "1", "sha256": hashlib.sha256(binary.read_bytes()).hexdigest()},
            "resources": {"cpu_millis": 1000, "memory_bytes": 268435456, "disk_bytes": 16777216, "process_count": 8, "elapsed_millis": 5000},
            "outputs": {"kinds": [], "max_artifact_count": 0, "max_artifact_bytes": 0, "max_report_count": 0, "max_report_bytes": 0},
        }
        policy.write_text(f'protected_reader = {CONFIGURATOR._toml_value(reader)}\n' + policy.read_text())
        CONFIGURATOR.preflight_factory_agent_profile(self.root, self.installation, policy)
        receipt = CONFIGURATOR.generate_agent_configuration(
            self.root, self.installation, repository=REPOSITORY, home=Path.home(), openssl=self.openssl,
            host_system="Darwin", host_machine="arm64", factory_agent_profile=policy,
        )
        configuration = Path(receipt["config"])
        original = configuration.read_bytes()
        document = tomllib.loads(original.decode())
        self.assertEqual(document["protected_reader"], reader)
        self.assertEqual(set(document["tool_executables"]), {"codex-cli"})
        CONFIGURATOR.validated_launch_plan(self.root.resolve(), self.installation, receipt)
        CONFIGURATOR.preflight_factory_agent_profile(self.root, self.installation, policy)
        policy.write_text(policy.read_text().replace('"elapsed_millis" = 5000', '"elapsed_millis" = 4000'))
        with self.assertRaisesRegex(CONFIGURATOR.AgentConfigurationError, "reset"):
            CONFIGURATOR.preflight_factory_agent_profile(self.root, self.installation, policy)
        self.assertEqual(configuration.read_bytes(), original)

    def _factory_capture_profile(self):
        policy, _, _ = self._factory_profile()
        executable = Path(self.temporary.name) / "operator-capture-git"
        executable.write_text("#!/bin/sh\nexit 0\n")
        executable.chmod(0o755)
        capture = {"provider": "git", "executable": {
            "path": str(executable), "version": "2.51.0",
            "platform": "macos-aarch64", "sha256": hashlib.sha256(executable.read_bytes()).hexdigest(),
        }}
        executable_toml = ", ".join(f"{name} = {json.dumps(value)}" for name, value in capture["executable"].items())
        policy.write_text(f'change_set_capture = {{ provider = "git", executable = {{ {executable_toml} }} }}\n' + policy.read_text())
        return policy, capture

    def test_factory_installs_the_explicit_trusted_capture_configuration(self):
        policy, capture = self._factory_capture_profile()
        credential_before = self.credential.read_bytes()
        CONFIGURATOR.preflight_factory_agent_profile(self.root, self.installation, policy)
        self.assertFalse((self.root / "config/agent.toml").exists())
        self.assertEqual(self.credential.read_bytes(), credential_before)
        receipt = CONFIGURATOR.generate_agent_configuration(
            self.root, self.installation, repository=REPOSITORY, home=Path.home(), openssl=self.openssl,
            host_system="Darwin", host_machine="arm64", factory_agent_profile=policy,
        )
        configuration = tomllib.loads(Path(receipt["config"]).read_text())
        self.assertEqual(configuration["change_set_capture"], capture)
        installed_policy = tomllib.loads(Path(receipt["factory_agent_profile"]).read_text())
        self.assertEqual(installed_policy["change_set_capture"], capture)
        CONFIGURATOR.validated_launch_plan(self.root.resolve(), self.installation, receipt)

    def test_factory_launch_rejects_capture_configuration_removed_from_installed_policy(self):
        policy, _ = self._factory_capture_profile()
        receipt = CONFIGURATOR.generate_agent_configuration(
            self.root, self.installation, repository=REPOSITORY, home=Path.home(), openssl=self.openssl,
            host_system="Darwin", host_machine="arm64", factory_agent_profile=policy,
        )
        config = Path(receipt["config"])
        config.write_text("\n".join(line for line in config.read_text().splitlines()
                                   if not line.startswith("change_set_capture = ")) + "\n")
        with self.assertRaisesRegex(CONFIGURATOR.AgentConfigurationError, "differs"):
            CONFIGURATOR.validated_launch_plan(self.root.resolve(), self.installation, receipt)

    def test_factory_capture_preflight_rejects_changed_bytes_before_agent_or_live_state(self):
        policy, capture = self._factory_capture_profile()
        observed = Path(self.temporary.name) / "capture-validator-observed"
        self._executable("agent/bin/octacity-agent",
                         '#!/usr/bin/env python3\nfrom pathlib import Path\n'
                         f'Path({str(observed)!r}).write_text("called")\n')
        self._write_manifest()
        Path(capture["executable"]["path"]).write_text("#!/bin/sh\necho changed-capture-byte-canary\n")
        credential_before = self.credential.read_bytes()
        with self.assertRaisesRegex(CONFIGURATOR.AgentConfigurationError, "profile is invalid") as error:
            CONFIGURATOR.preflight_factory_agent_profile(self.root, self.installation, policy)
        self.assertNotIn("changed-capture-byte-canary", str(error.exception))
        self.assertFalse(observed.exists())
        self.assertEqual(self.credential.read_bytes(), credential_before)
        self.assertFalse((self.root / "config/agent.toml").exists())

    def test_factory_capture_preflight_passes_exact_config_in_temporary_roots(self):
        policy, capture = self._factory_capture_profile()
        observed = Path(self.temporary.name) / "capture-preflight.json"
        self._executable(
            "agent/bin/octacity-agent",
            '#!/usr/bin/env python3\nimport json,sys,tomllib\nfrom pathlib import Path\n'
            'config=tomllib.loads(Path(sys.argv[2]).read_text())\n'
            f'Path({str(observed)!r}).write_text(json.dumps(config))\n'
            'print("configuration is valid")\n',
        )
        self._write_manifest()
        CONFIGURATOR.preflight_factory_agent_profile(self.root, self.installation, policy)
        configuration = json.loads(observed.read_text())
        self.assertEqual(configuration["change_set_capture"], capture)
        for name in ("work_root", "state_root", "credential_file"):
            self.assertFalse(Path(configuration[name]).is_relative_to(self.root.resolve()))
            self.assertFalse(Path(configuration[name]).exists())
        self.assertFalse((self.root / "config/agent.toml").exists())

    def test_factory_agent_validator_failure_does_not_disclose_private_diagnostics(self):
        policy, _, _ = self._factory_profile()
        self._executable(
            "agent/bin/octacity-agent",
            '#!/bin/sh\nif [ "$1" = --version ]; then echo "octacity-agent 0.1.0"; exit 0; fi\n'
            'echo "private-factory-command-canary" >&2\nexit 2\n',
        )
        self._write_manifest()
        with self.assertRaises(CONFIGURATOR.AgentConfigurationError) as error:
            CONFIGURATOR.generate_agent_configuration(
                self.root, self.installation, repository=REPOSITORY, home=Path.home(), openssl=self.openssl,
                host_system="Darwin", host_machine="arm64", factory_agent_profile=policy,
            )
        self.assertNotIn("private-factory-command-canary", str(error.exception))
        self.assertFalse((self.root / "config/agent.toml").exists())

    def test_factory_preflight_validates_in_temporary_roots_without_opening_live_agent_state(self):
        policy, _, _ = self._factory_profile()
        live = self.root / "agent/work/live-state-canary"
        live.parent.mkdir()
        live.write_bytes(b"live-state-must-not-change")
        observed = Path(self.temporary.name) / "observed.json"
        self._executable(
            "agent/bin/octacity-agent",
            '#!/usr/bin/env python3\nimport json,sys,tomllib\nfrom pathlib import Path\n'
            'if sys.argv[1] == "--version":\n print("octacity-agent 0.1.0");sys.exit(0)\n'
            'config=tomllib.loads(Path(sys.argv[2]).read_text())\n'
            f'Path({str(observed)!r}).write_text(json.dumps(config))\n'
            'print("configuration is valid")\n',
        )
        self._write_manifest()
        CONFIGURATOR.preflight_factory_agent_profile(self.root, self.installation, policy)
        config = json.loads(observed.read_text())
        for name in ("work_root", "state_root", "credential_file"):
            self.assertFalse(Path(config[name]).is_relative_to(self.root.resolve()))
            self.assertFalse(Path(config[name]).exists(), "temporary preflight inputs must be removed")
        self.assertEqual(live.read_bytes(), b"live-state-must-not-change")
        self.assertFalse((self.root / "config/agent.toml").exists())
        self.assertEqual(config["factory_permissions"]["secret_profiles"], ["model-coding"])

    def test_factory_preflight_cannot_change_a_published_ordinary_agent_configuration(self):
        original = self.generate()
        config = Path(original["config"])
        before = config.read_bytes()
        policy, _, _ = self._factory_profile()
        with self.assertRaisesRegex(CONFIGURATOR.AgentConfigurationError, "reset"):
            CONFIGURATOR.preflight_factory_agent_profile(self.root, self.installation, policy)
        self.assertEqual(config.read_bytes(), before)

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
