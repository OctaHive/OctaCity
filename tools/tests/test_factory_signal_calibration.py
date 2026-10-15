import copy
import hashlib
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


REPOSITORY = Path(__file__).resolve().parents[2]
TOOL = REPOSITORY / "tools/factory_signal_calibration.py"


class FactorySignalCalibrationTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.corpus = {
            "schema_version": 1,
            "label_version": "v1",
            "cases": [
                self.case("r-research", "routing", "research", "needs_context"),
                self.case("r-context", "routing", "needs_context", "needs_context"),
                self.case("t-allow", "tool_risk", "allow", "escalate"),
                self.case("t-deny", "tool_risk", "deny", "deny"),
            ],
        }
        self.binding = {
            name: {"id": name, "version": "fixture-v1", "sha256": "11" * 32}
            for name in [
                "provider", "adapter", "model", "probability_semantics",
                "question_set", "policy",
            ]
        }
        self.observations = {
            "schema_version": 1,
            "mode": "shadow",
            "source_kind": "fixture",
            "bindings": {purpose: copy.deepcopy(self.binding) for purpose in ["routing", "tool_risk"]},
            "observations": [
                self.observation("r-research", "completed", "research", 900_000, 10, "needs_context", 1),
                self.observation("r-context", "completed", "research", 800_000, 20, "needs_context", 2),
                self.observation("t-allow", "completed", "allow", 950_000, 30, "escalate", 3),
                self.observation("t-deny", "timeout", None, None, 40, "deny", 4),
            ],
        }

    @staticmethod
    def case(identity, purpose, expected, baseline):
        return {
            "id": identity,
            "purpose": purpose,
            "choices": ["research", "needs_context"] if purpose == "routing" else ["allow", "deny", "escalate"],
            "expected": expected,
            "baseline": baseline,
            "public_state": {"contract_ready": True, "within_permissions": True},
        }

    @staticmethod
    def observation(identity, status, selected, confidence, latency, final, receipt):
        return {
            "case_id": identity,
            "status": status,
            "selected": selected,
            "confidence_ppm": confidence,
            "latency_ms": latency,
            "final_disposition": final,
            "receipt_sha256": f"{receipt:064x}",
        }

    def evaluate(self, previous=None):
        corpus = self.root / "corpus.json"
        corpus.write_text(json.dumps(self.corpus), encoding="utf-8")
        observations = self.root / "observations.json"
        self.observations.setdefault("corpus_sha256", hashlib.sha256(corpus.read_bytes()).hexdigest())
        observations.write_text(json.dumps(self.observations), encoding="utf-8")
        report = self.root / "report.json"
        report.unlink(missing_ok=True)
        command = [
            sys.executable, str(TOOL), "evaluate", "--corpus", str(corpus),
            "--observations", str(observations), "--report", str(report),
        ]
        if previous is not None:
            command.extend(["--previous", str(previous)])
        result = subprocess.run(command, capture_output=True, text=True, timeout=10)
        return result, json.loads(report.read_text()) if report.exists() else None

    def test_shadow_report_compares_labels_baseline_confidence_latency_and_failures_per_purpose(self):
        result, report = self.evaluate()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue(report["comparison_only"])
        self.assertEqual(report["source_kind"], "fixture")
        self.assertEqual(report["observations_sha256"], hashlib.sha256((self.root / "observations.json").read_bytes()).hexdigest())
        self.assertEqual(report["purposes"]["routing"], {
            "cases": 2, "completed": 2, "baseline_accuracy_ppm": 500_000,
            "model_accuracy_ppm": 500_000, "disagreement_ppm": 1_000_000,
            "mean_confidence_ppm": 850_000, "brier_score_ppm": 325_000,
            "calibration_gap_ppm": 350_000, "failure_rate_ppm": 0,
            "expected_calibration_error_ppm": 450_000,
            "p95_latency_ms": 20,
        })
        self.assertEqual(report["purposes"]["tool_risk"], {
            "cases": 2, "completed": 1, "baseline_accuracy_ppm": 500_000,
            "model_accuracy_ppm": 1_000_000, "disagreement_ppm": 1_000_000,
            "mean_confidence_ppm": 950_000, "brier_score_ppm": 2_500,
            "calibration_gap_ppm": 50_000, "failure_rate_ppm": 500_000,
            "expected_calibration_error_ppm": 50_000,
            "p95_latency_ms": 40,
        })

    def test_report_rejects_signal_authority_in_shadow_and_advisory(self):
        result, shadow = self.evaluate()
        self.assertEqual(result.returncode, 0, result.stderr)
        previous = self.root / "shadow.json"
        previous.write_text(json.dumps(shadow), encoding="utf-8")
        for mode in ["shadow", "advisory"]:
            with self.subTest(mode=mode):
                self.observations["mode"] = mode
                self.observations["observations"][0]["final_disposition"] = "research"
                result, report = self.evaluate(previous if mode == "advisory" else None)
                self.assertEqual(result.returncode, 2)
                self.assertIsNone(report)
                self.assertEqual(result.stderr.strip(), "invalid calibration input")

    def test_report_rejects_unbounded_private_unpinned_and_ambiguous_documents(self):
        corpus = copy.deepcopy(self.corpus)
        observations = copy.deepcopy(self.observations)
        mutations = {
            "unknown provider payload": lambda: self.observations.update({"provider_payload": "PRIVATE-INPUT-MARKER"}),
            "bounded control is not a comparison mode": lambda: self.observations.update({"mode": "bounded_control"}),
            "mutable model": lambda: self.observations["bindings"]["routing"]["model"].update({"version": "latest"}),
            "different corpus": lambda: self.observations.update({"corpus_sha256": "22" * 32}),
            "duplicate case receipt": lambda: self.observations["observations"].append(copy.deepcopy(self.observations["observations"][0])),
            "invalid confidence": lambda: self.observations["observations"][0].update({"confidence_ppm": 1_000_001}),
            "nonfinite latency": lambda: self.observations["observations"][0].update({"latency_ms": float("nan")}),
            "private state": lambda: self.corpus["cases"][0]["public_state"].update({"task_body": "PRIVATE-INPUT-MARKER"}),
            "provider certification claim": lambda: self.observations.update({"source_kind": "provider-certified"}),
            "oversized corpus": lambda: self.corpus.update({"label_version": "x" * 1_100_000}),
        }
        for name, mutate in mutations.items():
            with self.subTest(case=name):
                self.corpus = copy.deepcopy(corpus)
                self.observations = copy.deepcopy(observations)
                mutate()
                result, report = self.evaluate()
                self.assertEqual(result.returncode, 2)
                self.assertIsNone(report)
                self.assertEqual(result.stderr.strip(), "invalid calibration input")
                self.assertNotIn("PRIVATE-INPUT-MARKER", result.stdout + result.stderr)

    def test_purposes_retain_independent_exact_model_question_and_policy_bindings(self):
        routing = self.observations["bindings"]["routing"]
        tool_risk = self.observations["bindings"]["tool_risk"]
        routing["model"]["version"] = "routing-v2"
        tool_risk["model"]["version"] = "tool-risk-v7"
        routing["question_set"]["sha256"] = "22" * 32
        tool_risk["policy"]["sha256"] = "33" * 32
        result, report = self.evaluate()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(report["bindings"]["routing"]["model"]["version"], "routing-v2")
        self.assertEqual(report["bindings"]["tool_risk"]["model"]["version"], "tool-risk-v7")
        self.assertEqual(report["bindings"]["routing"]["question_set"]["sha256"], "22" * 32)
        self.assertEqual(report["bindings"]["tool_risk"]["policy"]["sha256"], "33" * 32)

    def test_advisory_compares_drift_with_the_same_shadow_corpus_and_exact_bindings(self):
        result, shadow = self.evaluate()
        self.assertEqual(result.returncode, 0, result.stderr)
        previous = self.root / "shadow.json"
        previous.write_text(json.dumps(shadow), encoding="utf-8")
        self.observations["mode"] = "advisory"
        self.observations["observations"][1].update({"selected": "needs_context", "confidence_ppm": 700_000})
        self.observations["observations"][3]["latency_ms"] = 50
        result, advisory = self.evaluate(previous)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue(advisory["comparison_only"])
        self.assertEqual(advisory["drift"]["reference_sha256"], hashlib.sha256(previous.read_bytes()).hexdigest())
        self.assertEqual(advisory["drift"]["purposes"]["routing"]["model_accuracy_ppm"], 500_000)
        self.assertEqual(advisory["drift"]["purposes"]["routing"]["disagreement_ppm"], -500_000)
        self.assertEqual(advisory["drift"]["purposes"]["routing"]["mean_confidence_ppm"], -50_000)
        self.assertEqual(advisory["drift"]["purposes"]["tool_risk"]["p95_latency_ms"], 10)

    def test_advisory_cannot_start_without_shadow_or_reuse_another_calibration_subject(self):
        result, shadow = self.evaluate()
        self.assertEqual(result.returncode, 0, result.stderr)
        previous = self.root / "shadow.json"
        self.observations["mode"] = "advisory"
        result, report = self.evaluate()
        self.assertEqual(result.returncode, 2)
        self.assertIsNone(report)
        mutations = {
            "different model": lambda old: old["bindings"]["routing"]["model"].update({"version": "another-v1"}),
            "different corpus": lambda old: old.update({"corpus_sha256": "44" * 32}),
            "fixture is not live calibration": lambda old: old.update({"source_kind": "live"}),
            "unknown payload": lambda old: old.update({"provider_payload": "PRIVATE-INPUT-MARKER"}),
            "fabricated metrics without answers": lambda old: old["purposes"]["routing"].update({"completed": 0, "failure_rate_ppm": 1_000_000}),
        }
        for name, mutate in mutations.items():
            with self.subTest(case=name):
                old = copy.deepcopy(shadow)
                mutate(old)
                previous.write_text(json.dumps(old), encoding="utf-8")
                result, report = self.evaluate(previous)
                self.assertEqual(result.returncode, 2)
                self.assertIsNone(report)
                self.assertEqual(result.stderr.strip(), "invalid calibration input")

    def test_deep_documents_and_overflowing_numbers_use_fixed_diagnostics(self):
        self.observations["observations"][0]["latency_ms"] = 10 ** 400
        result, report = self.evaluate()
        self.assertEqual(result.returncode, 2)
        self.assertIsNone(report)
        self.assertEqual(result.stderr.strip(), "invalid calibration input")
        corpus = self.root / "deep.json"
        corpus.write_text('[' * 1_200 + '0' + ']' * 1_200, encoding="utf-8")
        result = subprocess.run([
            sys.executable, str(TOOL), "evaluate", "--corpus", str(corpus),
            "--observations", str(self.root / "observations.json"),
            "--report", str(self.root / "deep-report.json"),
        ], capture_output=True, text=True, timeout=10)
        self.assertEqual(result.returncode, 2)
        self.assertEqual(result.stderr.strip(), "invalid calibration input")

    def test_corpus_validation_exposes_only_digest_version_and_purpose_counts(self):
        corpus = self.root / "corpus.json"
        corpus.write_text(json.dumps(self.corpus), encoding="utf-8")
        result = subprocess.run([
            sys.executable, str(TOOL), "validate", "--corpus", str(corpus),
        ], capture_output=True, text=True, timeout=10)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(result.stdout), {
            "schema_version": 1, "label_version": "v1",
            "corpus_sha256": hashlib.sha256(corpus.read_bytes()).hexdigest(),
            "cases": {"routing": 2, "tool_risk": 2},
        })

    def test_release_seed_covers_both_purposes_without_private_inputs_or_new_grants(self):
        corpus = REPOSITORY / "tools/fixtures/factory-signal-calibration/corpus-v1.json"
        result = subprocess.run([
            sys.executable, str(TOOL), "validate", "--corpus", str(corpus),
        ], capture_output=True, text=True, timeout=10)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(result.stdout)["cases"], {"routing": 8, "tool_risk": 8})
        cases = {case["id"]: case for case in json.loads(corpus.read_text())["cases"]}
        self.assertEqual(cases["routing.missing-context"]["expected"], "needs_context")
        self.assertEqual(cases["tool.repository-destructive"]["expected"], "deny")
        self.assertTrue(all(case["public_state"]["within_permissions"] for case in cases.values()))

    def test_report_cannot_overwrite_its_input_evidence(self):
        result, _ = self.evaluate()
        self.assertEqual(result.returncode, 0, result.stderr)
        corpus = self.root / "corpus.json"
        before = corpus.read_bytes()
        result = subprocess.run([
            sys.executable, str(TOOL), "evaluate", "--corpus", str(corpus),
            "--observations", str(self.root / "observations.json"),
            "--report", str(corpus),
        ], capture_output=True, text=True, timeout=10)
        self.assertEqual(result.returncode, 2)
        self.assertEqual(corpus.read_bytes(), before)
        self.assertEqual(result.stderr.strip(), "invalid calibration input")

    def test_out_of_envelope_or_unverified_calls_cannot_be_model_calibration(self):
        for verified in [False, None]:
            with self.subTest(verified=verified):
                self.observations.pop("corpus_sha256", None)
                if verified is None:
                    self.corpus["cases"][0]["public_state"].pop("within_permissions", None)
                else:
                    self.corpus["cases"][0]["public_state"]["within_permissions"] = verified
                result, report = self.evaluate()
                self.assertEqual(result.returncode, 2)
                self.assertIsNone(report)
                self.assertEqual(result.stderr.strip(), "invalid calibration input")

    def test_failed_observations_keep_model_accuracy_and_calibration_unknown(self):
        for row in self.observations["observations"]:
            row.update({"status": "unavailable", "selected": None, "confidence_ppm": None})
        result, report = self.evaluate()
        self.assertEqual(result.returncode, 0, result.stderr)
        for metrics in report["purposes"].values():
            self.assertEqual(metrics["completed"], 0)
            self.assertEqual(metrics["failure_rate_ppm"], 1_000_000)
            for name in ["model_accuracy_ppm", "disagreement_ppm", "mean_confidence_ppm",
                         "brier_score_ppm", "calibration_gap_ppm", "expected_calibration_error_ppm"]:
                self.assertIsNone(metrics[name])


if __name__ == "__main__":
    unittest.main()
