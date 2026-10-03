"""Tests for the pinned local-stand server image builder."""

from __future__ import annotations

import importlib.util
import json
from pathlib import Path
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
        self.assertIn("FROM scratch AS server", dockerfile)
        self.assertIn("COPY --from=server-builder /out/rootfs /", dockerfile)
        self.assertIn("USER 65532:65532", dockerfile)
        self.assertNotIn("target-feature=+crt-static", dockerfile)
        self.assertNotIn("docker/dockerfile:", dockerfile)

    def test_runtime_user_must_be_an_explicit_non_root_numeric_identity(self):
        self.assertEqual(BUILDER.validate_runtime_user("65532:65532"), (65532, 65532))
        for value in ("", "root", "0", "0:0", "65532:0", "app:app"):
            with self.subTest(value=value):
                with self.assertRaises(BUILDER.LocalStandBuildError):
                    BUILDER.validate_runtime_user(value)

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
