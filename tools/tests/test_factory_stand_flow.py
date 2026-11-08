"""The optional stand example selects the evidence contract required by its gate."""
import copy
import json
from pathlib import Path
import sys
import unittest
import uuid

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import factory_stand_flow


class FactoryStandFlowTests(unittest.TestCase):
    def test_seeded_triage_selects_independent_mcp_evidence_without_changing_the_generic_binding(self):
        project = str(uuid.uuid4())
        profile = {"reference": {"id": "profile", "version": 1}, "role": {"id": "role"}, "definition": {
            "build_configuration": {"id": "worker", "version": 1}, "budget": {}, "permissions": {}}}
        binding = {"tool": {"identity": "codex-cli", "version": "1", "digest": "a" * 64}}
        class Client:
            def get(self, path):
                return copy.deepcopy(profile) if "factory-role-profiles" in path else {"definition": {"source": {"kind": "source_free_v1"}}}
            def post(self, path, key, body):
                if path.endswith("flow-data-schemas/validate"):
                    return {"reference": {"identity": body["identity"], "version": body["version"], "digest": "b" * 64}, "shape": body["shape"]}
                if path.endswith("factory-gate-programs/validate"):
                    return body["program"]
                return {"incompatibility": None, "build": binding}
        selected = {"profile": profile["reference"], "role": profile["role"], "capability": "triage-runtime"}
        context = {"goals": {"artifact_id": "goals"}}
        draft = factory_stand_flow.flow_draft(Client(), project, {"profiles": {name: selected for name in ("eligibility", "classification")},
            "context": {name: context for name in ("eligibility", "classification")}, "execution": {"budget": {}, "permissions": {}}}, uuid.uuid4(), {})
        expected = {"identity": "native-mcp-evidence", "version": "v1", "digest": "6921e3ae725944f5f3969ea11dc7a264d12e205774e4b92195f2f82933c08886"}
        for node in draft["root"]["nodes"]:
            if node["kind"] == "reasoning":
                actual = node["profile_selection"]["candidates"]["selected"]["build"]
                self.assertEqual(actual.get("result_tool"), expected if node["key"] == "eligibility" else None)
        self.assertNotIn("result_tool", binding, "the generic resolver contract remains unchanged")
