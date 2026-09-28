import importlib.util
import json
import sys
import tempfile
import unittest
from pathlib import Path


REPOSITORY = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPOSITORY / "tools"))
import backend_matrix as BACKEND_MATRIX


SPEC = importlib.util.spec_from_file_location(
    "lifecycle_matrix", REPOSITORY / "tools/lifecycle_matrix.py"
)
assert SPEC and SPEC.loader
LIFECYCLE_MATRIX = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(LIFECYCLE_MATRIX)


class LifecycleMatrixTests(unittest.TestCase):
    def test_catalog_covers_every_manager_provider_phase_and_pipeline_state(self):
        LIFECYCLE_MATRIX.validate_contracts()

        self.assertEqual(
            {target for contract in LIFECYCLE_MATRIX.CONTRACTS for target in contract.targets},
            set(LIFECYCLE_MATRIX.ReleaseTarget),
        )
        self.assertEqual(
            {phase for contract in LIFECYCLE_MATRIX.CONTRACTS for phase in contract.phases},
            set(LIFECYCLE_MATRIX.LifecyclePhase),
        )
        self.assertEqual(
            {state for contract in LIFECYCLE_MATRIX.CONTRACTS for state in contract.pipeline_states},
            set(LIFECYCLE_MATRIX.PipelineState),
        )

    def test_catalog_contains_only_release_qualified_platform_provider_pairs(self):
        self.assertEqual(
            set(LIFECYCLE_MATRIX.TARGET_DETAILS),
            {
                LIFECYCLE_MATRIX.ReleaseTarget.LINUX_HOST,
                LIFECYCLE_MATRIX.ReleaseTarget.LINUX_LEGACY_NATIVE,
                LIFECYCLE_MATRIX.ReleaseTarget.LINUX_CONTAINERD,
                LIFECYCLE_MATRIX.ReleaseTarget.LINUX_MICROSANDBOX,
                LIFECYCLE_MATRIX.ReleaseTarget.MACOS_HOST,
                LIFECYCLE_MATRIX.ReleaseTarget.MACOS_APPLE_VF,
                LIFECYCLE_MATRIX.ReleaseTarget.MACOS_MICROSANDBOX,
                LIFECYCLE_MATRIX.ReleaseTarget.WINDOWS_HOST,
            },
        )
        self.assertNotIn("windows-microsandbox-preview", LIFECYCLE_MATRIX.REQUIRED_RELEASE_JOBS)
        self.assertEqual(
            LIFECYCLE_MATRIX.REQUIRED_RELEASE_JOBS,
            BACKEND_MATRIX.LIFECYCLE_JOBS,
        )

    def test_runner_executes_every_contract_and_retains_failed_evidence(self):
        calls = []

        def runner(command, repository, log_path):
            calls.append((tuple(command), repository, log_path.name))
            log_path.write_text("lifecycle evidence\n", encoding="utf-8")
            return 9 if len(calls) == 2 else 0

        with tempfile.TemporaryDirectory() as directory:
            evidence = Path(directory)
            result = LIFECYCLE_MATRIX.run_contracts(
                REPOSITORY, evidence, runner=runner
            )
            report = json.loads(
                (evidence / "lifecycle-matrix.json").read_text(encoding="utf-8")
            )

        self.assertEqual(result, 1)
        self.assertEqual(len(calls), len(LIFECYCLE_MATRIX.CONTRACTS))
        self.assertEqual(report["result"], "failed")
        self.assertEqual(report["contracts"][1]["exit_code"], 9)

    def test_release_workflow_requires_and_retains_lifecycle_evidence(self):
        workflow = (
            REPOSITORY / ".github/workflows/backend-contracts.yml"
        ).read_text(encoding="utf-8")
        self.assertIn("run_lifecycle_matrix", workflow)
        self.assertIn("python3 tools/lifecycle_matrix.py run", workflow)
        self.assertIn("name: release-lifecycle-matrix", workflow)
        self.assertIn("      - lifecycle-matrix\n", workflow)


if __name__ == "__main__":
    unittest.main()
