"""Trusted finite validation reads real inputs through the installed Node CLI."""
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from tools.tests.fixture_environment import fixture_environment

FIXTURE = Path(__file__).resolve().parents[2] / "server/tests/octacity-release-harness/fixtures/factory/protected-validator.mjs"
NODE = shutil.which("node")


def digest(value):
    return hashlib.sha256(value).hexdigest()


class FactoryPilotValidationTests(unittest.TestCase):
    def setUp(self):
        self.assertIsNotNone(NODE, "restore the existing Node runtime before implementing validation")
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name).resolve()
        self.private = self.root / "private"
        self.candidate = self.root / "candidate"
        (self.private / "hidden").mkdir(parents=True)
        (self.candidate / "public").mkdir(parents=True)
        (self.candidate / "public/facts.txt").write_bytes(b"facts")
        self.assertion = self.private / "hidden/assertion.json"
        self.assertion.write_text(json.dumps({"path": "public/facts.txt", "expected": "facts"}))
        self.inventory_path = self.root / "inventory.json"
        self.invocation_path = self.root / "invocation.json"
        self.frozen_path = self.root / "frozen.json"
        self.primary = self.root / "report.json"
        self.facts = self.root / "facts.json"
        self.inventory = {"plan_digest": "4c" * 32, "input_digest": "5d" * 32,
                          "node_attempt_id": "00000000-0000-0000-0000-000000000004", "files": []}
        self.invocation = {
            "changeset_id": "00000000-0000-0000-0000-000000000001",
            "candidate": {"exact": {"project_id": "00000000-0000-0000-0000-000000000002",
                                     "repository_id": "00000000-0000-0000-0000-000000000003",
                                     "base_revision": "a" * 40},
                          "candidate_revision": "b" * 40, "changeset_digest": "6e" * 32},
            "tests": {}, "input_digest": "3b" * 32, "classes": ["arbitrary.contract"],
        }
        self.frozen_path.write_text(json.dumps({"node": {"input_digest": "2a" * 32, "deadline": 1900000000000}}))
        self.repin()

    def repin(self):
        raw = self.assertion.read_bytes()
        self.inventory["files"] = [{"class": "arbitrary.contract", "path": "hidden/assertion.json",
                                    "artifact": {"artifact_id": "00000000-0000-0000-0000-000000000005",
                                                 "content_digest": digest(raw), "encoded_size": len(raw)}}]
        self.repin_inventory()

    def repin_inventory(self):
        self.inventory_path.write_text(json.dumps(self.inventory))
        raw = self.inventory_path.read_bytes()
        self.invocation["tests"] = {"artifact_id": "00000000-0000-0000-0000-000000000006",
                                    "content_digest": digest(raw), "encoded_size": len(raw)}
        self.invocation_path.write_text(json.dumps(self.invocation))

    def invoke(self):
        return subprocess.run([NODE, str(FIXTURE), str(self.invocation_path), str(self.frozen_path),
                               str(self.inventory_path), str(self.private), str(self.candidate),
                               str(self.primary), str(self.facts)], capture_output=True, text=True,
                              timeout=10, env=fixture_environment())

    def test_actual_private_assertion_produces_two_exact_reports_with_distinct_input_digests(self):
        result = self.invoke()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout + result.stderr, "")
        expected = {**self.invocation, "classes": [{"class": "arbitrary.contract", "outcome": "passed"}]}
        self.assertEqual(json.loads(self.primary.read_bytes()), expected)
        self.assertEqual(json.loads(self.facts.read_bytes()), {
            "accepted": True, "input_digest": "2a" * 32, "report_digest": digest(self.primary.read_bytes()),
            "candidate_revision": "b" * 40, "candidate_digest": "6e" * 32,
            "protected_inventory": self.invocation["tests"], "covered_classes": ["arbitrary.contract"],
            "fresh_until": 1900000000000,
        })
        self.assertNotEqual(expected["input_digest"], json.loads(self.facts.read_bytes())["input_digest"])

    def test_failed_assertion_is_retained_negative_evidence_with_successful_job(self):
        self.assertion.write_text(json.dumps({"path": "public/facts.txt", "expected": "PRIVATE_EXPECTATION_CANARY"}))
        self.repin()
        result = self.invoke()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout + result.stderr, "")
        report = json.loads(self.primary.read_bytes())
        self.assertEqual(report["classes"], [{"class": "arbitrary.contract", "outcome": "failed"}])
        self.assertEqual(json.loads(self.facts.read_bytes())["report_digest"], digest(self.primary.read_bytes()))
        self.assertNotIn("PRIVATE_EXPECTATION_CANARY", self.primary.read_text() + self.facts.read_text())

    def assert_rejected(self):
        result = self.invoke()
        self.assertEqual(result.returncode, 64)
        self.assertEqual(result.stdout, "")
        self.assertEqual(result.stderr, "protected validation rejected its invocation\n")
        self.assertFalse(self.primary.exists())
        self.assertFalse(self.facts.exists())
        for canary in (str(self.private), str(self.candidate), "PRIVATE_EXPECTATION_CANARY"):
            self.assertNotIn(canary, result.stdout + result.stderr)

    def test_substituted_private_or_inventory_bytes_are_rejected_before_publication(self):
        for selected in (self.assertion, self.inventory_path):
            with self.subTest(file=selected.name):
                original = selected.read_bytes()
                selected.write_bytes(original + b" ")
                self.assert_rejected()
                selected.write_bytes(original)

    def test_candidate_paths_cannot_escape_through_traversal_absolute_paths_or_symlinks(self):
        outside = self.root / "outside.txt"
        outside.write_text("PRIVATE_EXPECTATION_CANARY")
        (self.candidate / "public/link.txt").symlink_to(outside)
        directory = self.root / "outside-directory"
        directory.mkdir()
        (directory / "facts.txt").write_text("PRIVATE_EXPECTATION_CANARY")
        (self.candidate / "linked").symlink_to(directory, target_is_directory=True)
        for selected in ("../outside.txt", str(outside), "public/link.txt", "linked/facts.txt"):
            with self.subTest(path=selected):
                self.assertion.write_text(json.dumps({"path": selected, "expected": "PRIVATE_EXPECTATION_CANARY"}))
                self.repin()
                self.assert_rejected()

    def test_private_inventory_paths_cannot_escape_or_follow_external_symlinks(self):
        outside = self.root / "outside-assertion.json"
        outside.write_bytes(self.assertion.read_bytes())
        (self.private / "hidden/link.json").symlink_to(outside)
        for selected in ("../outside-assertion.json", str(outside), "hidden/link.json"):
            with self.subTest(path=selected):
                self.inventory["files"][0]["path"] = selected
                self.repin_inventory()
                self.assert_rejected()

    def test_oversized_private_and_candidate_files_fail_with_only_safe_diagnostics(self):
        original = self.assertion.read_bytes()
        self.assertion.write_bytes(original + b" " * 65536)
        self.repin()
        self.assert_rejected()
        self.assertion.write_bytes(original)
        self.repin()
        (self.candidate / "public/facts.txt").write_bytes(b"PRIVATE_EXPECTATION_CANARY" * 3000)
        self.assert_rejected()

    def test_duplicate_classes_or_nonfinite_assertion_shapes_are_rejected(self):
        self.invocation["classes"] *= 2
        self.invocation_path.write_text(json.dumps(self.invocation))
        self.assert_rejected()
        self.invocation["classes"] = ["arbitrary.contract"]
        self.assertion.write_text(json.dumps({"path": "public/facts.txt", "expected": "facts", "script": "PRIVATE_EXPECTATION_CANARY"}))
        self.repin()
        self.assert_rejected()


if __name__ == "__main__":
    unittest.main()
