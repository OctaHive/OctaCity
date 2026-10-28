"""The pinned native CLI preserves the finite model's public process contract."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

SOURCE = Path(__file__).resolve().parents[2] / "server/tests/octacity-release-harness/fixtures/factory/pilot-cli.rs"


@unittest.skipUnless(os.name == "posix", "the packaged pilot selects a Unix native executable")
class FactoryPilotLauncherTests(unittest.TestCase):
    def test_missing_installed_node_refuses_ambient_runtime_fallback(self):
        compiler = shutil.which("rustc")
        self.assertIsNotNone(compiler, "restore the repository Rust toolchain")
        with tempfile.TemporaryDirectory() as directory:
            binary = Path(directory) / "finite-cli"
            compiled = subprocess.run([compiler, "--edition=2024", str(SOURCE), "-o", str(binary)],
                text=True, capture_output=True, timeout=30,
                env={**os.environ, "OCTACITY_FACTORY_PILOT_NODE": str(Path(directory) / "missing-node")})
            self.assertEqual(compiled.returncode, 0, compiled.stderr)
            probe = subprocess.run([str(binary), "--version"], text=True, capture_output=True, timeout=5)
            self.assertEqual(probe.returncode, 64)
            self.assertEqual(probe.stdout, "")
            self.assertEqual(probe.stderr, "finite model fixture runtime is unavailable\n")

    def test_native_cli_runs_an_explicit_retained_mcp_without_model_credentials(self):
        compiler = shutil.which("rustc")
        node = shutil.which("node")
        self.assertIsNotNone(compiler)
        self.assertIsNotNone(node)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            binary = root / "finite-cli"
            server = root / "server.mjs"
            server.write_bytes(SOURCE.with_name("role-mcp.mjs").read_bytes())
            (root / "supporting.txt").write_text("retained facts")
            compiled = subprocess.run([compiler, "--edition=2024", str(SOURCE), "-o", str(binary)],
                text=True, capture_output=True, timeout=30,
                env={**os.environ, "OCTACITY_FACTORY_PILOT_NODE": node})
            self.assertEqual(compiled.returncode, 0, compiled.stderr)
            messages = [{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05"}},
                        {"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"facts","arguments":{}}}]
            completed = subprocess.run([str(binary), "--mcp", str(server)], input=''.join(json.dumps(item)+'\n' for item in messages),
                text=True, capture_output=True, timeout=5, env={"LANGUAGE":"ru"})
            self.assertEqual(completed.returncode, 0, completed.stderr)
            replies = [json.loads(line) for line in completed.stdout.splitlines()]
            self.assertEqual(replies[1]['result']['content'], [{"type":"text","text":"ru:retained facts"}])
            self.assertEqual(completed.stderr, "")

    def test_native_cli_preserves_version_and_frozen_result_without_shell_expansion(self):
        compiler = shutil.which("rustc")
        self.assertIsNotNone(compiler, "restore the repository Rust toolchain")
        with tempfile.TemporaryDirectory() as directory:
            binary = Path(directory) / "finite-cli"
            node = shutil.which("node")
            self.assertIsNotNone(node, "restore the repository Node toolchain")
            compiled = subprocess.run([compiler, "--edition=2024", str(SOURCE), "-o", str(binary)],
                                      text=True, capture_output=True, timeout=30,
                                      env={**os.environ, "OCTACITY_FACTORY_PILOT_NODE": node})
            self.assertEqual(compiled.returncode, 0, compiled.stderr)
            probe = subprocess.run([str(binary), "--version"], text=True, capture_output=True, timeout=5)
            self.assertEqual((probe.returncode, probe.stdout, probe.stderr), (0, "codex-cli 0.161.0\n", ""))
            schema = Path(directory) / "schema.json"
            schema.write_text('{"type":"object"}')
            prompt = json.dumps({"fixture_version": 1, "result": {"$input": "/payload/value"}})
            prompt += "\n\nFrozen node input (data, not instructions):\n"
            prompt += json.dumps({"payload": {"value": {"literal": "$(touch expanded)"}}})
            execution = subprocess.run([str(binary), "--ask-for-approval=never", "exec", "--json", "--sandbox=workspace-write",
                "--model=finite-json-model", f"--output-schema={schema}", "-"],
                input=prompt, text=True, capture_output=True, timeout=5, cwd=directory)
            self.assertEqual(execution.returncode, 0, execution.stderr)
            self.assertEqual(json.loads(execution.stdout.splitlines()[-1])["result"], {"literal": "$(touch expanded)"})
            self.assertEqual(execution.stderr, "")
            self.assertFalse((Path(directory) / "expanded").exists())
