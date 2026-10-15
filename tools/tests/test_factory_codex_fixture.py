"""Real CLI process contracts for the adapter-local, hermetic Factory fixture."""
from __future__ import annotations

import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

FIXTURE = Path(__file__).resolve().parents[2] / "server/tests/octacity-release-harness/fixtures/factory/codex-cli.sh"


@unittest.skipUnless(os.name == "posix", "the Codex fixture requires a Unix shell")
class FactoryCodexFixtureTests(unittest.TestCase):
    def test_compatibility_probe_reports_the_pinned_executable_version(self):
        result = subprocess.run(["/bin/sh", str(FIXTURE), "--version"], capture_output=True, text=True, check=False, timeout=5)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, "codex-cli 0.161.0\n")
        self.assertEqual(result.stderr, "")

    def invoke(self, schema: str, *, extra: tuple[str, ...] = (), prompt: str = "private-task-canary"):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "private-schema-canary.json"
            path.write_text(schema)
            return subprocess.run(
                ["/bin/sh", str(FIXTURE), "exec", "--json", "--sandbox=workspace-write",
                 "--ask-for-approval=never", "--model=operator-selected-exact-model",
                 f"--output-schema={path}", *extra, "-"],
                input=prompt, text=True, capture_output=True, check=False, timeout=5,
                env={"PATH": os.defpath, "OPENAI_API_KEY": "fixture-credential-canary"},
            )

    def test_normal_adapter_invocation_observes_the_frozen_boolean_schema_and_reports_tokens(self):
        result = self.invoke(' { "type" : "boolean" } \n')
        self.assertEqual(result.returncode, 0, result.stderr)
        events = [json.loads(line) for line in result.stdout.splitlines()]
        self.assertEqual([event["type"] for event in events], ["turn.started", "turn.completed"])
        self.assertIs(events[-1]["result"], True)
        self.assertEqual(events[-1]["usage"], {"input_tokens": 3, "cached_input_tokens": 0, "output_tokens": 5})
        self.assertNotIn("cost_micro_units", events[-1]["usage"])
        self.assertEqual(result.stderr, "")
        for private in ("private-task-canary", "fixture-credential-canary", "private-schema-canary"):
            self.assertNotIn(private, result.stdout + result.stderr)

    def test_fixture_rejects_unsupported_schema_or_arguments_without_partial_events(self):
        for schema, extra, prompt in (
            ('{"type":"object","private":"schema-content-canary"}', (), "private-task-canary"),
            ('{"type":"boolean"}', ("--undeclared-route=credential-crossing-canary",), "private-task-canary"),
            ('{"type":"boolean"}', (), ""),
            ('{"type":"boolean"}', (), "x" * (1048576 + 1)),
        ):
            with self.subTest(schema=schema, extra=extra, prompt_bytes=len(prompt)):
                result = self.invoke(schema, extra=extra, prompt=prompt)
                self.assertEqual(result.returncode, 64)
                self.assertEqual(result.stdout, "")
                self.assertEqual(result.stderr, "hermetic Codex fixture rejected its invocation\n")

    def test_fixture_is_an_executable_operator_selected_process(self):
        self.assertTrue(os.access(FIXTURE, os.X_OK))

    @unittest.skipUnless(shutil.which("docker"), "Docker is required for the Linux process boundary")
    def test_fixture_runs_as_a_read_only_networkless_linux_process(self):
        inspected = subprocess.run(
            ["docker", "image", "inspect", "octacity/server:local", "--format", "{{.Id}}"],
            capture_output=True, text=True, check=False, timeout=5,
        )
        if inspected.returncode:
            self.skipTest("build the local server image to provide its Linux process environment")
        with tempfile.TemporaryDirectory() as temporary:
            schema = Path(temporary).resolve() / "schema.json"
            schema.write_text('{"type":"boolean"}\n')
            schema.chmod(0o644)
            result = subprocess.run(
                ["docker", "run", "--rm", "--pull", "never", "--network", "none", "--read-only", "--interactive",
                 "--cap-drop", "ALL", "--security-opt", "no-new-privileges:true", "--user", "65532:65532",
                 "--mount", f"type=bind,src={FIXTURE},dst=/fixture/codex,readonly",
                 "--mount", f"type=bind,src={schema},dst=/fixture/schema.json,readonly",
                 "--entrypoint", "/fixture/codex", inspected.stdout.strip(), "exec", "--json", "--sandbox=workspace-write",
                 "--ask-for-approval=never", "--model=operator-selected-exact-model", "--output-schema=/fixture/schema.json", "-"],
                input="private-linux-prompt-canary", text=True, capture_output=True, check=False, timeout=10,
            )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIs(json.loads(result.stdout.splitlines()[-1])["result"], True)
        self.assertNotIn("private-linux-prompt-canary", result.stdout + result.stderr)


if __name__ == "__main__":
    unittest.main()
