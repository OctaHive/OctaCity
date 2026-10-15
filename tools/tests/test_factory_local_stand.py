"""Factory stand data is published through the existing management contracts."""
import copy
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

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import factory_local_stand as demo
import bootstrap_local_stand as bootstrap
import local_stand_state as state


class FactoryLocalStandTests(unittest.TestCase):
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
        with patch.object(demo.integration, "LocalStandClient",
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
