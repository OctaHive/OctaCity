"""The optional stand example selects the evidence contract required by its gate."""
import copy
import json
from pathlib import Path
import sys
import unittest
from unittest.mock import patch
import uuid

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import factory_stand_flow


class FactoryStandFlowTests(unittest.TestCase):
    def test_stand_preserves_reviewed_limits_instead_of_resolver_product_defaults(self):
        project, namespace = str(uuid.uuid4()), uuid.uuid4()
        configured = {"example_preset": {"identity": "triage", "version": "v4"},
            "flow_limits": {"max_depth": 2, "max_expanded_nodes": 7, "max_repeat_count": 3,
                "max_fan_out": 2, "max_wip": 1}}
        selection = {"profiles": {}, "context": {}, "execution": {
            "budget": {"max_attempts": 8}, "permissions": {"plugins": []}}}
        resolved = {"root": {"nodes": []}, "limits": {"max_depth": 16,
            "max_expanded_nodes": 16384, "max_repeat_count": 100, "max_fan_out": 100,
            "max_wip": 10, "budget": {"max_attempts": 100}, "permissions": {"plugins": ["unselected"]}}}
        original = copy.deepcopy(resolved)
        class Client:
            def get(self, path):
                return {"items": [{"reference": {"identity": "triage", "version": "v4", "digest": "a" * 64}}]}
            def post(self, path, key, body):
                return {"diagnostics": [], "flow": resolved}
        with patch.object(factory_stand_flow, "POLICY") as policy:
            policy.read_text.return_value = json.dumps(configured)
            result = factory_stand_flow.flow_draft(Client(), project, selection, namespace, {})
        self.assertEqual(result["limits"], {"max_depth": 2, "max_expanded_nodes": 7,
            "max_repeat_count": 3, "max_fan_out": 2, "max_wip": 1,
            "budget": {"max_attempts": 8}, "permissions": {"plugins": []}})
        self.assertEqual(resolved, original, "reviewing the stand draft must not mutate the resolver response")

    def test_example_fields_seed_only_an_unpublished_project_catalog_without_issue_values(self):
        project, namespace = str(uuid.uuid4()), uuid.uuid4()
        catalog = {"version": 0, "definition": {"priorities": [{"id": "project_priority"}], "properties": [], "components": []}}
        publications = []
        class Client:
            def get(self, path):
                return copy.deepcopy(catalog)
            def post(self, path, key, body):
                publications.append(copy.deepcopy(body))
                assert path.endswith("/issue-property-definitions")
                assert body["expected_version"] == 0 and "values" not in body
                catalog.update(version=1, definition=body["definition"])
                return copy.deepcopy(catalog)
        client = Client()
        factory_stand_flow.prepare_project_fields(client, project, namespace)
        self.assertEqual(catalog["definition"]["priorities"], [{"id": "project_priority"}])
        self.assertEqual({item["id"] for item in catalog["definition"]["properties"]}, {"impact", "risk", "route", "repro"})
        self.assertEqual(catalog["definition"]["components"][0]["id"], "general")
        self.assertNotIn("classification", catalog["definition"], "field definitions need no global business roles")
        factory_stand_flow.prepare_project_fields(client, project, namespace)
        self.assertEqual(len(publications), 1, "restart must preserve the published catalog")
        catalog.update(version=3, definition={"properties": [{"id": "operator_field"}]})
        factory_stand_flow.prepare_project_fields(client, project, namespace)
        self.assertEqual(catalog["definition"], {"properties": [{"id": "operator_field"}]})
        self.assertEqual(len(publications), 1)

    def test_stand_uses_the_common_template_resolver_instead_of_copying_field_schemas(self):
        project, namespace = str(uuid.uuid4()), uuid.uuid4()
        preset = {"identity": "triage", "version": "v4", "digest": "a" * 64}
        role, profile = {"id": "role"}, {"id": "profile", "version": 1}
        calls = []
        class Client:
            def get(self, path):
                calls.append(("get", path))
                if path.endswith("/factory-presets"):
                    return {"items": [{"reference": preset}], "next_cursor": None}
                raise AssertionError("the stand must not reconstruct a published profile or schema")
            def post(self, path, key, body):
                calls.append(("post", path))
                self_body = body
                if not path.endswith("/factory-presets/resolve"):
                    raise AssertionError("templates resolve through their common owner, not schema-by-schema setup")
                assert self_body["flow_only"] is True
                assert self_body["profiles"]["analysis"] == {"role": role, "profile": profile}
                assert "input" not in self_body and "configuration" not in self_body
                return {"diagnostics": [], "flow": {"root": {"nodes": [{"key": "analysis"}]}}}
        selection = {"profiles": {"analysis": {"role": role, "profile": profile, "capability": "installed"}},
            "context": {}, "execution": {"budget": {}, "permissions": {}}}
        result = factory_stand_flow.flow_draft(Client(), project, selection, namespace, {"artifact_id": "criteria"})
        self.assertEqual(result["root"]["nodes"], [{"key": "analysis"}])
        self.assertEqual(result["root"]["process"]["criteria"]["recipients"], ["analysis"])
        self.assertEqual(len(calls), 2)

    def test_stand_preserves_exact_context_and_profile_pins_for_server_resolution(self):
        project = str(uuid.uuid4())
        profile = {"id": "reviewed-profile", "version": 3, "digest": "a" * 64}
        role = {"id": "reviewed-role", "version": 2, "digest": "b" * 64}
        context = {"goals": {"artifact_id": "goals"}, "documents": {"rules": {"artifact_id": "rules"}}}
        selection = {"profiles": {"custom_stage": {"profile": profile, "role": role, "capability": "native"}},
            "context": {"custom_stage": context}, "execution": {"budget": {}, "permissions": {}}}
        original = copy.deepcopy(selection)
        class Client:
            def get(self, path):
                return {"items": [{"reference": {"identity": "triage", "version": "v4", "digest": "c" * 64}}]}
            def post(self, path, key, body):
                assert body["profiles"] == {"custom_stage": {"profile": profile, "role": role}}
                assert body["context"] == {"custom_stage": context}
                return {"diagnostics": [], "flow": {"root": {"nodes": []}}}
        factory_stand_flow.flow_draft(Client(), project, selection, uuid.uuid4(), {})
        self.assertEqual(selection, original)
