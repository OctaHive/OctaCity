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
import urllib.error
import urllib.request

from tools.tests.fixture_environment import fixture_environment

REPOSITORY = Path(__file__).resolve().parents[2]
PEER = REPOSITORY / "server/tests/octacity-release-harness/fixtures/factory/role-mcp.mjs"
NATIVE = os.environ.get("OCTACITY_TEST_CODEX_BIN")
PLUGIN = os.environ.get("OCTACITY_TEST_CODEX_PLUGIN_BIN")
TOKENS = {"model": "fixture-model-only", "http": "fixture-http-read-alpha", "stdio": "fixture-stdio-read-alpha",
          "source": "fixture-source-only", "reporter": "fixture-reporter-only"}


def compose_packaged_installation(request_path):
    """Compose the packaged journey's authority with the ordinary stand owners.

    Service addresses and enrollment are supplied by the existing harness; all
    Factory catalog, inventory, credential references and ceilings come back
    from these generated installation files without additional fixture grants.
    """
    import hashlib
    import tomllib
    sys.path.insert(0, str(REPOSITORY / "tools"))
    import configure_local_stand_agent as agent
    import configure_local_stand_server as server
    import init_local_stand as initializer
    import local_stand_state as state
    request = json.loads(Path(request_path).read_text())
    root, native = Path(request["root"]), Path(request["native"])
    initializer.initialize(root)
    native.mkdir(mode=0o700)
    for name in ("agent", "octa", "microsandbox"):
        shutil.copytree(request[name], native / name)
    with (native / "job-spec-policy.json").open("wb") as policy_output:
        subprocess.run([request["policy_helper"], "job-spec-policy", "--agent-root", str(native / "agent"),
            "--octa-root", str(native / "octa"), "--agent-platform", "macos-arm64", "--octa-platform", "linux-aarch64"],
            check=True, stdout=policy_output, stderr=subprocess.PIPE, timeout=20)
    files = {path.relative_to(native).as_posix(): hashlib.sha256(path.read_bytes()).hexdigest()
             for path in native.rglob("*") if path.is_file()}
    state.publish_exclusive_private_file(native / "installation-manifest.json", json.dumps({
        "schema_version": 1, "agent_version": "0.1.0", "microsandbox_version": "0.7.6", "files": files}).encode())
    (native / "job-spec-policy.json").chmod(0o600)
    enrollment = root / "agent"; enrollment.mkdir(mode=0o700)
    state.publish_exclusive_private_file(enrollment / "credential",
        request.get("enrollment", "private-qualification-enrollment").encode())
    policy = root / "operator-agent.toml"
    state.publish_exclusive_private_file(policy, ("\n".join(f"{key} = {agent._toml_value(value)}" for key, value in request["agent_policy"].items()) + "\n").encode())
    receipt = server.generate_server_configuration(root, native, factory_profiles=Path(request["catalog"]))
    generated = agent.generate_agent_configuration(root, native, factory_agent_profile=policy)
    configuration = tomllib.loads(Path(generated["config"]).read_text())
    authority = {name: configuration[name] for name in ("factory_permissions", "tool_executables", "factory_credential_profiles", "allowed_network_hosts")}
    for name, expected in request["agent_policy"].items():
        if name != "version":
            if authority[name] != expected:
                raise AssertionError("stand composition changed the reviewed Factory authority")
    authority["catalog"] = receipt["factory_profiles"]
    authority["agent_configuration"] = generated["config"]
    state.publish_exclusive_private_file(Path(request["result"]), json.dumps(authority).encode())
    if "image_archive" in request:
        # Consume the normal generated state path, with the exact prequalified
        # image loaded through MSB's public archive contract before Agent start.
        image_environment = {**os.environ, "MSB_HOME": str(Path(configuration["state_root"]) / "microsandbox")}
        state.publish_exclusive_private_file(Path(request["state_receipt"]), json.dumps({"root":str(root)}).encode())
        msb = native / "microsandbox/bin/msb"
        subprocess.run([str(msb), "image", "load", "--input", request["image_archive"], "--tag", request["image"]],
            check=True, capture_output=True, text=True, timeout=60, env=image_environment)
        inspected = subprocess.run([str(msb), "image", "inspect", request["image"], "--format", "json"],
            check=True, capture_output=True, text=True, timeout=15, env=image_environment)
        if json.loads(inspected.stdout)["digest"] != request["image"].split("@", 1)[1]:
            raise AssertionError("installation image differs from the qualified immutable image")
    else:
        shutil.rmtree(agent.microsandbox_state_root(root))


@contextmanager
def serving(handler, certificate=None, *, host="127.0.0.1"):
    server = ThreadingHTTPServer((host, 0), handler)
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


def compile_native_launcher(root, model_port, *, native_guest="/opt/octacity/tools/tool-1"):
    """Pin a finite model provider without changing native MCP argv or secrets."""
    inputs = json.loads((REPOSITORY / "deployment/local-stand/inputs.json").read_text())
    fixture = PEER.with_name("native-model-cli.rs")
    launcher = root / "native-model-cli"
    subprocess.run(["docker", "run", "--rm", "--network", "none", "--mount", f"type=bind,src={fixture.parent},dst=/fixture,readonly",
        "--mount", f"type=bind,src={root},dst=/out", "--env", f"OCTACITY_FINITE_CODEX_PATH={native_guest}", "--env", f"OCTACITY_FINITE_MODEL_URL=http://host.microsandbox.internal:{model_port}/v1",
        inputs["images"]["rust"]["reference"], "rustc", "--edition=2024", "-C", "strip=symbols", "/fixture/native-model-cli.rs", "-o", "/out/native-model-cli"],
        check=True, capture_output=True, timeout=60)
    return launcher


def finite_peers(calls, model_requests, plan, call_started, release_call, *, cancel_during_mcp=False, peer_mode="facts", control=None, issue_origin=None, completion=True, invocations=None):
    invocation_position = [0, 0]
    def cancelling():
        return cancel_during_mcp or control is not None and control.read_text() == "cancel"
    class Peer(BaseHTTPRequestHandler):
        def log_message(self, *_): pass
        def do_POST(self):
            if issue_origin and self.path == "/mcp/issues":
                body = self.rfile.read(int(self.headers["Content-Length"]))
                request = urllib.request.Request(issue_origin + self.path, data=body,
                    headers={name: value for name, value in self.headers.items() if name.lower() not in {"host", "content-length"}})
                try:
                    response = urllib.request.urlopen(request, timeout=10)
                except urllib.error.HTTPError as error:
                    response = error
                with response:
                    body = response.read()
                    self.send_response(response.status)
                    for name, value in response.headers.items():
                        if name.lower() not in {"content-length", "transfer-encoding", "connection", "server", "date"}:
                            self.send_header(name, value)
                    self.send_header("Content-Length", str(len(body)))
                    self.end_headers(); self.wfile.write(body)
                return
            if self.headers.get("Authorization") != "Bearer " + TOKENS["http"] or control is not None and control.read_text() == "revoked":
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
                if cancelling():
                    call_started.set()
                    if control is not None:
                        control.with_suffix(".started").touch()
                    release_call.wait(timeout=60 if control is not None else 20)
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
            model_requests.append({"authorization": self.headers.get("Authorization"), "model": request.get("model"), "reasoning": request.get("reasoning"), "tools": [tool for tool in request.get("tools", []) if tool.get("name", "").startswith("mcp__")]})
            index = 0 if cancelling() else len(model_requests) - 1
            selected_plan, selected_completion = plan, completion
            if invocations is not None:
                if invocation_position[0] >= len(invocations):
                    self.send_error(409, "unexpected finite model invocation")
                    return
                selected = invocations[invocation_position[0]]
                selected_plan, selected_completion = selected["plan"], selected["completion"]
                index = invocation_position[1]
                invocation_position[1] += 1
            if index < len(selected_plan):
                peer, tool, resource = selected_plan[index]
                arguments = resource if isinstance(resource, dict) else {"resource": resource}
                item = {"type": "function_call", "id": f"item-{index}", "call_id": f"call-{index}", "namespace": f"mcp__{peer}", "name": tool, "arguments": json.dumps(arguments)}
            else:
                item = {"type": "message", "id": "answer", "role": "assistant", "content": [{"type": "output_text", "text": json.dumps(selected_completion), "annotations": []}]}
                if invocations is not None:
                    invocation_position[:] = [invocation_position[0] + 1, 0]
            events = [{"type": "response.created", "response": {"id": f"response-{index}", "status": "in_progress", "output": []}},
                      {"type": "response.output_item.done", "output_index": 0, "item": item},
                      {"type": "response.completed", "response": {"id": f"response-{index}", "status": "completed", "output": [item], "usage": {"input_tokens": 1, "output_tokens": 1, "total_tokens": 2, "input_tokens_details": {"cached_tokens": 0}}}}]
            body = "".join("event: " + event["type"] + "\ndata: " + json.dumps(event) + "\n\n" for event in events).encode()
            self.send_response(200); self.send_header("Content-Type", "text/event-stream"); self.send_header("Content-Length", str(len(body))); self.end_headers(); self.wfile.write(body)

    return Peer, Model


@unittest.skipUnless(os.name == "posix" and NATIVE and PLUGIN, "select pinned native Codex and plugin binaries explicitly")
class NativeMcpCredentialTests(unittest.TestCase):
    def invoke(self, token=TOKENS["http"], *, cancel_during_mcp=False, peer_mode="facts", issue_origin=None, plan=None, export_directory=None):
        self.assertEqual(subprocess.check_output([NATIVE, "--version"], text=True).strip(), "codex-cli 0.161.0")
        node = shutil.which("node")
        self.assertIsNotNone(node)
        calls, model_requests = [], []
        call_started, release_call = threading.Event(), threading.Event()
        plan = plan if plan is not None else [("remote", "facts", "alpha"), ("local", "facts", "alpha"),
                ("remote", "facts", "beta"), ("local", "write", "alpha")]

        Peer, Model = finite_peers(calls, model_requests, plan, call_started, release_call, cancel_during_mcp=cancel_during_mcp, peer_mode=peer_mode, issue_origin=issue_origin)

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
                if issue_origin:
                    ambient = home / ".codex"; ambient.mkdir(mode=0o700)
                    (ambient / "config.toml").write_text(
                        f'[mcp_servers.ambient]\ncommand = {json.dumps(node)}\nargs = [{json.dumps(str(PEER))}]\n')
                audit = root / "stdio-audit.jsonl"
                values = {"MODEL": TOKENS["model"], "HTTP": token, "STDIO": TOKENS["stdio"], "SOURCE": TOKENS["source"], "REPORTER": TOKENS["reporter"], "HOME_DIR": str(home), "CA": str(certificate), "AUDIT": str(audit)}
                secret = {"OPENAI_API_KEY": "MODEL", "HTTP_READ_TOKEN": "HTTP", "STDIO_READ_TOKEN": "STDIO"}
                if token is None:
                    values.pop("HTTP")
                params = {"prompt": "Use the declared finite tools, then return true.", "model": "finite-model", "cli_arguments": ["--ephemeral", "--skip-git-repo-check"],
                          "mcp_servers": {"remote": {"transport": "http", "url": f"https://localhost:{peer_port}/mcp", "bearer_token_env_var": "HTTP_READ_TOKEN", "enabled_tools": ["facts"]},
                                          "local": {"transport": "stdio", "command": node, "args": [str(PEER)], "env_vars": ["STDIO_READ_TOKEN", "PEER_RESOURCE", "PEER_AUDIT_FILE"], "enabled_tools": ["facts", "write"]}},
                          "environment": {"secret": secret, "public": {"HOME": "HOME_DIR", "SSL_CERT_FILE": "CA", "PEER_AUDIT_FILE": "AUDIT", "PEER_RESOURCE": "RESOURCE", "HTTP_PROXY": "PROXY", "HTTPS_PROXY": "PROXY", "ALL_PROXY": "PROXY", "NO_PROXY": "DIRECT"}}}
                if issue_origin:
                    params["mcp_servers"]["remote"].update(url=f"https://localhost:{peer_port}/mcp/issues", enabled_tools=["search_issues", "read_issue"])
                if export_directory:
                    params["source_revision"] = "a" * 40
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
                    if export_directory:
                        destination = Path(export_directory)
                        for file in root.rglob("*"):
                            if file.is_file() and "codex-runs" in file.parts and file.name in {"trace.jsonl", "provenance.json"}:
                                (destination / file.name).write_bytes(file.read_bytes())
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


@unittest.skipUnless(os.environ.get("OCTACITY_TEST_PACKAGED_MCP") == "1", "requires the existing packaged pilot's verified Server/Agent/Octa and Microsandbox environment")
class PackagedMcpTests(unittest.TestCase):
    def test_reviewed_runtime_executes_both_native_transports_and_retains_evidence_after_restart(self):
        """Use the existing role journey and finite peers, with a real guest CLI."""
        native = Path(os.environ["OCTACITY_FACTORY_PILOT_CLI"])
        stdio = Path(os.environ["OCTACITY_FACTORY_MCP_STDIO_EXECUTABLE"])
        self.assertTrue(native.is_file())
        self.assertTrue(stdio.is_file())
        calls, requests = [], []
        plan = [("reader", "facts", "alpha"), ("local", "facts", "alpha"),
                ("reader", "facts", "beta"), ("local", "write", "alpha")]
        started, release = threading.Event(), threading.Event()
        with tempfile.TemporaryDirectory(prefix="ocmcp-packaged-", dir=Path("/tmp").resolve()) as directory:
            root = Path(directory)
            control = root / "control"
            control.write_text("complete")
            Peer, Model = finite_peers(calls, requests, plan, started, release, control=control)
            selected_native = root / "codex-native"
            shutil.copyfile(native, selected_native)
            selected_native.chmod(0o755)
            certificate, key = root / "ca.pem", root / "key.pem"
            subprocess.run(["openssl", "req", "-x509", "-newkey", "rsa:2048", "-nodes", "-keyout", str(key), "-out", str(certificate), "-days", "1", "-subj", "/CN=host.microsandbox.internal", "-addext", "subjectAltName=DNS:host.microsandbox.internal", "-addext", "basicConstraints=critical,CA:FALSE"], check=True, capture_output=True, timeout=20)
            with serving(Peer, (certificate, key), host="0.0.0.0") as peer_port, serving(Model, host="0.0.0.0") as model_port:
                launcher = compile_native_launcher(root, model_port)
                configuration = root / "peers.json"
                configuration.write_text(json.dumps({"native_guest":"/opt/octacity/tools/tool-1", "stdio_guest":"/opt/octacity/tools/tool-2", "control": str(control), "native_executable": str(selected_native), "stdio_executable": str(stdio), "certificate": str(certificate),
                    "peer_url": f"https://host.microsandbox.internal:{peer_port}/mcp",
                    "model_url": f"http://host.microsandbox.internal:{model_port}/v1"}))
                configuration.chmod(0o600)
                completed = subprocess.run(["cargo", "test", "--locked", "-p", "octacity-release-harness", "--test", "factory_pilot",
                    "packaged_factory_pilot_runs_real_jobs_before_work_admission", "--", "--ignored", "--exact", "--nocapture"],
                    cwd=REPOSITORY, env={**os.environ, "OCTACITY_FACTORY_PILOT_CLI": str(launcher), "OCTACITY_FACTORY_PILOT_NATIVE_MCP": str(configuration),
                        "OCTACITY_FACTORY_MCP_POLICY_HELPER": os.environ["OCTACITY_FACTORY_MCP_POLICY_HELPER"]},
                    text=True, capture_output=True, timeout=600)
                self.assertEqual(completed.returncode, 0, completed.stdout[-6000:] + completed.stderr[-6000:])
        self.assertEqual([(call["tool"], call["resource"], call["allowed"]) for call in calls if "tool" in call],
                         [("facts", "alpha", True), ("facts", "beta", False), ("facts", "alpha", True)])
        self.assertTrue(any(call.get("authenticated") is False for call in calls), "the revoked token must reach its own peer without credential fallback")
        self.assertEqual(len(requests), 6, "revocation starts no model call; restart preserves the original call without replay")
        for request in requests:
            self.assertEqual(request["authorization"], "Bearer " + TOKENS["model"])
            self.assertEqual(request["model"], "finite-json-model-pro")
            self.assertEqual(request["reasoning"]["effort"], "high")
            self.assertTrue(all(tool.get("name", "").startswith(("mcp__reader", "mcp__local")) for tool in request["tools"]))
