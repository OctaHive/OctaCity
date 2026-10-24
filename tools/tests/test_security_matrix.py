import importlib.util
import json
import sys
import tempfile
import tomllib
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
    def test_locked_dependencies_exclude_known_vulnerable_releases(self):
        # RUSTSEC-2026-0381/0382/0383/0385/0386: retain patched stable dependencies
        # in every independently resolved graph audited by the security workflow.
        minimum_versions = {
            "ctutils": (0, 4, 4),
            "tokio": (1, 53, 3),
            "rustls-webpki": (0, 103, 16),
            "webpki-root-certs": (1, 0, 10),
            "webpki-roots": (1, 0, 10),
        }
        for relative in ("Cargo.lock", "fuzz/Cargo.lock"):
            packages = tomllib.loads((REPOSITORY / relative).read_text())["package"]
            for package in packages:
                if package["name"] not in minimum_versions:
                    continue
                with self.subTest(lockfile=relative, package=package["name"]):
                    self.assertRegex(package["version"], r"^\d+\.\d+\.\d+$")
                    self.assertGreaterEqual(
                        tuple(map(int, package["version"].split("."))),
                        minimum_versions[package["name"]],
                        "locked dependencies must retain patched stable releases",
                    )

    def test_fuzz_lockfile_retains_the_pinned_octa_wire_contract(self):
        workspace = tomllib.loads((REPOSITORY / "Cargo.toml").read_text())
        lockfile = tomllib.loads((REPOSITORY / "fuzz/Cargo.lock").read_text())
        packages = {package["name"]: package for package in lockfile["package"]}
        for name in ("octa-cache-protocol", "octa-plugin-lock", "octa-runner-protocol"):
            with self.subTest(package=name):
                self.assertEqual(
                    packages[name]["version"],
                    workspace["workspace"]["dependencies"][name]["version"],
                )
        self.assertIn(
            "octa-runner-protocol",
            packages["octacity-protocol"]["dependencies"],
            "fuzzing must include the shared public-operation observation contract",
        )

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

    def test_agent_security_gate_executes_lease_bound_hook_and_broker_negatives(self):
        commands = []

        def runner(command, _repository, log_path):
            commands.append(tuple(command))
            log_path.write_text("security evidence\n", encoding="utf-8")
            return 0

        with tempfile.TemporaryDirectory() as directory:
            self.assertEqual(
                SECURITY_MATRIX.run_contracts(
                    REPOSITORY, Path(directory), runner=runner
                ),
                0,
            )

        agent_command = next(
            command for command in commands if "octacity-job" in command
        )
        self.assertIn(
            "octacity-lifecycle",
            agent_command,
            "the released Agent security gate must exercise hook redaction and cancellation fencing",
        )

    def test_security_runner_executes_factory_definition_and_provider_negatives(self):
        commands = []

        def runner(command, _repository, log_path):
            commands.append(tuple(command))
            log_path.write_text("security evidence\n", encoding="utf-8")
            return 0

        with tempfile.TemporaryDirectory() as directory:
            self.assertEqual(
                SECURITY_MATRIX.run_contracts(REPOSITORY, Path(directory), runner=runner),
                0,
            )

        executed = {argument for command in commands for argument in command}
        self.assertTrue(
            {"octacity-server-factory", "octacity-server-decision-signal",
             "octacity-server-decision-signal-jev", "octacity-server-decision-signal-openai"} <= executed,
            "release security must execute graph/context/evidence and provider substitution negatives",
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
