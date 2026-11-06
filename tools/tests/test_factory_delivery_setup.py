"""Personal stand Delivery installation uses the ordinary Factory contracts."""
import copy
import json
import os
from pathlib import Path
import shutil
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))


class FactoryDeliverySetupTests(unittest.TestCase):
    def test_delivery_model_installation_keeps_triage_and_separates_stage_authority(self):
        from factory_delivery_setup import delivery_models

        project = "ab94c8a7-be1d-4bd4-9ea7-420319c562f6"
        model = {"identity": "selected-model", "version": "v1", "digest": "a" * 64}
        capture = {"identity": "git", "version": "2.50.0", "digest": "c" * 64}
        build = {"id": "c52aa123-b92c-4134-855a-728bf7696c25", "project_id": project,
                 "version": 1, "definition_digest": "b" * 64}
        toolchain = {"model": model, "plugin": {"identity": "codex", "version": "v1", "digest": "d" * 64},
                     "executable": {"identity": "codex-cli", "version": "v1", "digest": "e" * 64}}
        grants = {"mounts": [{"root": "/octacity/protected", "mode": "read_only"}],
                  "secret_profiles": ["triage-model", "triage-mcp"], "network_hosts": ["chatgpt.com", "issues.localhost"],
                  "outputs": {"kinds": ["codex-run-result"], "max_artifact_count": 4, "max_report_count": 4}}
        triage = {"toolchain": toolchain, "build_configuration": build, "credentials": {
            "model": "triage-author", "evaluator": "triage-model", "source": "triage-source", "delivery": "triage-delivery"},
            "authentication": "native_login_file", "credential_purpose": "evaluator", "project_permissions": grants,
            "local_permissions": {"grants": grants, "enforceable": []}, "pipeline_node": "worker"}
        runtime = {"ceiling": {"toolchain": toolchain, "build_configuration": build,
                               "permissions": grants, "execution": {"environment": {"ISSUE_MCP_TOKEN": {}}}},
                   "settings_support": {"authenticated_mcp": {"issues": {"server": {"url": "https://issues.localhost/mcp"}}}}}
        catalog = {"version": 1, "projects": [{"project_id": project, "role_profiles": {"triage-runtime": runtime},
                                              "references": [], "build_configurations": []}], "codex": [triage]}
        original = copy.deepcopy(catalog)

        code_build = {**build, "id": "4b525743-53a3-4f58-a85a-6fd8381f4aec"}
        additions = delivery_models(catalog, project, capture, ["tools", "tests"], code_build=code_build)
        self.assertEqual(catalog, original)
        self.assertEqual(additions["codex"][0], triage)
        self.assertEqual(additions["projects"][0]["role_profiles"]["triage-runtime"], runtime)
        coder, reviewer, devops = additions["codex"][1:]
        self.assertEqual(coder["capture"]["tool"], capture)
        self.assertEqual(coder["capture"]["instruction"]["policy"]["allowed_path_prefixes"], ["tests", "tools"])
        self.assertIn({"root": "/workspace/source", "mode": "read_write"}, coder["project_permissions"]["mounts"])
        self.assertIn({"root": "/workspace/source", "mode": "read_only"}, reviewer["project_permissions"]["mounts"])
        self.assertFalse(any(item["root"] == "/workspace/source" for item in devops["project_permissions"]["mounts"]))
        self.assertEqual(reviewer["candidate_capture_tool"], capture)
        self.assertNotIn("capture", reviewer)
        for profile in (coder, reviewer):
            self.assertEqual(profile["build_configuration"], code_build)
        self.assertEqual(devops["build_configuration"], build)
        for profile in (coder, reviewer, devops):
            self.assertEqual(profile["toolchain"]["model"]["identity"], "selected-model")
            self.assertNotIn("triage-mcp", profile["project_permissions"]["secret_profiles"])
            self.assertNotIn("issues.localhost", profile["project_permissions"]["network_hosts"])
        for alias in ("delivery-coder", "delivery-reviewer", "delivery-devops"):
            definition = additions["projects"][0]["role_profiles"][alias]
            self.assertNotIn("settings_support", definition)
            self.assertEqual(definition["ceiling"]["execution"]["environment"], {})
        self.assertEqual(delivery_models(additions, project, capture, ["tests", "tools"], code_build=code_build), additions)
        with self.assertRaisesRegex(ValueError, "conflict"):
            delivery_models(additions, project, capture, ["tools"], code_build=code_build)

    def test_build_registration_keeps_no_code_and_repository_modes_separate(self):
        from factory_delivery_setup import install_worker

        project_id, repository_id = "project", "repository"
        worker = {"build_configuration": {"id": "old", "version": 1}}
        original = {"runtime": {"immutable_image": "original"}, "source": {"kind": "source_free_v1"},
                    "repository_id": None, "repository_version": None}
        project = {"project_id": project_id, "build_configurations": []}
        definitions = []
        class Client:
            def get(self, path):
                return {"version": 1} if path.startswith("/api/v1/repositories/") else {"definition": original}

            def post(self, path, key, body):
                definitions.append(body["definition"])
                return {"resource": {"id": key, "project_id": project_id, "version": 1, **body}}

        source_free = install_worker(Client(), project, worker, "new-image", "namespace")
        code = install_worker(Client(), project, worker, "new-image", "namespace", repository_id=repository_id)
        self.assertNotEqual(code["build_configuration"], source_free["build_configuration"])
        self.assertEqual(len(project["build_configurations"]), 2)
        self.assertEqual(definitions[0], {**original, "runtime": {"immutable_image": "new-image"}})
        self.assertEqual(definitions[1], {**definitions[0], "source": None, "repository_id": repository_id, "repository_version": 1})
        self.assertEqual(original["source"], {"kind": "source_free_v1"})

    @unittest.skipUnless(os.name == "posix", "private test packs require POSIX ownership")
    def test_a_reviewed_pack_is_frozen_and_cannot_read_a_script_outside_its_directory(self):
        import tempfile
        from factory_delivery_setup import read_test_pack

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            script = root / "checks.sh"
            script.write_bytes(b"test -f \"$1/expected\"\n")
            script.chmod(0o600)
            manifest = root / "pack.json"
            definition = {"name": "issue-23", "task_sha256": "b" * 64,
                          "acceptance_sha256": "a" * 64, "script": "checks.sh"}
            manifest.write_text(json.dumps(definition))
            manifest.chmod(0o600)
            first = read_test_pack(manifest)
            self.assertEqual(first.program, script.read_bytes())
            self.assertEqual(first.name, "issue-23")
            script.write_text("exit 0\n")
            self.assertNotEqual(read_test_pack(manifest).digest, first.digest)
            definition["script"] = "../checks.sh"
            manifest.write_text(json.dumps(definition))
            with self.assertRaisesRegex(ValueError, "test pack"):
                read_test_pack(manifest)

    @unittest.skipUnless(os.name == "posix" and shutil.which("node"), "requires Node and POSIX shell")
    def test_protected_program_rejects_another_ticket_and_distinguishes_test_failure_from_setup_failure(self):
        import base64
        import hashlib
        import subprocess
        import tempfile

        script = Path(__file__).resolve().parents[2] / "deployment/local-stand/delivery-test-command.mjs"
        sha = lambda value: hashlib.sha256(value).hexdigest()
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            protected, source, output = (root / key for key in ("protected", "source", "output"))
            for path in (protected, source, output, protected / "tests"):
                path.mkdir()
            (protected / "work-task").write_bytes(b"selected ticket")
            (protected / "work-acceptance").write_bytes(b"accepted checks")
            (protected / "command-input.json").write_text(json.dumps({
                "node": {"input_digest": "a" * 64}, "input": {"subject": {"base_revision": "base"}},
                "profile": {"model_or_tool": {"identity": "baseline"}}
            }))
            payload = b'if [ -f "$1/pass" ]; then exit 0; fi\nif [ -f "$1/fail" ]; then exit 1; fi\nexit 2\n'
            pack = {"class": "issue-23", "task": sha(b"selected ticket"), "acceptance": sha(b"accepted checks"),
                    "digest": sha(payload), "program": base64.b64encode(payload).decode(),
                    "criterion": {"identity": "check"}, "environment": {"identity": "guest"}}

            def run(mode):
                return subprocess.run(["node", str(script), mode, json.dumps(pack), str(protected), str(source), str(output)],
                                      capture_output=True, check=False)

            (protected / "work-task").write_bytes(b"other ticket")
            self.assertNotEqual(run("author").returncode, 0)
            self.assertEqual(list(output.iterdir()), [])
            (protected / "work-task").write_bytes(b"selected ticket")
            self.assertEqual(run("author").returncode, 0)
            (output / "checks.sh").rename(protected / "tests/checks.sh")
            for marker, status in [("fail", "matched"), ("pass", "not_matched"), ("broken", "indeterminate")]:
                (source / marker).touch()
                result = run("baseline")
                self.assertEqual(result.returncode, 0, result.stderr.decode())
                report = output / "baseline.json"
                self.assertEqual(json.loads(report.read_text())["finding"], status)
                report.unlink()
                (source / marker).unlink()

    def test_each_pack_installs_one_author_and_pins_both_verifiers_to_its_original_inventory(self):
        from factory_delivery_setup import TestPack, install_test_commands
        project_id = "ab94c8a7-be1d-4bd4-9ea7-420319c562f6"
        project = {"project_id": project_id, "references": []}
        old = {"identity": "other-command"}
        catalog = {"projects": [project], "commands": [old]}
        pin = {"identity": "shell", "version": "v1", "digest": "a" * 64}
        capture = {"identity": "git", "version": "2.50.0", "digest": "c" * 64}
        worker = {"build_configuration": {"project_id": project_id}, "pipeline_node": "worker",
                  "project_permissions": {"mounts": [{"root": "/octacity/protected", "mode": "read_only"}],
                  "resources": {"elapsed_millis": 120000}, "outputs": {"max_report_count": 4, "max_artifact_count": 4}},
                  "local_permissions": {"enforceable": []}}
        pack = TestPack("issue-23", "d" * 64, "e" * 64, b"exit 0\n")
        author_worker = {**worker, "build_configuration": {**worker["build_configuration"], "id": "author-build"}}
        install_test_commands(catalog, project, worker, [pin], capture, pack, author=author_worker)
        author, = catalog["protected_test_authors"]
        base, validation = catalog["protected_validation"]
        self.assertEqual(catalog["commands"][0], old)
        reproduction, = catalog["commands"][1:]
        self.assertFalse(reproduction["observation_contract"]["requires_protected_tests"])
        self.assertEqual(reproduction["project_permissions"]["secret_profiles"], [])
        self.assertEqual(author["policy"]["policy"]["classes"], ["issue-23"])
        self.assertEqual(base["inputs"]["author_profile"], author["command"]["identity"])
        self.assertEqual(validation["inputs"]["author_profile"], author["command"]["identity"])
        self.assertEqual(base["inputs"]["inventory"], validation["inputs"]["inventory"])
        self.assertNotIn("capture_tool", base["inputs"])
        self.assertEqual(validation["inputs"]["capture_tool"], capture)
        # Candidate receives the inventory, two change-set artifacts, three generated
        # command documents, and may also explicitly receive task and acceptance.
        self.assertGreaterEqual(validation["inputs"]["policy"]["reserved_inputs"], 8)
        for entry in (author["command"], *base["profiles"], *validation["profiles"]):
            self.assertEqual(entry["project_permissions"]["secret_profiles"], [])
            self.assertEqual(entry["project_permissions"]["network_hosts"], [])
        installed = copy.deepcopy(catalog)
        self.assertEqual(author["command"]["build_configuration"], author_worker["build_configuration"])
        install_test_commands(catalog, project, worker, [pin], capture, pack, author=author_worker)
        self.assertEqual(catalog, installed)

    @unittest.skipUnless(os.name == "posix", "private configuration requires POSIX ownership")
    def test_publication_credentials_are_separate_and_the_review_target_is_explicit(self):
        import tempfile
        from factory_delivery_setup import stage_publication
        token = "private-test-delivery-token"
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            project = {"project_id": "ab94c8a7-be1d-4bd4-9ea7-420319c562f6", "references": []}
            repository = {"id": "c52aa123-b92c-4134-855a-728bf7696c25", "definition": {
                "repository_locator": "https://github.com/Example/Sandbox"}}
            catalog = {}
            adapter = stage_publication(root, catalog, project, repository, "trial", token, b"adapter", "d" * 64, None)
            row, = catalog["delivery"]
            target, = row["policies"]
            self.assertTrue(target["draft"]["review"]["draft"])
            self.assertEqual(target["draft"]["target"]["base_reference"], "refs/heads/trial")
            self.assertNotIn(token, json.dumps([catalog, project]))
            path, = list((adapter / "github/credentials").iterdir())
            self.assertEqual(path.stat().st_mode & 0o777, 0o600)
            self.assertEqual(json.loads(path.read_text())["token"], token)

    @unittest.skipUnless(os.name == "posix", "private test packs require POSIX ownership")
    def test_pack_reads_the_selected_issue_and_freezes_its_exact_manual_intake_bytes(self):
        import hashlib
        import tempfile
        from unittest.mock import Mock
        from factory_delivery_setup import read_test_pack

        project, issue = "ab94c8a7-be1d-4bd4-9ea7-420319c562f6", "c52aa123-b92c-4134-855a-728bf7696c25"
        client = Mock()
        client.get.return_value = {"content_available": True, "content": {"title": "Тема", "description": "Body\n", "acceptance": "- check\n"}}
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "checks.sh").write_text("exit 0\n")
            (root / "pack.json").write_text(json.dumps({"name": "issue-23", "issue_id": issue, "script": "checks.sh"}))
            for path in root.iterdir():
                path.chmod(0o600)
            selected = read_test_pack(root / "pack.json", client=client, project=project)
            self.assertEqual(selected.task_sha256, hashlib.sha256("Тема\n\nBody\n".encode()).hexdigest())
            self.assertEqual(selected.acceptance_sha256, hashlib.sha256(b"- check\n").hexdigest())
            client.get.assert_called_once_with(f"/api/v1/projects/{project}/issues/{issue}")
