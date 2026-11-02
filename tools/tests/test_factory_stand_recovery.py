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
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

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
        self.source_records, self.source_diagnostics, self.source_publications = [], {}, []
        self.policy_requests = []
        self.fail_policy_request = False
        self.retained_configuration = {
            "reference": {"project_id": self.project, "id": str(uuid.uuid4()), "version": 1, "digest": "03" * 32},
            "admission_policy": {"identity": "retained-admission", "version": "v2", "digest": "01" * 32},
            "permission_ceiling": {"identity": "retained-access", "version": "v3", "digest": "02" * 32},
        }
        self.policy_defaults = {"version": 4, "launch_policy": None, "access_profile": None}
        catalog = json.loads(self.profiles.read_text())
        catalog["projects"][0]["references"].append({"kind": "permission_ceiling", "alias": "triage-ceiling",
            "reference": {"identity": "triage-ceiling", "version": "v1", "digest": "06" * 32}})
        self.profiles.write_text(json.dumps(catalog))
        self.provisioned_defaults = None
        self.unresolved_policy_receipts = {}
        self.resolved_policy_references = []
        self.legacy_access_available = True
        fixture = self

        class Management(BaseHTTPRequestHandler):
            def log_message(self, *_):
                pass
            def reply(self, value, status=200):
                contents = json.dumps(value).encode()
                self.send_response(status)
                self.send_header("Content-Length", str(len(contents)))
                self.end_headers()
                self.wfile.write(contents)
            def do_GET(self):
                if self.path.endswith("/factory-policies/defaults"):
                    return self.reply({"defaults": fixture.policy_defaults})
                if self.path.startswith("/api/v1/factory-configurations/"):
                    if fixture.retained_configuration is None:
                        return self.reply({"error": "missing"}, 404)
                    return self.reply({"configuration": fixture.retained_configuration})
                if "/diagnostics" in self.path:
                    self.reply(fixture.source_diagnostics[self.path.split("/factory-source-inputs/")[1].split("/")[0]])
                else:
                    self.reply({"items": fixture.source_records, "next_cursor": None})
            def do_POST(self):
                body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
                if self.path.endswith("/factory-policies/legacy/resolve"):
                    return self.reply({"mapping": {"legacy": body} if body in fixture.resolved_policy_references else None})
                if self.path.endswith("/factory-policies/provision"):
                    fixture.policy_requests.append((self.path, self.headers.get("Idempotency-Key"), body))
                    if fixture.fail_policy_request:
                        fixture.fail_policy_request = False
                        return self.reply({"error": "temporarily unavailable"}, 503)
                    key = self.headers.get("Idempotency-Key")
                    replayed = key in fixture.unresolved_policy_receipts
                    unresolved = fixture.unresolved_policy_receipts.setdefault(key, [] if fixture.legacy_access_available else [
                        {"legacy": body["legacy_references"][-1], "reason": "reference_not_installed"}
                    ])
                    if not unresolved:
                        fixture.resolved_policy_references.extend(body["legacy_references"])
                    return self.reply({"disposition": "replayed" if replayed else "applied",
                        "defaults": fixture.provisioned_defaults or fixture.policy_defaults, "unresolved": unresolved})
                if self.path.endswith("/factory-runtimes/triage-runtime"):
                    return self.reply({})
                fixture.source_publications.append((self.path, self.headers.get("If-Match"), body))
                self.reply({"publication": {"inputs": []}})

        self.management = ThreadingHTTPServer(("127.0.0.1", 0), Management)
        threading.Thread(target=self.management.serve_forever, daemon=True).start()
        self.addCleanup(self.management.server_close)
        self.addCleanup(self.management.shutdown)
        self.management_client = stand.bootstrap.BootstrapClient(stand.BootstrapTransport(self.management.server_port),
            stand.bootstrap.RetryPolicy(attempts=1, timeout_seconds=2, initial_delay_seconds=0))
        self.readers = self._readers("initial", "ab")
        self._update(self.readers)
        integrations.configure_integrations(self.root, "Example/Tickets", native_repository_id="1234", adapter_digest="ab" * 32,
            preparation_mapping=stand.triage.preparation_mapping(self.project))
        self.installation = {"namespace": str(uuid.uuid4()), "source": "https://github.com/OctaHive/OctaCity", "revision": None}
        self.receipt = {"project": self.project, "configuration": self.retained_configuration["reference"]}
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

    def _resume(self, *arguments, expected_error=None):
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
        # Recovery uses a supported native host fixture, independently of the test runner.
        with mock.patch.object(fixtures.CONFIGURATOR, "_require_apple_silicon"), \
                mock.patch.object(stand.launcher, "_prepare_native_installation", return_value=stand.launcher.NativeInstallation(self.fixture.installation, {})), \
                mock.patch.object(stand.launcher, "_build_images"), \
                mock.patch.object(stand, "install_runtime_executable", return_value={}), \
                mock.patch.object(stand.triage, "prepare_guest_image", return_value="registry.example/worker@sha256:" + "c" * 64), \
                mock.patch.object(stand.launcher, "_run_compose"), \
                mock.patch.object(updates, "validate_factory_installation"), \
                mock.patch.object(stand.launcher.bootstrapper, "bootstrap", return_value=types.SimpleNamespace(state="registered")), \
                mock.patch.object(stand.launcher.agent_starter, "start_agent", side_effect=start_agent), \
                mock.patch.object(stand.integration, "LocalStandClient", return_value=self.management_client), \
                mock.patch.object(subprocess, "run", side_effect=artifact_process), \
                redirect_stdout(output), redirect_stderr(errors):
            code = stand.main(["--root", str(self.root), *arguments])
        if expected_error is not None:
            self.assertEqual(code, 1)
            self.assertIn(expected_error, errors.getvalue())
            return
        self.assertEqual(code, 0, errors.getvalue())
        self.assertEqual(json.loads(output.getvalue().splitlines()[-1]), self.receipt)

    def test_rebuild_retries_exact_legacy_provisioning_without_republishing_retained_state(self):
        before = (self.root / "evidence/factory-triage.json").read_bytes()
        self.fail_policy_request = True
        self._resume(expected_error="HTTP 503")
        self._resume()
        self.assertEqual(len(self.policy_requests), 2)
        self.assertEqual(self.policy_requests[0], self.policy_requests[1])
        path, key, request = self.policy_requests[1]
        self.assertEqual(path, f"/api/v1/projects/{self.project}/factory-policies/provision")
        self.assertTrue(key)
        self.assertEqual(request["designated_access"]["identity"], "triage-ceiling")
        self.assertEqual(request["designated_access"]["digest"], "06" * 32)
        self.assertEqual(request["legacy_references"], [
            {"kind": "launch", "reference": self.retained_configuration["admission_policy"]},
            {"kind": "access", "reference": self.retained_configuration["permission_ceiling"]},
        ])
        self.assertEqual((self.root / "evidence/factory-triage.json").read_bytes(), before)
        self.assertEqual(self.source_publications, [])
        self.assertEqual(self.policy_defaults, {"version": 4, "launch_policy": None, "access_profile": None})

    def test_default_resume_uses_the_retained_custom_code_repository(self):
        path = self.root / "evidence/factory-triage-installation.json"
        self.installation["source"] = "https://github.com/Example/Code"
        path.write_text(json.dumps(self.installation))
        before = path.read_bytes()
        self._resume()
        self.assertEqual(path.read_bytes(), before)

    def test_resume_reports_unresolved_policy_and_rechecks_after_catalog_repair(self):
        self.legacy_access_available = False
        self._resume(expected_error="retained-access")
        self.legacy_access_available = True
        self._resume()
        self.assertNotEqual(self.policy_requests[0][1], self.policy_requests[-1][1])
        self.assertEqual(self.policy_requests[0][2], self.policy_requests[-1][2])
        self._resume()
        self.assertEqual(self.policy_requests[-1][1], self.policy_requests[0][1])

    def test_first_publication_starts_with_the_same_pinned_vcs_bundle_as_its_bootstrap(self):
        import tomllib
        (self.root / "evidence/factory-triage.json").unlink()
        # Resource preparation is resumable before the first pipeline publication.
        # The initial private bundle is created by the real configurator below.
        import shutil
        shutil.rmtree(self.root / "config/server")
        shutil.rmtree(self.root / "config/integrations")
        self.retained_configuration = None
        self.policy_defaults.update(
            launch_policy={"project_id": self.project, "id": str(uuid.uuid4()), "version": 1, "digest": "01" * 32},
            access_profile={"project_id": self.project, "id": str(uuid.uuid4()), "version": 1, "digest": "02" * 32},
        )
        self.provisioned_defaults = {**self.policy_defaults, "version": 3,
            "access_profile": {**self.policy_defaults["access_profile"], "version": 2, "digest": "04" * 32}}
        settings = self.root / "triage/runtime-settings.json"
        stand._private_json(settings, {"permissions": {}})
        assets = self.root / "evidence/factory-triage-assets.json"
        value = json.loads(assets.read_text())
        value["runtime_settings"] = str(settings)
        assets.write_text(json.dumps(value))
        published = {"configuration": self.receipt["configuration"]}
        self.receipt = {"schema_version": 1, "origin": stand.bootstrap.DEFAULT_ORIGIN,
            "project": self.project, "profiles": {}, "context": {}, **published}
        with mock.patch.object(stand, "install_repository", side_effect=lambda root, repository, **_:
                integrations.configure_integrations(root, repository, native_repository_id="1234", adapter_digest="ab" * 32,
                    preparation_mapping=stand.triage.preparation_mapping(self.project))), \
                mock.patch.object(stand, "ensure_github_connection", return_value={}), \
                mock.patch.object(stand, "publish_triage_roles", return_value={}), \
                mock.patch.object(stand, "publish_triage_context", return_value={}), \
                mock.patch.object(stand.triage, "pipeline_selection", return_value={}) as select, \
                mock.patch.object(stand, "publish_triage", return_value=published):
            self._resume("--github-repository", "Example/Tickets")
        self.assertEqual(len(self.policy_requests), 1)
        self.assertEqual(self.policy_requests[0][2]["legacy_references"], [])
        self.assertEqual(select.call_args.kwargs["policy_defaults"], self.policy_defaults)
        configured = tomllib.loads((self.root / "config/server/server.toml").read_text())
        self.assertEqual(configured["vcs_integrations"][0]["integration_id"],
            str(uuid.uuid5(uuid.UUID(self.installation["namespace"]), "source")))

    def test_default_resume_adds_pinned_vcs_without_replacing_existing_resources_or_credentials(self):
        import tomllib
        before = self.receipt.copy()
        credential = self.fixture.credential.read_bytes()
        self._resume()
        configured = tomllib.loads((self.root / "config/server/server.toml").read_text())
        self.assertEqual(configured["vcs_adapter_registry"], "/usr/local/lib/octacity/vcs")
        self.assertEqual(configured["vcs_integrations"], [{
            "integration_id": str(uuid.uuid5(uuid.UUID(self.installation["namespace"]), "source")),
            "adapter_id": "git", "adapter_sha256": "ab" * 32, "credential_handle": "anonymous"}])
        self.assertEqual(json.loads((self.root / "evidence/factory-triage.json").read_text()), before)
        self.assertEqual(self.fixture.credential.read_bytes(), credential)
        integrated = tomllib.loads((self.root / "config/integrations/server.toml").read_text())
        self.assertEqual(integrated["vcs_integrations"], configured["vcs_integrations"])
        self._resume()
        self.assertEqual(tomllib.loads((self.root / "config/server/server.toml").read_text()), configured)

    def test_vcs_upgrade_retries_exhausted_enabled_source_through_versioned_publication(self):
        definition = {"enabled": True, "target": {"id": self.receipt["configuration"]["id"], "version": 2},
            "predicate": {"state": "open", "all_of": ["triage-test"]}, "reporter": None}
        for number, enabled, exhausted in ((1, True, True), (2, False, True), (3, True, False), (4, True, True)):
            identity = str(uuid.uuid5(uuid.NAMESPACE_OID, str(number)))
            self.source_records.append({"kind": "input", "definition": {"id": identity, "version": 4,
                "project_id": self.project, "definition": {**definition, "enabled": enabled}}})
            self.source_diagnostics[identity] = {"scan": {"schedule": {"exhausted": exhausted, "failure": "transient"}}}
        self.source_records.append({"kind": "group", "definition": {
            "members": [self.source_records[-1]["definition"]]}})
        self.fail_policy_request = True
        self._resume(expected_error="HTTP 503")
        self.assertEqual(self.source_publications, [])
        self._resume()
        self.assertEqual(len(self.source_publications), 1)
        path, precondition, request = self.source_publications[0]
        original = self.source_records[0]["definition"]
        self.assertIn(original["id"], path)
        self.assertEqual(precondition, '"4"')
        self.assertEqual(request, {"intent": {"project_id": self.project, "group": None, "inputs": [{
            "id": original["id"], "expected_version": 4, "definition": definition}]}})
        self._resume()
        self.assertEqual(len(self.source_publications), 1, "unchanged installation must not reset attempts repeatedly")

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
        self._resume()
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

    def test_installation_update_rejects_redefining_an_exact_access_ceiling(self):
        catalog = json.loads(self.profiles.read_text())
        ceiling, = catalog["projects"][0]["references"]
        ceiling["metadata"] = {"permission_declarations": {"network_hosts": ["old.example"]}}
        self.profiles.write_text(json.dumps(catalog))
        self._update(self.readers)
        ceiling["metadata"]["permission_declarations"]["network_hosts"] = ["new.example"]
        self.profiles.write_text(json.dumps(catalog))
        with self.assertRaisesRegex(stand.state.StateError, "access ceiling.*triage-ceiling"):
            self._update(self.readers)
