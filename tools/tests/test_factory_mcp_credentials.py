"""Opt-in real plugin/native-client qualification against finite local peers.

The launch shim changes only the model provider to a local Responses fixture;
it forwards the plugin's MCP settings and environment unchanged to pinned Codex.
No production client, provider wrapper or network credential is installed.
"""
from contextlib import contextmanager
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import shutil
import socket
import ssl
import subprocess
import sys
import tempfile
import threading
import time
import unittest

from tools.tests.fixture_environment import fixture_environment

REPOSITORY = Path(__file__).resolve().parents[2]
PEER = REPOSITORY / "server/tests/octacity-release-harness/fixtures/factory/role-mcp.mjs"
NATIVE = os.environ.get("OCTACITY_TEST_CODEX_BIN")
PLUGIN = os.environ.get("OCTACITY_TEST_CODEX_PLUGIN_BIN")
TOKENS = {"model": "fixture-model-only", "http": "fixture-http-read-alpha", "stdio": "fixture-stdio-read-alpha",
          "source": "fixture-source-only", "reporter": "fixture-reporter-only"}


@contextmanager
def serving(handler, certificate=None):
    server = ThreadingHTTPServer(("127.0.0.1", 0), handler)
    if certificate:
        context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
        context.load_cert_chain(*certificate)
        server.socket = context.wrap_socket(server.socket, server_side=True)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        yield server.server_port
    finally:
        server.shutdown()
        server.server_close()
        thread.join(timeout=3)


@unittest.skipUnless(os.name == "posix" and NATIVE and PLUGIN, "select pinned native Codex and plugin binaries explicitly")
class NativeMcpCredentialTests(unittest.TestCase):
    def invoke(self, token=TOKENS["http"], *, cancel_during_mcp=False, peer_mode="facts"):
        self.assertEqual(subprocess.check_output([NATIVE, "--version"], text=True).strip(), "codex-cli 0.161.0")
        node = shutil.which("node")
        self.assertIsNotNone(node)
        calls, model_requests = [], []
        call_started, release_call = threading.Event(), threading.Event()
        plan = [("remote", "facts", "alpha"), ("local", "facts", "alpha"),
                ("remote", "facts", "beta"), ("local", "write", "alpha")]

        class Peer(BaseHTTPRequestHandler):
            def log_message(self, *_): pass
            def do_POST(self):
                if self.headers.get("Authorization") != "Bearer " + TOKENS["http"]:
                    calls.append({"peer": "remote", "authenticated": False})
                    self.send_response(401); self.send_header("Content-Length", "0"); self.end_headers(); return
                request = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
                if "id" not in request:
                    self.send_response(202); self.send_header("Content-Length", "0"); self.end_headers(); return
                method = request["method"]
                if method == "initialize":
                    result = {"protocolVersion": request["params"]["protocolVersion"], "capabilities": {"tools": {}}, "serverInfo": {"name": "read-peer", "version": "1"}}
                elif method == "tools/list":
                    result = {"tools": [{"name": "facts", "description": "Finite read", "annotations": {"readOnlyHint": True}, "inputSchema": {"type": "object", "properties": {"resource": {"type": "string"}}, "required": ["resource"], "additionalProperties": False}}]}
                else:
                    params = request["params"]
                    allowed = params["name"] == "facts" and params.get("arguments", {}).get("resource") == "alpha"
                    calls.append({"peer": "remote", "tool": params["name"], "resource": params.get("arguments", {}).get("resource"), "allowed": allowed})
                    if cancel_during_mcp:
                        call_started.set()
                        release_call.wait(timeout=20)
                        return
                    if peer_mode == "unavailable":
                        self.send_response(503); self.send_header("Content-Length", "0"); self.end_headers(); return
                    if peer_mode == "malformed":
                        result = {"content": "invalid MCP content"}
                    elif peer_mode == "empty" and allowed:
                        result = {"content": [], "isError": False}
                    else:
                        result = {"content": [{"type": "text", "text": "retained alpha facts" if allowed else "credential does not permit this resource"}], "isError": not allowed}
                body = json.dumps({"jsonrpc": "2.0", "id": request["id"], "result": result}).encode()
                self.send_response(200); self.send_header("Content-Type", "application/json"); self.send_header("Content-Length", str(len(body))); self.end_headers(); self.wfile.write(body)

        class Model(BaseHTTPRequestHandler):
            def log_message(self, *_): pass
            def do_POST(self):
                request = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
                model_requests.append({"authorization": self.headers.get("Authorization"), "tools": [tool for tool in request.get("tools", []) if tool.get("name", "").startswith("mcp__")]})
                index = len(model_requests) - 1
                if index < len(plan):
                    peer, tool, resource = plan[index]
                    item = {"type": "function_call", "id": f"item-{index}", "call_id": f"call-{index}", "namespace": f"mcp__{peer}", "name": tool, "arguments": json.dumps({"resource": resource})}
                else:
                    item = {"type": "message", "id": "answer", "role": "assistant", "content": [{"type": "output_text", "text": "true", "annotations": []}]}
                events = [{"type": "response.created", "response": {"id": f"response-{index}", "status": "in_progress", "output": []}},
                          {"type": "response.output_item.done", "output_index": 0, "item": item},
                          {"type": "response.completed", "response": {"id": f"response-{index}", "status": "completed", "output": [item], "usage": {"input_tokens": 1, "output_tokens": 1, "total_tokens": 2, "input_tokens_details": {"cached_tokens": 0}}}}]
                body = "".join("event: " + event["type"] + "\ndata: " + json.dumps(event) + "\n\n" for event in events).encode()
                self.send_response(200); self.send_header("Content-Type", "text/event-stream"); self.send_header("Content-Length", str(len(body))); self.end_headers(); self.wfile.write(body)

        with tempfile.TemporaryDirectory(prefix="ocmcp-", dir=Path("/tmp").resolve()) as temporary:
            root = Path(temporary)
            certificate, key = root / "ca.pem", root / "key.pem"
            subprocess.run(["openssl", "req", "-x509", "-newkey", "rsa:2048", "-nodes", "-keyout", str(key), "-out", str(certificate), "-days", "1", "-subj", "/CN=localhost", "-addext", "subjectAltName=DNS:localhost,IP:127.0.0.1", "-addext", "basicConstraints=critical,CA:FALSE"], check=True, capture_output=True, timeout=20)
            with serving(Peer, (certificate, key)) as peer_port, serving(Model) as model_port:
                launcher = root / "finite-model-launcher"
                provider = f'--config=model_providers.finite={{name="Finite model",base_url="http://127.0.0.1:{model_port}/v1",wire_api="responses",env_key="OPENAI_API_KEY",supports_websockets=false,request_max_retries=0}}'
                launcher.write_text(f'#!{sys.executable}\nimport os,sys\nargs=sys.argv[1:]\nif "exec" in args: args=args[:-1]+[\'--config=model_provider="finite"\',{provider!r}]+args[-1:]\nos.execv({NATIVE!r},[{NATIVE!r}]+args)\n')
                launcher.chmod(0o700)
                home = root / "home"; home.mkdir(mode=0o700)
                audit = root / "stdio-audit.jsonl"
                values = {"MODEL": TOKENS["model"], "HTTP": token, "STDIO": TOKENS["stdio"], "SOURCE": TOKENS["source"], "REPORTER": TOKENS["reporter"], "HOME_DIR": str(home), "CA": str(certificate), "AUDIT": str(audit)}
                secret = {"OPENAI_API_KEY": "MODEL", "HTTP_READ_TOKEN": "HTTP", "STDIO_READ_TOKEN": "STDIO"}
                if token is None:
                    values.pop("HTTP")
                params = {"prompt": "Use the declared finite tools, then return true.", "model": "finite-model", "cli_arguments": ["--ephemeral", "--skip-git-repo-check"],
                          "mcp_servers": {"remote": {"transport": "http", "url": f"https://localhost:{peer_port}/mcp", "bearer_token_env_var": "HTTP_READ_TOKEN", "enabled_tools": ["facts"]},
                                          "local": {"transport": "stdio", "command": node, "args": [str(PEER)], "env_vars": ["STDIO_READ_TOKEN", "PEER_RESOURCE", "PEER_AUDIT_FILE"], "enabled_tools": ["facts", "write"]}},
                          "environment": {"secret": secret, "public": {"HOME": "HOME_DIR", "SSL_CERT_FILE": "CA", "PEER_AUDIT_FILE": "AUDIT", "PEER_RESOURCE": "RESOURCE", "HTTP_PROXY": "PROXY", "HTTPS_PROXY": "PROXY", "ALL_PROXY": "PROXY", "NO_PROXY": "DIRECT"}}}
                values.update(RESOURCE="alpha", PROXY="http://127.0.0.1:9", DIRECT="localhost,127.0.0.1")
                path = root / "p.sock"
                process = subprocess.Popen([PLUGIN, "--socket-path", str(path), "--log-dir", str(root / "logs")], cwd=root, env=fixture_environment(OCTA_CODEX_EXECUTABLE=str(launcher)), stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=True)
                try:
                    deadline = time.monotonic() + 5
                    while not path.exists() and time.monotonic() < deadline:
                        if process.poll() is not None: self.fail("pinned plugin failed to start")
                        time.sleep(0.02)
                    with socket.socket(socket.AF_UNIX) as connection:
                        connection.settimeout(45); connection.connect(str(path))
                        stream = connection.makefile("rwb", buffering=0)
                        def send(command): stream.write(json.dumps(command).encode()+b"\n")
                        def receive(): return json.loads(stream.readline(1024*1024+1))
                        send({"type": "Hello", "payload": {"protocol_version": 2, "version": "0.7.2", "features": []}})
                        self.assertEqual(receive()["type"], "Hello")
                        send({"type": "Schema"}); self.assertEqual(receive()["type"], "Schema")
                        send({"type": "Execute", "payload": {"id": "credential-qualification", "params": params, "args": [], "dir": str(root), "envs": {"PATH": os.defpath, "REPORTER_TOKEN": TOKENS["reporter"]}, "vars": values, "secret_vars": ["MODEL", "HTTP", "STDIO", "SOURCE", "REPORTER"], "redact_params": True, "raw": False, "dry": False}})
                        responses = []
                        cancelled = False
                        while True:
                            response = receive(); responses.append(response)
                            if cancel_during_mcp and not cancelled and response["type"] == "Progress" and response["payload"]["progress"]["message"] == "Codex is calling an MCP tool":
                                self.assertTrue(call_started.wait(timeout=15), "native client did not begin the MCP call")
                                send({"type": "Cancel", "payload": {"id": "credential-qualification"}})
                                cancelled = True
                            if response["type"] in {"Completed", "Error"}: break
                        self.assertEqual(cancelled, cancel_during_mcp)
                        send({"type": "Shutdown"})
                    retained = b"".join(file.read_bytes() for file in root.rglob("*") if file.is_file() and "codex-runs" in file.parts)
                    local = [json.loads(line) for line in audit.read_text().splitlines()] if audit.exists() else []
                    return responses, calls, local, model_requests, retained
                finally:
                    release_call.set()
                    if process.poll() is None:
                        process.terminate()
                    process.communicate(timeout=5)

    def test_native_mcp_cancellation_retains_observed_call_evidence(self):
        responses, remote, local, model, retained = self.invoke(cancel_during_mcp=True)
        self.assertEqual(responses[-1]["type"], "Completed")
        self.assertNotEqual(responses[-1]["payload"]["code"], 0)
        self.assertEqual([(row["tool"], row["resource"]) for row in remote], [("facts", "alpha")])
        self.assertTrue(
            any(response["type"] == "RegisterArtifact"
                and response["payload"]["artifact"]["name"] == "codex-run-trace"
                for response in responses),
            "an observed MCP call must remain inspectable after cancellation",
        )
        records, provenance = self.native_capture(retained)
        self.assertEqual(provenance["capture"]["status"], "cancelled")
        calls = [row["event"]["item"] for row in records if row["event"].get("item", {}).get("type") == "mcp_tool_call"]
        self.assertEqual([(row["server"], row["tool"], row["arguments"], row["status"]) for row in calls],
                         [("remote", "facts", {"resource": "alpha"}, "in_progress")])
        self.assertIsNone(calls[0]["result"])
        for token in TOKENS.values():
            self.assertNotIn(token, json.dumps(responses))
            self.assertNotIn(token.encode(), retained)

    def test_native_mcp_credentials_are_isolated_and_enforce_read_resource_scope(self):
        responses, remote, local, model, retained = self.invoke()
        self.assertEqual(responses[-1]["type"], "Completed", responses)
        self.assertEqual(responses[-1]["payload"]["code"], 0, responses)
        self.assertEqual([(row["resource"], row["allowed"]) for row in remote], [("alpha", True), ("beta", False)])
        self.assertEqual([(row["tool"], row["allowed"]) for row in local], [("facts", True), ("write", False)])
        self.assertTrue(model)
        self.assertTrue(all(row["authorization"] == "Bearer " + TOKENS["model"] for row in model))
        records, provenance = self.native_capture(retained)
        self.assertEqual(provenance["capture"]["status"], "complete")
        completed = [row["event"]["item"] for row in records if row["event"]["type"] == "item.completed" and row["event"].get("item", {}).get("type") == "mcp_tool_call"]
        self.assertEqual([(row["server"], row["tool"], row["arguments"], row["status"]) for row in completed],
                         [("remote", "facts", {"resource": "alpha"}, "completed"),
                          ("local", "facts", {"resource": "alpha"}, "completed"),
                          ("remote", "facts", {"resource": "beta"}, "failed"),
                          ("local", "write", {"resource": "alpha"}, "failed")])
        self.assertEqual(completed[0]["result"]["content"], [{"type": "text", "text": "retained alpha facts"}])
        self.assertEqual(completed[1]["result"]["content"], completed[0]["result"]["content"])
        self.assertIn("does not permit", completed[2]["result"]["content"][0]["text"])
        started = [row["event"]["item"]["id"] for row in records if row["event"]["type"] == "item.started" and row["event"].get("item", {}).get("type") == "mcp_tool_call"]
        self.assertEqual(started, [row["id"] for row in completed])
        for token in TOKENS.values():
            self.assertNotIn(token, json.dumps(responses))
            self.assertNotIn(token.encode(), retained)

    def native_capture(self, retained):
        """Inspect declared record documents, never the simulated model's prose."""
        documents, decoder, text = [], json.JSONDecoder(), retained.decode()
        while text.strip():
            text = text.lstrip()
            document, end = decoder.raw_decode(text)
            documents.append(document); text = text[end:]
        records = [row for row in documents if "event" in row]
        provenance, = [row for row in documents if "capture" in row]
        self.assertEqual(provenance["format_version"], 2)
        self.assertEqual(provenance["capture"]["coverage"], "emitted_native_events")
        self.assertEqual(provenance["capture"]["record_count"], len(records))
        self.assertEqual([row["sequence"] for row in records], list(range(len(records))))
        observed = [row["observed_at_unix_millis"] for row in records]
        self.assertEqual(observed, sorted(observed))
        for row in records:
            self.assertEqual(row["format_version"], 2)
            self.assertGreaterEqual(row["observed_at_unix_millis"], provenance["timing"]["started_unix_millis"])
            self.assertLessEqual(row["observed_at_unix_millis"], provenance["timing"]["finished_unix_millis"])
        return records, provenance

    def test_native_mcp_empty_unavailable_and_malformed_results_remain_distinct(self):
        for mode in ("empty", "unavailable", "malformed"):
            with self.subTest(mode=mode):
                responses, remote, local, model, retained = self.invoke(peer_mode=mode)
                records, provenance = self.native_capture(retained)
                completed = [row["event"]["item"] for row in records if row["event"]["type"] == "item.completed" and row["event"].get("item", {}).get("server") == "remote"]
                self.assertEqual(len(completed), 2)
                first = completed[0]
                if mode == "empty":
                    self.assertEqual(first["status"], "completed")
                    self.assertEqual(first["result"]["content"], [])
                    self.assertIsNone(first["error"])
                else:
                    self.assertEqual(first["status"], "failed")
                    self.assertIsNone(first["result"])
                    self.assertTrue(first["error"])
                for token in TOKENS.values():
                    self.assertNotIn(token, json.dumps(responses))
                    self.assertNotIn(token.encode(), retained)

    def test_native_mcp_missing_expired_or_revoked_credentials_have_no_model_or_reporter_fallback(self):
        for token in (None, "expired-mcp-read", "revoked-mcp-read"):
            with self.subTest(state=token):
                responses, remote, local, model, retained = self.invoke(token)
                self.assertTrue(responses[-1]["type"] == "Error" or responses[-1]["payload"].get("code") != 0, responses)
                self.assertFalse(any(row.get("allowed") for row in remote))
                self.assertFalse(local)
                for value in TOKENS.values():
                    self.assertNotIn(value, json.dumps(responses))
                    self.assertNotIn(value.encode(), retained)
