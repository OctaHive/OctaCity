"""Factory stand data is published through the existing management contracts."""
import copy
import importlib.util
from http.server import BaseHTTPRequestHandler, HTTPServer
import json
import os
import subprocess
import tempfile
import sys
from pathlib import Path
import unittest
from unittest.mock import patch
import uuid
import threading
import tomllib
import types
import io
import urllib.request

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import factory_local_stand as demo
import bootstrap_local_stand as bootstrap
import local_stand_state as state


class FactoryStandTestDependencyTests(unittest.TestCase):
    def test_non_posix_hosts_skip_private_bundle_tests_and_keep_portable_contracts(self):
        # Scope the platform to the test module; pathlib and the host keep their real OS.
        platform_os = types.ModuleType("os")
        platform_os.__dict__.update(vars(os))
        platform_os.name = "nt"
        spec = importlib.util.spec_from_file_location("non_posix_factory_stand_tests", __file__)
        module = importlib.util.module_from_spec(spec)
        with patch.dict(sys.modules, {"os": platform_os}):
            spec.loader.exec_module(module)
        names = (
            "test_integration_setup_refuses_to_adopt_or_change_an_unrecognized_existing_bundle",
            "test_issue_repository_installation_uses_public_identity_and_no_ambient_github_credentials",
            "test_retained_stand_installs_selected_issue_repository_without_replaying_demo_runs",
            "test_prepares_confined_integration_without_changing_existing_server_or_demo_data",
            "test_command_accepts_an_explicit_issue_repository_independent_of_build_source",
            "test_plain_command_installs_the_demo_issue_repository_without_an_extra_flag",
        )
        suite = unittest.TestSuite(module.FactoryLocalStandTests(name) for name in names)
        result = unittest.TestResult()
        with patch.object(state, "current_uid", side_effect=state.StateError(
            "local stand host state requires POSIX ownership"
        )):
            suite.run(result)
        self.assertEqual(result.errors, [])
        self.assertEqual(result.failures, [])
        self.assertEqual(result.testsRun, 6)
        self.assertEqual(len(result.skipped), 4)


class FactoryLocalStandTests(unittest.TestCase):
    def test_plain_command_installs_the_demo_issue_repository_without_an_extra_flag(self):
        with tempfile.TemporaryDirectory() as temporary:
            with patch.dict(os.environ, {"OCTACITY_LOCAL_STAND_ROOT": temporary}), \
                    patch.object(demo, "run_stand", return_value={}) as start:
                self.assertEqual(demo.main([]), 0)
            self.assertEqual(start.call_args.kwargs["github_repository"], "OctaHive/OctaCity")

    def test_plain_command_retains_an_installed_issue_repository_without_resolving_another(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            bundle = root / "config/integrations"
            bundle.mkdir(parents=True)
            retained = bundle / "server.toml"
            retained.write_text("retained private installation")
            with patch.object(demo, "run_stand", return_value={}) as start:
                self.assertEqual(demo.main(["--root", str(root)]), 0)
            self.assertIsNone(start.call_args.kwargs["github_repository"])
            self.assertEqual(retained.read_text(), "retained private installation")

    @unittest.skipUnless(os.name == "posix", "private integration bundles require POSIX ownership and modes")
    def test_integration_setup_refuses_to_adopt_or_change_an_unrecognized_existing_bundle(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            server = root / "config/server/server.toml"
            server.parent.mkdir(parents=True)
            server.write_text('management_bind = "0.0.0.0:8080"\n')
            server.chmod(0o600)
            bundle = root / "config/integrations"
            bundle.mkdir(mode=0o700)
            marker = bundle / "retained"
            marker.write_text("existing operator data")
            marker.chmod(0o600)
            with self.assertRaises(ValueError):
                demo.configure_integrations(root, "Example/Tickets", native_repository_id="1234", adapter_digest="ab"*32)
            self.assertEqual(list(bundle.iterdir()), [marker])
            self.assertEqual(marker.read_text(), "existing operator data")

    @unittest.skipUnless(os.name == "posix", "private integration bundles require POSIX ownership and modes")
    def test_issue_repository_installation_uses_public_identity_and_no_ambient_github_credentials(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            server = root / "config/server/server.toml"
            server.parent.mkdir(parents=True)
            server.write_text('management_bind = "0.0.0.0:8080"\n')
            server.chmod(0o600)
            response = io.BytesIO(b'{"id":1234,"full_name":"Example/Tickets"}')
            with patch.object(urllib.request.OpenerDirector, "open", return_value=response) as fetch, \
                    patch.object(subprocess, "run", return_value=subprocess.CompletedProcess([], 0, stdout="ab" * 32 + "  /usr/local/bin/octacity-github\n")), \
                    patch.dict(os.environ, {"GITHUB_TOKEN":"ambient-secret-must-not-be-used"}):
                bundle = demo.install_repository(root, "Example/Tickets")
            request = fetch.call_args.args[0]
            self.assertEqual(request.full_url, "https://api.github.com/repos/Example/Tickets")
            self.assertFalse(request.has_header("Authorization"))
            catalog = json.loads((bundle / "catalog.json").read_text())
            self.assertEqual(catalog["installations"][0]["resources"][0]["native_id"], "1234")
            self.assertNotIn("ambient-secret", json.dumps(catalog))

    @unittest.skipUnless(os.name == "posix", "private integration bundles require POSIX ownership and modes")
    def test_retained_stand_installs_selected_issue_repository_without_replaying_demo_runs(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            source = "https://github.com/OctaHive/OctaCity.git"
            demo._private_json(root / "evidence/factory-demo-resources.json", {"project":str(uuid.uuid4())})
            demo._private_json(root / "evidence/factory-demo-input.json", {"namespace":str(uuid.uuid4()), "source":source, "revision":"a"*40})
            retained = {"runs":{"manual":"retained-run"}}
            demo._private_json(root / "evidence/factory-demo.json", retained)
            startup = []
            with patch.object(demo.initializer, "initialize"), \
                    patch.object(demo.launcher, "_prepare_native_installation", return_value=types.SimpleNamespace(path=root / "native")), \
                    patch.object(demo.launcher, "_build_images"), \
                    patch.object(demo.server_config, "generate_server_configuration"), \
                    patch.object(demo.launcher, "run", side_effect=lambda *_: startup.append("start stand and agent")), \
                    patch.object(demo, "install_repository", side_effect=lambda *_: startup.append("install integrations")) as install, \
                    patch.object(demo.launcher, "_run_compose") as compose, \
                    patch.object(demo.integration, "LocalStandClient") as client:
                result = demo.run_stand(root, source, None, github_repository="Example/Tickets")
            self.assertEqual(result, retained)
            install.assert_called_once_with(root, "Example/Tickets")
            # Recreating the coordinator after worker startup cuts its pending lease poll.
            self.assertEqual(startup, ["install integrations", "start stand and agent"])
            compose.assert_not_called()
            self.assertEqual([call.args[0] for call in client.return_value.get.call_args_list],
                             ["/api/v1/integration-installations", "/api/v1/integrations"])
            client.return_value.post.assert_not_called()

    def test_command_accepts_an_explicit_issue_repository_independent_of_build_source(self):
        with patch.object(demo, "run_stand", return_value={}) as start:
            self.assertEqual(demo.main(["--github-repository", "Example/Tickets"]), 0)
        self.assertEqual(start.call_args.kwargs["github_repository"], "Example/Tickets")
        self.assertEqual(start.call_args.args[1], "https://github.com/OctaHive/OctaCity")

    @unittest.skipUnless(os.name == "posix", "private integration bundles require POSIX ownership and modes")
    def test_prepares_confined_integration_without_changing_existing_server_or_demo_data(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            server = root / "config/server/server.toml"
            server.parent.mkdir(parents=True)
            server.write_text('management_bind = "0.0.0.0:8080"\n')
            server.chmod(0o600)
            receipt = root / "evidence/factory-demo.json"
            receipt.parent.mkdir()
            receipt.write_text('{"retained":"run-history"}')
            bundle = demo.configure_integrations(root, "OctaHive/OctaCity", native_repository_id="1234", adapter_digest="ab" * 32)
            config = tomllib.loads((bundle / "server.toml").read_text())
            self.assertEqual(config["integration_management"]["peer_uid"], 101)
            self.assertEqual(config["integration_management"]["socket_path"], "/run/octacity-management/management.sock")
            catalog = json.loads((bundle / "catalog.json").read_text())
            installed, = catalog["installations"]
            self.assertEqual(installed["provider"], "github")
            resource, = installed["resources"]
            self.assertEqual(resource["native_id"], "1234")
            self.assertEqual(resource["locator"], "OctaHive/OctaCity")
            self.assertEqual({c["purpose"] for c in resource["capabilities"]}, {"task_read", "task_update"})
            self.assertEqual(len(catalog["checks"]), 2)
            for check in catalog["checks"]:
                self.assertEqual(check["resource"]["resource_id"], "1234")
                self.assertEqual(check["resource"]["resource_locator"], "OctaHive/OctaCity")
                self.assertEqual(check["network_hosts"], ["api.github.com"])
            self.assertNotIn("project_id", json.dumps(catalog))
            self.assertEqual(server.read_text(), 'management_bind = "0.0.0.0:8080"\n')
            self.assertEqual(json.loads(receipt.read_text()), {"retained":"run-history"})
            for path in bundle.iterdir():
                self.assertEqual(path.stat().st_mode & 0o777, 0o600)
            again = demo.configure_integrations(root, "OctaHive/OctaCity", native_repository_id="1234", adapter_digest="ab" * 32)
            self.assertEqual(again, bundle)

    def test_demo_rejects_a_failed_run_even_if_it_previously_reached_a_human_gate(self):
        class Api:
            def get(self, path):
                if path.endswith("node-attempts?limit=100"):
                    return {"items": [{"record": {"node_kind": "human_gate"}}]}
                return {"state": "failed"}

        with self.assertRaisesRegex(RuntimeError, "stopped in failed"):
            demo._wait_for_demo(Api(), "run", "human_gate")

    def test_command_honors_the_standard_stand_root_environment(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / "stand"
            with patch.dict(os.environ, {"OCTACITY_LOCAL_STAND_ROOT": str(root)}), \
                    patch.object(demo, "run_stand", return_value={}) as start:
                self.assertEqual(demo.main([]), 0)
            self.assertEqual(start.call_args.args[0], root)

    def test_command_refuses_to_replace_an_existing_ordinary_stand(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / "stand"
            bundle = root / "config/server"
            bundle.mkdir(parents=True)
            marker = bundle / "original"
            marker.write_text("retain this installation")
            result = subprocess.run([sys.executable, str(Path(demo.__file__).with_name("factory-stand")),
                                     "--root", str(root)], capture_output=True, text=True, timeout=5)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("another server installation", result.stderr)
            self.assertEqual(marker.read_text(), "retain this installation")
            self.assertEqual(list(root.iterdir()), [root / "config"])

    @unittest.skipUnless(os.environ.get("OCTACITY_RUN_FACTORY_STAND_DEMO") == "1",
                         "starts and retains the real local Factory demonstration stand")
    def test_one_launch_leaves_live_factory_with_completed_and_human_gated_runs(self):
        receipt = demo.run_stand(state.DEFAULT_ROOT, "https://github.com/OctaHive/OctaCity", None)
        self.assertEqual(receipt["origin"], "https://octacity.localhost:8443")
        self.assertEqual(set(receipt["runs"]), {"automatic", "manual"})
        client = demo.integration.LocalStandClient(state.DEFAULT_ROOT, receipt["origin"])
        automatic = client.get("/api/v1/factory-runs/" + receipt["runs"]["automatic"])
        manual = client.get("/api/v1/factory-runs/" + receipt["runs"]["manual"])
        self.assertEqual(automatic["state"], "completed")
        self.assertNotIn(manual["state"], {"failed", "cancelled", "completed"})
        attempts = client.get("/api/v1/factory-runs/" + receipt["runs"]["manual"] + "/node-attempts?limit=100")
        self.assertTrue(any(row["record"]["node_kind"] == "human_gate" for row in attempts["items"]))

    @unittest.skipUnless(os.environ.get("OCTACITY_RUN_FACTORY_STAND_DEMO") == "1",
                         "restarts the retained real demonstration stand")
    def test_restart_preserves_seed_receipt_without_replaying_operator_work(self):
        expected = demo._read_json(state.DEFAULT_ROOT / "evidence/factory-demo.json")
        with patch.object(demo.integration.LocalStandClient, "post",
                          side_effect=AssertionError("a seeded stand must not publish demonstration work again")):
            actual = demo.run_stand(state.DEFAULT_ROOT, "https://github.com/OctaHive/OctaCity", None)
        self.assertEqual(actual, expected)

    def test_prerequisites_bind_one_real_source_pipeline_configuration_and_trigger(self):
        calls = []
        namespace = uuid.UUID("00000000-0000-0000-0000-000000000009")

        class Api:
            def post(self, path, key, body):
                identity = str(uuid.uuid5(namespace, key))
                calls.append((path, identity, copy.deepcopy(body)))
                return {"resource": {"id": identity}}

        result = demo.prepare_resources(Api(), "00000000-0000-0000-0000-000000000003",
                                        "https://github.com/OctaHive/OctaCity", "registry.example/runtime@sha256:" + "a" * 64,
                                        str(namespace))
        configuration = next(row for row in calls if row[0] == "/api/v1/build-configurations")
        repository = next(row for row in calls if row[0] == "/api/v1/repositories")
        trigger = next(row for row in calls if row[0] == "/api/v1/trigger-definitions/manual")
        self.assertEqual(configuration[2]["definition"]["repository_id"], repository[1])
        self.assertEqual(trigger[2]["configuration_id"], configuration[1])
        self.assertEqual(result["configuration"], configuration[1])
        self.assertEqual(result["trigger"], trigger[1])
        self.assertEqual(result["repository"], repository[1])

    @unittest.skipUnless(os.environ.get("OCTACITY_RUN_FACTORY_STAND_BOOTSTRAP") == "1",
                         "requires prepared stand images, native installation and fresh PostgreSQL/MinIO")
    def test_bootstrap_server_leaves_the_final_factory_bundle_unpublished(self):
        root = state.DEFAULT_ROOT
        native = Path(os.environ["OCTACITY_FACTORY_STAND_NATIVE"])
        self.assertFalse((root / "config/server").exists())
        with demo.bootstrap_server(root, native) as client:
            client.ready()
            self.assertIsInstance(client.get("/api/v1/projects?limit=1")["items"], list)
            self.assertFalse((root / "config/server").exists())
        self.assertFalse((root / "config/server").exists())

    def test_bootstrap_transport_uses_loopback_and_retains_json_http_status(self):
        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *_args):
                pass

            def do_GET(self):
                self.send_response(200)
                self.send_header("Content-Type", "application/json")
                self.end_headers()
                self.wfile.write(b'{"status":"ready"}')

        server = HTTPServer(("127.0.0.1", 0), Handler)
        server.timeout = 0.2
        worker = threading.Thread(target=server.handle_request)
        worker.start()
        try:
            response = demo.BootstrapTransport(server.server_port).request("GET", "/health/ready", {}, None, 5)
            self.assertEqual(response.status, 200)
            self.assertEqual(json.loads(response.body), {"status": "ready"})
        finally:
            server.server_close()
            worker.join(timeout=5)

    def test_publication_uses_server_schema_and_published_flow_without_node_dispatch(self):
        project = str(uuid.uuid4())
        schema = {"reference": {"identity": "approved", "version": "v1", "sha256": "a" * 64},
                  "shape": {"type": "boolean"}}
        published = {"closure": {"root": {"id": str(uuid.uuid4()), "version": 1, "digest": "b" * 64}}}
        calls = []

        class Api:
            def post(self, path, key, body):
                calls.append((path, key, copy.deepcopy(body)))
                if path == "/api/v1/flow-data-schemas/validate":
                    return schema
                if path == "/api/v1/flow-definitions":
                    return {"resource": {"flow": published}}
                if path == "/api/v1/factory-configurations":
                    return {"resource": {"configuration": {"reference": {"id": body["id"], "version": 1}}}}
                raise AssertionError("undeclared management operation")

        fixture = {"settings": {"budget": {"max_attempts": 3}},
                   "schemas": {"decision": {"identity": "approved", "version": "v1", "shape": {"type": "boolean"}}},
                   "flows": [{"key": "operator-chosen-flow", "definition": {
                       "root": {"entry": "arbitrary-review", "nodes": [{"key": "arbitrary-review", "kind": "human_gate",
                           "input_schema": {"$fixture": "schemas.decision.reference"}}]},
                       "data_schemas": [{"$fixture": "schemas.decision"}]},
                       "configuration": {"enabled": True, "hard_budget": {"$fixture": "settings.budget"}}}]}
        fixture["flows"].append({"key": "configured-caller", "definition": {
            "root": {"nodes": [{"kind": "subflow_call", "subflow": {
                "$fixture": "flows.operator-chosen-flow.closure.root"}}]}}, "configuration": {"enabled": True}})
        result = demo.publish_flows(Api(), project, fixture, uuid.UUID("00000000-0000-0000-0000-000000000001"))
        self.assertEqual(calls[0][2], {"project_id": project, "identity": "approved", "version": "v1", "shape": {"type": "boolean"}})
        self.assertEqual(calls[1][2]["definition"]["root"], {"entry": "arbitrary-review", "nodes": [
            {"key": "arbitrary-review", "kind": "human_gate", "input_schema": schema["reference"]}]})
        self.assertEqual(calls[1][2]["definition"]["data_schemas"], [schema])
        self.assertEqual(calls[2][2]["definition"], {"enabled": True, "hard_budget": {"max_attempts": 3}, "flow": published})
        self.assertEqual(result["operator-chosen-flow"]["configuration"], calls[2][2]["id"])
        self.assertEqual(calls[3][2]["definition"]["root"]["nodes"][0]["subflow"], published["closure"]["root"])


if __name__ == "__main__":
    unittest.main()
