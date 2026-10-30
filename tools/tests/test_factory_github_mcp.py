"""Opt-in qualification of the unmodified official GitHub MCP server.

Only its GitHub API peer is finite. This is separate from packaged Agent and
triage qualification: no provider-specific production wrapper is installed.
"""
from contextlib import contextmanager
from http.server import BaseHTTPRequestHandler
import hashlib
import json
import os
from pathlib import Path
import select
import subprocess
import tempfile
import unittest
import uuid
from urllib.parse import parse_qs, urlsplit

from tools.tests.fixture_environment import fixture_environment
from tools.tests.test_factory_mcp_credentials import serving


EXTERNAL_TOKEN = "fixture-external-mcp-read-only"
SERVER = os.environ.get("OCTACITY_TEST_GITHUB_MCP_SERVER")


def finite_github(requests):
    class GitHub(BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass

        def do_GET(self):
            requests.append((self.command, self.path, self.headers.get("Authorization")))
            url = urlsplit(self.path)
            issue = {"id": 9001, "number": 17, "title": "Candidate external defect",
                     "body": "Finite external evidence", "state": "open", "labels": [],
                     "html_url": "https://github.example/owner/tickets/issues/17"}
            status, payload = 404, {"message": "Not Found"}
            if self.headers.get("Authorization") not in {"Bearer " + EXTERNAL_TOKEN, "token " + EXTERNAL_TOKEN}:
                status, payload = 401, {"message": "Bad credentials"}
            elif url.path == "/api/v3/search/issues" and parse_qs(url.query).get("q") == ["repo:owner/tickets is:issue Candidate"]:
                status, payload = 200, {"total_count": 1, "incomplete_results": False, "items": [issue]}
            elif url.path == "/api/v3/repos/owner/tickets/issues/17":
                status, payload = 200, issue
            body = json.dumps(payload).encode()
            self.send_response(status)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

    return GitHub


@contextmanager
def running_server(origin):
    process = subprocess.Popen(
        [SERVER, "stdio", "--gh-host", origin, "--read-only",
         "--toolsets=issues", "--tools=search_issues,issue_read,issue_write"],
        env=fixture_environment(GITHUB_PERSONAL_ACCESS_TOKEN=EXTERNAL_TOKEN),
        stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
    )
    sequence = 0

    def call(method, params):
        nonlocal sequence
        sequence += 1
        process.stdin.write(json.dumps({"jsonrpc": "2.0", "id": sequence,
                                        "method": method, "params": params}).encode() + b"\n")
        process.stdin.flush()
        if not select.select([process.stdout], [], [], 15)[0]:
            raise AssertionError(f"native GitHub MCP did not answer {method}")
        response = json.loads(process.stdout.readline())
        if response.get("id") != sequence:
            raise AssertionError("native MCP returned a different request identity")
        return response

    try:
        yield call
    finally:
        process.stdin.close()
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            process.terminate()
            process.wait(timeout=5)
        process.stdout.close()
        process.stderr.close()


@unittest.skipUnless(SERVER, "select checksum-verified GitHub MCP v2.0.3 explicitly")
class ReadyMadeGitHubMcpTests(unittest.TestCase):
    def test_native_search_and_read_preserve_scope_and_read_only_tools(self):
        self.assertTrue(Path(SERVER).is_file())
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        evidence = Path(os.environ.get("OCTACITY_RELEASE_EVIDENCE_DIR", temporary.name)) / f"github-mcp-{uuid.uuid4()}.json"
        requests = []
        with serving(finite_github(requests)) as port, running_server(f"http://127.0.0.1:{port}") as call:
            initialized = call("initialize", {"protocolVersion": "2025-11-25", "capabilities": {},
                                               "clientInfo": {"name": "octacity-qualification", "version": "1"}})
            self.assertNotIn("error", initialized)
            tools = call("tools/list", {})["result"]["tools"]
            self.assertIn("search_issues", [tool["name"] for tool in tools])
            self.assertIn("issue_read", [tool["name"] for tool in tools])
            self.assertNotIn("issue_write", [tool["name"] for tool in tools])
            search = call("tools/call", {"name": "search_issues", "arguments": {
                "query": "repo:owner/tickets is:issue Candidate", "perPage": 1}})["result"]
            self.assertFalse(search.get("isError", False), search)
            self.assertIn("Candidate external defect", json.dumps(search))
            detail = call("tools/call", {"name": "issue_read", "arguments": {
                "method": "get", "owner": "owner", "repo": "tickets", "issue_number": 17}})["result"]
            self.assertFalse(detail.get("isError", False), detail)
            self.assertIn("Finite external evidence", json.dumps(detail))
            hidden = call("tools/call", {"name": "issue_read", "arguments": {
                "method": "get", "owner": "foreign", "repo": "private", "issue_number": 18}})["result"]
            self.assertTrue(hidden.get("isError", False), hidden)
            denied = call("tools/call", {"name": "issue_write", "arguments": {
                "method": "update", "owner": "owner", "repo": "tickets", "issue_number": 17,
                "state": "closed"}})
            self.assertTrue("error" in denied or denied.get("result", {}).get("isError"), denied)
        self.assertTrue(requests)
        self.assertTrue(all(method == "GET" for method, _, _ in requests))
        self.assertTrue(all(auth in {"Bearer " + EXTERNAL_TOKEN, "token " + EXTERNAL_TOKEN}
                            for _, _, auth in requests))
        evidence.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
        with Path(SERVER).open("rb") as executable:
            executable_digest = hashlib.file_digest(executable, "sha256").hexdigest()
        report = {
            "server": {key: initialized["result"]["serverInfo"][key] for key in ("name", "version")},
            "executable_sha256": executable_digest,
            "protocol_version": initialized["result"]["protocolVersion"],
            "configuration": {"host": f"http://127.0.0.1:{port}", "read_only": True,
                              "toolsets": ["issues"], "tools": ["search_issues", "issue_read", "issue_write"]},
            "available_tools": [tool["name"] for tool in tools],
            "observations": {"search": search, "detail": detail, "foreign": hidden, "write": denied},
            "provider_requests": [{"method": method, "path": path} for method, path, _ in requests],
            "boundary": "finite GitHub API; unmodified official MCP executable",
        }
        with evidence.open("x") as output:
            json.dump(report, output, indent=2)
        self.assertTrue(evidence.is_file(), "retain the exact native server identity and observed protocol results")
        retained = evidence.read_text()
        report = json.loads(retained)
        self.assertEqual(report["server"]["version"], "2.0.3")
        self.assertEqual(report["boundary"], "finite GitHub API; unmodified official MCP executable")
        self.assertIn("Candidate external defect", retained)
        self.assertIn("Finite external evidence", retained)
        self.assertNotIn(EXTERNAL_TOKEN, retained)
