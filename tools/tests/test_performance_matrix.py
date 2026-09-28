import importlib.util
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


REPOSITORY = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location(
    "performance_matrix", REPOSITORY / "tools/performance_matrix.py"
)
assert SPEC and SPEC.loader
PERFORMANCE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(PERFORMANCE)


class PerformanceMatrixTests(unittest.TestCase):
    def setUp(self):
        self.budgets = {
            "schema_version": 1,
            "budgets": {
                "latency_ms": {
                    "unit": "milliseconds",
                    "statistic": "p95",
                    "limit": 20,
                    "minimum_samples": 3,
                },
                "throughput": {
                    "unit": "events_per_second",
                    "statistic": "minimum",
                    "limit": 10,
                    "minimum_samples": 1,
                },
            },
        }
        self.measurements = {
            "schema_version": 1,
            "producer": "fixture",
            "measurements": [
                {"name": "latency_ms", "unit": "milliseconds", "samples": [10, 20, 15]},
                {"name": "throughput", "unit": "events_per_second", "samples": [12]},
            ],
        }

    def test_catalog_covers_every_required_task_metric(self):
        budget_document = PERFORMANCE.load_object(REPOSITORY / "tools/performance_budgets.json")
        budgets = PERFORMANCE.validate_budgets(budget_document)
        self.assertEqual(
            set(budgets),
            {
                "trigger_latency_ms",
                "dag_transition_latency_ms",
                "queue_acquisition_latency_ms",
                "rest_latency_ms",
                "event_throughput_per_second",
                "log_index_lag_ms",
                "log_query_latency_ms",
                "backend_startup_ms",
                "backend_teardown_ms",
                "artifact_throughput_bytes_per_second",
                "cache_restore_ms",
                "database_contention_latency_ms",
                "agent_soak_registration_ms",
                "agent_soak_poll_ms",
                "agent_soak_errors",
                "agent_soak_agents",
            },
        )
        measurements = {
            "schema_version": 1,
            "producer": "complete-fixture",
            "measurements": [
                {
                    "name": name,
                    "unit": budget["unit"],
                    "samples": [budget["limit"]] * budget["minimum_samples"],
                }
                for name, budget in budgets.items()
            ],
        }
        self.assertEqual(PERFORMANCE.evaluate(budget_document, measurements)["result"], "passed")

    def test_gate_keeps_raw_samples_and_applies_directional_budgets(self):
        report = PERFORMANCE.evaluate(self.budgets, self.measurements)
        self.assertEqual(report["result"], "passed")
        self.measurements["measurements"][0]["samples"] = [10, 20, 21]
        self.assertEqual(PERFORMANCE.evaluate(self.budgets, self.measurements)["result"], "failed")
        self.measurements["measurements"][0]["samples"] = [10, 20, 15]
        self.measurements["measurements"][1]["samples"] = [9]
        self.assertEqual(PERFORMANCE.evaluate(self.budgets, self.measurements)["result"], "failed")

    def test_gate_rejects_missing_duplicate_and_insufficient_samples(self):
        missing = {**self.measurements, "measurements": self.measurements["measurements"][:1]}
        with self.assertRaisesRegex(ValueError, "coverage differs"):
            PERFORMANCE.evaluate(self.budgets, missing)

        duplicate = {
            **self.measurements,
            "measurements": self.measurements["measurements"] + [self.measurements["measurements"][0]],
        }
        with self.assertRaisesRegex(ValueError, "unique"):
            PERFORMANCE.evaluate(self.budgets, duplicate)

        too_few = json.loads(json.dumps(self.measurements))
        too_few["measurements"][0]["samples"] = [10, 11]
        with self.assertRaisesRegex(ValueError, "expected at least 3"):
            PERFORMANCE.evaluate(self.budgets, too_few)

    def test_cli_retains_a_report_and_fails_a_regression(self):
        measurements = json.loads(json.dumps(self.measurements))
        measurements["measurements"][0]["samples"] = [21, 22, 23]
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            budget_path = root / "budgets.json"
            measurements_path = root / "measurements.json"
            report_path = root / "report.json"
            budget_path.write_text(json.dumps(self.budgets), encoding="utf-8")
            measurements_path.write_text(json.dumps(measurements), encoding="utf-8")
            result = subprocess.run(
                [
                    sys.executable,
                    str(REPOSITORY / "tools/performance_matrix.py"),
                    "evaluate",
                    "--budgets",
                    str(budget_path),
                    "--measurements",
                    str(measurements_path),
                    "--report",
                    str(report_path),
                ],
                check=False,
                capture_output=True,
                text=True,
            )
            report = json.loads(report_path.read_text(encoding="utf-8"))
        self.assertEqual(result.returncode, 1)
        self.assertEqual(report["result"], "failed")


if __name__ == "__main__":
    unittest.main()
