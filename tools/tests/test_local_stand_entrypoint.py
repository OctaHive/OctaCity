"""Container-boundary tests for the local stand's private server inputs."""
from __future__ import annotations

from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
import sys
import json
import os

REPOSITORY = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPOSITORY / "tools"))
from factory_integrations import configure_integrations


class LocalStandEntrypointTests(unittest.TestCase):
    @unittest.skipUnless(shutil.which("docker"), "Docker is required for the container boundary")
    def test_packaged_vcs_adapter_loads_its_git_configuration_as_the_server_user(self):
        image = os.environ.get("OCTACITY_TEST_SERVER_IMAGE", "octacity/server:local")
        if subprocess.run(["docker", "image", "inspect", image], capture_output=True).returncode:
            self.skipTest("build the local server image before checking VCS")
        request = json.loads((REPOSITORY / "server/protocols/octacity-vcs-protocol/fixtures/resolve-revision-v1.json").read_text())
        request["command"]["payload"]["reference"] = "not-a-fully-qualified-reference"
        request["command"]["payload"]["repository"]["credential_handle"] = "anonymous"
        result = subprocess.run(["docker", "run", "--rm", "-i", "--pull", "never", "--network", "none", "--read-only",
            "--workdir", "/usr/local/lib/octacity/vcs/git", "--entrypoint", "/usr/local/bin/octacity-vcs-git", image],
            input=json.dumps(request) + "\n", capture_output=True, text=True, timeout=20)
        self.assertEqual(result.returncode, 0, result.stderr)
        response = json.loads(result.stdout)
        self.assertEqual(response["request_id"], request["request_id"])
        self.assertEqual(response["outcome"]["payload"]["code"], "unsupported_reference")

    @unittest.skipUnless(shutil.which("docker"), "Docker is required for the container boundary")
    def test_server_image_exposes_plugin_owned_model_metadata_without_task_or_credentials(self):
        image = os.environ.get("OCTACITY_TEST_SERVER_IMAGE", "octacity/server:local")
        if subprocess.run(["docker", "image", "inspect", image], capture_output=True).returncode:
            self.skipTest("build the local server image before checking Runtime discovery")
        result = subprocess.run([
            "docker", "run", "--rm", "--pull", "never", "--network", "none", "--read-only",
            "--entrypoint", "/usr/local/bin/octa_plugin_codex", image, "--describe-models",
        ], input="{}\n", capture_output=True, text=True, timeout=20)
        self.assertEqual(result.returncode, 0, result.stderr)
        catalog = json.loads(result.stdout)
        self.assertIs(catalog["manual_model"], True)
        self.assertEqual(catalog["models"], [])
        self.assertIs(catalog["available"], False)

    @unittest.skipUnless(shutil.which("docker"), "Docker is required for the container boundary")
    def test_server_image_ships_an_independently_invocable_github_provider(self):
        image = os.environ.get("OCTACITY_TEST_SERVER_IMAGE", "octacity/server:local")
        if subprocess.run(["docker", "image", "inspect", image], capture_output=True).returncode:
            self.skipTest("build the local server image before testing its provider boundary")
        result = subprocess.run(["docker", "run", "--rm", "--pull", "never", "--network", "none", "--read-only",
                                 "--entrypoint", "/usr/local/bin/octacity-github", image],
                                input="invalid-protocol-frame\n", capture_output=True, text=True, timeout=20)
        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertNotIn("executable file not found", result.stderr)
        self.assertNotIn("no such file", result.stderr.lower())

    @unittest.skipUnless(shutil.which("docker"), "Docker is required for the container boundary")
    def test_integration_installation_retains_credentials_after_restart_and_drops_root(self):
        if subprocess.run(["docker", "image", "inspect", "octacity/server:local"], capture_output=True).returncode:
            self.skipTest("build the local server image before testing its container boundary")
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            server = root / "config/server/server.toml"
            server.parent.mkdir(parents=True)
            server.write_text('management_bind = "0.0.0.0:8080"\n')
            server.chmod(0o600)
            bundle = configure_integrations(root, "Example/Tickets", native_repository_id="1234", adapter_digest="ab" * 32)
            catalog = json.loads((bundle / "catalog.json").read_text())
            provider = catalog["credential_publishers"][0]["root"]
            selector, = (bundle / "selections").iterdir()
            mapping, = (bundle / "mappings").iterdir()
            secrets = root / "secrets"
            secrets.mkdir()
            for name in ("postgres-url", "object-access-key", "object-secret-key", "signing-key",
                         "agent-enrollment-key", "cache-credential-key"):
                path = secrets / name
                path.write_text("fixture-value\n")
                path.chmod(0o600)
            observer = root / "observe"
            observer.write_text(f'''#!/bin/sh
set -eu
test "$(id -u)" = 65532
test "$(stat -c %a /run/octacity-management)" = 711
test "$(stat -c %a {provider}/credentials)" = 700
test "$(stat -c %a {provider}/github-work.toml)" = 600
test -x {provider}/plugin
test -r {catalog['source_profiles_file']}
test "$(stat -c %a {catalog['source_profiles_file']})" = 600
test -r {provider}/selections/{selector.name}
test "$(stat -c %a {provider}/selections)" = 700
test -r {provider}/mappings/{mapping.name}
test "$(stat -c %a {provider}/mappings)" = 700
test -r /run/octacity-secrets/integration-server.toml
if [ -e {provider}/credentials/retained ]; then
  test "$(cat {provider}/credentials/retained)" = credential-retained
  echo retained
else
  umask 077
  echo credential-retained > {provider}/credentials/retained
  echo installed
fi
''')
            observer.chmod(0o755)
            storage = root / "storage"
            channel = root / "channel"
            storage.mkdir()
            channel.mkdir()
            command = ["docker", "run", "--rm", "--pull", "never", "--network", "none", "--read-only", "--user", "0:0",
                "--tmpfs", "/run/octacity-secrets:mode=0700,size=8m",
                "--mount", f"type=bind,src={secrets},dst=/run/secrets,readonly",
                "--mount", f"type=bind,src={bundle},dst=/run/octacity-integration-installation,readonly",
                "--mount", f"type=bind,src={storage},dst=/var/lib/octacity-integrations",
                "--mount", f"type=bind,src={channel},dst=/run/octacity-management",
                "--mount", f"type=bind,src={observer},dst=/usr/local/bin/octacity-server,readonly",
                "--mount", f"type=bind,src={observer},dst=/usr/local/bin/octacity-github,readonly",
                "--mount", f"type=bind,src={REPOSITORY / 'deployment/local-stand/server-entrypoint.sh'},dst=/entrypoint,readonly",
                "--entrypoint", "/bin/sh", "octacity/server:local", "/entrypoint"]
            for expected in ("installed", "retained"):
                result = subprocess.run(command, capture_output=True, text=True, timeout=30)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(result.stdout.strip(), expected)

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
            runtime = bundle / "factory-runtime"
            runtime.mkdir(mode=0o700)
            program = runtime / "plugin"
            program.write_text('#!/bin/sh\nprintf runtime-catalog-ready\n')
            program.chmod(0o700)
            private_home = runtime / "home/.codex"
            private_home.mkdir(parents=True, mode=0o700)
            (runtime / "home").chmod(0o700)
            auth = private_home / "auth.json"
            auth.write_text("runtime-auth-canary")
            auth.chmod(0o600)
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
                'test "$(/run/octacity-runtime/plugin)" = runtime-catalog-ready\n'
                'test "$(cat /run/octacity-runtime/home/.codex/auth.json)" = runtime-auth-canary\n'
                'test "$(stat -c %a /run/octacity-runtime/home/.codex/auth.json)" = 600\n'
                'echo private-connectors-ready\n'
            )
            observer.chmod(0o755)
            result = subprocess.run([
                "docker", "run", "--rm", "--pull", "never", "--network", "none", "--read-only",
                "--user", "0:0", "--tmpfs", "/run/octacity-secrets:mode=0700,size=64k",
                "--tmpfs", "/run/octacity-delivery:exec,mode=0700,size=1m",
                "--tmpfs", "/run/octacity-runtime:exec,mode=0700,size=1m",
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
            bundle = root / "reader-bundle"
            bundle.mkdir()
            for name, value in (("factory-issue-mcp-readers.json", '{"version":1,"readers":[]}'),
                                ("factory-issue-mcp-0000-credential", "issue-reader-private-canary"),
                                ("unselected-private-file", "unselected-private-canary")):
                (bundle / name).write_text(value)
                (bundle / name).chmod(0o600)
            observer = root / "observe"
            observer.write_text(
                '#!/bin/sh\nset -eu\n'
                'test "$(id -u)" = 65532\n'
                'profile=/run/octacity-secrets/factory-profiles.json\n'
                'test -r "$profile"\n'
                'test "$(stat -c %a "$profile")" = 600\n'
                'test "$(stat -c %u "$profile")" = 65532\n'
                'test "$(cat "$profile")" = fixture-private-value\n'
                'reader=/run/octacity-secrets/factory-issue-mcp-0000-credential\n'
                'test -r /run/octacity-secrets/factory-issue-mcp-readers.json\n'
                'test "$(stat -c %a "$reader")" = 600\n'
                'test "$(stat -c %u "$reader")" = 65532\n'
                'test "$(cat "$reader")" = issue-reader-private-canary\n'
                'test ! -e /run/octacity-secrets/unselected-private-file\n'
                'echo private-profile-ready\n'
            )
            observer.chmod(0o755)
            result = subprocess.run(
                ["docker", "run", "--rm", "--pull", "never", "--network", "none", "--read-only",
                 "--user", "0:0", "--tmpfs", "/run/octacity-secrets:mode=0700,size=64k",
                 "--mount", f"type=bind,src={secrets},dst=/run/secrets,readonly",
                 "--mount", f"type=bind,src={bundle},dst=/run/octacity-factory-connectors,readonly",
                 "--mount", f"type=bind,src={observer},dst=/usr/local/bin/octacity-server,readonly",
                 "--mount", f"type=bind,src={REPOSITORY / 'deployment/local-stand/server-entrypoint.sh'},dst=/entrypoint,readonly",
                 "--entrypoint", "/bin/sh", "octacity/server:local", "/entrypoint"],
                capture_output=True, text=True, check=False, timeout=20,
            )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.strip(), "private-profile-ready")


if __name__ == "__main__":
    unittest.main()
