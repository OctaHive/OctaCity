"""Tests for deterministic Microsandbox guest-agent staging."""

from __future__ import annotations

import hashlib
from contextlib import redirect_stderr
import importlib.util
import io
import json
import os
import sys
import tempfile
import tarfile
import subprocess
import textwrap
import tomllib
import unittest
from unittest import mock
from pathlib import Path
from urllib.error import URLError


REPOSITORY = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPOSITORY / "tools"))
SPEC = importlib.util.spec_from_file_location(
    "stage_microsandbox_agentd", REPOSITORY / "tools/stage_microsandbox_agentd.py"
)
assert SPEC and SPEC.loader
STAGER = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = STAGER
SPEC.loader.exec_module(STAGER)


class MicrosandboxAgentdStagingTests(unittest.TestCase):
    def test_cli_reports_a_failed_runtime_build_without_a_traceback(self):
        error = subprocess.CalledProcessError(1, ["docker", "run"])
        diagnostic = io.StringIO()
        with (
            mock.patch.object(sys, "argv", [
                "stage-agentd", "--architecture", "aarch64",
                "--output", "/unused/agentd", "--repaired",
            ]),
            mock.patch.object(STAGER, "stage_repaired_agentd", side_effect=error),
            redirect_stderr(diagnostic),
        ):
            self.assertEqual(STAGER.main(), 1)
        self.assertIn("docker", diagnostic.getvalue())
        self.assertNotIn("Traceback", diagnostic.getvalue())

    @unittest.skipUnless(os.name == "posix","requires the composite action shell")
    def test_action_forwards_qualified_mode_and_exports_the_staged_path(self):
        action = REPOSITORY / ".github/actions/setup-microsandbox-agentd/action.yml"
        script = textwrap.dedent(action.read_text().split("      run: |\n",1)[1])
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary); commands = root / "bin"; commands.mkdir()
            fake = commands / "python3"
            fake.write_text('#!/bin/sh\nprintf "%s\\n" "$@" > "$ARG_LOG"\nprintf "%s\\n" "$STAGED_GUEST"\n')
            fake.chmod(0o755)
            arguments = root / "arguments"
            environment = dict(os.environ,PATH=str(commands)+os.pathsep+os.environ.get("PATH",""),
                RUNNER_ARCH="ARM64",RUNNER_TEMP=str(root),GITHUB_ACTION_PATH=str(action.parent),
                GITHUB_ENV=str(root/"environment"),GITHUB_OUTPUT=str(root/"outputs"),
                ARG_LOG=str(arguments),STAGED_GUEST=str(root/"qualified-agentd"),OCTACITY_REPAIRED_AGENTD="true")
            subprocess.run(["/bin/bash","-c",script],env=environment,check=True,capture_output=True,text=True)
            self.assertIn("--repaired",arguments.read_text().splitlines())
            self.assertIn("MSB_AGENTD_PATH="+str(root/"qualified-agentd"),(root/"environment").read_text())
    def test_cli_explicitly_selects_the_qualified_repair(self):
        with mock.patch.object(sys,"argv",["stage-agentd","--architecture","aarch64","--output","/tmp/agentd","--repaired"]):
            self.assertTrue(STAGER.parse_args().repaired)
    def test_repaired_guest_reuses_exact_qualified_bytes_without_a_build(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            output = root / "agentd"
            output.write_bytes(self.payload)
            pins = root / "SHA256SUMS"
            pins.write_text(f"{hashlib.sha256(self.payload).hexdigest()}  agentd-x86_64\n")
            self.assertEqual(
                STAGER.stage_repaired_agentd(
                    self.architecture, output, policy_root=root,
                    execute=lambda *_args, **_kwargs: self.fail("a qualified cached guest was rebuilt"),
                ),
                output.resolve(),
            )

    @unittest.skipUnless(os.name == "posix","repaired guest builds require Linux or macOS")
    def test_repaired_guest_builds_pinned_source_and_publishes_only_qualified_bytes(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            pins = root / "SHA256SUMS"
            pins.write_text(f"{hashlib.sha256(self.payload).hexdigest()}  agentd-x86_64\n")
            archive = root / "source.tar.gz"
            source_files = {"upstream/Cargo.toml": b'[workspace]\nmembers = ["guest"]\n', "upstream/guest.txt": b'old\n'}
            with tarfile.open(archive, "w:gz") as bundle:
                for name, contents in source_files.items():
                    info = tarfile.TarInfo(name); info.size = len(contents)
                    bundle.addfile(info, io.BytesIO(contents))
            (root / "readonly-file-mount.patch").write_text("--- a/guest.txt\n+++ b/guest.txt\n@@ -1 +1 @@\n-old\n+repaired\n")
            (root / "Cargo.lock").write_text("# pinned guest dependencies\n")
            (root / "inputs.json").write_text(json.dumps({
                "source": {"archive_url": archive.as_uri(), "sha256": hashlib.sha256(archive.read_bytes()).hexdigest(),
                    "tag":"v0.7.6", "archive_max_bytes":1048576, "expanded_max_bytes":1048576,
                    "file_max_bytes":65536,"member_max_count":16},
                "builders": {"x86_64":{"image":"registry.test/rust@sha256:"+"a"*64,
                    "platform":"linux/amd64","target":"x86_64-unknown-linux-musl"}},
            }))
            built = []
            def execute(command, **kwargs):
                if command[0] != "docker":
                    return subprocess.run(command, **kwargs)
                mount = command[command.index("--mount") + 1]
                source = Path(mount.split("src=",1)[1].split(",dst=",1)[0])
                self.assertEqual((source / "guest.txt").read_text(), "repaired\n")
                self.assertEqual((source / "Cargo.lock").read_text(), "# pinned guest dependencies\n")
                self.assertIn("--locked",command)
                self.assertIn("linux/amd64",command)
                self.assertEqual(command[command.index("--user")+1],f"{os.getuid()}:{os.getgid()}")
                target = source / "target-musl-x86_64/x86_64-unknown-linux-musl/release/agentd"
                target.parent.mkdir(parents=True)
                target.write_bytes(self.payload)
                built.append(command)
                return subprocess.CompletedProcess(command, 0)
            output = root / "installed/agentd"
            result = STAGER.stage_repaired_agentd(self.architecture, output, policy_root=root, execute=execute)
            self.assertEqual(result,output.resolve())
            self.assertEqual(output.read_bytes(),self.payload)
            self.assertEqual(len(built),1)
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
