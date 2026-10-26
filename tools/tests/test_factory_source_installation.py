"""Trusted Source installation through the normal stand's public configurator."""
from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import sys
import subprocess
import tempfile
import tomllib
import unittest
from unittest.mock import patch
import urllib.error

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from factory_integrations import configure_integrations, install_repository


@unittest.skipUnless(os.name == "posix", "private stand bundles require POSIX ownership")
class FactorySourceInstallationTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.baseline = self.root / "config/server/server.toml"
        self.baseline.parent.mkdir(parents=True)
        self.original = b'management_bind = "0.0.0.0:8080"\n'
        self.baseline.write_bytes(self.original)
        self.baseline.chmod(0o600)

    def install(self, repository, *, native_repository_id, adapter_digest):
        return configure_integrations(self.root, repository, native_repository_id=native_repository_id,
                                      adapter_digest=adapter_digest)

    def test_retained_repository_is_reused_offline_without_changing_pins_or_state(self):
        bundle = self.install("Example/Tickets", native_repository_id="1234", adapter_digest="ab" * 32)
        before = {path.relative_to(bundle): (path.read_bytes(), path.stat().st_mtime_ns)
                  for path in bundle.rglob("*") if path.is_file()}
        history = self.root / "retained-history"
        history.write_bytes(b"original-runtime-profiles-and-runs")
        with patch("factory_integrations.urllib.request.OpenerDirector.open",
                   side_effect=urllib.error.URLError("offline fixture")) as provider, \
                patch("factory_integrations.subprocess.run", return_value=subprocess.CompletedProcess(
                    [], 0, stdout="ab" * 32 + "  /usr/local/bin/octacity-github\n")):
            self.assertEqual(install_repository(self.root, "Example/Tickets"), bundle)
        provider.assert_not_called()
        self.assertEqual({path.relative_to(bundle): (path.read_bytes(), path.stat().st_mtime_ns)
                          for path in bundle.rglob("*") if path.is_file()}, before)
        self.assertEqual(history.read_bytes(), b"original-runtime-profiles-and-runs")

    def test_offline_reuse_rejects_mismatched_or_incomplete_installation_without_replacing_it(self):
        for change in ("repository", "profiles", "adapter", "missing-catalog", "symbolic-catalog"):
            with self.subTest(change=change), tempfile.TemporaryDirectory(dir=self.root) as directory:
                root = Path(directory)
                baseline = root / "config/server/server.toml"
                baseline.parent.mkdir(parents=True)
                baseline.write_bytes(self.original)
                baseline.chmod(0o600)
                bundle = configure_integrations(root, "Example/Tickets", native_repository_id="1234", adapter_digest="ab" * 32)
                if change == "profiles":
                    (bundle / "source-profiles.json").write_text('{"version":1,"readers":[],"reporters":[]}')
                elif change == "missing-catalog":
                    (bundle / "catalog.json").unlink()
                elif change == "symbolic-catalog":
                    target = root / "catalog-copy.json"
                    (bundle / "catalog.json").rename(target)
                    (bundle / "catalog.json").symlink_to(target)
                before = {path.relative_to(bundle): path.read_bytes() for path in bundle.rglob("*") if path.is_file()}
                with patch("factory_integrations.urllib.request.OpenerDirector.open",
                           side_effect=AssertionError("retained installation must not fall back to GitHub")) as provider, \
                        patch("factory_integrations.subprocess.run", return_value=subprocess.CompletedProcess(
                            [], 0, stdout=("cd" if change == "adapter" else "ab") * 32 + "  /usr/local/bin/octacity-github\n")):
                    with self.assertRaises(ValueError):
                        install_repository(root, "Other/Tickets" if change == "repository" else "Example/Tickets")
                provider.assert_not_called()
                self.assertEqual({path.relative_to(bundle): path.read_bytes()
                                  for path in bundle.rglob("*") if path.is_file()}, before)

    def test_fresh_installation_declares_bounded_issue_reads_without_project_or_execution_target(self):
        bundle = self.install("Example/Tickets", native_repository_id="1234", adapter_digest="ab" * 32)
        catalog = json.loads((bundle / "catalog.json").read_text())
        self.assertIn("source_profiles_file", catalog)
        self.assertTrue(Path(catalog["source_profiles_file"]).is_absolute())
        profiles = json.loads((bundle / "source-profiles.json").read_text())
        reader, = profiles["readers"]
        self.assertEqual(reader["read"], catalog["checks"][0])
        self.assertEqual(reader["readable"]["states"], ["open", "closed"])
        self.assertGreater(reader["polling"]["max_pages"], 0)
        self.assertGreater(reader["polling"]["max_page_bytes"], 0)
        self.assertNotIn("project_id", json.dumps(profiles))
        self.assertNotIn("target", json.dumps(profiles))
        self.assertEqual(self.baseline.read_bytes(), self.original)

    def test_source_selection_is_installed_by_its_exact_native_digest(self):
        bundle = self.install("Other/Issues", native_repository_id="5678", adapter_digest="cd" * 32)
        native = tomllib.loads((bundle / "github-work.toml").read_text())
        self.assertEqual(native.get("selection_directory"), "selections")
        reader, = json.loads((bundle / "source-profiles.json").read_text())["readers"]
        selection = reader["selection"]
        name = hashlib.sha256(selection["identity"].encode()).hexdigest() + ".json"
        payload = (bundle / "selections" / name).read_bytes()
        self.assertEqual(hashlib.sha256(payload).hexdigest(), selection["sha256"])
        self.assertEqual(json.loads(payload), {"state": "all", "all_of": [], "any_of": [], "none_of": []})
        self.assertEqual(reader["read"]["resource"]["resource_locator"], "Other/Issues")
        self.assertEqual(reader["read"]["provider_configuration"], hashlib.sha256((bundle / "github-work.toml").read_bytes()).hexdigest())

    def test_reporter_uses_its_separate_write_scope_and_reviewed_native_mapping(self):
        bundle = self.install("Example/Tickets", native_repository_id="1234", adapter_digest="ab" * 32)
        catalog = json.loads((bundle / "catalog.json").read_text())
        profiles = json.loads((bundle / "source-profiles.json").read_text())
        self.assertIn("reporters", profiles)
        writer, = profiles["reporters"]
        self.assertEqual(writer["write"], catalog["checks"][1])
        self.assertEqual(writer["write"]["resource"]["credential"]["class"], "work_reporter_write")
        self.assertNotEqual(writer["credential_profile"], profiles["readers"][0]["read"]["resource"]["credential"]["handle"])
        self.assertGreater(writer["issue_delivery"]["window_millis"], 0)
        native = tomllib.loads((bundle / "github-work.toml").read_text())
        self.assertEqual(native["mapping_directory"], "mappings")
        mapping = writer["mapping"]
        filename = hashlib.sha256(mapping["identity"].encode()).hexdigest() + ".json"
        payload = (bundle / "mappings" / filename).read_bytes()
        self.assertEqual(hashlib.sha256(payload).hexdigest(), mapping["sha256"])
        self.assertEqual(json.loads(payload)["managed_label_groups"], {})
        for effect in writer["reporting"]["mapping"]["statuses"]:
            self.assertEqual(effect["add"], [])
            self.assertEqual(effect["remove"], [])
            self.assertIn(effect["state"], ["open", "closed"])
        self.assertNotIn("enabled", json.dumps(profiles))

    def test_factory_preparation_is_bounded_and_does_not_pin_a_project_repository_or_target(self):
        bundle = self.install("Example/Tickets", native_repository_id="1234", adapter_digest="ab" * 32)
        reader, = json.loads((bundle / "source-profiles.json").read_text())["readers"]
        self.assertIn("preparation", reader)
        preparation = reader["preparation"]
        read = reader["read"]
        self.assertEqual(preparation["mapping"]["source"], {
            "installation": read["installation"], "capability": read["capability"]["capability"],
            "configuration": read["capability"]["configuration"], "resource": read["resource_reference"],
        })
        self.assertEqual(preparation["mapping"]["acceptance"], {"kind": "body_section", "value": "## Acceptance"})
        self.assertEqual(preparation["source"], {"kind": "default_reference"})
        self.assertGreater(preparation["mapping"]["max_artifact_bytes"], 0)
        self.assertEqual(reader["predicate_states"], ["open", "closed"])
        self.assertEqual(reader["selection_labels"], {"state": "all", "all_of": [], "any_of": [], "none_of": []})
        self.assertNotIn("repository_id", json.dumps(preparation))
        self.assertNotIn("project_id", json.dumps(preparation))
