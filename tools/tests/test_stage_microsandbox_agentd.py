"""Tests for deterministic Microsandbox guest-agent staging."""

from __future__ import annotations

import hashlib
import importlib.util
import io
import sys
import tempfile
import tomllib
import unittest
from pathlib import Path
from urllib.error import URLError


REPOSITORY = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location(
    "stage_microsandbox_agentd", REPOSITORY / "tools/stage_microsandbox_agentd.py"
)
assert SPEC and SPEC.loader
STAGER = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = STAGER
SPEC.loader.exec_module(STAGER)


class MicrosandboxAgentdStagingTests(unittest.TestCase):
    def setUp(self):
        self.architecture = "x86_64"
        self.payload = b"verified agentd fixture"
        self.original_asset = STAGER.ASSETS[self.architecture]
        STAGER.ASSETS[self.architecture] = STAGER.Asset(
            self.original_asset.name,
            hashlib.sha256(self.payload).hexdigest(),
            self.original_asset.api_id,
        )

    def tearDown(self):
        STAGER.ASSETS[self.architecture] = self.original_asset

    def test_retries_a_transient_gateway_failure(self):
        calls = []
        delays = []

        def open_fixture(_request, *, timeout):
            calls.append(timeout)
            if len(calls) == 1:
                raise URLError("HTTP Error 504: Gateway Timeout")
            return io.BytesIO(self.payload)

        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary) / "agentd"
            staged = STAGER.stage_agentd(
                self.architecture,
                output,
                opener=open_fixture,
                sleeper=delays.append,
            )

            self.assertEqual(staged, output.resolve())
            self.assertEqual(output.read_bytes(), self.payload)
            self.assertEqual(calls, [60, 60])
            self.assertEqual(delays, [1])

    def test_uses_the_asset_api_when_the_release_cdn_returns_only_504(self):
        requested_urls = []

        def open_fixture(request, *, timeout):
            requested_urls.append(request.full_url)
            if request.full_url.startswith(STAGER.RELEASE_URL):
                raise URLError("HTTP Error 504: Gateway Timeout")
            return io.BytesIO(self.payload)

        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary) / "agentd"
            staged = STAGER.stage_agentd(
                self.architecture,
                output,
                opener=open_fixture,
                sleeper=lambda _delay: None,
            )

            self.assertEqual(staged, output.resolve())
            self.assertEqual(output.read_bytes(), self.payload)
            self.assertTrue(
                any(url.startswith(STAGER.ASSET_API_URL) for url in requested_urls)
            )

    def test_rejects_an_invalid_download_without_replacing_the_cached_file(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary) / "agentd"
            output.write_bytes(b"previous invalid cache")

            with self.assertRaisesRegex(STAGER.StageError, "checksum mismatch"):
                STAGER.stage_agentd(
                    self.architecture,
                    output,
                    attempts=1,
                    opener=lambda _request, *, timeout: io.BytesIO(b"corrupted"),
                )

            self.assertEqual(output.read_bytes(), b"previous invalid cache")
            self.assertFalse(output.with_name("agentd.part").exists())

    def test_reuses_a_verified_file_without_network_access(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary) / "agentd"
            output.write_bytes(self.payload)

            staged = STAGER.stage_agentd(
                self.architecture,
                output,
                opener=lambda *_args, **_kwargs: self.fail("unexpected download"),
            )

            self.assertEqual(staged, output.resolve())

    def test_version_matches_the_workspace_dependency(self):
        cargo_toml = tomllib.loads(
            (REPOSITORY / "Cargo.toml").read_text(encoding="utf-8")
        )
        dependency = cargo_toml["workspace"]["dependencies"]["microsandbox"]
        self.assertEqual(dependency["version"], f"={STAGER.VERSION}")


if __name__ == "__main__":
    unittest.main()
