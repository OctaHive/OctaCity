"""Native-client qualification invoked by the real Issue MCP router test.

The HTTP fixture only terminates test TLS and forwards the native protocol.
Issue search, visibility and current authority belong to the Rust application.
"""
import json
import os
from pathlib import Path
import unittest

from tools.tests import test_factory_mcp_credentials as native


@unittest.skipUnless(os.environ.get("OCTACITY_TEST_ISSUE_MCP"), "run the native_triage Rust integration test")
class TriageNativeMcpTests(unittest.TestCase):
    def test_native_issue_search_detail_external_isolation_and_current_revocation(self):
        setup = json.loads(os.environ["OCTACITY_TEST_ISSUE_MCP"])
        client = native.NativeMcpCredentialTests()
        plan = [("remote", "search_issues", {"query": "Candidate", "limit": 1}),
                ("remote", "read_issue", {"issue_id": setup["visible"]}),
                ("remote", "read_issue", {"issue_id": setup["hidden"]}),
                ("local", "facts", "alpha"), ("local", "write", "alpha"),
                ("remote", "search_issues", {"query": "Candidate", "project_id": setup["foreign"]})]
        responses, _, external, model, retained = client.invoke(
            token=setup["token"], issue_origin=setup["origin"], plan=plan)
        self.assertEqual(responses[-1]["type"], "Completed")
        self.assertEqual(responses[-1]["payload"]["code"], 0)
        records, provenance = client.native_capture(retained)
        self.assertEqual(provenance["capture"]["status"], "complete")
        calls = [row["event"]["item"] for row in records
                 if row["event"]["type"] == "item.completed"
                 and row["event"].get("item", {}).get("type") == "mcp_tool_call"]
        self.assertEqual([(row["server"], row["tool"], row["status"]) for row in calls],
                         [("remote", "search_issues", "completed"), ("remote", "read_issue", "completed"),
                          ("remote", "read_issue", "failed"), ("local", "facts", "completed"),
                          ("local", "write", "failed"), ("remote", "search_issues", "failed")])
        search = json.loads(calls[0]["result"]["content"][0]["text"])
        self.assertEqual([item["id"] for item in search["items"]], [setup["visible"]])
        self.assertEqual(search["scope"], {"project_id": setup["project"], "mode": "issue_titles"})
        detail = json.loads(calls[1]["result"]["content"][0]["text"])
        self.assertEqual(detail["issue"]["version"], 1)
        self.assertEqual(detail["issue"]["content"]["description"], setup["injection"])
        self.assertEqual(calls[2]["result"]["content"], [{"type": "text", "text": "not_available"}])
        self.assertEqual([(row["tool"], row["allowed"]) for row in external], [("facts", True), ("write", False)])
        self.assertNotIn("private-hidden-candidate-canary", retained.decode())
        for request in model:
            self.assertEqual(request["authorization"], "Bearer " + native.TOKENS["model"])
            self.assertTrue(request["tools"], "the native model request must advertise selected MCP tools")
            self.assertTrue(all(tool["name"].startswith(("mcp__remote", "mcp__local")) for tool in request["tools"]))
        for token in [setup["token"], *native.TOKENS.values()]:
            self.assertNotIn(token, json.dumps(responses))
            self.assertNotIn(token.encode(), retained)

        Path(setup["revoked"]).touch()
        responses, _, external, model, _ = client.invoke(
            token=setup["token"], issue_origin=setup["origin"], plan=plan)
        terminal = responses[-1]
        self.assertIn(terminal["type"], {"Error", "Completed"})
        if terminal["type"] == "Completed":
            self.assertNotEqual(terminal["payload"]["code"], 0)
        self.assertFalse(model, "a revoked required connection must block inference without a fallback")
        self.assertFalse(external)
