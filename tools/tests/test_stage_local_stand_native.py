"""Tests for atomic, offline native local-stand staging."""

from __future__ import annotations

from copy import deepcopy
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile
import unittest
from unittest import mock


REPOSITORY = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPOSITORY / "tools"))
SPEC = importlib.util.spec_from_file_location(
    "stage_local_stand_native", REPOSITORY / "tools/stage_local_stand_native.py"
)
assert SPEC and SPEC.loader
STAGER = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = STAGER
SPEC.loader.exec_module(STAGER)
REAL_DERIVE_JOB_SPEC_POLICY = STAGER.derive_job_spec_policy
REVISION = "a" * 40


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def write_executable(path: Path, body: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(body, encoding="utf-8")
    path.chmod(0o755)


def write_checksums(root: Path) -> None:
    files = sorted(
        path
        for path in root.rglob("*")
        if path.is_file() and path.name != "SHA256SUMS"
    )
    (root / "SHA256SUMS").write_text(
        "".join(f"{digest(path)}  {path.relative_to(root).as_posix()}\n" for path in files),
        encoding="utf-8",
    )


def archive(root: Path, output: Path) -> None:
    with tarfile.open(output, "w:gz") as bundle:
        for path in sorted(root.rglob("*")):
            bundle.add(path, arcname=path.relative_to(root), recursive=False)


class NativeStandStagingTests(unittest.TestCase):
    def setUp(self):
        self.document = json.loads(
            (REPOSITORY / "deployment/local-stand/inputs.json").read_text(encoding="utf-8")
        )
        self.entries = STAGER.context.load_staging_allowlist(
            REPOSITORY / "deployment/local-stand/staging-allowlist.json",
            REPOSITORY / "deployment/local-stand/inputs.json",
        )
        self.policy = {
            "source": {"provider": "git"},
            "octa": {"version": self.document["native"]["octa"]["version"]},
            "validity": 900,
        }
        policy = mock.patch.object(
            STAGER, "derive_job_spec_policy", return_value=self.policy
        )
        policy.start()
        self.addCleanup(policy.stop)

    def create_inputs(self, root: Path):
        document = deepcopy(self.document)
        agent_root = root / "agent-root"
        agent_root.mkdir()
        agent = agent_root / "bin/octacity-agent"
        write_executable(
            agent,
            "#!/bin/sh\n"
            "if [ \"$1\" = \"--version\" ]; then echo 'octacity-agent 0.1.0'; exit 0; fi\n"
            "if [ \"$1\" = \"validate\" ]; then echo 'configuration is valid'; exit 0; fi\n"
            "exit 1\n",
        )
        plugin = agent_root / "source-plugins/git/octacity-source-git"
        write_executable(plugin, "#!/bin/sh\nexit 0\n")
        (plugin.parent / "plugin.toml").write_text(
            "\n".join(
                (
                    "manifest_version = 1",
                    'name = "git"',
                    'version = "0.1.0"',
                    "protocol_min = 1",
                    "protocol_max = 1",
                    'executable = "octacity-source-git"',
                    f'sha256 = "{digest(plugin)}"',
                    'platforms = ["macos-aarch64"]',
                    "",
                )
            ),
            encoding="utf-8",
        )
        (agent_root / "release-contract.json").write_text("{}\n", encoding="utf-8")
        (agent_root / "release-manifest.json").write_text(
            json.dumps(
                {
                    "format_version": 1,
                    "product": "octacity-agent",
                    "version": "0.1.0",
                    "platform": "macos-arm64",
                    "build_inputs": {
                        "octacity_revision": REVISION,
                        "octa_revision": document["native"]["octa"]["source_revision"],
                    },
                    "protocols": {},
                    "components": {
                        "agent": {"path": "bin/octacity-agent", "sha256": digest(agent)},
                        "source_git": {
                            "path": "source-plugins/git/octacity-source-git",
                            "sha256": digest(plugin),
                        },
                    },
                }
            ),
            encoding="utf-8",
        )
        write_checksums(agent_root)
        agent_archive = root / self.entries["octacity_agent"]["filename"]
        archive(agent_root, agent_archive)
        agent_archive.with_name(f"{agent_archive.name}.sha256").write_text(
            f"{digest(agent_archive)}  {agent_archive.name}\n", encoding="utf-8"
        )

        octa_root = root / "octa-root"
        octa_root.mkdir()
        write_executable(octa_root / "octa-runner", "#!/bin/sh\nexit 0\n")
        (octa_root / "Octa.lock").write_text("version: 1\nplugins: {}\n", encoding="utf-8")
        (octa_root / "octa-release-contract.json").write_text(
            json.dumps({"format_version": 1, "checksums": "SHA256SUMS"}), encoding="utf-8"
        )
        (octa_root / "octa-runner-capabilities.json").write_text(
            json.dumps(
                {
                    "type": "capabilities",
                    "octa_version": document["native"]["octa"]["version"],
                    "build_commit": document["native"]["octa"]["source_revision"],
                    "platform": "linux-aarch64",
                }
            ),
            encoding="utf-8",
        )
        write_checksums(octa_root)
        octa_archive = root / self.entries["octa"]["filename"]
        archive(octa_root, octa_archive)
        octa_archive.with_name(f"{octa_archive.name}.sha256").write_text(
            f"{digest(octa_archive)}  {octa_archive.name}\n", encoding="utf-8"
        )
        document["native"]["octa"]["sha256"] = digest(octa_archive)

        msb_root = root / "msb-root"
        msb_root.mkdir()
        write_executable(msb_root / "msb", "#!/bin/sh\necho 'msb 0.7.6'\n")
        (msb_root / "libkrunfw.5.dylib").write_bytes(b"firmware")
        msb_archive = root / self.entries["microsandbox"]["filename"]
        archive(msb_root, msb_archive)
        document["native"]["microsandbox"]["sha256"] = digest(msb_archive)

        policy_helper = root / self.entries[STAGER.POLICY_HELPER_ROLE]["filename"]
        write_executable(policy_helper, "#!/bin/sh\nexit 0\n")
        policy_helper.with_name(f"{policy_helper.name}.sha256").write_text(
            f"{digest(policy_helper)}  {policy_helper.name}\n",
            encoding="utf-8",
        )

        for directory in (agent_root, octa_root, msb_root):
            for path in sorted(directory.rglob("*"), reverse=True):
                path.unlink() if path.is_file() else path.rmdir()
            directory.rmdir()
        return document

    def native_processes(self, *, validation_succeeds: bool = True):
        """Replace macOS binaries with their observable process contract."""

        def run(command, **kwargs):
            executable = Path(command[0]).name
            arguments = command[1:]
            self.assertTrue(kwargs["check"])
            self.assertTrue(kwargs["capture_output"])
            self.assertTrue(kwargs["text"])
            if arguments == ["--version"] and executable == "octacity-agent":
                return subprocess.CompletedProcess(
                    command, 0, stdout="octacity-agent 0.1.0\n"
                )
            if arguments == ["--version"] and executable == "msb":
                return subprocess.CompletedProcess(command, 0, stdout="msb 0.7.6\n")
            if arguments == ["--version"] and executable.startswith(
                "octacity-release-harness-"
            ):
                return subprocess.CompletedProcess(
                    command, 0, stdout="octacity-release-harness 0.1.0\n"
                )
            if arguments[:1] == ["validate"] and executable == "octacity-agent":
                if not validation_succeeds:
                    raise subprocess.CalledProcessError(1, command)
                environment = kwargs["env"]
                self.assertEqual(environment["HTTP_PROXY"], "http://127.0.0.1:9")
                self.assertEqual(environment["NO_PROXY"], "")
                return subprocess.CompletedProcess(
                    command, 0, stdout="configuration is valid\n"
                )
            raise AssertionError(f"unexpected native process: {command!r}")

        return mock.patch.object(STAGER.subprocess, "run", side_effect=run)

    def test_names_bind_every_native_version_and_the_agent_revision(self):
        name = STAGER.input_set_name(self.document, REVISION)
        self.assertEqual(
            name,
            "inputs-agent-0.1.0-aaaaaaaaaaaa-octa-0.4.0-msb-0.7.6",
        )
        self.assertEqual(
            STAGER.installation_name(self.document, REVISION),
            "agent-0.1.0-aaaaaaaaaaaa-octa-0.4.0-msb-0.7.6",
        )

    def test_prepare_proves_microsandbox_release_and_asset_identity(self):
        with tempfile.TemporaryDirectory() as temporary:
            destination = Path(temporary) / "inputs"

            def write_agent_archive(agent, policy_helper, *_args, **_kwargs):
                agent.write_bytes(b"agent archive")
                policy_helper.write_bytes(b"policy helper")
                policy_helper.with_name(f"{policy_helper.name}.sha256").write_text(
                    f"{digest(policy_helper)}  {policy_helper.name}\n",
                    encoding="utf-8",
                )

            def write_archive(source, destination):
                del source
                destination.write_bytes(b"archive")

            with (
                mock.patch.object(
                    STAGER, "_build_agent_archive", side_effect=write_agent_archive
                ),
                mock.patch.object(STAGER, "_download_archive", side_effect=write_archive),
                mock.patch.object(
                    STAGER.context, "validate_staging_directory"
                ),
                mock.patch.object(STAGER, "verify_release_ref") as verify_ref,
                mock.patch.object(
                    STAGER, "verify_github_release_asset"
                ) as verify_asset,
            ):
                result = STAGER.prepare_inputs(
                    destination,
                    self.document,
                    self.entries,
                    REVISION,
                    REPOSITORY,
                )

            self.assertEqual(result, destination.resolve())
            verify_ref.assert_called_once()
            verify_asset.assert_called_once()
            verified = verify_asset.call_args.args[0]
            self.assertEqual(
                verified["repository"],
                "https://github.com/superradcompany/microsandbox",
            )
            self.assertEqual(
                verified["asset_id"],
                self.document["native"]["microsandbox"]["asset_id"],
            )

    def test_offline_install_is_atomic_verified_and_reusable(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            inputs_root = root / "inputs"
            inputs_root.mkdir()
            document = self.create_inputs(inputs_root)
            destination = root / "install/root"
            with (
                mock.patch.object(STAGER, "download", side_effect=AssertionError("network used")),
                mock.patch.object(STAGER, "_build_agent_archive", side_effect=AssertionError("build used")),
                self.native_processes(),
            ):
                installed = STAGER.install_native_bundle(
                    inputs_root,
                    destination,
                    document,
                    self.entries,
                    REVISION,
                    REPOSITORY / "deployment/local-stand/fixtures/agent.toml.in",
                )
                self.assertEqual(installed, destination.resolve())
                self.assertTrue((installed / "installation-manifest.json").is_file())
                self.assertEqual(
                    json.loads(
                        (installed / "job-spec-policy.json").read_text(encoding="utf-8")
                    ),
                    self.policy,
                )
                if os.name == "posix":
                    self.assertEqual(
                        (installed / "job-spec-policy.json").stat().st_mode & 0o777,
                        0o600,
                    )
                self.assertTrue((installed / "microsandbox/bin/msb").is_file())
                self.assertEqual(
                    STAGER.install_native_bundle(
                        inputs_root,
                        destination,
                        document,
                        self.entries,
                        REVISION,
                        REPOSITORY / "deployment/local-stand/fixtures/agent.toml.in",
                    ),
                    installed,
                )

    def test_failed_validation_never_publishes_a_partial_installation(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            inputs_root = root / "inputs"
            inputs_root.mkdir()
            document = self.create_inputs(inputs_root)
            destination = root / "install/root"
            with (
                self.native_processes(validation_succeeds=False),
                self.assertRaises(subprocess.CalledProcessError),
            ):
                STAGER.install_native_bundle(
                    inputs_root,
                    destination,
                    document,
                    self.entries,
                    REVISION,
                    REPOSITORY / "deployment/local-stand/fixtures/agent.toml.in",
                )
            self.assertFalse(destination.exists())
            self.assertEqual(list(destination.parent.glob(".root.staging-*")), [])

    def test_reuse_rejects_installed_file_tampering(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            inputs_root = root / "inputs"
            inputs_root.mkdir()
            document = self.create_inputs(inputs_root)
            destination = root / "install/root"
            with self.native_processes():
                STAGER.install_native_bundle(
                    inputs_root,
                    destination,
                    document,
                    self.entries,
                    REVISION,
                    REPOSITORY / "deployment/local-stand/fixtures/agent.toml.in",
                )
                (destination / "microsandbox/lib/libkrunfw.5.dylib").write_bytes(
                    b"tampered"
                )
                with self.assertRaisesRegex(STAGER.NativeStageError, "file inventory"):
                    STAGER.install_native_bundle(
                        inputs_root,
                        destination,
                        document,
                        self.entries,
                        REVISION,
                        REPOSITORY / "deployment/local-stand/fixtures/agent.toml.in",
                    )

    def test_policy_derivation_is_offline_bounded_and_platform_explicit(self):
        installation = Path("/verified/native-installation")
        helper = Path("/verified/native-inputs/octacity-release-harness-macos-arm64")
        encoded = json.dumps(self.policy)
        result = mock.Mock(stdout=encoded)

        with mock.patch.object(STAGER.subprocess, "run", return_value=result) as run:
            policy = REAL_DERIVE_JOB_SPEC_POLICY(
                installation,
                self.document,
                helper,
            )

        self.assertEqual(policy, self.policy)
        command = run.call_args.args[0]
        self.assertEqual(Path(command[0]), helper)
        self.assertNotIn("cargo", command)
        self.assertIn("--agent-platform", command)
        self.assertIn("macos-arm64", command)
        self.assertIn("--octa-platform", command)
        self.assertIn("linux-aarch64", command)

    def test_checksum_inventory_rejects_unlisted_files(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "known").write_text("known", encoding="utf-8")
            write_checksums(root)
            (root / "unknown").write_text("unknown", encoding="utf-8")
            with self.assertRaisesRegex(STAGER.NativeStageError, "exact files"):
                STAGER.verify_checksum_inventory(root)


if __name__ == "__main__":
    unittest.main()
