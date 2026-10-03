"""Tests for the pinned local-stand gateway image builder."""

from __future__ import annotations

import importlib.util
import json
from pathlib import Path
import sys
import unittest
from unittest import mock


REPOSITORY = Path(__file__).resolve().parents[2]
MANIFEST = REPOSITORY / "deployment/local-stand/inputs.json"
sys.path.insert(0, str(REPOSITORY / "tools"))
SPEC = importlib.util.spec_from_file_location(
    "build_local_stand_gateway", REPOSITORY / "tools/build_local_stand_gateway.py"
)
assert SPEC and SPEC.loader
BUILDER = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = BUILDER
SPEC.loader.exec_module(BUILDER)


class LocalStandGatewayBuildTests(unittest.TestCase):
    def setUp(self):
        self.document = json.loads(MANIFEST.read_text(encoding="utf-8"))

    def test_ui_toolchain_matches_package_node_and_dependency_script_policy(self):
        self.assertEqual(
            BUILDER.ui_toolchain(REPOSITORY),
            ("24.21.0", "12.8.1"),
        )

    def test_docker_build_uses_only_pinned_arm64_inputs(self):
        octa_source = Path("/verified/octa-source")
        octacity_source = Path("/verified/octacity-source")
        with (
            mock.patch.object(BUILDER, "workspace_version", return_value="0.1.0"),
            mock.patch.object(
                BUILDER, "ui_toolchain", return_value=("24.21.0", "12.8.1")
            ),
        ):
            arguments = BUILDER.docker_arguments(
                self.document,
                "octacity/gateway:test",
                octa_source,
                octacity_source,
                "a" * 40,
            )

        self.assertIn("linux/arm64", arguments)
        self.assertIn("gateway", arguments)
        self.assertIn(f"octa-source={octa_source.resolve()}", arguments)
        self.assertEqual(arguments[-1], str(octacity_source))
        self.assertIn("OCTACITY_REVISION=" + "a" * 40, arguments)
        for role in ("rust", "node", "nginx"):
            self.assertIn(
                f"{role.upper()}_IMAGE={self.document['images'][role]['reference']}",
                arguments,
            )
        self.assertIn("NODE_VERSION=24.21.0", arguments)
        self.assertIn("PNPM_VERSION=12.8.1", arguments)

    def test_dockerfile_builds_locked_ui_and_copies_only_static_output(self):
        dockerfile = BUILDER.DOCKERFILE.read_text(encoding="utf-8")

        self.assertIn("FROM ${NODE_IMAGE} AS ui-builder", dockerfile)
        self.assertIn("corepack pnpm install --frozen-lockfile --ignore-scripts", dockerfile)
        self.assertIn("OCTACITY_OPENAPI_SCHEMA_FILE=/tmp/octacity-openapi.json", dockerfile)
        self.assertIn("corepack pnpm build", dockerfile)
        self.assertIn("FROM ${NGINX_IMAGE} AS gateway", dockerfile)
        self.assertIn(
            "COPY --from=ui-builder /workspace/ui/dist/ /srv/octacity-ui/",
            dockerfile,
        )
        self.assertIn(
            "COPY deployment/local-stand/nginx.conf /etc/nginx/nginx.conf",
            dockerfile,
        )
        gateway = dockerfile.split("FROM ${NGINX_IMAGE} AS gateway", 1)[1]
        self.assertIn("USER 101:101", gateway)
        self.assertIn("EXPOSE 8080", gateway)
        self.assertNotIn("node_modules", gateway)
        self.assertNotIn("COPY ui/", gateway)

    def test_runtime_contract_rejects_build_inputs_and_development_tools(self):
        command = BUILDER.runtime_contract_command("octacity/gateway:test")
        contract = command[-1]

        self.assertIn("--network", command)
        self.assertIn("none", command)
        self.assertIn("--read-only", command)
        self.assertIn("--tmpfs", command)
        self.assertIn("/srv/octacity-ui/index.html", contract)
        self.assertIn("http://127.0.0.1:8080/", contract)
        self.assertIn("cmp /tmp/index.html /srv/octacity-ui/index.html", contract)
        self.assertIn("http://127.0.0.1:8080/api/v1/unknown", contract)
        self.assertIn("http://127.0.0.1:8080/health/unknown", contract)
        for excluded in (
            "test ! -e /workspace",
            "test ! -e /pnpm",
            "package.json",
            "pnpm-lock.yaml",
        ):
            self.assertIn(excluded, contract)
        for executable in ("node", "pnpm", "vite"):
            self.assertIn(f"! command -v {executable}", contract)

    def test_nginx_serves_spa_routes_but_reserves_api_and_health_paths(self):
        config = (REPOSITORY / "deployment/local-stand/nginx.conf").read_text(
            encoding="utf-8"
        )

        self.assertIn("listen 8080", config)
        self.assertIn("try_files $uri $uri/ /index.html", config)
        self.assertIn("location ^~ /api/v1/", config)
        self.assertIn("location ^~ /health/", config)
        self.assertIn("pid /tmp/nginx.pid", config)


if __name__ == "__main__":
    unittest.main()
