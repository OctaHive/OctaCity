"""Contract tests for the Factory graph semantic matrix and neutral corpus."""

from __future__ import annotations

import json
from pathlib import Path
import unittest


REPOSITORY = Path(__file__).resolve().parents[2]
MATRIX = REPOSITORY / "docs/planning/proposals/dark-factory-flow-graph-semantics.md"
SPECIFICATION = (
    REPOSITORY
    / "openspec/changes/add-dark-factory-mode/specs/server/dark-factory/spec.md"
)
CORPUS = (
    REPOSITORY
    / "server/core/octacity-server-factory/tests/fixtures/flow-graph-conformance-v1.json"
)


class FactoryGraphSemanticsTests(unittest.TestCase):
    def setUp(self) -> None:
        self.document = MATRIX.read_text(encoding="utf-8")
        self.corpus_text = CORPUS.read_text(encoding="utf-8")
        self.corpus = json.loads(self.corpus_text)

    def test_matrix_records_all_three_domains_and_the_deferred_extraction_gate(self):
        required = (
            "Octa task DAG (current)",
            "OctaCity Job DAG (current)",
            "Factory Flow Definition (target)",
            "Normalized orientation",
            "Duplicate node",
            "Duplicate edge",
            "Missing reference",
            "Canonical ordering",
            "Reachability",
            "Components and cycles",
            "Topological projection",
            "Structural measurements",
            "Cycle policy",
            "Execution semantics",
            "Data authority",
            "No shared-kernel commitment",
            "at least two working consumers",
        )
        for text in required:
            with self.subTest(text=text):
                self.assertIn(text, self.document)

        self.assertNotIn("SHALL come from one exact-versioned", self.document)
        specification = SPECIFICATION.read_text(encoding="utf-8")
        self.assertIn("private to the OctaCity Factory core", specification)
        self.assertNotIn("SHALL come from one exact-versioned", specification)

    def test_corpus_has_complete_neutral_and_canonical_expectations(self):
        self.assertEqual(self.corpus["format_version"], 1)
        self.assertEqual(
            self.corpus["semantics"],
            {
                "edge_orientation": "predecessor_to_successor",
                "identity_order": "ascending_utf8_bytes",
                "topological_tie_break": "smallest_ready_identity",
                "depth_unit": "edge_count",
                "facts_require_well_formed_structure": True,
            },
        )

        cases = self.corpus["cases"]
        names = [case["name"] for case in cases]
        self.assertEqual(len(names), len(set(names)))
        coverage = {item for case in cases for item in case["covers"]}
        self.assertGreaterEqual(
            coverage,
            {
                "canonical_order",
                "cycle",
                "duplicate_edge",
                "duplicate_node",
                "edge_orientation",
                "identity",
                "measurements",
                "missing_reference",
                "reachability",
                "self_loop",
                "strongly_connected_components",
                "topological_order",
            },
        )

        for case in cases:
            with self.subTest(case=case["name"]):
                self.assertEqual(case["covers"], sorted(set(case["covers"])))
                self.assertTrue(
                    all(isinstance(node, str) and node for node in case["nodes"])
                )
                self.assertTrue(
                    all(set(edge) == {"from", "to"} for edge in case["edges"])
                )
                self.assertTrue(
                    all(isinstance(root, str) and root for root in case["roots"])
                )
                expected = case["expected"]
                self.assertIn(expected["status"], {"valid", "invalid"})
                if expected["status"] == "valid":
                    canonical_nodes = sorted(set(case["nodes"]))
                    canonical_edges = sorted(
                        {(edge["from"], edge["to"]) for edge in case["edges"]}
                    )
                    self.assertEqual(expected["canonical_nodes"], canonical_nodes)
                    self.assertEqual(
                        [(edge["from"], edge["to"]) for edge in expected["canonical_edges"]],
                        canonical_edges,
                    )
                    self.assertEqual(
                        expected["measurements"]["node_count"], len(canonical_nodes)
                    )
                    self.assertEqual(
                        expected["measurements"]["edge_count"], len(canonical_edges)
                    )
                    self.assertEqual(expected["reachable"], sorted(expected["reachable"]))
                    self.assertEqual(
                        sorted(node for component in expected["components"] for node in component),
                        canonical_nodes,
                    )
                else:
                    self.assertTrue(expected["issues"])
                    self.assertEqual(set(expected), {"status", "issues"})

        for forbidden in (
            "octa",
            "factory",
            "job",
            "provider",
            "plugin",
            "command",
            "permission",
            "persistence",
            "scheduler",
            "retry",
            "budget",
        ):
            with self.subTest(forbidden=forbidden):
                self.assertNotIn(forbidden, self.corpus_text.lower())

    def test_factory_does_not_commit_to_an_octa_graph_dependency(self):
        manifests = (
            REPOSITORY / "Cargo.toml",
            REPOSITORY / "server/core/octacity-server-factory/Cargo.toml",
        )
        for manifest in manifests:
            contents = manifest.read_text(encoding="utf-8").lower()
            with self.subTest(manifest=manifest.relative_to(REPOSITORY)):
                self.assertNotIn("octa-dag", contents)
                self.assertNotIn("octa-graph", contents)


if __name__ == "__main__":
    unittest.main()
