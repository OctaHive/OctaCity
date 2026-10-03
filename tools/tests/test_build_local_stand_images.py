"""Tests for the complete local-stand image build orchestrator."""

from __future__ import annotations

import json
from pathlib import Path
import sys
import unittest
from unittest import mock


REPOSITORY = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPOSITORY / "tools"))

import build_local_stand_images as builder  # noqa: E402


class LocalStandImageBuildTests(unittest.TestCase):
    def test_every_target_builds_from_one_staging_pass_per_source_family(self):
        document = json.loads(builder.DEFAULT_MANIFEST.read_text(encoding="utf-8"))
        with (
            mock.patch.object(builder, "load_build_inputs", return_value=document),
            mock.patch.object(builder, "git_revision", return_value="a" * 40),
            mock.patch.object(builder, "stage_octa_source") as stage_octa,
            mock.patch.object(builder, "stage_octacity_source") as stage_octacity,
            mock.patch.object(builder.server_builder, "build_staged") as build_server,
            mock.patch.object(builder.gateway_builder, "build_staged") as build_gateway,
            mock.patch.object(builder.minio_builder, "build_targets") as build_minio,
        ):
            images = builder.build_all(
                builder.DEFAULT_MANIFEST,
                builder.DEFAULT_SERVER_FIXTURE,
                REPOSITORY,
            )

        stage_octa.assert_called_once()
        stage_octacity.assert_called_once()
        build_server.assert_called_once()
        build_gateway.assert_called_once()
        build_minio.assert_called_once()
        self.assertEqual(build_minio.call_args.args[2], "arm64")
        self.assertEqual(set(build_minio.call_args.args[1]), {"minio", "mc"})
        self.assertEqual(set(images), {"server", "gateway", "minio", "mc"})


if __name__ == "__main__":
    unittest.main()
