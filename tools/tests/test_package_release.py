"""Black-box tests for the deterministic release packager."""

from __future__ import annotations

import importlib.util
import hashlib
import json
import re
import subprocess
import sys
import tarfile
import tempfile
import unittest
import zipfile
import xml.etree.ElementTree as ET
from pathlib import Path


REPOSITORY = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPOSITORY / "tools"))
SPEC = importlib.util.spec_from_file_location("package_release", REPOSITORY / "tools/package_release.py")
assert SPEC and SPEC.loader
PACKAGE_RELEASE = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = PACKAGE_RELEASE
SPEC.loader.exec_module(PACKAGE_RELEASE)
SERVER_SPEC = importlib.util.spec_from_file_location(
    "package_server_release", REPOSITORY / "tools/package_server_release.py"
)
assert SERVER_SPEC and SERVER_SPEC.loader
PACKAGE_SERVER_RELEASE = importlib.util.module_from_spec(SERVER_SPEC)
sys.modules[SERVER_SPEC.name] = PACKAGE_SERVER_RELEASE
SERVER_SPEC.loader.exec_module(PACKAGE_SERVER_RELEASE)


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
                "source_allow_file": False,
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
                self.assertEqual(manifest["protocols"]["coordinator"], {"min": 1, "max": 1})
                contract = json.load(archive.extractfile("release-contract.json"))
                self.assertEqual(contract["manifest"], "release-manifest.json")
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

    def test_contract_candidate_can_explicitly_allow_a_local_source(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            arguments = self.fixture(root, "linux-amd64", "agent.tar.gz")
            arguments.source_allow_file = True

            output = PACKAGE_RELEASE.package(arguments)

            with tarfile.open(output, "r:gz") as archive:
                plugin = archive.extractfile("source-plugins/git/plugin.toml").read().decode()
                self.assertIn("allow_file = true", plugin)

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

    def test_server_archive_is_reproducible_and_self_verifying(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            server = root / "octacity-server"
            server.write_bytes(b"server-release")

            def arguments(output: str):
                return type(
                    "Arguments",
                    (),
                    {
                        "repository": REPOSITORY,
                        "platform": "linux-amd64",
                        "version": "1.2.3",
                        "server": server,
                        "octacity_revision": "a" * 40,
                        "output": root / output,
                    },
                )()

            first = PACKAGE_SERVER_RELEASE.package(arguments("server-first.tar.gz"))
            second = PACKAGE_SERVER_RELEASE.package(arguments("server-second.tar.gz"))
            self.assertEqual(PACKAGE_RELEASE.sha256(first), PACKAGE_RELEASE.sha256(second))
            with tarfile.open(first, "r:gz") as archive:
                files = {member.name for member in archive.getmembers() if member.isfile()}
                self.assertIn("bin/octacity-server", files)
                self.assertIn("release-contract.json", files)
                manifest = json.load(archive.extractfile("release-manifest.json"))
                self.assertEqual(manifest["product"], "octacity-server")
                self.assertEqual(manifest["components"]["server"]["sha256"], PACKAGE_RELEASE.sha256(server))
                self.assertEqual(manifest["protocols"]["vcs_provider"], {"min": 1, "max": 1})
                checksums = archive.extractfile("SHA256SUMS").read().decode().splitlines()
                checked = set()
                for line in checksums:
                    expected, name = line.split("  ", 1)
                    self.assertEqual(hashlib.sha256(archive.extractfile(name).read()).hexdigest(), expected)
                    checked.add(name)
                self.assertEqual(checked, files - {"SHA256SUMS"})

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

    def test_release_scenarios_install_every_product_through_the_harness(self):
        workflow = (REPOSITORY / ".github/workflows/backend-contracts.yml").read_text(encoding="utf-8")
        self.assertIn("uses: ./octacity/.github/actions/package-release-candidate", workflow)
        self.assertIn("server-root: ${{ steps.release-candidate.outputs.server-root }}", workflow)
        self.assertIn("agent-root: ${{ steps.release-candidate.outputs.agent-root }}", workflow)
        action = (REPOSITORY / ".github/actions/release-agent-vertical-slice/action.yml").read_text(encoding="utf-8")
        self.assertIn("cargo run --quiet --locked -p octacity-release-harness -- prepare", action)
        self.assertIn("--server-root", action)
        self.assertIn("--agent-root", action)
        self.assertIn("--octa-root", action)
        self.assertIn("released_linux_native_matrix_satisfies_the_end_to_end_contract", action)
        self.assertIn("MINIO_DEFAULT_BUCKETS=octacity-artifacts", action)
        self.assertNotIn("server /bitnami/minio/data", action)
        self.assertIn('${{ steps.test.outputs.evidence }}', action)
        self.assertIn('> "${OCTACITY_RELEASE_EVIDENCE_DIR}/minio.log"', action)
        self.assertIn("cargo test --locked -p octacity-release-harness --test release_vertical_slice", action)
        fixture = REPOSITORY / "fixtures/release/linux-native/Octafile.yml"
        self.assertTrue(fixture.is_file())
        fixture_text = fixture.read_text(encoding="utf-8")
        self.assertIn("cache: {}", fixture_text)
        self.assertIn("failing:", fixture_text)
        self.assertIn("exit 23", fixture_text)
        release = (REPOSITORY / ".github/workflows/release.yml").read_text(encoding="utf-8")
        self.assertIn("tools/package_server_release.py", release)
        self.assertIn("octacity-server-${{ matrix.platform }}", release)

    def test_linux_native_release_gate_uses_a_disposable_github_runner(self):
        workflow = (REPOSITORY / ".github/workflows/backend-contracts.yml").read_text(encoding="utf-8")
        linux_native = workflow.split("  linux-native:\n", 1)[1].split("  linux-containerd:\n", 1)[0]
        self.assertIn("runs-on: ubuntu-24.04", linux_native)
        self.assertNotIn("self-hosted", linux_native)
        self.assertIn("github-hosted-native.sh setup", linux_native)
        self.assertIn("github-hosted-native.sh cleanup", linux_native)
        self.assertIn("github-hosted-native.sh run", linux_native)
        self.assertIn("execution-wrapper:", linux_native)
        self.assertIn("apparmor-profiles", linux_native)
        self.assertIn("OCTA_RELEASE_VERSION: ${{ steps.octa-source.outputs.version }}", linux_native)
        self.assertIn("if: always()", linux_native)

        checkout = (REPOSITORY / ".github/actions/checkout-octa/action.yml").read_text(encoding="utf-8")
        self.assertIn("value: ${{ steps.metadata.outputs.version }}", checkout)

        provisioner = (REPOSITORY / "tools/runner/github-hosted-native.sh").read_text(encoding="utf-8")
        release_stager = (REPOSITORY / "tools/runner/github-hosted-release.sh").read_text(encoding="utf-8")
        self.assertIn("sha256sum --check --strict", release_stager)
        self.assertIn("octa-runner-capabilities.json", release_stager)
        self.assertIn("OCTACITY_CONTRACT_NATIVE_CGROUP_ROOT", provisioner)
        self.assertIn("OCTACITY_RELEASE_NATIVE_CACHE_ROOT", provisioner)
        self.assertIn("OCTACITY_CONTRACT_NATIVE_ENVIRONMENT_IDENTITY", provisioner)
        self.assertIn("ImageOS", provisioner)
        self.assertIn("ImageVersion", provisioner)
        self.assertIn("octacity-hosted-native/runner/cgroup.procs", provisioner)
        self.assertIn("bwrap-userns-restrict", provisioner)
        self.assertIn("apparmor_parser", provisioner)

        self.assertIn("probe_bubblewrap", provisioner)
        self.assertIn('provision_workspace "$root/work" "$workspace_bytes"', provisioner)
        self.assertNotIn('provision_filesystem "$root/work.ext4"', provisioner)
        bubblewrap_probe = provisioner.split("probe_bubblewrap() {", 1)[1].split("\n}", 1)[0]
        self.assertLess(
            bubblewrap_probe.index("--unshare-user"),
            bubblewrap_probe.index("--disable-userns"),
            "Bubblewrap requires an explicit user namespace before it can disable nested user namespaces",
        )
        self.assertIn("Native work and cache roots must use separate filesystems", provisioner)

    def test_released_host_gate_covers_every_supported_host_without_self_hosting(self):
        workflow = (REPOSITORY / ".github/workflows/backend-contracts.yml").read_text(encoding="utf-8")
        host = workflow.split("  released-host:\n", 1)[1].split("  phase6-vault-minio:\n", 1)[0]
        for runner in ("ubuntu-24.04", "macos-14", "windows-2025"):
            self.assertIn(f"runner: {runner}", host)
        self.assertNotIn("self-hosted", host)
        self.assertIn("uses: ./octacity/.github/actions/stage-octa-release", host)
        self.assertIn("host_backend_satisfies_the_real_runner_contract", host)
        self.assertIn("released_host_agent_satisfies_the_portable_execution_contract", host)
        self.assertIn("include-server: \"false\"", host)
        self.assertIn("source-allow-file: \"true\"", host)
        self.assertIn("OCTACITY_CONTRACT_HOST_SOURCE_ROOT: ${{ github.workspace }}/octa", host)

        package_action = (REPOSITORY / ".github/actions/package-release-candidate/action.yml").read_text(
            encoding="utf-8"
        )
        self.assertIn("source-allow-file:", package_action)
        self.assertIn("source_policy+=(--source-allow-file)", package_action)

    def test_linux_oci_release_gates_use_disposable_github_runners(self):
        workflow = (REPOSITORY / ".github/workflows/backend-contracts.yml").read_text(encoding="utf-8")
        containerd = workflow.split("  linux-containerd:\n", 1)[1].split("  linux-microsandbox:\n", 1)[0]
        microsandbox = workflow.split("  linux-microsandbox:\n", 1)[1].split("  macos-microsandbox:\n", 1)[0]
        for job, backend in ((containerd, "containerd"), (microsandbox, "microsandbox")):
            self.assertIn("runs-on: ubuntu-24.04", job)
            self.assertNotIn("self-hosted", job)
            self.assertIn(f"github-hosted-oci.sh setup {backend}", job)
            self.assertIn(f"github-hosted-oci.sh verify-clean {backend}", job)
            self.assertIn(f"github-hosted-oci.sh cleanup {backend}", job)
            self.assertIn("uses: ./octacity/.github/actions/package-release-candidate", job)
            self.assertIn("uses: ./octacity/.github/actions/release-agent-vertical-slice", job)
            self.assertIn("if: always()", job)

        provisioner = (REPOSITORY / "tools/runner/github-hosted-oci.sh").read_text(encoding="utf-8")
        self.assertIn("CONTAINERD_VERSION=2.3.6", provisioner)
        self.assertIn("@sha256:", provisioner)
        self.assertIn("63773f454664cd77e239f8e0b13ae7f18effe9e3d6612a325b5646eb3bda11f1", provisioner)
        self.assertIn("83205934094144b56f645f86c42b84f81083f423b0bea9cb233f91c21bab0919", provisioner)
        self.assertIn("sha256sum --check --strict", provisioner)
        self.assertIn("containerd-2.3.6-linux-amd64.tar.gz", provisioner)
        self.assertIn("containerd-2.3.6-linux-arm64.tar.gz", provisioner)
        self.assertIn("containerd Transfer plugin is unavailable", provisioner)
        self.assertIn('"OCTACITY_CONTRACT_CONTAINERD_SNAPSHOTTER=overlayfs"', provisioner)
        self.assertIn("[[ -c /dev/kvm ]]", provisioner)
        self.assertIn("tasks list --quiet", provisioner)
        self.assertIn("containers list --quiet", provisioner)

    def test_apple_vf_release_gate_is_explicit_and_self_hosted(self):
        workflow = (REPOSITORY / ".github/workflows/backend-contracts.yml").read_text(encoding="utf-8")
        macos_microsandbox = workflow.split("  macos-microsandbox:\n", 1)[1].split(
            "  windows-microsandbox-preview:\n", 1
        )[0]
        apple_vf = workflow.split("  macos-apple-vf:\n", 1)[1]
        self.assertIn("backend: microsandbox", macos_microsandbox)
        self.assertIn("name: release-slice-macos-microsandbox", macos_microsandbox)
        self.assertIn("runs-on: [self-hosted, macOS, ARM64, octacity-apple-vf]", apple_vf)
        self.assertIn("self-hosted-apple-vf.sh setup", apple_vf)
        self.assertIn("apple_vf_isolation_provider_satisfies_the_real_runner_contract", apple_vf)
        self.assertIn("backend: apple-vf-isolation", apple_vf)
        self.assertNotIn("backend: microsandbox", apple_vf)
        self.assertEqual(apple_vf.count("id: release-candidate"), 1)
        self.assertEqual(apple_vf.count("id: release-slice"), 1)
        self.assertIn("self-hosted-apple-vf.sh verify-clean", apple_vf)
        self.assertIn("self-hosted-apple-vf.sh cleanup", apple_vf)
        self.assertIn("if: always()", apple_vf)

        provisioner = (REPOSITORY / "tools/runner/self-hosted-apple-vf.sh").read_text(encoding="utf-8")
        self.assertIn("APPLE_CONTAINER_MINIMUM=0.6.0", provisioner)
        self.assertIn("hdiutil create", provisioner)
        self.assertIn("octa-Linux-arm64.tar.gz", provisioner)
        self.assertIn("linux-aarch64", provisioner)
        self.assertIn("@sha256:", provisioner)
        self.assertIn("OCTACITY_CONTRACT_APPLE_VF_CACHE_ROOT", provisioner)

        registration = (REPOSITORY / "tools/runner/register-backend-runner.sh").read_text(encoding="utf-8")
        self.assertIn("workflow_suite=macos-apple-vf", registration)
        self.assertIn("preflight_arguments=(--host-only)", registration)
        self.assertIn("default_registration_url=https://github.com/$organization", registration)
        self.assertIn("--runnergroup", registration)
        self.assertIn("octacity-release", registration)
        self.assertIn("backend-contracts.yml@refs/heads/main", registration)
        self.assertNotIn("Start the wizard on the other backend host", registration)

    def test_windows_microsandbox_remains_an_explicit_preview_gate(self):
        workflow = (REPOSITORY / ".github/workflows/backend-contracts.yml").read_text(encoding="utf-8")
        preview = workflow.split("  windows-microsandbox-preview:\n", 1)[1].split("  macos-apple-vf:\n", 1)[0]
        self.assertIn("inputs.suite == 'windows-microsandbox-preview'", preview)
        self.assertNotIn("inputs.suite == 'all'", preview)
        self.assertIn("runs-on: [self-hosted, Windows, X64, octacity-microsandbox-whp]", preview)
        self.assertIn("self-hosted-microsandbox-whp.ps1 setup", preview)
        self.assertIn("microsandbox_backend_satisfies_the_real_runner_contract", preview)
        self.assertIn("include-server: \"false\"", preview)
        self.assertIn("self-hosted-microsandbox-whp.ps1 verify-clean", preview)
        self.assertIn("if: always()", preview)

        provisioner = (REPOSITORY / "tools/runner/self-hosted-microsandbox-whp.ps1").read_text(encoding="utf-8")
        self.assertIn('$MicrosandboxVersion = "0.6.18"', provisioner)
        self.assertIn("7e89516b78911dac58df546b24af4956f3772ea44ba13e13306712ffdde374e2", provisioner)
        self.assertIn("msb.FullName doctor", provisioner)
        self.assertIn("--platform linux-amd64", provisioner)
        self.assertIn("OCTACITY_CONTRACT_MICROSANDBOX_ENVIRONMENT_IDENTITY", provisioner)

    @unittest.skipIf(sys.platform == "win32", "Apple VF provisioner executes only on POSIX hosts")
    def test_apple_vf_volume_planning_is_safe_under_nounset(self):
        provisioner = (REPOSITORY / "tools/runner/self-hosted-apple-vf.sh").read_text(encoding="utf-8")
        function = provisioner.split("attach_volume() {\n", 1)[1].split("\n}\n", 1)[0]
        script = "\n".join(
            (
                "set -u",
                "root=/outer-scope-that-must-not-be-used",
                "attach_volume() {",
                function,
                "}",
                "mkdir() { :; }",
                "hdiutil() { :; }",
                "chmod() { :; }",
                "attach_volume /bounded-root work 1073741824",
            )
        )
        result = subprocess.run(["bash", "-c", script], check=False, text=True, capture_output=True)
        self.assertEqual(result.returncode, 0, result.stderr)

    @unittest.skipIf(sys.platform == "win32", "Apple VF provisioner executes only on POSIX hosts")
    def test_apple_vf_cleanup_ignores_apfs_metadata_but_rejects_owned_workspaces(self):
        with tempfile.TemporaryDirectory() as temporary:
            runner_temp = Path(temporary)
            root = runner_temp / "octacity-self-hosted-apple-vf"
            work = root / "work"
            (root / "state" / "apple-vf-isolation").mkdir(parents=True)
            work.mkdir()
            (root / ".octacity-apple-vf").write_text("apple-vf-isolation\n", encoding="utf-8")
            (work / ".fseventsd").mkdir()
            environment = {"RUNNER_TEMP": str(runner_temp)}

            clean = subprocess.run(
                ["bash", str(REPOSITORY / "tools/runner/self-hosted-apple-vf.sh"), "verify-clean"],
                check=False,
                text=True,
                capture_output=True,
                env=environment,
            )
            self.assertEqual(clean.returncode, 0, clean.stderr)

            (work / ("job-" + "a" * 64)).mkdir()
            leaked = subprocess.run(
                ["bash", str(REPOSITORY / "tools/runner/self-hosted-apple-vf.sh"), "verify-clean"],
                check=False,
                text=True,
                capture_output=True,
                env=environment,
            )
            self.assertNotEqual(leaked.returncode, 0)
            self.assertIn("Apple VF workspaces remain", leaked.stderr)

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

    def test_coverage_runs_every_postgres_integration_contract(self):
        workflow = (REPOSITORY / ".github/workflows/ci.yml").read_text(encoding="utf-8")
        self.assertRegex(
            workflow,
            re.compile(
                r"cargo llvm-cov --no-clean --all-features\s+"
                r"-p octacity-server-store-postgres --tests --\s+--ignored"
            ),
        )
        self.assertNotRegex(
            workflow,
            re.compile(r"-p octacity-server-store-postgres --test\s"),
            "enumerating PostgreSQL test targets lets new contracts silently drop out of coverage",
        )

    def test_workflow_service_images_retain_tags_and_pin_manifest_digests(self):
        image = re.compile(r"(?:hashicorp/vault|quay\.io/minio/minio):[^\s@]+@sha256:[0-9a-f]{64}")
        for name in ("ci.yml", "backend-contracts.yml"):
            workflow = (REPOSITORY / ".github/workflows" / name).read_text(encoding="utf-8")
            for line in workflow.splitlines():
                if "hashicorp/vault:" in line or "quay.io/minio/minio:" in line:
                    self.assertRegex(line, image, f"mutable service image in {name}: {line}")


if __name__ == "__main__":
    unittest.main()
