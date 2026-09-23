"""Tests for the Cargo workspace architecture policy."""

from __future__ import annotations

import importlib.util
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


REPOSITORY = Path(__file__).resolve().parents[2]
TOOL = REPOSITORY / "tools/check_architecture.py"
FIXTURES = Path(__file__).resolve().parent / "fixtures/architecture"
SPEC = importlib.util.spec_from_file_location("check_architecture", TOOL)
assert SPEC and SPEC.loader
ARCHITECTURE = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = ARCHITECTURE
SPEC.loader.exec_module(ARCHITECTURE)


class ArchitecturePolicyTests(unittest.TestCase):
    def test_current_workspace_satisfies_the_policy(self):
        graph = ARCHITECTURE.graph_from_metadata(ARCHITECTURE.cargo_metadata(REPOSITORY))
        self.assertEqual(
            ARCHITECTURE.check(graph) + ARCHITECTURE.check_shared_sources(graph),
            [],
        )

    def test_fixture_graphs_have_the_expected_violations(self):
        for path in sorted(FIXTURES.glob("*.json")):
            with self.subTest(path=path.name):
                metadata = json.loads(path.read_text(encoding="utf-8"))
                expected = metadata.pop("expected_codes")
                graph = ARCHITECTURE.graph_from_metadata(metadata)
                actual = [violation.code for violation in ARCHITECTURE.check(graph)]
                self.assertEqual(actual, expected)

    def test_cli_rejects_every_intentionally_invalid_fixture(self):
        for path in sorted(FIXTURES.glob("invalid-*.json")):
            with self.subTest(path=path.name):
                metadata = json.loads(path.read_text(encoding="utf-8"))
                completed = subprocess.run(
                    [sys.executable, str(TOOL), "--metadata", str(path)],
                    check=False,
                    capture_output=True,
                    text=True,
                )
                self.assertEqual(completed.returncode, 1, completed.stderr)
                for code in metadata["expected_codes"]:
                    self.assertIn(code, completed.stderr)

    def test_shared_source_representations_are_rejected(self):
        invalid_sources = {
            "rest DTO": "pub struct ProjectRestResponse;",
            "SQL row": "#[derive(sqlx::FromRow)]\npub struct StoredThing;",
            "provider payload": "pub struct GitHubPushPayload;",
            "server domain entity": "pub struct Build;",
        }
        for label, source in invalid_sources.items():
            with self.subTest(label=label), tempfile.TemporaryDirectory() as temporary_directory:
                workspace = Path(temporary_directory)
                package_root = workspace / "shared/fixture-shared"
                (package_root / "src").mkdir(parents=True)
                (package_root / "src/lib.rs").write_text(source, encoding="utf-8")
                metadata = {
                    "workspace_root": str(workspace),
                    "packages": [
                        {
                            "name": "fixture-shared",
                            "manifest_path": str(package_root / "Cargo.toml"),
                            "metadata": {
                                "octacity": {
                                    "shared": {
                                        "contract": "fixture-contract",
                                        "consumers": ["agent", "server"],
                                    }
                                }
                            },
                            "dependencies": [],
                        }
                    ],
                }
                graph = ARCHITECTURE.graph_from_metadata(metadata)
                self.assertEqual(
                    [violation.code for violation in ARCHITECTURE.check_shared_sources(graph)],
                    ["ARCH008_SHARED_REPRESENTATION"],
                )

    def test_only_named_infrastructure_support_packages_may_be_shared_by_adapters(self):
        def package(name: str) -> ARCHITECTURE.Package:
            return ARCHITECTURE.Package(
                name=name,
                manifest_path=REPOSITORY / name / "Cargo.toml",
                role="infrastructure",
                dependencies=(),
                external_dependencies=(),
                shared_contract=None,
                shared_consumers=(),
                shared_scaffold=False,
                shared_policy_valid=True,
            )

        source = package("fixture-adapter")
        support = package("octacity-server-adapter-host")
        concrete = package("fixture-concrete-adapter")
        graph = ARCHITECTURE.Graph(
            packages=(source, support, concrete),
            edges=(
                ARCHITECTURE.Edge(source=source, target=support, kind="normal"),
                ARCHITECTURE.Edge(source=source, target=concrete, kind="normal"),
            ),
        )

        violations = ARCHITECTURE.check(graph)
        self.assertEqual([violation.code for violation in violations], ["ARCH003_ADAPTER_SELECTION"])
        self.assertIn(concrete.name, violations[0].message)

if __name__ == "__main__":
    unittest.main()
