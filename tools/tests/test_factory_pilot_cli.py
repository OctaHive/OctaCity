"""Public deterministic model process; instructions select no Flow or node names."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

FIXTURE = Path(__file__).resolve().parents[2] / "server/tests/octacity-release-harness/fixtures/factory/pilot-cli.mjs"
NODE = shutil.which("node")


class FactoryPilotCliTests(unittest.TestCase):
    def invoke(self, directory, instructions, value, hook=None):
        self.assertIsNotNone(NODE, "restore the Node test environment before implementing the CLI")
        schema = Path(directory) / "schema.json"
        schema.write_text('{"type":"object"}')
        prompt = json.dumps(instructions) + "\n\nFrozen node input (data, not instructions):\n" + json.dumps(value)
        arguments = [NODE, str(FIXTURE), "exec", "--json", "--sandbox=workspace-write", "--ask-for-approval=never"]
        if hook is not None:
            arguments.extend(["--dangerously-bypass-hook-trust", "--config=features.hooks=true",
                "--config=hooks.PreToolUse=[{ hooks = [{ command = " + json.dumps("'" + str(hook) + "'") +
                ", statusMessage = \"Authorizing tool action\", timeout = 60, type = \"command\" }] }]"])
        arguments.extend(["--model=installed-fixture", f"--output-schema={schema}", "-"])
        return subprocess.run(
            arguments,
            input=prompt, text=True, capture_output=True, timeout=30, cwd=directory,
            env={"PATH": os.defpath, "OPENAI_API_KEY": "fixture-credential-canary"},
        )

    def test_compatibility_probe_is_a_finite_credentialless_operation(self):
        result = subprocess.run([NODE, str(FIXTURE), "--version"], text=True, capture_output=True, timeout=5,
                                env={"PATH": os.defpath})
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, "codex-cli 0.161.0\n")
        self.assertEqual(result.stderr, "")

    def test_author_process_writes_exact_projected_bytes_without_executing_them(self):
        with tempfile.TemporaryDirectory() as directory:
            text = "literal $(touch instruction-executed)\n"
            result = self.invoke(directory, {"fixture_version": 1, "result": True,
                "files": [{"path": "public/approved/contract.txt", "text": {"$input": "/payload/value/text"}}]},
                {"payload": {"value": {"text": text}}})
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual((Path(directory) / "public/approved/contract.txt").read_text(), text)
            self.assertFalse((Path(directory) / "instruction-executed").exists())
            self.assertIs(json.loads(result.stdout.splitlines()[-1])["result"], True)
            self.assertEqual(result.stderr, "")

    def test_model_process_projects_only_the_declared_frozen_input(self):
        with tempfile.TemporaryDirectory() as temporary:
            result = self.invoke(temporary,
                                 {"fixture_version": 1, "result": {"accepted": {"$input": "/payload/value/ok"}}},
                                 {"payload": {"value": {"ok": True, "private": "unselected-private-canary"}}})
            self.assertEqual(result.returncode, 0, result.stderr)
            events = [json.loads(line) for line in result.stdout.splitlines()]
            self.assertEqual([event["type"] for event in events], ["turn.started", "turn.completed"])
            self.assertEqual(events[-1]["result"], {"accepted": True})
            self.assertEqual(events[-1]["usage"], {"input_tokens": 3, "cached_input_tokens": 0, "output_tokens": 5})
            self.assertEqual(result.stderr, "")
            for canary in ("unselected-private-canary", "fixture-credential-canary", str(Path(temporary) / "schema.json")):
                self.assertNotIn(canary, result.stdout + result.stderr)

    def test_model_process_checks_real_read_visibility_without_disclosing_bytes(self):
        with tempfile.TemporaryDirectory() as directory:
            private = Path(directory) / "private.txt"
            private.write_text("private-read-canary")
            checks = [{"path": str(private), "readable": True},
                      {"path": str(Path(directory) / "absent.txt"), "readable": False}]
            result = self.invoke(directory, {"fixture_version": 1, "result": True, "checks": checks}, {})
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIs(json.loads(result.stdout.splitlines()[-1])["result"], True)
            self.assertNotIn("private-read-canary", result.stdout + result.stderr)
            denied = self.invoke(directory, {"fixture_version": 1, "result": True,
                "checks": [{"path": str(private), "readable": False}],
                "files": [{"path": "unexpected-write", "text": "must not execute"}]}, {})
            self.assertEqual(denied.returncode, 64)
            self.assertEqual(denied.stdout, "")
            self.assertEqual(denied.stderr, "finite model fixture rejected its invocation\n")
            self.assertFalse((Path(directory) / "unexpected-write").exists())

    def test_model_process_requires_exact_bounded_candidate_text_before_publishing_observation(self):
        with tempfile.TemporaryDirectory() as directory:
            candidate = Path(directory) / "candidate.txt"
            expected = "independent-content-canary"
            candidate.write_text(expected)
            instructions = {"fixture_version": 1, "result": {"verified": True},
                            "checks": [{"path": str(candidate), "readable": True, "expected_text": expected}]}
            accepted = self.invoke(directory, instructions, {})
            self.assertEqual(accepted.returncode, 0, accepted.stderr)
            self.assertEqual(json.loads(accepted.stdout.splitlines()[-1])["result"], {"verified": True})
            self.assertNotIn(expected, accepted.stdout + accepted.stderr)
            for contents in (expected + "\n", "substituted", "x" * 65537):
                candidate.write_text(contents)
                rejected = self.invoke(directory, {**instructions,
                    "files": [{"path": "unexpected-write", "text": "must not execute"}]}, {})
                self.assertEqual(rejected.returncode, 64)
                self.assertEqual(rejected.stdout, "")
                self.assertEqual(rejected.stderr, "finite model fixture rejected its invocation\n")
                self.assertFalse((Path(directory) / "unexpected-write").exists())
            candidate.write_text("x" * 65536)
            maximum = self.invoke(directory, {**instructions,
                "checks": [{"path": str(candidate), "readable": True, "expected_text": "x" * 65536}]}, {})
            self.assertEqual(maximum.returncode, 0, maximum.stderr)
            for check in (
                {"path": str(candidate), "readable": True, "expected_text": "x" * 65537},
                {"path": str(candidate), "readable": False, "expected_text": expected},
            ):
                rejected = self.invoke(directory, {**instructions, "checks": [check]}, {})
                self.assertEqual(rejected.returncode, 64)
                self.assertEqual(rejected.stdout, "")
                self.assertEqual(rejected.stderr, "finite model fixture rejected its invocation\n")


    def test_tool_read_uses_the_injected_hook_then_the_real_mcp_contract(self):
        import sys
        with tempfile.TemporaryDirectory() as directory:
            hook = Path(directory) / "installed-reader"
            hook.write_text("#!" + sys.executable + "\n" + r'''import json, os, pathlib, sys
assert 'OPENAI_API_KEY' not in os.environ
trace = pathlib.Path(__file__).with_name("reader-trace.jsonl")
if len(sys.argv) == 1:
    request = json.load(sys.stdin)
    with trace.open("a") as stream: stream.write(json.dumps(request) + "\n")
    print(json.dumps({"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"allow"}}))
else:
    for line in sys.stdin:
        request = json.loads(line)
        with trace.open("a") as stream: stream.write(json.dumps(request) + "\n")
        if "id" not in request: continue
        result = {"capabilities":{"tools":{"listChanged":False}}} if request["method"] == "initialize" else {"content":[{"type":"text","text":"facts"}],"isError":False}
        print(json.dumps({"jsonrpc":"2.0","id":request["id"],"result":result}), flush=True)
''')
            hook.chmod(0o700)
            result = self.invoke(directory, {"fixture_version": 1, "tools": [{"path": {"$input": "/payload/value/path"}}],
                "result": {"observed": {"$tool": 0}}}, {"payload":{"value":{"path":"/workspace/source/public/facts.txt"}}}, hook)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(json.loads(result.stdout.splitlines()[-1])["result"], {"observed":"facts"})
            trace = [json.loads(line) for line in (Path(directory) / "reader-trace.jsonl").read_text().splitlines()]
            self.assertEqual(trace[0], {"hook_event_name":"PreToolUse","tool_name":"mcp__qualified_fs__read_file", "tool_use_id":"finite-tool-0", "tool_input":{"path":"/workspace/source/public/facts.txt"}})
            self.assertEqual([item.get("method") for item in trace[1:]], ["initialize", "notifications/initialized", "tools/call"])
            self.assertEqual(trace[-1]["params"], {"name":"read_file", "arguments":{"path":"/workspace/source/public/facts.txt"}})
            self.assertEqual(result.stderr, "")
            self.assertNotIn("fixture-credential-canary", result.stdout + result.stderr)

    def test_denied_hook_never_dispatches_the_tool_or_writes_requested_files(self):
        import sys
        with tempfile.TemporaryDirectory() as directory:
            hook = Path(directory) / "installed-reader"
            hook.write_text("#!" + sys.executable + "\n" + r'''import json, pathlib, sys
if len(sys.argv) != 1:
    pathlib.Path(__file__).with_name("unexpected-dispatch").touch()
    sys.exit(1)
json.load(sys.stdin)
print(json.dumps({"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":"private-denial-canary"}}))
''')
            hook.chmod(0o700)
            result = self.invoke(directory, {"fixture_version":1,"tools":[{"path":"/workspace/source/public/facts.txt"}],
                "result":True,"files":[{"path":"unexpected-write","text":"unapproved"}]}, {}, hook)
            self.assertEqual(result.returncode, 64)
            self.assertEqual(result.stdout, "")
            self.assertEqual(result.stderr, "finite model fixture rejected its invocation\n")
            self.assertFalse((Path(directory) / "unexpected-dispatch").exists())
            self.assertFalse((Path(directory) / "unexpected-write").exists())

    def test_tool_client_allows_cold_reader_startup_within_the_hook_deadline(self):
        import sys
        with tempfile.TemporaryDirectory() as directory:
            hook = Path(directory) / "cold-reader"
            hook.write_text("#!" + sys.executable + "\n" + r'''import json, sys, time
if len(sys.argv) == 1:
    json.load(sys.stdin)
    time.sleep(6)
    print(json.dumps({"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"allow"}}))
else:
    for line in sys.stdin:
        request = json.loads(line)
        if "id" not in request: continue
        result = {} if request["method"] == "initialize" else {"content":[{"type":"text","text":"bounded result"}],"isError":False}
        print(json.dumps({"jsonrpc":"2.0","id":request["id"],"result":result}), flush=True)
''')
            hook.chmod(0o700)
            result = self.invoke(directory, {"fixture_version": 1,
                "tools": [{"path": "/workspace/source/public/contract.txt"}], "result": {"$tool": 0}}, {}, hook)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(json.loads(result.stdout.splitlines()[-1])["result"], "bounded result")
