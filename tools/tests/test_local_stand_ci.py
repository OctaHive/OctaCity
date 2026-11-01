"""Contracts for the portable local-stand CI gates and their diagnostics."""

import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import textwrap
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
            "  local-stand-apple-silicon:\n", 1
        )[0]

        self.assertIn("runs-on: macos-15", job)
        self.assertIn("tools/stage_local_stand_native.py all", job)
        self.assertIn("tools/configure_local_stand_server.py", job)
        self.assertIn("tools/configure_local_stand_agent.py", job)
        credential = "configuration-validation-only"
        self.assertIn(credential, job)
        self.assertLess(
            job.index(credential), job.index("tools/configure_local_stand_agent.py")
        )
        self.assertIn("report_native_staging_failure", job)
        self.assertIn("trap report_native_staging_failure EXIT", job)
        self.assertIn("python3 tools/check_local_stand_logs.py", job)
        staging_step_header = job.split(
            "      - name: Stage every native input and validate generated configurations\n",
            1,
        )[1].split("        shell: bash\n", 1)[0]
        self.assertIn("GITHUB_TOKEN: ${{ github.token }}", staging_step_header)
        self.assertNotIn("continue-on-error", job)

    def test_apple_silicon_gate_is_opt_in_and_runs_the_complete_idempotent_lifecycle(self):
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
        self.assertIn("vars.OCTACITY_APPLE_SILICON_LOCAL_STAND == 'true'", job)
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


@unittest.skipUnless(os.name == "posix" and shutil.which("bash"), "requires Bash")
class LocalStandPortableDiagnosticsTests(unittest.TestCase):
    def run_portable_step(self, failed_stage, output):
        workflow = CI.read_text(encoding="utf-8")
        step = workflow.split(
            "      - name: Validate local-stand policies, Compose, and portable contracts\n", 1
        )[1].split("      - name:", 1)[0]
        script = textwrap.dedent(step.split("        run: |\n", 1)[1])
        peers = r'''python3() {
          case "$1" in
            tools/init_local_stand.py) stage=init ;;
            -m) stage=contracts ;;
            tools/check_local_stand_logs.py) "$TEST_PYTHON" "$@"; return $? ;;
            *) return 0 ;;
          esac
          if [[ "$stage" == "$FAILED_STAGE" ]]; then
            printf '%s\n' "$STAGE_OUTPUT"
            return 17
          fi
          printf '%s\n' "fixture $stage completed"
        }
        docker() { return 0; }
        '''
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            secrets = root / "octacity-local-stand-portable" / "secrets"
            secrets.mkdir(parents=True)
            (secrets / "fixture-secret").write_text("synthetic-private-value")
            return subprocess.run(
                [shutil.which("bash"), "--noprofile", "--norc", "-c", peers + script],
                cwd=REPOSITORY,
                env={**os.environ, "RUNNER_TEMP": str(root), "TEST_PYTHON": sys.executable,
                     "FAILED_STAGE": failed_stage, "STAGE_OUTPUT": output},
                capture_output=True, text=True, check=False,
            )

    def test_failures_print_safe_diagnostics_and_keep_the_original_exit_status(self):
        for stage in ("init", "contracts"):
            with self.subTest(stage=stage):
                result = self.run_portable_step(stage, "fixture portable contract failed")
                self.assertEqual(result.returncode, 17, result.stderr)
                self.assertIn("fixture portable contract failed", result.stdout)

    def test_unsafe_failure_logs_are_withheld_without_replacing_the_original_status(self):
        result = self.run_portable_step("contracts", "synthetic-private-value")
        self.assertEqual(result.returncode, 17, result.stderr)
        self.assertNotIn("synthetic-private-value", result.stdout + result.stderr)
        self.assertIn("refusing to emit", result.stderr)

    def test_success_retains_the_existing_checked_log_output(self):
        result = self.run_portable_step("", "")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("fixture contracts completed", result.stdout)


@unittest.skipUnless(os.name == "posix" and shutil.which("bash"), "requires Bash")
class LocalStandImageDiagnosticsTests(unittest.TestCase):
    def run_image_step(self, build_status, build_output):
        workflow = CI.read_text(encoding="utf-8")
        step = workflow.split(
            "      - name: Build and verify every Linux ARM64 local-stand image\n", 1
        )[1].split("\n  local-stand-native-staging:", 1)[0]
        script = textwrap.dedent(step.split("        run: |\n", 1)[1])
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            secrets = root / "octacity-local-stand-portable" / "secrets"
            secrets.mkdir(parents=True)
            (secrets / "fixture-secret").write_text("synthetic-private-value")
            executable = root / "python3"
            executable.write_text(
                f"#!{sys.executable}\n"
                "import os, sys\n"
                "if sys.argv[1] == 'tools/build_local_stand_images.py':\n"
                "    print(os.environ['BUILD_OUTPUT'])\n"
                "    sys.exit(int(os.environ['BUILD_STATUS']))\n"
                "os.execv(sys.executable, [sys.executable, *sys.argv[1:]])\n"
            )
            executable.chmod(0o700)
            return subprocess.run(
                [shutil.which("bash"), "--noprofile", "--norc", "-c", script],
                cwd=REPOSITORY,
                env={
                    **os.environ,
                    "PATH": str(root) + os.pathsep + os.defpath,
                    "RUNNER_TEMP": str(root),
                    "BUILD_STATUS": str(build_status),
                    "BUILD_OUTPUT": build_output,
                },
                capture_output=True,
                text=True,
                check=False,
            )

    def test_failed_build_prints_safe_diagnostics_and_preserves_exit_status(self):
        result = self.run_image_step(2, "fixture image verification failed")

        self.assertEqual(result.returncode, 2, result.stderr)
        self.assertIn("fixture image verification failed", result.stdout)

    def test_successful_build_prints_safe_diagnostics(self):
        result = self.run_image_step(0, "fixture images verified")

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("fixture images verified", result.stdout)

    def test_unsafe_diagnostics_are_withheld_for_success_and_failure(self):
        for build_status, expected_status in ((0, 2), (17, 17)):
            with self.subTest(build_status=build_status):
                result = self.run_image_step(build_status, "synthetic-private-value")

                self.assertEqual(result.returncode, expected_status)
                self.assertNotIn("synthetic-private-value", result.stdout + result.stderr)
                self.assertIn("refusing to emit", result.stderr)


if __name__ == "__main__":
    unittest.main()
