"""Behavioral tests against a real local deployed-contract endpoint."""
import importlib.util
import json
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
import threading
import time
import subprocess
import sys
import unittest

PROBE = Path(__file__).resolve().parents[1] / "factory_verification_probe.py"


class VerificationProbeTests(unittest.TestCase):
    def setUp(self):
        self.requests = []
        requests = self.requests
        self.response_json = {"flag": True, "nested": {"items": [True]}, "items": ["a", "b"], "bad~2": "value"}
        response_json = self.response_json

        class Contract(BaseHTTPRequestHandler):
            def do_GET(self):
                requests.append(self.path)
                if self.path == "/malformed-http":
                    self.wfile.write(b"opaque-response-sentinel\r\n\r\n")
                    self.close_connection = True
                    return
                if self.path == "/slow":
                    time.sleep(0.05)
                self.send_response(302 if self.path == "/redirect" else 403 if self.path == "/private" else 200)
                if self.path == "/redirect":
                    self.send_header("Location", "/undeclared")
                self.send_header("Content-Type", "text/html")
                self.send_header("X-Content-Type-Options", "nosniff")
                self.end_headers()
                if self.path == "/json":
                    self.wfile.write(json.dumps(response_json).encode())
                    return
                if self.path == "/stream":
                    try:
                        for _ in range(50):
                            self.wfile.write(b"x")
                            self.wfile.flush()
                            time.sleep(0.02)
                    except (BrokenPipeError, ConnectionResetError):
                        pass
                    return
                self.wfile.write(json.dumps({"release":"candidate-42","changeset_digest":"ad"*32,"deployment":"stage-receipt-17","deployment_receipt":{"identity":"stage-receipt-17","version":"v1","digest":"ac"*32}}).encode() if self.path == "/deployment" else b'<html><button aria-label="Save">Save</button></html>')

            def log_message(self, *_args):
                pass

        self.server = ThreadingHTTPServer(("127.0.0.1", 0), Contract)
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.thread.start()
        spec = importlib.util.spec_from_file_location("verification_probe", PROBE)
        self.probe = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(self.probe)

    def tearDown(self):
        self.server.shutdown()
        self.server.server_close()
        self.thread.join()

    def config(self, **changes):
        return {
            "plan_digest": "ab" * 32,
            "input_digest": "cd" * 32,
            "check": "operator-selected-node",
            "base_url": f"http://127.0.0.1:{self.server.server_port}",
            "allowed_hosts": ["127.0.0.1"],
            "limits": {"max_requests": 4, "timeout_ms": 1000, "max_response_bytes": 4096, "max_elapsed_ms": 3000},
            "steps": [{"path": "/contract", "status": 200, "contains": ["Save"]}],
            **changes,
        }

    def test_contract_sequence_publishes_typed_failure_when_one_assertion_fails(self):
        result = self.probe.run(self.config())
        self.assertEqual(result["plan_digest"], "ab" * 32)
        self.assertEqual(result["check"], "operator-selected-node")
        self.assertEqual(result["outcome"], "passed")
        self.assertIn({"name": "requests", "value": 1}, result["measurements"])
        result = self.probe.run(self.config(steps=[
            {"path": "/first", "status": 200},
            {"path": "/second", "status": 200, "contains": ["Absent contract"]},
        ]))
        self.assertEqual(result["outcome"], "failed")
        self.assertEqual(self.requests, ["/contract", "/first", "/second"])

    def test_observation_binds_the_exact_frozen_input_inside_a_shared_plan(self):
        result = self.probe.run(self.config(input_digest="cd" * 32))
        self.assertEqual(result["input_digest"], "cd" * 32)
        other = self.probe.run(self.config(input_digest="ef" * 32))
        self.assertEqual(other["plan_digest"], result["plan_digest"])
        self.assertEqual(other["input_digest"], "ef" * 32)

    def test_ui_security_latency_and_bounded_observation_are_ordinary_assertions(self):
        cases = [
            ({"path": "/", "status": 200, "contains": ['aria-label="Save"']}, "passed"),
            ({"path": "/private", "status": 403, "headers": {"X-Content-Type-Options": "nosniff"}}, "passed"),
            ({"path": "/", "status": 200, "headers": {"X-Missing": "required"}}, "failed"),
            ({"path": "/slow", "status": 200, "max_latency_ms": 10}, "failed"),
        ]
        for step, expected in cases:
            with self.subTest(step=step):
                self.assertEqual(self.probe.run(self.config(steps=[step]))["outcome"], expected)
        before = len(self.requests)
        result = self.probe.run(self.config(repetitions=3, interval_ms=1))
        self.assertEqual(result["outcome"], "passed")
        self.assertEqual(len(self.requests) - before, 3)

    def test_undeclared_destinations_and_unbounded_work_are_rejected_before_network(self):
        for changes in [
            {"allowed_hosts": ["not-the-endpoint.test"]},
            {"steps": [{"path": "http://outside.test/", "status": 200}]},
            {"repetitions": 5},
            {"repetitions": 0},
            {"target": {"endpoint": "https://stage.example.test/other-deployment"}},
            {"limits": {"max_requests": 4, "timeout_ms": 0, "max_response_bytes": 4096, "max_elapsed_ms": 3000}},
        ]:
            with self.subTest(changes=changes), self.assertRaises(ValueError):
                self.probe.run(self.config(**changes))
        self.assertEqual(self.requests, [])

    def test_deployment_contract_must_match_the_exact_selected_release(self):
        step = {"path": "/deployment", "status": 200, "json_equals": {"/release": "candidate-42", "/deployment": "stage-receipt-17"}}
        self.assertEqual(self.probe.run(self.config(steps=[step]))["outcome"], "passed")
        step["json_equals"]["/release"] = "other-candidate"
        self.assertEqual(self.probe.run(self.config(steps=[step]))["outcome"], "failed")

    def test_json_assertions_distinguish_booleans_from_numbers_in_nested_values(self):
        for pointer, expected in [("/flag", 1), ("/nested", {"items": [1]})]:
            with self.subTest(pointer=pointer):
                step = {"path": "/json", "status": 200, "json_equals": {pointer: expected}}
                self.assertEqual(self.probe.run(self.config(steps=[step]))["outcome"], "failed")
        step = {"path": "/json", "status": 200, "json_equals": {"/nested": {"items": [True]}}}
        self.assertEqual(self.probe.run(self.config(steps=[step]))["outcome"], "passed")

    def test_json_pointer_rejects_invalid_array_indices_and_escape_sequences(self):
        for pointer, expected in [("/items/-1", "b"), ("/items/+1", "b"), ("/items/01", "b"), ("/bad~2", "value")]:
            with self.subTest(pointer=pointer):
                step = {"path": "/json", "status": 200, "json_equals": {pointer: expected}}
                self.assertEqual(self.probe.run(self.config(steps=[step]))["outcome"], "failed")
        self.response_json.update({"a/b": {"~": "escaped"}, "number": 1.0})
        step = {"path": "/json", "status": 200, "json_equals": {"/items/1": "b", "/a~1b/~0": "escaped", "/number": 1}}
        self.assertEqual(self.probe.run(self.config(steps=[step]))["outcome"], "passed")

    def test_assertions_select_release_identity_from_frozen_target_without_candidate_literals(self):
        step = {"path": "/deployment", "status": 200, "json_equals": {"/release": {"source": "target", "path": "/subject/candidate_revision"}}}
        target = {"subject": {"candidate_revision": "candidate-42"}}
        self.assertEqual(self.probe.run(self.config(steps=[step], target=target))["outcome"], "passed")
        target["subject"]["candidate_revision"] = "other-candidate"
        self.assertEqual(self.probe.run(self.config(steps=[step], target=target))["outcome"], "failed")

    def test_observation_window_stops_a_streaming_response_before_it_is_fully_consumed(self):
        started = time.monotonic()
        result = self.probe.run(self.config(steps=[{"path": "/stream", "status": 200}], limits={"max_requests": 1, "timeout_ms": 1000, "max_response_bytes": 4096, "max_elapsed_ms": 50}))
        self.assertEqual(result["outcome"], "indeterminate")
        self.assertLess(time.monotonic() - started, 0.4)

    def test_redirect_does_not_grant_another_contract_or_publish_success(self):
        result = self.probe.run(self.config(steps=[{"path": "/redirect", "status": 200}]))
        self.assertEqual(result["outcome"], "failed")
        self.assertEqual(self.requests, ["/redirect"])

    def test_cli_emits_only_the_typed_report_and_rejects_oversized_input(self):
        completed = subprocess.run([sys.executable, str(PROBE)], input=json.dumps(self.config()), text=True, capture_output=True, timeout=5)
        self.assertEqual(completed.returncode, 0, completed.stderr)
        self.assertEqual(json.loads(completed.stdout)["outcome"], "passed")
        self.assertEqual(completed.stderr, "")
        invalid = subprocess.run([sys.executable, str(PROBE)], input=" " * 1048577, text=True, capture_output=True, timeout=5)
        self.assertNotEqual(invalid.returncode, 0)
        self.assertEqual(invalid.stdout, "")

    def test_malformed_http_emits_indeterminate_without_response_or_traceback(self):
        config = self.config(steps=[{"path": "/malformed-http", "status": 200}])
        completed = subprocess.run([sys.executable, str(PROBE)], input=json.dumps(config), text=True, capture_output=True, timeout=5)
        self.assertEqual(completed.returncode, 0, "protocol errors must produce typed observations")
        self.assertEqual(json.loads(completed.stdout)["outcome"], "indeterminate")
        self.assertEqual(completed.stderr, "", "response bytes must not reach diagnostics")
        config["steps"].insert(0, {"path": "/contract", "status": 200, "contains": ["missing"]})
        self.assertEqual(self.probe.run(config)["outcome"], "failed", "transport errors cannot erase an earlier failure")

    def test_supplied_profiles_cover_contract_latency_ui_security_deployment_and_bounded_observation(self):
        root = PROBE.parents[1] / "config" / "factory" / "verification-probes"
        profiles = {path.stem: json.loads(path.read_text()) for path in root.glob("*.json")}
        self.assertEqual(set(profiles), {"contract_sequence", "latency_budget", "public_interface", "access_boundary", "staged_contract", "deployment_observation"})
        expected = {"contract_sequence": "passed", "latency_budget": "passed", "public_interface": "passed", "access_boundary": "passed", "staged_contract": "passed", "deployment_observation": "passed"}
        for key, profile in profiles.items():
            with self.subTest(profile=key):
                target={"subject":{"candidate_revision":"candidate-42","changeset_digest":"ad"*32},"environment":{"identity":"stage-receipt-17","version":"v1","digest":"ac"*32}}
                result = self.probe.run(self.config(target=target,**profile))
                self.assertEqual(result["outcome"], expected[key])
                self.assertEqual(result["check"], "operator-selected-node")
                if key in {"staged_contract","deployment_observation"}:
                    target["environment"]["digest"]="ff"*32
                    self.assertEqual(self.probe.run(self.config(target=target,**profile))["outcome"],"failed","a different receipt must fail even when its logical environment name is unchanged")


if __name__ == "__main__":
    unittest.main()
