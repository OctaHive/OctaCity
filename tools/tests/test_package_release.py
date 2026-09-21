"""Black-box tests for the deterministic release packager."""

from __future__ import annotations

import importlib.util
import hashlib
import json
import re
import sys
import tarfile
import tempfile
import unittest
import zipfile
import xml.etree.ElementTree as ET
from pathlib import Path


REPOSITORY = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("package_release", REPOSITORY / "tools/package_release.py")
assert SPEC and SPEC.loader
PACKAGE_RELEASE = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = PACKAGE_RELEASE
SPEC.loader.exec_module(PACKAGE_RELEASE)


class PackageReleaseTests(unittest.TestCase):
    def fixture(self, root: Path, platform: str, output: str):
        agent = root / ("agent.exe" if platform.startswith("windows-") else "agent")
        plugin = root / ("source.exe" if platform.startswith("windows-") else "source")
        source_metadata = root / f"{platform}-source-metadata.json"
        agent.write_bytes(b"agent-release")
        plugin.write_bytes(b"source-release")
        source_metadata.write_text(
            json.dumps(
                {
                    "manifest_version": 1,
                    "name": "git",
                    "version": "1.2.3",
                    "protocol_min": 1,
                    "protocol_max": 1,
                    "platform": PACKAGE_RELEASE.PLATFORMS[platform].source_platform,
                }
            ),
            encoding="utf-8",
        )
        return type(
            "Arguments",
            (),
            {
                "repository": REPOSITORY,
                "platform": platform,
                "version": "1.2.3",
                "agent": agent,
                "source_git": plugin,
                "source_metadata": source_metadata,
                "octacity_revision": "a" * 40,
                "octa_revision": "b" * 40,
                "output": root / output,
            },
        )()

    def test_linux_archive_is_reproducible_and_self_verifying(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            first = PACKAGE_RELEASE.package(self.fixture(root, "linux-amd64", "first.tar.gz"))
            second = PACKAGE_RELEASE.package(self.fixture(root, "linux-amd64", "second.tar.gz"))
            self.assertEqual(PACKAGE_RELEASE.sha256(first), PACKAGE_RELEASE.sha256(second))
            with tarfile.open(first, "r:gz") as archive:
                members = archive.getmembers()
                names = {member.name for member in members}
                files = {member.name for member in members if member.isfile()}
                self.assertIn("share/systemd/octacity-agent.service", names)
                self.assertIn("share/systemd/native-runtime.conf", names)
                self.assertIn("share/systemd/containerd-runtime.conf", names)
                self.assertIn("share/systemd/microsandbox-runtime.conf", names)
                manifest = json.load(archive.extractfile("release-manifest.json"))
                self.assertEqual(manifest["platform"], "linux-amd64")
                self.assertEqual(manifest["build_inputs"]["octacity_revision"], "a" * 40)
                self.assertEqual(manifest["build_inputs"]["octa_revision"], "b" * 40)
                config = archive.extractfile("share/agent.example.toml").read().decode()
                self.assertIn('agent_id = "linux-builder-01"', config)
                self.assertIn('source_plugins_dir = "/opt/octacity/current/source-plugins"', config)
                plugin = archive.extractfile("source-plugins/git/plugin.toml").read().decode()
                self.assertIn('platforms = ["linux-x86_64"]', plugin)
                self.assertIn(PACKAGE_RELEASE.sha256(root / "source"), plugin)
                self.assertIn('[settings]\ngit_path = "/usr/bin/git"', plugin)
                self.assertIn("allow_file = false", plugin)
                self.assertIn("max_diagnostic_bytes = 65536", plugin)
                checksums = archive.extractfile("SHA256SUMS").read().decode().splitlines()
                checked = set()
                for line in checksums:
                    expected, name = line.split("  ", 1)
                    contents = archive.extractfile(name).read()
                    self.assertEqual(hashlib.sha256(contents).hexdigest(), expected)
                    checked.add(name)
                self.assertEqual(checked, files - {"SHA256SUMS"})

    def test_windows_archive_contains_service_install_and_removal(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            output = PACKAGE_RELEASE.package(self.fixture(root, "windows-amd64", "agent.zip"))
            with zipfile.ZipFile(output) as archive:
                names = set(archive.namelist())
                self.assertIn("bin/octacity-agent.exe", names)
                self.assertIn("share/windows/install-service.ps1", names)
                self.assertIn("share/windows/uninstall-service.ps1", names)
                config = archive.read("share/agent.example.toml").decode()
                self.assertIn('agent_id = "windows-builder-01"', config)
                self.assertNotIn("native_linux_cgroup_root", config)
                plugin = archive.read("source-plugins/git/plugin.toml").decode()
                self.assertIn('executable = "octacity-source-git.exe"', plugin)
                self.assertIn('git_path = "C:\\\\Program Files\\\\Git\\\\cmd\\\\git.exe"', plugin)

    def test_linux_arm64_archive_contains_arm_runtime_identity_and_labels(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            output = PACKAGE_RELEASE.package(self.fixture(root, "linux-arm64", "agent.tar.gz"))
            with tarfile.open(output, "r:gz") as archive:
                config = archive.extractfile("share/agent.example.toml").read().decode()
                self.assertIn('agent_id = "linux-arm64-builder-01"', config)
                self.assertIn('cache.native_environment_identities."linux-arm64"', config)
                self.assertIn('arch = "aarch64"', config)
                self.assertNotIn('cache.native_environment_identities."linux-amd64"', config)
                self.assertNotIn('arch = "x86_64"', config)
                plugin = archive.extractfile("source-plugins/git/plugin.toml").read().decode()
                self.assertIn('platforms = ["linux-aarch64"]', plugin)

    def test_macos_archive_contains_an_unschedulable_platform_configuration(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            output = PACKAGE_RELEASE.package(self.fixture(root, "macos-arm64", "agent.tar.gz"))
            with tarfile.open(output, "r:gz") as archive:
                config = archive.extractfile("share/agent.example.toml").read().decode()
                self.assertIn('agent_id = "macos-builder-01"', config)
                self.assertIn("enabled_runtime_modes = []", config)
                self.assertNotIn("native_linux_cgroup_root", config)

    def test_rejects_wrong_archive_type_and_non_file_inputs(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            arguments = self.fixture(root, "windows-amd64", "agent.tar.gz")
            with self.assertRaisesRegex(ValueError, "must end with .zip"):
                PACKAGE_RELEASE.package(arguments)
            arguments.output = root / "agent.zip"
            arguments.agent = root
            with self.assertRaisesRegex(ValueError, "regular file"):
                PACKAGE_RELEASE.package(arguments)

            arguments.agent = root / "agent.exe"
            arguments.version = '1.2.3"\nexecutable = "other.exe'
            with self.assertRaisesRegex(ValueError, "release-token"):
                PACKAGE_RELEASE.package(arguments)

    def test_rejects_source_metadata_that_drifted_from_the_binary_release(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            arguments = self.fixture(root, "linux-amd64", "agent.tar.gz")
            metadata = json.loads(arguments.source_metadata.read_text(encoding="utf-8"))
            metadata["version"] = "9.9.9"
            arguments.source_metadata.write_text(json.dumps(metadata), encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "release version"):
                PACKAGE_RELEASE.package(arguments)

            arguments = self.fixture(root, "linux-amd64", "agent.tar.gz")
            arguments.octa_revision = "main"
            with self.assertRaisesRegex(ValueError, "Git revision"):
                PACKAGE_RELEASE.package(arguments)

    def test_service_definitions_use_dedicated_unprivileged_accounts(self):
        systemd = (REPOSITORY / "packaging/systemd/octacity-agent.service").read_text(encoding="utf-8")
        self.assertIn("User=octacity", systemd)
        self.assertIn("NoNewPrivileges=yes", systemd)
        self.assertIn("ProtectSystem=strict", systemd)
        self.assertIn("StateDirectory=octacity", systemd)
        self.assertIn("RuntimeDirectory=octacity-identities", systemd)
        self.assertIn("/opt/octacity/current/bin/octacity-agent", systemd)
        self.assertIn("TimeoutStopSec=infinity", systemd)

        native = (REPOSITORY / "packaging/systemd/native-runtime.conf").read_text(encoding="utf-8")
        self.assertIn("Delegate=cpu io memory pids", native)
        self.assertIn("DelegateSubgroup=agent", native)
        self.assertIn("ProtectControlGroups=no", native)

        microsandbox = (REPOSITORY / "packaging/systemd/microsandbox-runtime.conf").read_text(encoding="utf-8")
        self.assertIn("DevicePolicy=closed", microsandbox)
        self.assertIn("DeviceAllow=/dev/kvm rw", microsandbox)

        launchd = ET.parse(REPOSITORY / "packaging/launchd/com.octahive.octacity-agent.plist")
        values = [element.text for element in launchd.iter() if element.text]
        self.assertIn("UserName", values)
        self.assertIn("octacity", values)

        windows = (REPOSITORY / "packaging/windows/install-service.ps1").read_text(encoding="utf-8")
        self.assertIn('NT SERVICE\\$ServiceName', windows)
        self.assertIn('service --service-name', windows)
        self.assertIn("New-EventLog", windows)
        self.assertIn('$env:ProgramData\\OctaCity\\config\\agent.toml', windows)
        self.assertIn("^[A-Za-z0-9][A-Za-z0-9_.-]{0,255}$", windows)
        self.assertNotIn("Start-Service", windows)

        uninstall = (REPOSITORY / "packaging/windows/uninstall-service.ps1").read_text(encoding="utf-8")
        self.assertIn("StopTimeoutSeconds", uninstall)
        self.assertIn("Remove-EventLog -Source $ServiceName", uninstall)
        self.assertNotIn("FromSeconds(90)", uninstall)

        operations = (REPOSITORY / "docs/operations.md").read_text(encoding="utf-8")
        self.assertIn("mv -Tf", operations)
        self.assertIn("mv -hf", operations)
        self.assertIn("Get-WinEvent", operations)

    def test_workflows_share_one_checked_in_octa_revision(self):
        revision = (REPOSITORY / ".github/octa-source-revision").read_text(encoding="ascii").strip()
        self.assertEqual(len(revision), 40)
        self.assertTrue(all(character in "0123456789abcdef" for character in revision))
        for name in ("ci.yml", "release.yml", "security.yml", "backend-contracts.yml"):
            workflow = (REPOSITORY / ".github/workflows" / name).read_text(encoding="utf-8")
            self.assertIn("uses: ./octacity/.github/actions/checkout-octa", workflow)
            self.assertNotIn(revision, workflow)
        checkout = (REPOSITORY / ".github/actions/checkout-octa/action.yml").read_text(encoding="utf-8")
        self.assertIn("../../octa-source-revision", checkout)
        self.assertIn("repository: OctaHive/octa", checkout)
        self.assertRegex(checkout, r"actions/checkout@[0-9a-f]{40}")
        release = (REPOSITORY / ".github/workflows/release.yml").read_text(encoding="utf-8")
        self.assertIn("validate_octa_release_contract.py", release)
        self.assertNotIn("grep -F", release)

    def test_workflows_pin_actions_runners_and_toolchains(self):
        action = re.compile(r"^\s*-?\s*uses:\s+[^\s@]+@([0-9a-f]{40})(?:\s+#.*)?$")
        for path in sorted((REPOSITORY / ".github/workflows").glob("*.yml")):
            workflow = path.read_text(encoding="utf-8")
            for line in workflow.splitlines():
                if "uses:" in line:
                    if "uses: ./" not in line:
                        self.assertRegex(line, action, f"mutable action reference in {path.name}: {line}")
            self.assertNotIn("ubuntu-latest", workflow)
            self.assertNotIn("windows-latest", workflow)
            if "rust-toolchain@" in workflow:
                self.assertIn("toolchain: 1.98.1", workflow)

    def test_workflow_service_images_retain_tags_and_pin_manifest_digests(self):
        image = re.compile(r"(?:hashicorp/vault|quay\.io/minio/minio):[^\s@]+@sha256:[0-9a-f]{64}")
        for name in ("ci.yml", "backend-contracts.yml"):
            workflow = (REPOSITORY / ".github/workflows" / name).read_text(encoding="utf-8")
            for line in workflow.splitlines():
                if "hashicorp/vault:" in line or "quay.io/minio/minio:" in line:
                    self.assertRegex(line, image, f"mutable service image in {name}: {line}")


if __name__ == "__main__":
    unittest.main()
