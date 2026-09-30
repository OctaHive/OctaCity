import importlib.util
import json
import sys
import tempfile
import unittest
from pathlib import Path


REPOSITORY = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPOSITORY / "tools"))
SPEC = importlib.util.spec_from_file_location(
    "security_matrix", REPOSITORY / "tools/security_matrix.py"
)
assert SPEC and SPEC.loader
SECURITY_MATRIX = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(SECURITY_MATRIX)


class SecurityMatrixTests(unittest.TestCase):
    def test_catalog_covers_every_boundary_and_security_property(self):
        SECURITY_MATRIX.validate_contracts()

        boundaries = {
            boundary
            for contract in SECURITY_MATRIX.CONTRACTS
            for boundary in contract.boundaries
        }
        properties = {
            prop
            for contract in SECURITY_MATRIX.CONTRACTS
            for prop in contract.properties
        }
        self.assertEqual(boundaries, set(SECURITY_MATRIX.Boundary))
        self.assertEqual(properties, set(SECURITY_MATRIX.SecurityProperty))

    def test_runner_executes_every_contract_and_retains_failed_evidence(self):
        calls = []

        def runner(command, repository, log_path):
            calls.append((tuple(command), repository, log_path.name))
            log_path.write_text("security evidence\n", encoding="utf-8")
            return 7 if len(calls) == 3 else 0

        with tempfile.TemporaryDirectory() as directory:
            evidence = Path(directory)
            result = SECURITY_MATRIX.run_contracts(
                REPOSITORY, evidence, runner=runner
            )
            report = json.loads(
                (evidence / "security-matrix.json").read_text(encoding="utf-8")
            )

        self.assertEqual(result, 1)
        self.assertEqual(len(calls), len(SECURITY_MATRIX.CONTRACTS))
        self.assertEqual(report["result"], "failed")
        self.assertEqual(report["contracts"][2]["exit_code"], 7)
        self.assertTrue(
            all(
                "fail-closed" in contract["properties"]
                for contract in report["contracts"]
            )
        )

    def test_release_workflow_requires_and_retains_security_evidence(self):
        workflow = (
            REPOSITORY / ".github/workflows/backend-contracts.yml"
        ).read_text(encoding="utf-8")
        self.assertIn("run_security_matrix", workflow)
        self.assertIn("python3 tools/security_matrix.py run", workflow)
        self.assertIn("name: release-security-matrix", workflow)
        self.assertIn("      - security-matrix\n", workflow)

    def test_release_workflow_checks_out_the_pinned_octa_dependency(self):
        workflow = (
            REPOSITORY / ".github/workflows/backend-contracts.yml"
        ).read_text(encoding="utf-8")
        security_job = workflow.split("\n  security-matrix:\n", 1)[1].split(
            "\n  vault-artifact-contract:\n", 1
        )[0]

        self.assertIn(
            "uses: ./octacity/.github/actions/checkout-octa", security_job
        )


if __name__ == "__main__":
    unittest.main()
