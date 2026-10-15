"""The finite deployment CLI must change and observe a separate actual target."""
import base64
import hashlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
import threading
import time
import unittest

FIXTURE = Path(__file__).resolve().parents[2] / "server/tests/octacity-release-harness/fixtures/factory/deployer.mjs"


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def reference(name, digest):
    return {"identity": name, "version": "v1", "digest": digest}


class FactoryPilotDeploymentTests(unittest.TestCase):
    def setUp(self):
        self.node = shutil.which("node")
        self.assertIsNotNone(self.node, "restore Node before implementing the deployment fixture")
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name).resolve()
        self.source = self.root / "source"
        self.source.mkdir()
        (self.source / "facts.txt").write_bytes(b"actual candidate bytes")
        self.deployed = self.root / "target.bin"
        self.requests = []
        self.receipt = None
        self.mode = "normal"
        owner = self

        class Target(BaseHTTPRequestHandler):
            def log_message(self, *_args):
                pass

            def reply(self, value):
                raw = json.dumps(value).encode()
                self.send_response(200)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(raw)))
                self.end_headers()
                try:
                    self.wfile.write(raw)
                except (BrokenPipeError, ConnectionResetError):
                    pass

            def do_POST(self):
                raw = self.rfile.read(int(self.headers["Content-Length"]))
                value = json.loads(raw)
                owner.requests.append(("POST", self.path, value))
                owner.deployed.write_bytes(base64.b64decode(value["content"], validate=True))
                owner.receipt = {"deployment": reference("target-receipt", sha(raw)),
                                 "candidate": value["candidate"], "content_sha256": sha(owner.deployed.read_bytes())}
                if owner.mode == "drop":
                    self.close_connection = True
                    return
                if owner.mode == "slow_ack":
                    time.sleep(3.2)
                self.reply(owner.receipt)

            def do_GET(self):
                owner.requests.append(("GET", self.path, None))
                raw = owner.deployed.read_bytes()
                if owner.mode == "tamper":
                    raw += b" substituted"
                response = {**owner.receipt, "content": base64.b64encode(raw).decode()}
                if owner.mode == "oversized":
                    response["padding"] = "x" * 65537
                self.reply(response)

        self.server = ThreadingHTTPServer(("127.0.0.1", 0), Target)
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.thread.start()
        self.addCleanup(self.stop)
        self.environment = reference("arbitrary-stage", "4c" * 32)
        self.target = {"authority": reference("installed-target", "5d" * 32),
                       "environment": self.environment,
                       "endpoint": f"http://127.0.0.1:{self.server.server_port}/deployment",
                       "credential_profile": None}
        self.subject = {"exact": {"project_id": "00000000-0000-0000-0000-000000000001",
                                  "repository_id": "00000000-0000-0000-0000-000000000002",
                                  "base_revision": "a" * 40},
                        "candidate_revision": "b" * 40, "changeset_digest": "6e" * 32}
        self.installed_strategy = "configured.static.stage"
        self.promotion = {"plan_digest": "3b" * 32,
                          "plan": {"subject": self.subject, "input_digest": "2a" * 32,
                                   "policy": {"strategy": self.installed_strategy, "allocation": {}, "production_environments": [], "environments": {"configured-alias": self.environment},
                                              "observation_window_millis": 30}}}
        self.frozen = {"node": {"input_digest": "2a" * 32, "deadline": int(time.time() * 1000) + 10000}}
        self.relative = "facts.txt"
        self.report = self.root / "observation.json"

    def stop(self):
        self.server.shutdown()
        self.server.server_close()
        self.thread.join(timeout=2)

    def invoke(self):
        for name, value in (("target", self.target), ("promotion", self.promotion), ("frozen", self.frozen)):
            (self.root / f"{name}.json").write_text(json.dumps(value))
        return subprocess.run([self.node, str(FIXTURE), str(self.root / "target.json"),
                               str(self.root / "promotion.json"), str(self.root / "frozen.json"),
                               str(self.source), self.relative, str(self.report), self.installed_strategy],
                              capture_output=True, text=True, timeout=20)

    def test_deployment_changes_separate_target_and_observes_the_actual_bytes_for_the_frozen_window(self):
        before = int(time.time() * 1000)
        result = self.invoke()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout + result.stderr, "")
        self.assertEqual(self.deployed.read_bytes(), b"actual candidate bytes")
        self.assertEqual([r[0] for r in self.requests], ["POST", "GET", "GET"])
        sent = self.requests[0][2]
        self.assertEqual(sent["operation_id"], "2a" * 32)
        self.assertEqual(sent["candidate"], self.subject)
        observation = json.loads(self.report.read_bytes())
        self.assertEqual(observation, {
            "plan_digest": "3b" * 32, "input_digest": "2a" * 32, "subject": self.subject,
            "environment": "configured-alias", "environment_contract": self.environment,
            "deployment": self.receipt["deployment"], "outcome": "passed",
            "started_at": observation["started_at"], "finished_at": observation["finished_at"],
            "measurements": {"matching_reads": 2},
        })
        self.assertGreaterEqual(observation["started_at"], before)
        self.assertEqual(observation["finished_at"] - observation["started_at"], 30)
        self.assertLessEqual(observation["finished_at"], int(time.time() * 1000))


    def assert_rejected(self):
        result = self.invoke()
        self.assertEqual(result.returncode, 64)
        self.assertEqual(result.stdout, "")
        self.assertEqual(result.stderr, "deployment rejected its invocation\n")
        self.assertFalse(self.report.exists())
        self.assertNotIn(str(self.root), result.stdout + result.stderr)

    def test_substituted_scope_or_expired_budget_is_rejected_before_any_remote_effect(self):
        original_target = json.loads(json.dumps(self.target))
        original_promotion = json.loads(json.dumps(self.promotion))
        original_frozen = json.loads(json.dumps(self.frozen))
        for selected in ("environment", "input", "expired", "multiple", "userinfo", "credential", "window"):
            with self.subTest(substitution=selected):
                self.requests.clear()
                self.deployed.unlink(missing_ok=True)
                self.report.unlink(missing_ok=True)
                self.target = json.loads(json.dumps(original_target))
                self.promotion = json.loads(json.dumps(original_promotion))
                self.frozen = json.loads(json.dumps(original_frozen))
                if selected == "environment":
                    self.target["environment"] = reference("other-stage", "7f" * 32)
                elif selected == "input":
                    self.promotion["plan"]["input_digest"] = "8a" * 32
                elif selected == "expired":
                    self.frozen["node"]["deadline"] = int(time.time() * 1000) - 1
                elif selected == "multiple":
                    self.promotion["plan"]["policy"]["environments"]["other"] = self.environment
                elif selected == "userinfo":
                    self.target["endpoint"] = self.target["endpoint"].replace("http://", "http://PRIVATE_TOKEN@")
                elif selected == "credential":
                    self.target["credential_profile"] = "unsupported-credential"
                elif selected == "window":
                    self.promotion["plan"]["policy"]["observation_window_millis"] = 20000
                self.assert_rejected()
                self.assertEqual(self.requests, [])
                self.assertFalse(self.deployed.exists())

    def test_stage_only_program_refuses_uninstalled_strategy_allocation_or_production_before_effects(self):
        original = json.loads(json.dumps(self.promotion["plan"]["policy"]))
        for field, value in (("strategy", "uninstalled.strategy"), ("allocation", {"portion": 1}),
                             ("production_environments", ["configured-alias"])):
            with self.subTest(policy=field):
                self.requests.clear()
                self.deployed.unlink(missing_ok=True)
                self.report.unlink(missing_ok=True)
                self.promotion["plan"]["policy"] = {**original, field: value}
                self.assert_rejected()
                self.assertEqual(self.requests, [])
                self.assertFalse(self.deployed.exists())

    def test_candidate_paths_and_sizes_are_bounded_before_remote_writes(self):
        outside = self.root / "private.txt"
        outside.write_text("PRIVATE_CANDIDATE_CANARY")
        (self.source / "link.txt").symlink_to(outside)
        directory = self.root / "private-directory"
        directory.mkdir()
        (directory / "facts.txt").write_text("PRIVATE_CANDIDATE_CANARY")
        (self.source / "linked").symlink_to(directory, target_is_directory=True)
        (self.source / "large.txt").write_bytes(b"x" * 65537)
        for selected in ("../private.txt", str(outside), "link.txt", "linked/facts.txt", "large.txt"):
            with self.subTest(path=selected):
                self.requests.clear()
                self.deployed.unlink(missing_ok=True)
                self.report.unlink(missing_ok=True)
                self.relative = selected
                self.assert_rejected()
                self.assertEqual(self.requests, [])
                self.assertFalse(self.deployed.exists())

    def test_unknown_post_response_never_retries_the_remote_write_or_claims_success(self):
        self.mode = "drop"
        self.assert_rejected()
        self.assertEqual([r[0] for r in self.requests], ["POST"])
        self.assertEqual(self.deployed.read_bytes(), b"actual candidate bytes")

    def test_slow_ack_within_the_original_deadline_still_completes_without_duplicate_writes(self):
        self.mode = "slow_ack"
        self.frozen["node"]["deadline"] = int(time.time() * 1000) + 30000
        result = self.invoke()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.deployed.read_bytes(), b"actual candidate bytes")
        self.assertEqual([r[0] for r in self.requests], ["POST", "GET", "GET"])
        self.assertEqual(json.loads(self.report.read_bytes())["deployment"], self.receipt["deployment"])

    def test_remote_response_bytes_are_bounded_before_report_publication(self):
        self.mode = "oversized"
        self.assert_rejected()
        self.assertEqual([r[0] for r in self.requests], ["POST", "GET"])

    def test_actual_readback_mismatch_cannot_publish_a_successful_observation(self):
        self.mode = "tamper"
        self.assert_rejected()
        self.assertEqual([r[0] for r in self.requests], ["POST", "GET"])


if __name__ == "__main__":
    unittest.main()
