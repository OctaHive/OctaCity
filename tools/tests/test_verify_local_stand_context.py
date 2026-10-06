"""Tests for the local-stand Docker context and native staging allowlist."""

from __future__ import annotations

import importlib.util
import hashlib
import io
import json
from pathlib import Path
import tarfile
import tempfile
import unittest


REPOSITORY = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location(
    "verify_local_stand_context", REPOSITORY / "tools/verify_local_stand_context.py"
)
assert SPEC is not None and SPEC.loader is not None
VERIFY = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(VERIFY)
AGENT_REVISION = "a" * 40


class LocalStandContextTests(unittest.TestCase):
    def staging_fixture(self, root: Path, entries):
        immutable = json.loads(
            (REPOSITORY / "deployment/local-stand/inputs.json").read_text(encoding="utf-8")
        )
        identities = {
            "octacity_agent": (
                "release-manifest.json",
                {
                    "format_version": 1,
                    "product": "octacity-agent",
                    "version": immutable["native"]["octacity_agent"]["version"],
                    "platform": immutable["native"]["octacity_agent"]["platform"],
                    "build_inputs": {
                        "octacity_revision": AGENT_REVISION,
                        "octa_revision": immutable["native"]["octa"]["source_revision"],
                    },
                },
            ),
            "octa": (
                "octa-runner-capabilities.json",
                {
                    "octa_version": immutable["native"]["octa"]["version"],
                    "build_commit": immutable["native"]["octa"]["source_revision"],
                    "platform": "linux-aarch64",
                },
            ),
        }
        for role, entry in entries.items():
            archive = root / entry["filename"]
            if role == "octacity_release_harness":
                archive.write_bytes(b"verified policy helper fixture")
                digest = hashlib.sha256(archive.read_bytes()).hexdigest()
                (root / f"{entry['filename']}.sha256").write_text(
                    f"{digest}  {entry['filename']}\n", encoding="utf-8"
                )
                continue
            if role == "microsandbox":
                archive.write_bytes(b"verified Microsandbox fixture")
                immutable["native"][role]["sha256"] = hashlib.sha256(
                    archive.read_bytes()
                ).hexdigest()
                continue
            identity_name, document = identities[role]
            payload = json.dumps(document).encode()
            with tarfile.open(archive, "w:gz") as bundle:
                member = tarfile.TarInfo(identity_name)
                member.size = len(payload)
                bundle.addfile(member, io.BytesIO(payload))
            digest = hashlib.sha256(archive.read_bytes()).hexdigest()
            (root / f"{entry['filename']}.sha256").write_text(
                f"{digest}  {entry['filename']}\n", encoding="utf-8"
            )
            if role == "octa":
                immutable["native"][role]["sha256"] = digest
        return immutable

    def test_repository_policy_is_valid(self):
        VERIFY.validate_repository(REPOSITORY)

    def test_required_sources_and_lockfiles_are_included(self):
        rules = VERIFY.validate_dockerignore(REPOSITORY / ".dockerignore")
        for path in VERIFY.REQUIRED_CONTEXT_PATHS:
            with self.subTest(path=path):
                self.assertTrue(VERIFY.docker_path_included(path, rules))

    def test_ui_api_contract_is_generated_inside_the_builder(self):
        rules = VERIFY.validate_dockerignore(REPOSITORY / ".dockerignore")
        for path in (
            "ui/architecture-policy.json",
            "ui/scripts/generate-api.mjs",
            "ui/scripts/verify-architecture.mjs",
        ):
            with self.subTest(path=path):
                self.assertTrue(VERIFY.docker_path_included(path, rules))
        self.assertFalse(
            VERIFY.docker_path_included("ui/.generated/api/schema.d.ts", rules)
        )
        self.assertFalse(
            VERIFY.docker_path_included("ui/.generated/api/constraints.ts", rules)
        )

    def test_console_proxy_contract_is_included_in_the_build_context(self):
        rules = VERIFY.validate_dockerignore(REPOSITORY / ".dockerignore")
        for path in (
            "deployment/console/nginx/cache-map.conf",
            "deployment/console/nginx/routes.conf",
            "deployment/console/nginx/security-headers.conf",
        ):
            with self.subTest(path=path):
                self.assertTrue(VERIFY.docker_path_included(path, rules))
        self.assertFalse(
            VERIFY.docker_path_included("deployment/console/nginx/nginx.conf", rules)
        )

    def test_developer_state_build_outputs_and_secrets_are_excluded(self):
        rules = VERIFY.validate_dockerignore(REPOSITORY / ".dockerignore")
        for path in VERIFY.FORBIDDEN_CONTEXT_PATHS:
            with self.subTest(path=path):
                self.assertFalse(VERIFY.docker_path_included(path, rules))

    def test_a_broader_docker_allow_rule_is_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            dockerignore = Path(temporary) / ".dockerignore"
            dockerignore.write_text(
                (REPOSITORY / ".dockerignore").read_text(encoding="utf-8") + "\n!README.md\n",
                encoding="utf-8",
            )
            with self.assertRaisesRegex(VERIFY.ContextError, "must follow every allow rule"):
                VERIFY.validate_dockerignore(dockerignore)

    def test_staging_allowlist_tracks_the_immutable_manifest(self):
        entries = VERIFY.load_staging_allowlist(
            REPOSITORY / "deployment/local-stand/staging-allowlist.json",
            REPOSITORY / "deployment/local-stand/inputs.json",
        )
        self.assertEqual(set(entries), set(VERIFY.STAGING_POLICY))
        self.assertEqual(entries["microsandbox"]["filename"], "microsandbox-darwin-aarch64.tar.gz")
        self.assertEqual(entries["octa"]["filename"], "octa-linux-arm64-v0.5.0.tar.gz")
        self.assertEqual(
            entries["octacity_release_harness"]["filename"],
            "octacity-release-harness-macos-arm64",
        )

    def test_complete_regular_staging_directory_is_accepted(self):
        entries = VERIFY.load_staging_allowlist(
            REPOSITORY / "deployment/local-stand/staging-allowlist.json",
            REPOSITORY / "deployment/local-stand/inputs.json",
        )
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            immutable = self.staging_fixture(root, entries)
            VERIFY.validate_staging_directory(root, entries, immutable, AGENT_REVISION)

    def test_staging_rejects_unknown_missing_symbolic_and_oversized_inputs(self):
        entries = VERIFY.load_staging_allowlist(
            REPOSITORY / "deployment/local-stand/staging-allowlist.json",
            REPOSITORY / "deployment/local-stand/inputs.json",
        )
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            immutable = self.staging_fixture(root, entries)

            unknown = root / "developer-notes.txt"
            unknown.write_text("not an input", encoding="utf-8")
            with self.assertRaisesRegex(VERIFY.ContextError, "staging files differ"):
                VERIFY.validate_staging_directory(root, entries, immutable, AGENT_REVISION)
            unknown.unlink()

            missing = root / entries["octa"]["filename"]
            missing.unlink()
            with self.assertRaisesRegex(VERIFY.ContextError, "staging files differ"):
                VERIFY.validate_staging_directory(root, entries, immutable, AGENT_REVISION)
            immutable = self.staging_fixture(root, entries)

            agent = root / entries["octacity_agent"]["filename"]
            agent.unlink()
            agent.symlink_to(missing)
            with self.assertRaisesRegex(VERIFY.ContextError, "regular file"):
                VERIFY.validate_staging_directory(root, entries, immutable, AGENT_REVISION)
            agent.unlink()
            agent.write_bytes(b"too large")

            bounded = {role: dict(entry) for role, entry in entries.items()}
            bounded["octacity_agent"]["max_bytes"] = 1
            with self.assertRaisesRegex(VERIFY.ContextError, "size limit"):
                VERIFY.validate_staging_directory(root, bounded, immutable, AGENT_REVISION)

    def test_staging_rejects_checksum_and_release_identity_drift(self):
        entries = VERIFY.load_staging_allowlist(
            REPOSITORY / "deployment/local-stand/staging-allowlist.json",
            REPOSITORY / "deployment/local-stand/inputs.json",
        )
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            immutable = self.staging_fixture(root, entries)
            microsandbox = root / entries["microsandbox"]["filename"]
            microsandbox.write_bytes(b"tampered")
            with self.assertRaisesRegex(VERIFY.ContextError, "checksum mismatch"):
                VERIFY.validate_staging_directory(root, entries, immutable, AGENT_REVISION)

        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            immutable = self.staging_fixture(root, entries)
            helper = root / entries["octacity_release_harness"]["filename"]
            helper.write_bytes(b"tampered")
            with self.assertRaisesRegex(VERIFY.ContextError, "checksum mismatch"):
                VERIFY.validate_staging_directory(root, entries, immutable, AGENT_REVISION)

        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            immutable = self.staging_fixture(root, entries)
            with self.assertRaisesRegex(VERIFY.ContextError, "Agent release identity"):
                VERIFY.validate_staging_directory(root, entries, immutable, "b" * 40)

    def test_staging_rejects_manifest_drift(self):
        with tempfile.TemporaryDirectory() as temporary:
            allowlist = Path(temporary) / "staging-allowlist.json"
            document = json.loads(
                (REPOSITORY / "deployment/local-stand/staging-allowlist.json").read_text(encoding="utf-8")
            )
            document["inputs"][1]["filename"] = "octa-latest.tar.gz"
            allowlist.write_text(json.dumps(document), encoding="utf-8")
            with self.assertRaisesRegex(VERIFY.ContextError, "filename"):
                VERIFY.load_staging_allowlist(
                    allowlist,
                    REPOSITORY / "deployment/local-stand/inputs.json",
                )

    def test_materialized_context_rejects_an_excluded_file(self):
        rules = VERIFY.validate_dockerignore(REPOSITORY / ".dockerignore")
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for relative in VERIFY.REQUIRED_CONTEXT_PATHS:
                path = root / relative
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text("fixture", encoding="utf-8")
            (root / "developer-notes.txt").write_text("private", encoding="utf-8")
            with self.assertRaisesRegex(VERIFY.ContextError, "forbidden input"):
                VERIFY.validate_materialized_context(root, rules)


if __name__ == "__main__":
    unittest.main()
