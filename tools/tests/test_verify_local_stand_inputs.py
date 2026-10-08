"""Tests for immutable local-stand input validation."""

from __future__ import annotations

from copy import deepcopy
import importlib.util
import json
import sys
import unittest
from pathlib import Path


REPOSITORY = Path(__file__).resolve().parents[2]
MANIFEST = REPOSITORY / "deployment/local-stand/inputs.json"
AGENT_REVISION = "a" * 40
SPEC = importlib.util.spec_from_file_location(
    "verify_local_stand_inputs", REPOSITORY / "tools/verify_local_stand_inputs.py"
)
assert SPEC and SPEC.loader
VERIFIER = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = VERIFIER
SPEC.loader.exec_module(VERIFIER)


class LocalStandInputTests(unittest.TestCase):
    def setUp(self):
        self.document = json.loads(MANIFEST.read_text(encoding="utf-8"))

    def validate(self, document=None, *, revision=AGENT_REVISION):
        value = deepcopy(self.document if document is None else document)
        return VERIFIER.validate_manifest(value, revision)

    def test_checked_in_manifest_is_complete_and_matches_repository_pins(self):
        tags = self.validate()
        VERIFIER.validate_repository_pins(self.document, tags, REPOSITORY)
        self.assertEqual(set(tags), VERIFIER.EXPECTED_IMAGE_ROLES)

    def test_rejects_an_image_without_a_digest(self):
        document = deepcopy(self.document)
        document["images"]["rust"]["reference"] = "docker.io/library/rust:1.99.0-bookworm"
        with self.assertRaisesRegex(VERIFIER.InputError, "exactly one sha256 digest"):
            self.validate(document)

    def test_rejects_a_digest_without_a_human_readable_tag(self):
        document = deepcopy(self.document)
        document["images"]["node"]["reference"] = "docker.io/library/node@sha256:" + "b" * 64
        with self.assertRaisesRegex(VERIFIER.InputError, "explicit human-readable tag"):
            self.validate(document)

    def test_rejects_a_mutable_latest_tag(self):
        document = deepcopy(self.document)
        document["images"]["nginx"]["reference"] = (
            "docker.io/library/nginx:latest@sha256:" + "c" * 64
        )
        with self.assertRaisesRegex(VERIFIER.InputError, "mutable or empty tag"):
            self.validate(document)

    def test_rejects_a_missing_image_role(self):
        document = deepcopy(self.document)
        document["images"].pop("postgres")
        with self.assertRaisesRegex(VERIFIER.InputError, "images roles differ"):
            self.validate(document)

    def test_rejects_a_non_arm64_image_contract(self):
        document = deepcopy(self.document)
        document["images"]["alpine"]["platform"] = "linux/amd64"
        with self.assertRaisesRegex(VERIFIER.InputError, "must equal linux/arm64"):
            self.validate(document)

    def test_rejects_a_non_amd64_ci_image_contract(self):
        document = deepcopy(self.document)
        document["ci_images"]["go"]["platform"] = "linux/arm64"
        with self.assertRaisesRegex(VERIFIER.InputError, "must equal linux/amd64"):
            self.validate(document)

    def test_rejects_a_missing_source_role(self):
        document = deepcopy(self.document)
        document["sources"].pop("minio_client")
        with self.assertRaisesRegex(VERIFIER.InputError, "source roles differ"):
            self.validate(document)

    def test_rejects_a_symbolic_minio_source_revision(self):
        document = deepcopy(self.document)
        document["sources"]["minio"]["source_revision"] = "main"
        with self.assertRaisesRegex(VERIFIER.InputError, "invalid format"):
            self.validate(document)

    def test_rejects_an_unversioned_minio_tag(self):
        document = deepcopy(self.document)
        document["sources"]["minio_client"]["tag"] = "latest"
        with self.assertRaisesRegex(VERIFIER.InputError, "invalid format"):
            self.validate(document)

    def test_rejects_a_minio_archive_without_a_checksum(self):
        document = deepcopy(self.document)
        document["sources"]["minio"]["sha256"] = ""
        with self.assertRaisesRegex(VERIFIER.InputError, "non-empty trimmed string"):
            self.validate(document)

    def test_rejects_a_minio_archive_url_for_another_revision(self):
        document = deepcopy(self.document)
        document["sources"]["minio_client"]["archive_url"] = (
            "https://codeload.github.com/minio/mc/tar.gz/" + "b" * 40
        )
        with self.assertRaisesRegex(VERIFIER.InputError, "does not match"):
            self.validate(document)

    def test_rejects_unbounded_source_archives(self):
        document = deepcopy(self.document)
        document["sources"]["minio"]["archive_max_bytes"] = 0
        with self.assertRaisesRegex(VERIFIER.InputError, "positive bounded integer"):
            self.validate(document)

        document = deepcopy(self.document)
        document["sources"]["minio_client"]["file_max_bytes"] = (
            document["sources"]["minio_client"]["expanded_max_bytes"] + 1
        )
        with self.assertRaisesRegex(VERIFIER.InputError, "limits are inconsistent"):
            self.validate(document)

    def test_rejects_incomplete_microsandbox_release_metadata(self):
        document = deepcopy(self.document)
        document["native"]["microsandbox"].pop("asset_id")
        with self.assertRaisesRegex(VERIFIER.InputError, "fields differ"):
            self.validate(document)

    def test_rejects_a_microsandbox_url_that_does_not_match_its_release(self):
        document = deepcopy(self.document)
        document["native"]["microsandbox"]["release_url"] = (
            "https://example.invalid/microsandbox.tar.gz"
        )
        with self.assertRaisesRegex(VERIFIER.InputError, "does not match tag and asset"):
            self.validate(document)

    def test_rejects_an_unexpected_microsandbox_repository(self):
        document = deepcopy(self.document)
        document["native"]["microsandbox"]["repository"] = (
            "https://github.com/example/microsandbox"
        )
        with self.assertRaisesRegex(VERIFIER.InputError, "repository is unexpected"):
            self.validate(document)

    def test_rejects_an_unpinned_microsandbox_guest_image(self):
        document = deepcopy(self.document)
        document["native"]["microsandbox"]["guest_image"] = (
            "quay.io/fedora/fedora:latest"
        )
        with self.assertRaisesRegex(VERIFIER.InputError, "sha256 digest"):
            self.validate(document)

    def test_rejects_a_microsandbox_guest_image_from_another_repository(self):
        document = deepcopy(self.document)
        document["native"]["microsandbox"]["guest_image"] = (
            "example.invalid/fedora@sha256:" + "b" * 64
        )
        with self.assertRaisesRegex(VERIFIER.InputError, "approved Fedora"):
            self.validate(document)

    def test_rejects_a_symbolic_agent_revision(self):
        with self.assertRaisesRegex(VERIFIER.InputError, "invalid format"):
            self.validate(revision="main")

    def test_rejects_octa_release_name_drift(self):
        document = deepcopy(self.document)
        document["native"]["octa"]["release_name"] = "octa-linux-arm64-vnext"
        with self.assertRaisesRegex(VERIFIER.InputError, "does not match"):
            self.validate(document)

    def test_rejects_octa_release_or_source_archive_drift(self):
        document = deepcopy(self.document)
        document["native"]["octa"]["sha256"] = "not-a-digest"
        with self.assertRaisesRegex(VERIFIER.InputError, "invalid format"):
            self.validate(document)

        document = deepcopy(self.document)
        document["native"]["octa"]["source_archive_url"] = (
            "https://codeload.github.com/OctaHive/octa/tar.gz/" + "b" * 40
        )
        with self.assertRaisesRegex(VERIFIER.InputError, "does not match its revision"):
            self.validate(document)

    def test_rejects_octa_codex_license_and_provenance_drift(self):
        mutations = (
            ("codex", {**self.document["native"]["octa"]["codex"], "plugin_protocol": 3}),
            (
                "codex",
                {
                    **self.document["native"]["octa"]["codex"],
                    "required_capabilities": [],
                },
            ),
            ("license", "Apache-2.0"),
            ("provenance", "unsigned"),
        )
        for field, value in mutations:
            document = deepcopy(self.document)
            document["native"]["octa"][field] = value
            with self.subTest(field=field), self.assertRaises(VERIFIER.InputError):
                self.validate(document)

    def test_rejects_unbounded_octa_source_archives(self):
        document = deepcopy(self.document)
        document["native"]["octa"]["source_archive_member_max_count"] = 0
        with self.assertRaisesRegex(VERIFIER.InputError, "positive bounded integer"):
            self.validate(document)


if __name__ == "__main__":
    unittest.main()
