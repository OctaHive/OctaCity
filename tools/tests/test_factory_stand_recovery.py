"""Default CLI recovery over real private configurators and installation updates.

Native package/image producers, HTTP enrollment and process startup are finite
test adapters. File validation, atomic updates and argument selection stay real.
"""
from contextlib import redirect_stderr, redirect_stdout
import io
import json
import os
from pathlib import Path
import subprocess
import time
import types
import unittest
from unittest import mock
import uuid

import test_configure_local_stand_agent as fixtures
import factory_local_stand as stand
import factory_integrations as integrations
import local_stand_installation_update as updates
import local_stand_lifecycle as lifecycle


@unittest.skipUnless(os.name == "posix", "private installation recovery requires POSIX ownership")
class FactoryStandRecoveryTests(unittest.TestCase):
    def setUp(self):
        self.fixture = fixtures.LocalStandAgentConfigurationTests()
        self.fixture.setUp()
        self.addCleanup(self.fixture.doCleanups)
        self.root = self.fixture.root.resolve()
        self.profiles, self.policy = self.fixture._factory_update_inputs()
        self.project = "10000000-0000-4000-8000-000000000001"
        self.readers = self._readers("initial", "ab")
        self._update(self.readers)
        integrations.configure_integrations(self.root, "Example/Tickets", native_repository_id="1234", adapter_digest="ab" * 32,
            preparation_mapping=stand.triage.preparation_mapping(self.project))
        self.installation = {"namespace": str(uuid.uuid4()), "source": "https://github.com/OctaHive/OctaCity", "revision": None}
        self.receipt = {"project": self.project, "configuration": {"id": str(uuid.uuid4()), "version": 1}}
        for name, value in {
            "factory-triage-installation": self.installation,
            "factory-triage-resources": {"project": self.project},
            "factory-triage-assets": {"profiles": str(self.profiles), "agent_profile": str(self.policy), "issue_mcp_readers": str(self.readers)},
            "factory-triage": self.receipt,
            "factory-github-connection": {"project_id": self.project},
        }.items():
            stand._private_json(self.root / "evidence" / (name + ".json"), value)

    def _readers(self, name, token):
        base = Path(self.fixture.temporary.name)
        credential = base / (name + "-credential")
        credential.write_text("ocmcp.recovery." + token * 32)
        credential.chmod(0o600)
        path = base / (name + "-readers.json")
        path.write_text(json.dumps({"version": 1, "readers": [{"id": "recovery", "project_id": self.project,
            "credential_file": str(credential), "expires_at_unix_ms": time.time_ns() // 1000000 + 3600000}]}))
        path.chmod(0o600)
        return path

    def _update(self, readers):
        with lifecycle.LifecycleState(self.root) as owner, mock.patch.object(fixtures.CONFIGURATOR, "_require_apple_silicon"):
            with updates.factory_installation_update(self.root, self.fixture.installation, owner=owner,
                    factory_profiles=self.profiles, factory_agent_profile=self.policy,
                    factory_issue_mcp_readers=readers, preflight=lambda _: None, stop=lambda: None):
                pass

    def _resume(self, *arguments):
        execute = subprocess.run

        def artifact_process(command, *args, **options):
            if command[0] == "docker":
                return types.SimpleNamespace(stdout="ab" * 32 + "  adapter\n", returncode=0)
            return execute(command, *args, **options)

        def start_agent(root, native, **options):
            fixtures.CONFIGURATOR.generate_agent_configuration(root, native,
                factory_agent_profile=options["factory_agent_profile"], host_system="Darwin", host_machine="arm64")
            return types.SimpleNamespace(state="registered", pid=42)

        output, errors = io.StringIO(), io.StringIO()
        with mock.patch.object(stand.launcher, "_prepare_native_installation", return_value=stand.launcher.NativeInstallation(self.fixture.installation, {})), \
                mock.patch.object(stand.launcher, "_build_images"), \
                mock.patch.object(stand, "install_runtime_executable", return_value={}), \
                mock.patch.object(stand.triage, "prepare_guest_image", return_value="registry.example/worker@sha256:" + "c" * 64), \
                mock.patch.object(stand.launcher, "_run_compose"), \
                mock.patch.object(updates, "validate_factory_installation"), \
                mock.patch.object(stand.launcher.bootstrapper, "bootstrap", return_value=types.SimpleNamespace(state="registered")), \
                mock.patch.object(stand.launcher.agent_starter, "start_agent", side_effect=start_agent), \
                mock.patch.object(subprocess, "run", side_effect=artifact_process), \
                redirect_stdout(output), redirect_stderr(errors):
            code = stand.main(["--root", str(self.root), *arguments])
        self.assertEqual(code, 0, errors.getvalue())
        self.assertEqual(json.loads(output.getvalue().splitlines()[-1]), self.receipt)

    def test_default_resume_uses_the_retained_custom_code_repository(self):
        path = self.root / "evidence/factory-triage-installation.json"
        self.installation["source"] = "https://github.com/Example/Code"
        path.write_text(json.dumps(self.installation))
        before = path.read_bytes()
        self._resume()
        self.assertEqual(path.read_bytes(), before)

    def test_resume_rejects_an_explicitly_different_code_repository_without_replacing_the_selection(self):
        path = self.root / "evidence/factory-triage-installation.json"
        before = path.read_bytes()
        output, errors = io.StringIO(), io.StringIO()
        with redirect_stdout(output), redirect_stderr(errors):
            code = stand.main(["--root", str(self.root), "--source-repository", "https://github.com/Example/Other"])
        self.assertEqual(code, 1)
        self.assertIn("another selected code source", errors.getvalue())
        self.assertEqual(path.read_bytes(), before)

    def test_default_resume_keeps_the_current_reviewed_installation_after_an_explicit_update(self):
        renewed = self._readers("renewed", "cd")
        updated_policy = self.policy.with_name("renewed-agent-policy.toml")
        updated_policy.write_text(self.policy.read_text().replace("mcp.example", "reviewed.mcp.example"))
        updated_policy.chmod(0o600)
        self.policy = updated_policy
        self._update(renewed)
        installed = self.root / "config"
        before = {path.relative_to(installed): path.read_bytes() for path in installed.rglob("*") if path.is_file()}
        credential = self.fixture.credential.read_bytes()
        self._resume()
        self.assertEqual({path.relative_to(installed): path.read_bytes() for path in installed.rglob("*") if path.is_file()}, before)
        self.assertEqual(self.fixture.credential.read_bytes(), credential)
        self.assertEqual((installed / "server/factory-issue-mcp-0000-credential").read_text(), "ocmcp.recovery." + "cd" * 32)
