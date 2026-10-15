"""Finite installed author derives an assertion from actual frozen requirements."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from tools.tests.fixture_environment import fixture_environment

FIXTURE = Path(__file__).resolve().parents[2] / "server/tests/octacity-release-harness/fixtures/factory/requirements-author.mjs"
NODE = shutil.which("node")


@unittest.skipUnless(os.name == "posix", "the packaged pilot author is qualified for Unix paths and symlinks")
class FactoryPilotRequirementsAuthorTests(unittest.TestCase):
    def setUp(self):
        self.assertIsNotNone(NODE, "restore the existing Node runtime before implementing the author")
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name).resolve()
        self.output = self.root / "output"
        self.output.mkdir()
        self.frozen = self.root / "frozen.json"
        reference = {"artifact_id": "00000000-0000-0000-0000-000000000001",
                     "content_digest": "ab" * 32, "encoded_size": 128}
        self.accepted = {"eligible": True, "risk": "low", "work_kind": "feature_request", "size": "small",
                         "task": reference, "acceptance": {**reference, "artifact_id": "00000000-0000-0000-0000-000000000002"},
                         "criteria_artifact": {**reference, "artifact_id": "00000000-0000-0000-0000-000000000003"},
                         "path": "public/new-contract.txt", "expected_text": "a different accepted requirement"}
        self.persist()

    def persist(self):
        self.frozen.write_text(json.dumps({"input": {"payload": {"value": self.accepted}}}))

    def invoke(self, relative="private-contract.json"):
        return subprocess.run([NODE, str(FIXTURE), str(self.frozen), str(self.output), relative],
                              capture_output=True, text=True, timeout=10, env=fixture_environment())

    def test_real_frozen_requirements_generate_the_exact_private_assertion(self):
        result = self.invoke()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout + result.stderr, "")
        self.assertEqual(json.loads((self.output / "private-contract.json").read_bytes()), {
            "path": "public/new-contract.txt", "expected": "a different accepted requirement"})

    def assert_rejected(self, relative="private-contract.json", expect_absent=True):
        result = self.invoke(relative)
        self.assertEqual(result.returncode, 64)
        self.assertEqual(result.stdout, "")
        self.assertEqual(result.stderr, "requirements author rejected its input\n")
        if expect_absent:
            self.assertFalse((self.output / "private-contract.json").exists())
        for canary in (str(self.root), "PRIVATE_REQUIREMENT_CANARY"):
            self.assertNotIn(canary, result.stdout + result.stderr)

    def test_candidate_paths_cannot_escape_the_declared_repository(self):
        for value in ("../outside", "/absolute", "public/../outside", "public//facts", "public/./facts", "public\\facts", "public/\x00facts"):
            with self.subTest(path=value):
                self.accepted["path"] = value
                self.persist()
                self.assert_rejected()

    def test_invalid_or_unbounded_contract_and_artifact_pins_are_rejected(self):
        original = json.dumps(self.accepted)
        for field, value in (("expected_text", 7), ("expected_text", "x" * 1025),
                             ("expected_text", "Ж" * 513), ("expected_text", "PRIVATE_REQUIREMENT_CANARY\x00"),
                             ("criteria_artifact", None), ("criteria_artifact", {"artifact_id": "invented"}),
                             ("task", {**self.accepted["task"], "content_digest": "zz" * 32}),
                             ("acceptance", {**self.accepted["acceptance"], "encoded_size": 0}),
                             ("extra_script", "PRIVATE_REQUIREMENT_CANARY")):
            with self.subTest(field=field):
                self.accepted = json.loads(original)
                self.accepted[field] = value
                self.persist()
                self.assert_rejected()

    def test_output_cannot_escape_follow_symlinks_or_overwrite_an_existing_file(self):
        outside = self.root / "outside"
        outside.mkdir()
        victim = outside / "private-contract.json"
        victim.write_text("PRIVATE_REQUIREMENT_CANARY")
        (self.output / "linked").symlink_to(outside, target_is_directory=True)
        for relative in ("../outside/private-contract.json", str(victim), "linked/private-contract.json"):
            with self.subTest(output=relative):
                self.assert_rejected(relative)
                self.assertEqual(victim.read_text(), "PRIVATE_REQUIREMENT_CANARY")
        target = self.output / "private-contract.json"
        target.symlink_to(victim)
        self.assert_rejected(expect_absent=False)
        self.assertEqual(victim.read_text(), "PRIVATE_REQUIREMENT_CANARY")
        target.unlink()
        target.write_text("PRIVATE_REQUIREMENT_CANARY")
        self.assert_rejected(expect_absent=False)
        self.assertEqual(target.read_text(), "PRIVATE_REQUIREMENT_CANARY")

    def test_frozen_input_is_bounded_regular_and_has_no_symlink_components(self):
        original = self.frozen.read_bytes()
        target = self.root / "real.json"
        target.write_bytes(original)
        self.frozen.unlink()
        self.frozen.symlink_to(target)
        self.assert_rejected()
        self.frozen.unlink()
        directory = self.root / "real-directory"
        directory.mkdir()
        (directory / "frozen.json").write_bytes(original)
        alias = self.root / "alias"
        alias.symlink_to(directory, target_is_directory=True)
        self.frozen = alias / "frozen.json"
        self.assert_rejected()
        self.frozen = self.root / "frozen.json"
        self.frozen.write_bytes(original + b" " * 65536)
        self.assert_rejected()

    def test_shell_looking_requirements_are_literal_data_and_not_commands(self):
        marker = self.root / "must-not-exist"
        self.accepted["expected_text"] = "'; $(touch " + str(marker) + "); \nPRIVATE_REQUIREMENT_CANARY"
        self.persist()
        result = self.invoke()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout + result.stderr, "")
        self.assertFalse(marker.exists())
        self.assertEqual(json.loads((self.output / "private-contract.json").read_bytes()), {
            "path": self.accepted["path"], "expected": self.accepted["expected_text"]})

    def test_missing_or_malformed_frozen_payload_produces_no_private_output(self):
        for value in (b"not JSON PRIVATE_REQUIREMENT_CANARY", b"{}", b'{"input":{"payload":{"value":null}}}'):
            with self.subTest(value=value):
                self.frozen.write_bytes(value)
                self.assert_rejected()


if __name__ == "__main__":
    unittest.main()
