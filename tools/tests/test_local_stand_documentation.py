"""Contracts that keep the local-stand runbook aligned with its CLIs."""

from pathlib import Path
import sys
import unittest


REPOSITORY = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPOSITORY / "tools"))

import local_stand_launcher as launcher  # noqa: E402
import verify_local_stand_integration as integration  # noqa: E402


GUIDE = REPOSITORY / "docs/operations/local-stand.md"


class LocalStandDocumentationTests(unittest.TestCase):
    def test_runbook_is_discoverable_and_covers_the_operator_contract(self):
        guide = GUIDE.read_text(encoding="utf-8")
        index = (REPOSITORY / "docs/README.md").read_text(encoding="utf-8")
        readme = (REPOSITORY / "README.md").read_text(encoding="utf-8")

        self.assertIn("operations/local-stand.md", index)
        self.assertIn("docs/operations/local-stand.md", readme)
        for heading in (
            "## Prerequisites",
            "## Start, inspect, and stop",
            "## URLs and local CA",
            "## Verify readiness and a real job",
            "## Destructive reset",
            "## Troubleshooting",
            "## Non-production limitations",
        ):
            self.assertIn(heading, guide)
        for boundary in (
            "not a production deployment",
            "management API is unauthenticated",
            "does not replace release-candidate packaging",
        ):
            self.assertIn(boundary, guide)

    def test_documented_lifecycle_commands_match_the_launcher_parser(self):
        guide = GUIDE.read_text(encoding="utf-8")
        cases = (
            ("tools/local-stand up", ["up"]),
            ("tools/local-stand down", ["down"]),
            ("tools/local-stand status", ["status"]),
            ("tools/local-stand logs --tail 200", ["logs", "--tail", "200"]),
            (
                "tools/local-stand reset --confirm DELETE-OCTACITY-LOCAL-STAND",
                ["reset", "--confirm", launcher.RESET_CONFIRMATION],
            ),
        )
        for documented, arguments in cases:
            with self.subTest(command=documented):
                self.assertIn(documented, guide)
                self.assertEqual(launcher.parse_arguments(arguments).command, arguments[0])

    def test_urls_trust_reset_and_verifier_arguments_are_exact(self):
        guide = GUIDE.read_text(encoding="utf-8")

        for hostname in ("octacity", "agent", "cache", "objects"):
            self.assertIn(f"https://{hostname}.localhost:8443/", guide)
        self.assertIn("security add-trusted-cert", guide)
        self.assertIn("security delete-certificate -t -Z", guide)
        self.assertIn("octacity-local_postgres-data", guide)
        self.assertIn("octacity-local_minio-data", guide)

        run = integration.parse_arguments(
            [
                "--root",
                "/private/tmp/octacity-local-stand-doc-test",
                "run",
                "--source-repository",
                "https://github.com/OctaHive/OctaCity",
                "--source-revision",
                "a" * 40,
                "--receipt",
                "/private/tmp/octacity-local-stand-doc-test/receipt.json",
            ]
        )
        observe = integration.parse_arguments(
            [
                "--root",
                "/private/tmp/octacity-local-stand-doc-test",
                "observe",
                "--receipt",
                "/private/tmp/octacity-local-stand-doc-test/receipt.json",
            ]
        )
        self.assertEqual(run.command, "run")
        self.assertEqual(observe.command, "observe")


if __name__ == "__main__":
    unittest.main()
