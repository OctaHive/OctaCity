"""Container-boundary tests for the local stand's private server inputs."""
from __future__ import annotations

from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

REPOSITORY = Path(__file__).resolve().parents[2]


class LocalStandEntrypointTests(unittest.TestCase):
    @unittest.skipUnless(shutil.which("docker"), "Docker is required for the container boundary")
    def test_oversized_profile_is_rejected_before_starting_the_server(self):
        inspected = subprocess.run(["docker", "image", "inspect", "octacity/server:local"],
                                   capture_output=True, check=False)
        if inspected.returncode:
            self.skipTest("build the local server image before testing its container boundary")
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            secrets = root / "secrets"
            secrets.mkdir()
            for name in ("postgres-url", "object-access-key", "object-secret-key", "signing-key",
                         "agent-enrollment-key", "cache-credential-key"):
                path = secrets / name
                path.write_text("fixture-value\n")
                path.chmod(0o600)
            profile = secrets / "factory-profiles.json"
            profile.write_bytes(b"x" * (4194304 + 1))
            profile.chmod(0o600)
            observer = root / "observe"
            observer.write_text('#!/bin/sh\necho server-started\n')
            observer.chmod(0o755)
            result = subprocess.run([
                "docker", "run", "--rm", "--pull", "never", "--network", "none", "--read-only",
                "--user", "0:0", "--tmpfs", "/run/octacity-secrets:mode=0700,size=8m",
                "--mount", f"type=bind,src={secrets},dst=/run/secrets,readonly",
                "--mount", f"type=bind,src={observer},dst=/usr/local/bin/octacity-server,readonly",
                "--mount", f"type=bind,src={REPOSITORY / 'deployment/local-stand/server-entrypoint.sh'},dst=/entrypoint,readonly",
                "--entrypoint", "/bin/sh", "octacity/server:local", "/entrypoint",
            ], capture_output=True, text=True, check=False, timeout=60)
        self.assertNotEqual(result.returncode, 0)
        self.assertNotIn("server-started", result.stdout)
        self.assertEqual(result.stderr.strip(), "Factory profile catalog exceeds its bound")

    @unittest.skipUnless(shutil.which("docker"), "Docker is required for the container boundary")
    def test_separate_connector_files_are_private_after_dropping_privileges(self):
        inspected = subprocess.run(["docker", "image", "inspect", "octacity/server:local"],
                                   capture_output=True, check=False)
        if inspected.returncode:
            self.skipTest("build the local server image before testing its container boundary")
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            secrets = root / "secrets"
            secrets.mkdir()
            for name in ("postgres-url", "object-access-key", "object-secret-key", "signing-key",
                         "agent-enrollment-key", "cache-credential-key", "factory-profiles.json"):
                path = secrets / name
                path.write_text("fixture-value\n")
                path.chmod(0o600)
            bundle = root / "bundle"
            bundle.mkdir(mode=0o700)
            names = ("factory-signal-connectors.json", "factory-signal-0000-configuration", "factory-signal-0000-credential")
            for name in names:
                path = bundle / name
                path.write_text("private-connector-canary\n")
                path.chmod(0o600)
            delivery = bundle / "factory-delivery" / "opaque-provider"
            delivery.mkdir(parents=True, mode=0o700)
            (delivery / "inputs").mkdir(mode=0o700)
            executable = delivery / "plugin"
            executable.write_text('#!/bin/sh\nprintf hermetic-adapter-ready\n')
            executable.chmod(0o700)
            credential = delivery / "credential"
            credential.write_text("private-delivery-canary\n")
            credential.chmod(0o600)
            observer = root / "observe"
            observer.write_text(
                '#!/bin/sh\nset -eu\ntest "$(id -u)" = 65532\n'
                'for name in factory-signal-connectors.json factory-signal-0000-configuration factory-signal-0000-credential; do\n'
                'path=/run/octacity-secrets/$name\n'
                'test -r "$path"\n'
                'test "$(stat -c %a "$path")" = 600\n'
                'test "$(stat -c %u "$path")" = 65532\n'
                'test "$(cat "$path")" = private-connector-canary\n'
                'done\n'
                'delivery=/run/octacity-delivery/opaque-provider\n'
                'test -x "$delivery/plugin"\n'
                'test "$(stat -c %u "$delivery/plugin")" = 65532\n'
                'test "$(stat -c %a "$delivery/credential")" = 600\n'
                'test "$(stat -c %u "$delivery/credential")" = 65532\n'
                'test "$(stat -c %a "$delivery/inputs")" = 700\n'
                'test "$(cat "$delivery/credential")" = private-delivery-canary\n'
                'test "$("$delivery/plugin")" = hermetic-adapter-ready\n'
                'echo private-connectors-ready\n'
            )
            observer.chmod(0o755)
            result = subprocess.run([
                "docker", "run", "--rm", "--pull", "never", "--network", "none", "--read-only",
                "--user", "0:0", "--tmpfs", "/run/octacity-secrets:mode=0700,size=64k",
                "--tmpfs", "/run/octacity-delivery:exec,mode=0700,size=1m",
                "--mount", f"type=bind,src={secrets},dst=/run/secrets,readonly",
                "--mount", f"type=bind,src={bundle},dst=/run/octacity-factory-connectors,readonly",
                "--mount", f"type=bind,src={observer},dst=/usr/local/bin/octacity-server,readonly",
                "--mount", f"type=bind,src={REPOSITORY / 'deployment/local-stand/server-entrypoint.sh'},dst=/entrypoint,readonly",
                "--entrypoint", "/bin/sh", "octacity/server:local", "/entrypoint",
            ], capture_output=True, text=True, check=False, timeout=20)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.strip(), "private-connectors-ready")
        self.assertNotIn("canary", result.stdout + result.stderr)

    @unittest.skipUnless(shutil.which("docker"), "Docker is required for the container boundary")
    def test_opt_in_profile_is_readable_only_by_the_unprivileged_server(self):
        inspected = subprocess.run(
            ["docker", "image", "inspect", "octacity/server:local"],
            capture_output=True, check=False,
        )
        if inspected.returncode:
            self.skipTest("build the local server image before testing its container boundary")
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            secrets = root / "secrets"
            secrets.mkdir()
            for name in ("postgres-url", "object-access-key", "object-secret-key", "signing-key",
                         "agent-enrollment-key", "cache-credential-key", "factory-profiles.json"):
                path = secrets / name
                path.write_text("fixture-private-value\n")
                path.chmod(0o600)
            observer = root / "observe"
            observer.write_text(
                '#!/bin/sh\nset -eu\n'
                'test "$(id -u)" = 65532\n'
                'profile=/run/octacity-secrets/factory-profiles.json\n'
                'test -r "$profile"\n'
                'test "$(stat -c %a "$profile")" = 600\n'
                'test "$(stat -c %u "$profile")" = 65532\n'
                'test "$(cat "$profile")" = fixture-private-value\n'
                'echo private-profile-ready\n'
            )
            observer.chmod(0o755)
            result = subprocess.run(
                ["docker", "run", "--rm", "--pull", "never", "--network", "none", "--read-only",
                 "--user", "0:0", "--tmpfs", "/run/octacity-secrets:mode=0700,size=64k",
                 "--mount", f"type=bind,src={secrets},dst=/run/secrets,readonly",
                 "--mount", f"type=bind,src={observer},dst=/usr/local/bin/octacity-server,readonly",
                 "--mount", f"type=bind,src={REPOSITORY / 'deployment/local-stand/server-entrypoint.sh'},dst=/entrypoint,readonly",
                 "--entrypoint", "/bin/sh", "octacity/server:local", "/entrypoint"],
                capture_output=True, text=True, check=False, timeout=20,
            )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.strip(), "private-profile-ready")


if __name__ == "__main__":
    unittest.main()
