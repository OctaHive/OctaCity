"""Tests for the pinned local-stand server image builder."""

from __future__ import annotations

import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest import mock


REPOSITORY = Path(__file__).resolve().parents[2]
MANIFEST = REPOSITORY / "deployment/local-stand/inputs.json"
sys.path.insert(0, str(REPOSITORY / "tools"))
SPEC = importlib.util.spec_from_file_location(
    "build_local_stand_server", REPOSITORY / "tools/build_local_stand_server.py"
)
assert SPEC and SPEC.loader
BUILDER = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = BUILDER
SPEC.loader.exec_module(BUILDER)


class LocalStandServerBuildTests(unittest.TestCase):
    def setUp(self):
        self.document = json.loads(MANIFEST.read_text(encoding="utf-8"))

    @unittest.skipUnless(os.environ.get("OCTACITY_TEST_DOCKER_BUILDS") == "1",
                         "requires Docker and the pinned ARM64 builder images")
    def test_rust_builders_use_the_image_toolchain_without_installing_development_components(self):
        """Exercise the real build stages offline with dependency-free Rust packages."""
        guest = json.loads((REPOSITORY / "config/runtime/microsandbox-agentd-v1/inputs.json").read_text())
        with tempfile.TemporaryDirectory(prefix="octacity-rust-builder-test-") as temporary:
            context = Path(temporary)
            octa = context / "octa"
            octa.mkdir()
            (octa / "README").write_text("No Octa dependencies are needed for this build fixture.\n")
            shutil.copyfile(REPOSITORY / "rust-toolchain.toml", context / "rust-toolchain.toml")
            (context / "Cargo.toml").write_text(
                '[workspace]\nmembers = ["controls", "api"]\nresolver = "2"\n')
            (context / "Cargo.lock").write_text(
                'version = 4\n\n[[package]]\nname = "octacity-lifecycle"\nversion = "0.1.0"\n'
                '\n[[package]]\nname = "octacity-server-api-rest"\nversion = "0.1.0"\n')
            controls = context / "controls"
            controls.mkdir()
            (controls / "Cargo.toml").write_text(
                '[package]\nname = "octacity-lifecycle"\nversion = "0.1.0"\nedition = "2021"\n'
                '\n[[bin]]\nname = "octacity-protected-reader"\npath = "main.rs"\n'
                '\n[[bin]]\nname = "octacity-codex-reader"\npath = "main.rs"\n')
            (controls / "main.rs").write_text('fn main() { println!("protected build fixture"); }\n')
            api = context / "api"
            (api / "examples").mkdir(parents=True)
            (api / "Cargo.toml").write_text(
                '[package]\nname = "octacity-server-api-rest"\nversion = "0.1.0"\nedition = "2021"\n')
            (api / "examples/export_openapi.rs").write_text('fn main() { println!("{{}}"); }\n')
            for target in ("factory-control-builder", "openapi-builder"):
                with self.subTest(target=target):
                    result = subprocess.run([
                        "docker", "build", "--platform", "linux/arm64", "--network", "none",
                        "--progress", "plain", "--file", str(REPOSITORY / "deployment/local-stand/server.Dockerfile"),
                        "--target", target, "--build-context", f"octa-source={octa}",
                        "--build-arg", f"GUEST_RUST_IMAGE={guest['builders']['aarch64']['image']}",
                        "--build-arg", f"RUST_IMAGE={self.document['images']['rust']['reference']}", str(context),
                    ], capture_output=True, text=True, timeout=180)
                    self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_octa_source_policy_uses_the_pinned_archive_and_bounds(self):
        octa = self.document["native"]["octa"]
        source = BUILDER.octa_source_policy(self.document)

        self.assertEqual(source["archive_url"], octa["source_archive_url"])
        self.assertEqual(source["sha256"], octa["source_archive_sha256"])
        self.assertEqual(source["tag"], octa["source_revision"])
        self.assertGreater(source["archive_max_bytes"], 0)
        self.assertGreater(source["expanded_max_bytes"], source["archive_max_bytes"])
        self.assertGreater(source["file_max_bytes"], 0)
        self.assertGreater(source["member_max_count"], 0)

    def test_docker_build_uses_the_verified_named_context_and_locked_inputs(self):
        octa_source = Path("/verified/octa-source")
        octacity_source = Path("/verified/octacity-source")
        with mock.patch.object(BUILDER, "workspace_version", return_value="0.1.0"):
            arguments = BUILDER.docker_arguments(
                self.document,
                "octacity/server:test",
                octa_source,
                octacity_source,
                "a" * 40,
            )
        command = "\n".join(arguments)

        self.assertIn("linux/arm64", arguments)
        self.assertIn(f"octa-source={octa_source.resolve()}", arguments)
        self.assertEqual(arguments[-1], str(octacity_source))
        self.assertIn("OCTACITY_REVISION=" + "a" * 40, arguments)
        self.assertIn(
            "RUST_IMAGE=" + self.document["images"]["rust"]["reference"],
            arguments,
        )
        self.assertIn(
            "ALPINE_IMAGE=" + self.document["images"]["alpine"]["reference"],
            arguments,
        )
        self.assertTrue(any(item.startswith("GUEST_RUST_IMAGE=docker.io/library/rust:1.99.0-alpine3.24@sha256:") for item in arguments),
                        "the server packages native Linux guest controls from a pinned static builder")
        self.assertIn(
            "OCTA_REVISION=" + self.document["native"]["octa"]["source_revision"],
            arguments,
        )
        self.assertNotIn(str(REPOSITORY.parent / "octa"), command)

    def test_dockerfile_builds_locked_and_copies_only_a_minimal_runtime_root(self):
        dockerfile = (
            REPOSITORY / "deployment/local-stand/server.Dockerfile"
        ).read_text(encoding="utf-8")

        self.assertIn("COPY --from=octa-source", dockerfile)
        self.assertIn("ARG RUST_IMAGE=scratch", dockerfile)
        self.assertIn("cargo build --locked --release", dockerfile)
        self.assertIn("ldd \"$binary\"", dockerfile)
        self.assertIn("FROM ${ALPINE_IMAGE} AS server", dockerfile)
        self.assertIn("COPY --from=server-builder /out/rootfs /", dockerfile)
        self.assertIn("server-entrypoint.sh", dockerfile)
        self.assertIn("USER 65532:65532", dockerfile)
        self.assertNotIn("target-feature=+crt-static", dockerfile)
        self.assertNotIn("docker/dockerfile:", dockerfile)

    def test_reuse_rejects_an_image_without_the_vcs_adapter(self):
        import subprocess
        results = [
            subprocess.CompletedProcess([], 0, json.dumps({"User": "65532:65532", "Labels": {
                "org.opencontainers.image.version": "0.1.0", "org.opencontainers.image.revision": "a" * 40}})),
            subprocess.CompletedProcess([], 0, "octacity-server 0.1.0"),
            subprocess.CompletedProcess([], 0, "server configuration is valid"),
            subprocess.CompletedProcess([], 0, '{"manual_model":true,"models":[]}'),
            subprocess.CalledProcessError(127, ["docker", "run"]),
        ]
        with mock.patch.object(BUILDER.subprocess, "run", side_effect=results):
            with self.assertRaises(subprocess.CalledProcessError):
                BUILDER.verify_image("local:test", Path("fixture.toml"), "0.1.0", "a" * 40)

    def test_reuse_rejects_an_image_without_the_model_catalog_plugin(self):
        import subprocess
        results = [
            subprocess.CompletedProcess([], 0, json.dumps({"User":"65532:65532", "Labels":{
                "org.opencontainers.image.version":"0.1.0", "org.opencontainers.image.revision":"a"*40}})),
            subprocess.CompletedProcess([], 0, "octacity-server 0.1.0"),
            subprocess.CompletedProcess([], 0, "server configuration is valid"),
            subprocess.CalledProcessError(127, ["docker", "run"]),
        ]
        with mock.patch.object(BUILDER.subprocess, "run", side_effect=results):
            with self.assertRaises(subprocess.CalledProcessError):
                BUILDER.verify_image("local:test",Path("fixture.toml"),"0.1.0","a"*40)

    def test_runtime_user_must_be_an_explicit_non_root_numeric_identity(self):
        self.assertEqual(BUILDER.validate_runtime_user("65532:65532"), (65532, 65532))
        for value in ("", "root", "0", "0:0", "65532:0", "app:app"):
            with self.subTest(value=value):
                with self.assertRaises(BUILDER.LocalStandBuildError):
                    BUILDER.validate_runtime_user(value)

    def test_reuse_rejects_a_server_without_the_packaged_guest_controller(self):
        import subprocess
        results = [
            subprocess.CompletedProcess([], 0, json.dumps({"User": "65532:65532", "Labels": {
                "org.opencontainers.image.version": "0.1.0", "org.opencontainers.image.revision": "a" * 40}})),
            subprocess.CompletedProcess([], 0, "octacity-server 0.1.0"),
            subprocess.CompletedProcess([], 0, "server configuration is valid"),
            subprocess.CompletedProcess([], 0, '{"manual_model":true,"models":[]}'),
            subprocess.CompletedProcess([], 0, "b" * 64 + "  /usr/local/lib/octacity/vcs/git/adapter\n"),
            subprocess.CalledProcessError(1, ["docker", "run"]),
        ]
        with mock.patch.object(BUILDER.subprocess, "run", side_effect=results):
            with self.assertRaises(subprocess.CalledProcessError):
                BUILDER.verify_image("local:test", Path("fixture.toml"), "0.1.0", "a" * 40)

    def test_verification_runs_version_and_configuration_without_network_or_writes(self):
        with tempfile.TemporaryDirectory() as temporary:
            fixture = Path(temporary) / "server.toml"
            fixture.write_text("fixture", encoding="utf-8")
            version, validate = BUILDER.verification_commands(
                "octacity/server:test", fixture
            )

        for command in (version, validate):
            self.assertIn("--network", command)
            self.assertIn("none", command)
            self.assertIn("--read-only", command)
            self.assertIn("linux/arm64", command)
        self.assertEqual(version[-1], "--version")
        self.assertEqual(validate[-3:], ["validate", "--syntax-only", "/fixtures/server.toml"])
        self.assertIn(
            f"type=bind,src={fixture.resolve()},dst=/fixtures/server.toml,readonly",
            validate,
        )


if __name__ == "__main__":
    unittest.main()
