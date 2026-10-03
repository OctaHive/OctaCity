"""Static contracts for the portable local-stand CI gates."""

from pathlib import Path
import unittest


REPOSITORY = Path(__file__).resolve().parents[2]
CI = REPOSITORY / ".github/workflows/ci.yml"
SECURITY = REPOSITORY / ".github/workflows/security.yml"


class LocalStandCiContractTests(unittest.TestCase):
    def test_container_gate_builds_every_arm64_target_and_runs_portable_contracts(self):
        workflow = CI.read_text(encoding="utf-8")
        job = workflow.split("  local-stand-containers:\n", 1)[1].split(
            "  local-stand-native-staging:\n", 1
        )[0]

        self.assertIn("runs-on: ubuntu-24.04-arm", job)
        self.assertIn("python3 tools/build_local_stand_images.py", job)
        self.assertIn("docker compose", job)
        self.assertIn("config --format json", job)
        self.assertIn("tools.tests.test_build_local_stand_gateway", job)
        self.assertIn("tools.tests.test_local_stand_launcher", job)
        self.assertIn("tools.tests.test_bootstrap_local_stand", job)
        self.assertIn("python3 tools/check_local_stand_logs.py", job)

    def test_native_gate_stages_and_validates_the_complete_apple_silicon_bundle(self):
        workflow = CI.read_text(encoding="utf-8")
        job = workflow.split("  local-stand-native-staging:\n", 1)[1].split(
            "  quality:\n", 1
        )[0]

        self.assertIn("runs-on: macos-14", job)
        self.assertIn("tools/stage_local_stand_native.py all", job)
        self.assertIn("tools/configure_local_stand_server.py", job)
        self.assertIn("tools/configure_local_stand_agent.py", job)
        self.assertIn("python3 tools/check_local_stand_logs.py", job)
        self.assertNotIn("continue-on-error", job)

    def test_apple_silicon_gate_runs_the_complete_idempotent_lifecycle(self):
        workflow = CI.read_text(encoding="utf-8")
        job = workflow.split("  local-stand-apple-silicon:\n", 1)[1].split(
            "  quality:\n", 1
        )[0]

        self.assertIn("group: octacity-release", job)
        self.assertIn(
            "labels: [self-hosted, macOS, ARM64, octacity-microsandbox]", job
        )
        self.assertIn("group: octacity-local-stand-apple-silicon", job)
        self.assertIn("cancel-in-progress: false", job)
        self.assertIn("persist-credentials: false", job)
        self.assertIn("if: github.event_name != 'pull_request'", job)
        self.assertIn("Require a clean dedicated stand namespace", job)
        self.assertIn('install -m 0600 /dev/null "${evidence}/lifecycle.log"', job)
        self.assertIn('install -m 0600 /dev/null "${evidence}/runtime-running.log"', job)
        self.assertGreaterEqual(job.count("tools/local-stand up"), 3)
        self.assertGreaterEqual(job.count("tools/local-stand down"), 2)
        self.assertIn("verify_local_stand_integration.py", job)
        self.assertGreaterEqual(job.count("observe --receipt"), 2)
        self.assertIn('status["state"] == "stopped"', job)
        self.assertIn("python3 tools/check_local_stand_logs.py", job)
        self.assertIn("--confirm DELETE-OCTACITY-LOCAL-STAND", job)
        self.assertNotIn("continue-on-error", job)

    def test_committed_secret_scan_runs_for_local_stand_changes(self):
        workflow = SECURITY.read_text(encoding="utf-8")

        self.assertIn("  pull_request:\n", workflow)
        for path in (
            '      - "compose.yaml"',
            '      - "deployment/local-stand/**"',
            '      - "tools/*local_stand*.py"',
        ):
            self.assertGreaterEqual(workflow.count(path), 2)
        self.assertIn("gitleaks dir --config octacity/.gitleaks.toml", workflow)


if __name__ == "__main__":
    unittest.main()
