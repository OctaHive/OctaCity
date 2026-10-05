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
CONSOLE_NGINX = REPOSITORY / "deployment/console/nginx"
CONSOLE_ROUTES = CONSOLE_NGINX / "routes.conf"
sys.path.insert(0, str(REPOSITORY / "tools"))
SPEC = importlib.util.spec_from_file_location(
    "build_local_stand_gateway", REPOSITORY / "tools/build_local_stand_gateway.py"
)
assert SPEC and SPEC.loader
BUILDER = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = BUILDER
SPEC.loader.exec_module(BUILDER)


def nginx_server(config: str, name: str) -> str:
    """Return one named nginx server block from the checked-in configuration."""

    marker = f"server_name {name};"
    marker_index = config.index(marker)
    start = config.rfind("server {", 0, marker_index)
    depth = 0
    for index in range(start, len(config)):
        if config[index] == "{":
            depth += 1
        elif config[index] == "}":
            depth -= 1
            if depth == 0:
                return config[start : index + 1]
    raise AssertionError(f"unterminated nginx server block for {name}")


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
        for name in ("cache-map.conf", "routes.conf", "security-headers.conf"):
            self.assertIn(
                f"COPY deployment/console/nginx/{name} "
                f"/etc/nginx/octacity-console/{name}",
                dockerfile,
            )
        self.assertNotIn("COPY deployment/console/nginx/nginx.conf", dockerfile)
        gateway = dockerfile.split("FROM ${NGINX_IMAGE} AS gateway", 1)[1]
        self.assertIn("USER 101:101", gateway)
        self.assertIn("EXPOSE 8443", gateway)
        self.assertNotIn("node_modules", gateway)
        self.assertNotIn("COPY ui/", gateway)

    def test_runtime_contract_rejects_build_inputs_and_development_tools(self):
        command = BUILDER.runtime_contract_command(
            "octacity/gateway:test",
            Path("/verified/gateway.pem"),
            Path("/verified/gateway-key.pem"),
        )
        contract = command[-1]

        self.assertIn("--network", command)
        self.assertIn("none", command)
        self.assertIn("--read-only", command)
        self.assertIn("--tmpfs", command)
        self.assertIn("server:127.0.0.1", command)
        self.assertIn("minio:127.0.0.1", command)
        self.assertIn("objects.localhost:127.0.0.1", command)
        self.assertIn("/srv/octacity-ui/index.html", contract)
        self.assertIn("https://octacity.localhost:8443/", contract)
        self.assertIn("cmp /tmp/index.html /srv/octacity-ui/index.html", contract)
        self.assertIn("-name '*.js'", contract)
        self.assertIn("application/javascript*|text/javascript*", contract)
        self.assertIn('cmp /tmp/javascript "$javascript"', contract)
        self.assertIn("/api/v1/management-only", contract)
        self.assertIn("/api/v1/unknown", contract)
        self.assertIn("/builds/example-build", contract)
        self.assertIn("/health/unknown", contract)
        self.assertIn("/health/ready", contract)
        self.assertIn("/missing.js", contract)
        self.assertIn("frame-ancestors 'none'", contract)
        self.assertIn("max-age=31536000, immutable", contract)
        self.assertIn("cmp /tmp/deep-link /srv/octacity-ui/index.html", contract)
        self.assertIn("! cmp -s /tmp/api-response /srv/octacity-ui/index.html", contract)
        self.assertIn("! cmp -s /tmp/health-response /srv/octacity-ui/index.html", contract)
        self.assertIn(
            "objects.localhost:8443|/bucket/key%%2Fsegment?partNumber=7&X-Amz-SignedHeaders=host",
            contract,
        )
        for excluded in (
            "test ! -e /workspace",
            "test ! -e /pnpm",
            "package.json",
            "pnpm-lock.yaml",
        ):
            self.assertIn(excluded, contract)
        for executable in ("node", "pnpm", "vite"):
            self.assertIn(f"! command -v {executable}", contract)

    def test_nginx_uses_four_tls_hosts_and_one_console_spa_fallback(self):
        config = (REPOSITORY / "deployment/local-stand/nginx.conf").read_text(
            encoding="utf-8"
        )

        self.assertIn("ssl_certificate /run/secrets/gateway.pem", config)
        self.assertIn("ssl_certificate_key /run/secrets/gateway-key.pem", config)
        self.assertIn("include /etc/nginx/mime.types;", config)
        self.assertIn("default_type application/octet-stream;", config)
        self.assertIn("proxy_set_header Host $http_host", config)
        for host in (
            "octacity.localhost",
            "agent.localhost",
            "cache.localhost",
            "objects.localhost",
        ):
            self.assertIn("listen 8443 ssl", nginx_server(config, host))
        console = nginx_server(config, "octacity.localhost")
        self.assertIn(
            "include /etc/nginx/octacity-console/security-headers.conf", console
        )
        self.assertIn("include /etc/nginx/octacity-console/routes.conf", console)
        routes = CONSOLE_ROUTES.read_text(encoding="utf-8")
        self.assertIn("try_files $uri $uri/ /index.html", routes)
        self.assertEqual(routes.count("try_files $uri $uri/ /index.html"), 1)
        self.assertIn("pid /tmp/nginx.pid", config)

    def test_access_log_never_records_presigned_query_credentials(self):
        config = (REPOSITORY / "deployment/local-stand/nginx.conf").read_text(
            encoding="utf-8"
        )

        self.assertIn("log_format safe", config)
        self.assertIn('"$request_method $uri $server_protocol"', config)
        self.assertIn("access_log /dev/stdout safe", config)
        log_format = config.split("log_format safe", 1)[1].split(";", 1)[0]
        for sensitive_variable in ("$request_uri", "$request ", "$args"):
            self.assertNotIn(sensitive_variable, log_format)

        # Nginx warning and error messages can embed its internal $request and
        # upstream URL regardless of the access-log format. Credential-bearing
        # virtual hosts therefore emit only process-critical diagnostics.
        for host in ("cache.localhost", "objects.localhost"):
            self.assertIn("error_log /dev/stderr crit;", nginx_server(config, host))

    def test_console_api_and_health_prefixes_always_reach_management(self):
        config = (REPOSITORY / "deployment/local-stand/nginx.conf").read_text(
            encoding="utf-8"
        )
        console = nginx_server(config, "octacity.localhost")
        routes = CONSOLE_ROUTES.read_text(encoding="utf-8")

        for location in (
            "location = /api/v1",
            "location ^~ /api/v1/",
            "location = /health",
            "location ^~ /health/",
        ):
            self.assertIn(location, routes)
        self.assertIn("location = /metrics", console)
        self.assertEqual(routes.count("proxy_pass http://octacity_management;"), 4)
        self.assertEqual(console.count("proxy_pass http://octacity_management;"), 1)
        self.assertNotIn("return 404", console)

    def test_protocol_and_object_hosts_cannot_reach_management_listener(self):
        config = (REPOSITORY / "deployment/local-stand/nginx.conf").read_text(
            encoding="utf-8"
        )
        expected_upstreams = {
            "agent.localhost": "http://server:8081",
            "cache.localhost": "http://server:8082",
            "objects.localhost": "http://minio:9000",
        }
        for host, upstream in expected_upstreams.items():
            block = nginx_server(config, host)
            self.assertIn(f"proxy_pass {upstream};", block)
            self.assertNotIn("server:8080", block)
            self.assertNotIn("try_files", block)
        objects = nginx_server(config, "objects.localhost")
        self.assertNotIn("rewrite", objects)
        self.assertNotIn("$request_uri", objects)
        self.assertNotIn("proxy_pass http://minio:9000/;", objects)


if __name__ == "__main__":
    unittest.main()
