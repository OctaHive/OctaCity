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


def management_security_violation_codes(relative_path: str, source: str) -> list[str]:
    with tempfile.TemporaryDirectory() as temporary_directory:
        workspace = Path(temporary_directory)
        source_path = workspace / relative_path
        source_path.parent.mkdir(parents=True)
        source_path.write_text(source, encoding="utf-8")
        return [
            violation.code
            for violation in ARCHITECTURE.check_management_security_sources(workspace)
        ]


def package_fixture(
    name: str,
    manifest_path: Path,
    *,
    dependencies: tuple[str, ...] = (),
    external_dependencies: tuple[str, ...] = (),
) -> ARCHITECTURE.Package:
    return ARCHITECTURE.Package(
        name=name,
        manifest_path=manifest_path,
        role="core",
        dependencies=dependencies,
        external_dependencies=external_dependencies,
        shared_contract=None,
        shared_consumers=(),
        shared_scaffold=False,
        shared_policy_valid=True,
    )


class ArchitecturePolicyTests(unittest.TestCase):
    def test_current_workspace_satisfies_the_policy(self):
        graph = ARCHITECTURE.graph_from_metadata(ARCHITECTURE.cargo_metadata(REPOSITORY))
        self.assertEqual(
            ARCHITECTURE.check(graph)
            + ARCHITECTURE.check_factory_graph(graph)
            + ARCHITECTURE.check_shared_sources(graph)
            + ARCHITECTURE.check_factory_core_sources(graph)
            + ARCHITECTURE.check_management_route_sources(REPOSITORY)
            + ARCHITECTURE.check_management_security_sources(REPOSITORY)
            + ARCHITECTURE.check_postgres_management_actor_sources(REPOSITORY),
            [],
        )

    def test_factory_core_rejects_provider_persistence_transport_and_ui_types(self):
        invalid_sources = {
            "provider type": "struct CodexProtocolRequest;",
            "provider import alias": "use jev_sdk::Client as SignalClient;\npub struct FactoryPolicy;",
            "SQL row": "#[derive(sqlx::FromRow)]\npub struct StoredFactory;",
            "HTTP DTO": "pub(crate) struct FactoryRestResponse;",
            "UI state": "struct FactoryUiState;",
        }
        for label, source in invalid_sources.items():
            with self.subTest(label=label), tempfile.TemporaryDirectory() as temporary_directory:
                package_root = Path(temporary_directory) / "server/core/octacity-server-factory"
                (package_root / "src").mkdir(parents=True)
                (package_root / "src/lib.rs").write_text(source, encoding="utf-8")
                package = package_fixture(
                    ARCHITECTURE.FACTORY_CORE_PACKAGE,
                    package_root / "Cargo.toml",
                )

                violations = ARCHITECTURE.check_factory_core_sources(
                    ARCHITECTURE.Graph(packages=(package,), edges=())
                )

                self.assertEqual(
                    [violation.code for violation in violations],
                    ["ARCH018_FACTORY_CORE_COUPLING"],
                )

        with tempfile.TemporaryDirectory() as temporary_directory:
            package_root = Path(temporary_directory) / "server/core/octacity-server-factory"
            (package_root / "src").mkdir(parents=True)
            (package_root / "src/lib.rs").write_text(
                "pub struct FactoryPolicy;\n"
                "#[cfg(test)]\nmod tests { struct CodexProtocolRequest; }\n",
                encoding="utf-8",
            )
            (package_root / "src/policy_tests.rs").write_text(
                "struct FactoryUiState;\n",
                encoding="utf-8",
            )
            package = package_fixture(
                ARCHITECTURE.FACTORY_CORE_PACKAGE,
                package_root / "Cargo.toml",
            )

            self.assertEqual(
                ARCHITECTURE.check_factory_core_sources(
                    ARCHITECTURE.Graph(packages=(package,), edges=())
                ),
                [],
            )

    def test_factory_core_rejects_provider_sdks_and_codex_contract_dependencies(self):
        package = package_fixture(
            ARCHITECTURE.FACTORY_CORE_PACKAGE,
            REPOSITORY / ARCHITECTURE.FACTORY_CORE_SOURCE.parent / "Cargo.toml",
            dependencies=("aws-sdk-s3", "octa-plugin-codex-protocol", "sqlx"),
            external_dependencies=("aws-sdk-s3", "sqlx"),
        )
        graph = ARCHITECTURE.Graph(packages=(package,), edges=())

        self.assertEqual(
            [violation.code for violation in ARCHITECTURE.check(graph)],
            ["ARCH010_LAYER_IMPLEMENTATION_DEPENDENCY"],
        )
        self.assertEqual(
            [violation.code for violation in ARCHITECTURE.check_factory_graph(graph)],
            ["ARCH018_FACTORY_CORE_COUPLING"],
        )

        provider_package = package_fixture(
            ARCHITECTURE.FACTORY_CORE_PACKAGE,
            REPOSITORY / ARCHITECTURE.FACTORY_CORE_SOURCE.parent / "Cargo.toml",
            dependencies=("jev-sdk",),
            external_dependencies=("jev-sdk",),
        )
        self.assertEqual(
            [
                violation.code
                for violation in ARCHITECTURE.check_factory_graph(
                    ARCHITECTURE.Graph(packages=(provider_package,), edges=())
                )
            ],
            ["ARCH018_FACTORY_CORE_COUPLING"],
        )

    def test_build_and_job_lifecycle_cores_cannot_depend_on_factory(self):
        def package(name: str) -> ARCHITECTURE.Package:
            return package_fixture(
                name,
                REPOSITORY / "server/core" / name / "Cargo.toml",
            )

        factory = package(ARCHITECTURE.FACTORY_CORE_PACKAGE)
        for name in sorted(ARCHITECTURE.FACTORY_INDEPENDENT_LIFECYCLE_PACKAGES):
            with self.subTest(package=name):
                source = package(name)
                graph = ARCHITECTURE.Graph(
                    packages=(factory, source),
                    edges=(ARCHITECTURE.Edge(source=source, target=factory, kind="normal"),),
                )
                self.assertEqual(
                    [violation.code for violation in ARCHITECTURE.check_factory_graph(graph)],
                    ["ARCH019_FACTORY_EXECUTION_DEPENDENCY"],
                )

    def test_factory_core_cannot_be_an_empty_or_duplicate_seam(self):
        duplicate = package_fixture(
            "octacity-server-factory-evaluation",
            REPOSITORY / "server/core/octacity-server-factory-evaluation/Cargo.toml",
        )
        self.assertEqual(
            [
                violation.code
                for violation in ARCHITECTURE.check_factory_graph(
                    ARCHITECTURE.Graph(packages=(duplicate,), edges=())
                )
            ],
            ["ARCH020_FACTORY_MODULE_SHAPE"],
        )

        invalid_sources = {
            "empty": "//! Placeholder Factory seam.\n",
            "duplicate lifecycle": "pub struct Build;\n",
        }
        for label, source in invalid_sources.items():
            with self.subTest(label=label), tempfile.TemporaryDirectory() as temporary_directory:
                package_root = Path(temporary_directory) / "server/core/octacity-server-factory"
                (package_root / "src").mkdir(parents=True)
                (package_root / "src/lib.rs").write_text(source, encoding="utf-8")
                factory = package_fixture(
                    ARCHITECTURE.FACTORY_CORE_PACKAGE,
                    package_root / "Cargo.toml",
                )

                violations = ARCHITECTURE.check_factory_core_sources(
                    ARCHITECTURE.Graph(packages=(factory,), edges=())
                )

                self.assertEqual(
                    [violation.code for violation in violations],
                    ["ARCH020_FACTORY_MODULE_SHAPE"],
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

    def test_management_routes_cannot_bypass_the_typed_registry(self):
        invalid_sources = {
            "v1 direct route": (
                "server/api/octacity-server-api-rest/src/v1/bypass.rs",
                'router.route("/api/v1/shadow", handler);',
            ),
            "top-level management route": (
                "server/api/octacity-server-api-rest/src/lib.rs",
                'Router::new().route("/api/v1/shadow", handler);',
            ),
            "management prefix outside owner": (
                "server/api/octacity-server-api-rest/src/lib.rs",
                'let path = format!("{API_PREFIX}/shadow");',
            ),
        }
        for label, (relative_path, source) in invalid_sources.items():
            with self.subTest(label=label), tempfile.TemporaryDirectory() as temporary_directory:
                workspace = Path(temporary_directory)
                source_path = workspace / relative_path
                source_path.parent.mkdir(parents=True)
                source_path.write_text(source, encoding="utf-8")

                violations = ARCHITECTURE.check_management_route_sources(workspace)

                self.assertEqual(
                    {violation.code for violation in violations},
                    {"ARCH013_UNTYPED_MANAGEMENT_ROUTE"},
                )

    def test_postgres_mutations_cannot_select_management_actors(self):
        invalid_sources = {
            "literal": 'let actor_kind = "unauthenticated_management";',
            "enum": "let actor = AuditActorKind::AuthenticatedManagement;",
        }
        for label, source in invalid_sources.items():
            with self.subTest(label=label), tempfile.TemporaryDirectory() as temporary_directory:
                workspace = Path(temporary_directory)
                source_path = workspace / ARCHITECTURE.POSTGRES_STORE_SOURCE / "mutation.rs"
                source_path.parent.mkdir(parents=True)
                source_path.write_text(source, encoding="utf-8")

                violations = ARCHITECTURE.check_postgres_management_actor_sources(workspace)

                self.assertEqual(
                    [violation.code for violation in violations],
                    ["ARCH014_STORE_SELECTED_MANAGEMENT_ACTOR"],
                )

    def test_postgres_adapters_cannot_construct_untyped_mutation_facts(self):
        with tempfile.TemporaryDirectory() as temporary_directory:
            workspace = Path(temporary_directory)
            source_path = workspace / ARCHITECTURE.POSTGRES_STORE_SOURCE / "adapter.rs"
            source_path.parent.mkdir(parents=True)
            source_path.write_text("let facts = MutationFacts { actor_kind };", encoding="utf-8")

            violations = ARCHITECTURE.check_postgres_management_actor_sources(workspace)

            self.assertEqual(
                [violation.code for violation in violations],
                ["ARCH014_STORE_SELECTED_MANAGEMENT_ACTOR"],
            )

    def test_rest_and_infrastructure_cannot_own_management_authorization(self):
        invalid_sources = {
            "REST policy": (
                "server/api/octacity-server-api-rest/src/policy.rs",
                "impl ManagementAuthorizationPolicy for RestPolicy {}",
            ),
            "infrastructure decision": (
                "server/infrastructure/fixture-adapter/src/lib.rs",
                "policy.authorize(&context, action, &resource).await;",
            ),
        }
        for label, (relative_path, source) in invalid_sources.items():
            with self.subTest(label=label):
                self.assertEqual(
                    management_security_violation_codes(relative_path, source),
                    ["ARCH015_AUTHORIZATION_OUTSIDE_APPLICATION"],
                )

    def test_management_application_cannot_assemble_raw_use_cases(self):
        for seam in ("ManagementCommandUseCase", "ManagementQueryUseCase"):
            with self.subTest(seam=seam):
                self.assertEqual(
                    management_security_violation_codes(
                        "server/app/src/runtime/application.rs",
                        f"let handler: Arc<dyn {seam}<Request>> = undecorated;",
                    ),
                    ["ARCH016_UNDECORATED_MANAGEMENT_HANDLER"],
                )

    def test_management_context_cannot_contain_raw_credential_fields(self):
        for field in (
            "authorization_header",
            "bearer_token",
            "cookie",
            "password",
            "client_certificate",
            "provider_claims",
            "raw_credential",
        ):
            with self.subTest(field=field):
                self.assertEqual(
                    management_security_violation_codes(
                        "server/application/src/management_security/context.rs",
                        f"pub struct ManagementRequestContext {{ pub {field}: String }}",
                    ),
                    ["ARCH017_RAW_MANAGEMENT_CREDENTIAL"],
                )

    def test_cfg_test_security_fixtures_do_not_change_production_policy(self):
        with tempfile.TemporaryDirectory() as temporary_directory:
            workspace = Path(temporary_directory)
            source_path = workspace / "server/api/octacity-server-api-rest/src/lib.rs"
            source_path.parent.mkdir(parents=True)
            source_path.write_text(
                "#[cfg(test)]\nmod tests { impl ManagementAuthorizationPolicy for FixturePolicy {} }\n"
                "fn expose(handler: &dyn ManagementQueryUseCase<Request>) {}",
                encoding="utf-8",
            )
            (source_path.parent / "tests.rs").write_text(
                "impl ManagementAuthorizationPolicy for StandaloneFixturePolicy {}",
                encoding="utf-8",
            )

            violations = ARCHITECTURE.check_management_security_sources(workspace)
            self.assertEqual(
                [violation.code for violation in violations],
                ["ARCH016_UNDECORATED_MANAGEMENT_HANDLER"],
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

    def test_agent_provisioning_protocol_is_isolated_from_capacity_interfaces(self):
        graph = ARCHITECTURE.graph_from_metadata(ARCHITECTURE.cargo_metadata(REPOSITORY))
        provisioning = "octacity-agent-provisioning-protocol"
        capacity_interfaces = {
            "octacity-protocol",
            "octacity-server-domain",
            "octacity-server-job",
            "octacity-server-orchestrator",
            "octacity-server-scheduler",
        }
        leaking_edges = [
            edge
            for edge in graph.edges
            if (
                edge.source.name == provisioning
                and edge.target.name in capacity_interfaces
            )
            or (
                edge.target.name == provisioning
                and edge.source.name in capacity_interfaces
            )
        ]

        self.assertEqual(
            leaking_edges,
            [],
            "dynamic capacity protocol types must not enter JobSpec, Agent, Pool, Orchestrator, or Placement interfaces",
        )

    def test_first_release_has_no_dynamic_provisioning_runtime(self):
        graph = ARCHITECTURE.graph_from_metadata(ARCHITECTURE.cargo_metadata(REPOSITORY))
        provisioning = "octacity-agent-provisioning-protocol"
        consumers = [
            edge.source.name for edge in graph.edges if edge.target.name == provisioning
        ]
        provider_packages = [
            package.name
            for package in graph.packages
            if package.name != provisioning
            and any(
                marker in package.name
                for marker in ("agent-provision", "proxmox", "vsphere")
            )
        ]

        self.assertEqual(
            consumers,
            [],
            "the v1 runtime must not load an agent-provisioning host or reconciliation loop",
        )
        self.assertEqual(
            provider_packages,
            [],
            "the v1 workspace must not ship a production dynamic-provisioning adapter",
        )

    def test_deferred_extension_sdks_are_rejected_in_implementation_layers(self):
        package = ARCHITECTURE.Package(
            name="octacity-server",
            manifest_path=REPOSITORY / "server/app/Cargo.toml",
            role="composition",
            dependencies=("async-graphql", "octocrab"),
            external_dependencies=("async-graphql", "octocrab"),
            shared_contract=None,
            shared_consumers=(),
            shared_scaffold=False,
            shared_policy_valid=True,
        )

        violations = ARCHITECTURE.check(
            ARCHITECTURE.Graph(packages=(package,), edges=())
        )

        self.assertEqual(
            [violation.code for violation in violations],
            [
                "ARCH011_DEFERRED_EXTENSION_SDK",
                "ARCH011_DEFERRED_EXTENSION_SDK",
            ],
        )
        self.assertIn("GraphQL", violations[0].message)
        self.assertIn("managed GitHub/Gerrit", violations[1].message)

    def test_unknown_sdk_is_rejected_where_concrete_implementations_are_selected(self):
        package = ARCHITECTURE.Package(
            name="fixture-adapter",
            manifest_path=REPOSITORY / "server/infrastructure/fixture-adapter/Cargo.toml",
            role="infrastructure",
            dependencies=("future-provider-sdk",),
            external_dependencies=("future-provider-sdk",),
            shared_contract=None,
            shared_consumers=(),
            shared_scaffold=False,
            shared_policy_valid=True,
        )

        violations = ARCHITECTURE.check(
            ARCHITECTURE.Graph(packages=(package,), edges=())
        )

        self.assertEqual(
            [violation.code for violation in violations],
            ["ARCH012_UNREVIEWED_EXTERNAL_DEPENDENCY"],
        )

if __name__ == "__main__":
    unittest.main()
