"""The pilot image executor uses pinned sources and verifies imported identity."""
from __future__ import annotations
import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("prepare_factory_pilot_image", ROOT / "tools/prepare_factory_pilot_image.py")
assert SPEC and SPEC.loader
BUILDER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(BUILDER)

class PilotImageTests(unittest.TestCase):
    def test_executor_builds_exact_sources_and_returns_only_verified_imported_digest(self):
        inputs = json.loads((ROOT / "deployment/local-stand/inputs.json").read_text())
        digest = "sha256:" + "b" * 64
        calls = []
        with tempfile.TemporaryDirectory() as temporary:
            work = Path(temporary)
            def execute(args, **kwargs):
                if not calls:
                    self.assertEqual(args[:2], ["docker", "build"], "the executor must support the standard Docker image-store driver")
                calls.append(args)
                stdout = json.dumps({"digest":digest,"architecture":"arm64","os":"linux"}) if "inspect" in args else ""
                return subprocess.CompletedProcess(args,0,stdout,"")
            image = BUILDER.prepare_image(inputs, Path("/operator/msb"), work, execute=execute)
            self.assertEqual(image,"local.octacity/factory-pilot@"+digest)
            dockerfile = (work / "Dockerfile").read_text()
            self.assertIn("FROM "+inputs["images"]["node"]["reference"]+" AS node",dockerfile)
            self.assertIn("FROM "+inputs["native"]["microsandbox"]["guest_image"],dockerfile)
            self.assertNotIn("RUN ",dockerfile)
            self.assertIn("--platform=linux/arm64",calls[0])
            self.assertEqual(calls[1][0:3],["docker","image","save"])
            self.assertEqual(calls[2][0:2],["/operator/msb","load"])
            self.assertEqual(calls[3][0:3],["/operator/msb","image","inspect"])
            self.assertIn(image,calls[4])
            self.assertIn(image,calls[5])
            self.assertEqual(calls[6][0:2],["/operator/msb","save"])

    def test_mutable_or_injected_sources_refuse_before_any_executor_or_context_write(self):
        import copy
        original = json.loads((ROOT / "deployment/local-stand/inputs.json").read_text())
        for location, value in (("node", "node:latest"), ("base", "fedora:latest\nRUN untrusted"), ("platform", "linux/amd64")):
            inputs = copy.deepcopy(original)
            if location == "node": inputs["images"]["node"]["reference"] = value
            elif location == "base": inputs["native"]["microsandbox"]["guest_image"] = value
            else: inputs["images"]["node"]["platform"] = value
            with self.subTest(location=location), tempfile.TemporaryDirectory() as temporary:
                work=Path(temporary)
                def forbidden(*args, **kwargs):
                    self.fail("unverified image input reached an external executor")
                with self.assertRaises(ValueError):
                    BUILDER.prepare_image(inputs, Path("/operator/msb"), work, execute=forbidden)
                self.assertEqual(list(work.iterdir()), [], "unverified inputs must not become executable build instructions")

    def test_foreign_imported_identity_or_platform_cannot_be_qualified_or_exported(self):
        inputs = json.loads((ROOT / "deployment/local-stand/inputs.json").read_text())
        for changed in ({"digest":"sha256:"+"c"*64}, {"architecture":"amd64"}, {"os":"windows"}):
            with self.subTest(changed=changed), tempfile.TemporaryDirectory() as temporary:
                inspections=0
                def execute(args, **kwargs):
                    nonlocal inspections
                    metadata={"digest":"sha256:"+"b"*64,"architecture":"arm64","os":"linux"}
                    if "inspect" in args:
                        inspections+=1
                        if inspections == 2: metadata.update(changed)
                    if args[0:2] == ["/operator/msb","save"]:
                        self.fail("foreign imported image reached qualified export")
                    return subprocess.CompletedProcess(args,0,json.dumps(metadata),"")
                with self.assertRaises(ValueError):
                    BUILDER.prepare_image(inputs,Path("/operator/msb"),Path(temporary),execute=execute)

if __name__ == "__main__": unittest.main()
