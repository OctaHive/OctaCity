"""Finite model process for the clean-state manual authoring qualification."""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

MODEL = Path(__file__).resolve().parents[2] / "server/tests/octacity-release-harness/fixtures/factory/manual-model-cli.sh"


@unittest.skipUnless(os.name == "posix", "qualification peer uses the pinned Unix CLI contract")
class ManualFactoryModelTests(unittest.TestCase):
    def test_classifier_requires_its_guidance_and_rejects_predecessor_private_context(self):
        prompt = "Classify only the accepted eligible ticket ticket-request-canary process-criteria-canary eligible-public-summary component-guide-canary priority-guide-canary"
        with tempfile.TemporaryDirectory() as temporary:
            schema = Path(temporary) / "schema.json"
            schema.write_text(json.dumps({"type":"object","properties":{"component":{"type":"string"}}}))
            command = ["/bin/sh",str(MODEL),"exec","--json","--sandbox=workspace-write","--ask-for-approval=never","--model=finite-model",f"--output-schema={schema}","-"]
            result = subprocess.run(command,input=prompt,text=True,capture_output=True,check=False)
            self.assertEqual(result.returncode,0,result.stderr)
            self.assertEqual(json.loads(result.stdout.splitlines()[-1])["result"],{"component":"console","priority":"normal"})
            for leaked in ["project-goals-canary","first-private-investigation-canary","duplicate_search"]:
                rejected = subprocess.run(command,input=prompt+" "+leaked,text=True,capture_output=True,check=False)
                self.assertEqual(rejected.returncode,64)
            missing = subprocess.run(command,input=prompt.replace("component-guide-canary",""),text=True,capture_output=True,check=False)
            self.assertEqual(missing.returncode,64)

    def test_classifier_requires_the_explicitly_assigned_code_guide(self):
        prompt = "Classify only the accepted eligible ticket ticket-request-canary process-criteria-canary eligible-public-summary component-guide-canary priority-guide-canary Read the selected code guide."
        with tempfile.TemporaryDirectory() as temporary:
            schema = Path(temporary) / "schema.json"
            schema.write_text(json.dumps({"type":"object","properties":{"component":{"type":"string"}}}))
            command = ["/bin/sh",str(MODEL),"exec","--json","--sandbox=workspace-write","--ask-for-approval=never","--model=finite-model",f"--output-schema={schema}","-"]
            result = subprocess.run(command,input=prompt,text=True,capture_output=True,check=False)
            self.assertEqual(result.returncode,64,"an assigned source cannot be silently omitted")

    def test_empty_mcp_is_explicit_and_foreign_tool_configuration_is_rejected(self):
        prompt = "Assess Project fit. ticket-request-canary project-goals-canary process-criteria-canary"
        with tempfile.TemporaryDirectory() as temporary:
            schema = Path(temporary) / "schema.json"
            schema.write_text(json.dumps({"type":"object","properties":{"eligible":{"type":"boolean"}}}))
            command = ["/bin/sh",str(MODEL),"--ask-for-approval=never","exec","--json","--sandbox=workspace-write","--model=finite-model",f"--output-schema={schema}","--ignore-user-config","--config=mcp_servers={}","-"]
            result = subprocess.run(command,input=prompt,text=True,capture_output=True,check=False)
            self.assertEqual(result.returncode,0,result.stderr)
            self.assertEqual(json.loads(result.stdout.splitlines()[-1])["result"]["eligible"],True)
            command[-2] = '--config=mcp_servers={foreign={command="foreign"}}'
            self.assertEqual(subprocess.run(command,input=prompt,text=True,capture_output=True,check=False).returncode,64)

    def test_assessment_accepts_both_pinned_cli_approval_positions(self):
        prompt = "Assess Project fit. ticket-request-canary project-goals-canary process-criteria-canary"
        with tempfile.TemporaryDirectory() as temporary:
            schema = Path(temporary) / "schema.json"
            schema.write_text(json.dumps({"type":"object","properties":{"eligible":{"type":"boolean"}}}))
            for prefix in [
                ["--ask-for-approval=never", "exec", "--json", "--sandbox=workspace-write"],
                ["exec", "--json", "--sandbox=workspace-write", "--ask-for-approval=never"],
            ]:
                with self.subTest(prefix=prefix):
                    result = subprocess.run(["/bin/sh",str(MODEL),*prefix,"--model=finite-model",f"--output-schema={schema}","-"],input=prompt,text=True,capture_output=True,check=False)
                    self.assertEqual(result.returncode,0,result.stderr)
                    events = [json.loads(line) for line in result.stdout.splitlines()]
                    self.assertEqual(events[-1]["result"],{"eligible":True,"summary":"eligible-public-summary","duplicate_search":"not_requested"})


if __name__ == "__main__":
    unittest.main()
