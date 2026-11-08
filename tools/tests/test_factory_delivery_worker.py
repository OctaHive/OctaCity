"""Delivery checks have an independent ordinary Agent and bounded Pool."""
import copy
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
import uuid

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

class DeliveryWorkerTests(unittest.TestCase):
    def test_standard_shutdown_stops_both_owned_worker_slots(self):
        import local_stand_launcher as launcher
        import local_stand_lifecycle as lifecycle
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "agent-validation").mkdir()
            (root / "agent-validation/worker.json").write_text("{}")
            calls = []
            class Owner:
                def __init__(self):
                    self.root = root
                def shutdown(self, name, **kwargs):
                    calls.append(name)
                    return lifecycle.ShutdownDisposition.ALREADY_STOPPED
            launcher._shutdown_agent(Owner())
            self.assertEqual(calls, ["validation-agent", "agent"])

    @unittest.skipUnless(os.name == "posix", "private stand state requires POSIX")
    def test_worker_configuration_preserves_pins_but_separates_all_writable_state(self):
        from factory_delivery_worker import configuration
        import tomllib
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            primary = {"agent_id": "primary", "credential_file": "/primary/credential", "work_root": "/primary/work",
                       "state_root": "/primary/state", "cache": {"root": "/primary/cache", "max_bytes": 100},
                       "octa_release_root": "/verified/release", "virtualization_providers": [{"provider": "microsandbox"}]}
            original = copy.deepcopy(primary)
            path = configuration(root, primary)
            installed = tomllib.loads(path.read_text())
            self.assertEqual(primary, original)
            self.assertEqual(installed["agent_id"], "local-stand-delivery-validation")
            self.assertEqual(installed["octa_release_root"], original["octa_release_root"])
            self.assertEqual(installed["virtualization_providers"], original["virtualization_providers"])
            for field in ("credential_file", "work_root", "state_root"):
                self.assertNotEqual(installed[field], original[field])
            self.assertNotEqual(installed["cache"]["root"], original["cache"]["root"])
            self.assertEqual(path.stat().st_mode & 0o777, 0o600)

    @unittest.skipUnless(os.name == "posix", "private stand state requires POSIX")
    def test_preparation_reserves_a_separate_pool_and_credential_without_replacing_the_primary(self):
        from factory_delivery_worker import prepare
        import bootstrap_local_stand as bootstrap
        from test_bootstrap_local_stand import ENROLLMENT
        calls = []
        original = {"allowed_pools": ["primary"], "runtime": {"class": "virtualization"}, "artifacts": {"kept": True}}
        class Client:
            def post(self, path, key, body, **kwargs):
                calls.append((path, key, copy.deepcopy(body), kwargs))
                if path == bootstrap.POOL_PATH:
                    return {"resource": {"id": "66666666-6666-4666-8666-666666666666", "version": 1, **body}}
                if path == bootstrap.ENROLLMENT_PATH:
                    return {"disposition": "applied", "credential": ENROLLMENT, "pool_id": "66666666-6666-4666-8666-666666666666", "pool_version": 1,
                            "expires_at_unix_ms": 8000000000000}
                return {"version": 2}
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "agent").mkdir(mode=0o700)
            primary = root / "agent/credential"
            primary.write_text("original registration")
            primary.chmod(0o600)
            definition = copy.deepcopy(original)
            result = prepare(root, Client(), "project", "repository", definition, uuid.uuid4())
            self.assertEqual(result, "66666666-6666-4666-8666-666666666666")
            self.assertEqual(definition, original)
            self.assertEqual(primary.read_text(), "original registration")
            self.assertEqual((root / "agent-validation/credential").read_text(), ENROLLMENT + "\n")
            self.assertEqual((root / "agent-validation/credential").stat().st_mode & 0o777, 0o600)
            policy_call, = (row for row in calls if row[0].endswith("policy-versions"))
            self.assertEqual(policy_call[3], {"expected_version": 1})
            self.assertEqual(policy_call[2]["policy"]["concurrency"]["value"], {"active_builds": 2, "active_jobs": 2})
            self.assertEqual(policy_call[2]["policy"]["pools"]["value"], ["66666666-6666-4666-8666-666666666666", "primary"])
            self.assertEqual(calls[0][2]["definition"]["concurrency_limit"], 1)
