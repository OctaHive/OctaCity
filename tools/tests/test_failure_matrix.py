import importlib.util
import json
import sys
import tempfile
import unittest
from pathlib import Path


REPOSITORY = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPOSITORY / "tools"))
SPEC = importlib.util.spec_from_file_location(
    "failure_matrix", REPOSITORY / "tools/failure_matrix.py"
)
assert SPEC and SPEC.loader
FAILURE_MATRIX = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(FAILURE_MATRIX)


class FailureMatrixTests(unittest.TestCase):
    def test_catalog_covers_every_boundary_phase_and_invariant(self):
        FAILURE_MATRIX.validate_contracts()

        boundaries = {
            boundary
            for contract in FAILURE_MATRIX.CONTRACTS
            for boundary in contract.boundaries
        }
        phases = {
            phase
            for contract in FAILURE_MATRIX.CONTRACTS
            for phase in contract.phases
        }
        invariants = {
            invariant
            for contract in FAILURE_MATRIX.CONTRACTS
            for invariant in contract.invariants
        }
        self.assertEqual(boundaries, set(FAILURE_MATRIX.Boundary))
        self.assertEqual(phases, set(FAILURE_MATRIX.Phase))
        self.assertEqual(invariants, set(FAILURE_MATRIX.Invariant))

    def test_runner_executes_every_contract_and_retains_failed_evidence(self):
        calls = []

        def runner(command, repository, log_path):
            calls.append((tuple(command), repository, log_path.name))
            log_path.write_text("injected failure evidence\n", encoding="utf-8")
            return 9 if len(calls) == 2 else 0

        with tempfile.TemporaryDirectory() as directory:
            evidence = Path(directory)
            result = FAILURE_MATRIX.run_contracts(REPOSITORY, evidence, runner=runner)
            report = json.loads(
                (evidence / "failure-matrix.json").read_text(encoding="utf-8")
            )

        self.assertEqual(result, 1)
        self.assertEqual(len(calls), len(FAILURE_MATRIX.CONTRACTS))
        self.assertEqual(report["result"], "failed")
        self.assertEqual(report["contracts"][1]["exit_code"], 9)
        self.assertTrue(
            all(
                "no-false-success" in contract["invariants"]
                for contract in report["contracts"]
            )
        )

    def test_release_workflow_requires_and_retains_failure_evidence(self):
        workflow = (
            REPOSITORY / ".github/workflows/backend-contracts.yml"
        ).read_text(encoding="utf-8")
        self.assertIn("run_failure_matrix", workflow)
        self.assertIn("python3 tools/failure_matrix.py run", workflow)
        self.assertIn("name: release-failure-matrix", workflow)
        self.assertIn("      - failure-matrix\n", workflow)

    def test_postgresql_contract_retains_replica_recovery_evidence(self):
        contract = next(
            contract
            for contract in FAILURE_MATRIX.CONTRACTS
            if contract.name == "postgresql-recovery"
        )
        self.assertIn(FAILURE_MATRIX.Invariant.REPLICA_RECOVERY, contract.invariants)
        self.assertIn("--tests", contract.command)
        self.assertIn("--ignored", contract.command)


if __name__ == "__main__":
    unittest.main()
