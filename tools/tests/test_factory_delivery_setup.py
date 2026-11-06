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
        from factory_delivery_setup import TEST_AUTHOR_RUNTIME, delivery_models
        from factory_triage_setup import _reference

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
        defaults = delivery_models(catalog, project, capture, [], code_build=code_build)
        default_coder = defaults["codex"][1]
        self.assertNotIn("capture", default_coder)
        self.assertIn({"root": "/workspace/source", "mode": "read_only"}, default_coder["project_permissions"]["mounts"])
        self.assertIn("delivery-coder-readonly", defaults["projects"][0]["role_profiles"])
        extended = delivery_models(defaults, project, capture, ["tools"], code_build=code_build)
        self.assertEqual(extended["codex"][1], default_coder)
        self.assertEqual(delivery_models(extended, project, capture, [], code_build=code_build), extended)
        additions = delivery_models(catalog, project, capture, ["tools", "tests"], code_build=code_build)
        self.assertEqual(catalog, original)
        self.assertEqual(additions["codex"][0], triage)
        self.assertEqual(additions["projects"][0]["role_profiles"]["triage-runtime"], runtime)
        coder, reviewer, devops, tester = additions["codex"][1:]
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
        self.assertEqual(tester["build_configuration"], code_build)
        self.assertIn({"root": "/workspace/source", "mode": "read_only"}, tester["project_permissions"]["mounts"])
        self.assertNotIn("capture", tester)
        self.assertEqual(tester["protected_tests"]["files"], [
            {"output": "tester-checks", "class": "acceptance", "path": "checks.sh"},
        ])
        self.assertEqual(tester["protected_tests"]["transcript"], "codex-run-trace")
        self.assertIn(TEST_AUTHOR_RUNTIME, defaults["projects"][0]["role_profiles"])
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

        legacy = copy.deepcopy(defaults)
        old = legacy["codex"][-1]
        old["instructions"] = "Return exit 1 for a completed failing assertion."
        old["toolchain"]["model"]["version"] = "delivery-tester-v1"
        old["protected_tests"]["inventory"]["schema"] = _reference("delivery-test-inventory")
        legacy_runtimes = legacy["projects"][0]["role_profiles"]
        legacy_runtimes["delivery-tester"] = legacy_runtimes.pop(TEST_AUTHOR_RUNTIME)
        legacy_runtimes["delivery-tester"]["ceiling"]["toolchain"] = old["toolchain"]
        for reference in legacy["projects"][0]["references"]:
            if reference["alias"] == TEST_AUTHOR_RUNTIME:
                reference["alias"] = "delivery-tester"
            if reference["alias"] == TEST_AUTHOR_RUNTIME + "-model":
                reference.update(alias="delivery-tester-model", reference=old["toolchain"]["model"])
        upgraded = delivery_models(legacy, project, capture, [], code_build=code_build)
        self.assertIn(old, upgraded["codex"])
        new = next(entry for entry in upgraded["codex"]
                   if "octacity:assertion:failed" in entry.get("instructions", ""))
        self.assertNotEqual(new["toolchain"], old["toolchain"])
        self.assertNotEqual(new["protected_tests"]["inventory"]["schema"],
                            old["protected_tests"]["inventory"]["schema"])

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
            self.assertNotEqual(run("baseline").returncode, 0)
            self.assertFalse((output / "baseline.json").exists())
            (protected / "test-inventory.json").write_text(json.dumps({"files": [{
                "class": "issue-23", "path": "checks.sh",
                "artifact": {"encoded_size": len(payload), "content_digest": sha(payload)},
            }]}))
            pack["entrypoints"] = ["checks.sh"]
            for marker, status in [("fail", "matched"), ("pass", "not_matched"), ("broken", "indeterminate")]:
                (source / marker).touch()
                result = run("baseline")
                self.assertEqual(result.returncode, 0, result.stderr.decode())
                report = output / "baseline.json"
                self.assertEqual(json.loads(report.read_text())["finding"], status)
                report.unlink()
                (source / marker).unlink()

    @unittest.skipUnless(os.name == "posix" and shutil.which("node"), "requires Node and POSIX shell")
    def test_baseline_and_candidate_run_every_retained_check_and_reject_changed_bytes(self):
        import hashlib
        import subprocess
        import tempfile

        script = Path(__file__).resolve().parents[2] / "deployment/local-stand/delivery-test-command.mjs"
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            protected, source, output = (root / name for name in ("protected", "source", "output"))
            checks = protected / "tests"
            for path in (protected, source, output, checks):
                path.mkdir()
            files = []
            for filename, body in (
                ("author.sh", f'if [ ! -f "$1/ready" ]; then\n'
                    f'  {json.dumps(shutil.which("node"))} "$1/missing-harness.mjs"\n  exit $?\nfi\n'
                    'if [ ! -f "$1/implementation" ]; then\n'
                    '  printf "%s\\n" octacity:assertion:failed\n  exit 1\nfi\n'.encode()),
                ("additional.sh", b'sourced=1\n. "$OCTACITY_TESTS_ROOT/helpers.sh"\ncheck_integration "$1"\n'),
                ("helpers.sh", b'test "${sourced:-}" = 1 || exit 2\n'
                    b'check_integration() {\n'
                    b'  if [ ! -f "$1/integration" ]; then\n'
                    b'    printf "%s\\n" octacity:assertion:failed\n    return 1\n  fi\n}\n'),
            ):
                (checks / filename).write_bytes(body)
                files.append({"class": "acceptance", "path": filename, "artifact": {
                    "encoded_size": len(body), "content_digest": hashlib.sha256(body).hexdigest(),
                }})
            (protected / "test-inventory.json").write_text(json.dumps({"files": files}))
            digest = "a" * 64
            (protected / "command-input.json").write_text(json.dumps({
                "node": {"input_digest": digest}, "input": {"subject": {"base_revision": "base"}},
                "profile": {"model_or_tool": {"identity": "baseline"}},
            }))
            (protected / "validation-input.json").write_text(json.dumps({
                "input_digest": digest, "classes": ["acceptance"],
                "candidate": {"candidate_revision": "candidate", "changeset_digest": "b" * 64}, "tests": {},
            }))

            def run(mode):
                policy = {"criterion": {"identity": "check"}, "environment": {"identity": "guest"},
                          "entrypoints": ["author.sh", "additional.sh"], "explicit_assertion_failure": True}
                return subprocess.run(["node", str(script), mode, json.dumps(policy), str(protected),
                    str(source), str(output)], capture_output=True, check=False)

            baseline = run("baseline")
            self.assertEqual(baseline.returncode, 0, baseline.stderr.decode())
            report_path = output / "baseline.json"
            report = json.loads(report_path.read_text())
            self.assertEqual((report["status"], report["finding"]), ("environment_unavailable", "indeterminate"))
            report_path.unlink()
            (source / "ready").touch()
            baseline = run("baseline")
            self.assertEqual(baseline.returncode, 0, baseline.stderr.decode())
            report = json.loads((output / "baseline.json").read_text())
            self.assertEqual(report["finding"], "matched")
            self.assertEqual(report["test_classes"], [{"class": "acceptance", "outcome": "failed"}])
            (source / "implementation").touch()
            candidate = run("candidate")
            self.assertEqual(candidate.returncode, 0, candidate.stderr.decode())
            report = json.loads((output / "validation.json").read_text())
            self.assertEqual(report["classes"], [{"class": "acceptance", "outcome": "failed"}])
            for name in ("validation.json", "validation-facts.json"):
                (output / name).unlink()
            (source / "integration").touch()
            candidate = run("candidate")
            self.assertEqual(candidate.returncode, 0, candidate.stderr.decode())
            report = json.loads((output / "validation.json").read_text())
            self.assertEqual(report["classes"], [{"class": "acceptance", "outcome": "passed"}])
            for name in ("validation.json", "validation-facts.json"):
                (output / name).unlink()
            (checks / "additional.sh").write_text("exit 0\n")
            self.assertNotEqual(run("candidate").returncode, 0)
            self.assertFalse((output / "validation.json").exists())

    @unittest.skipUnless(os.name == "posix" and shutil.which("node"), "requires Node and POSIX shell")
    def test_frozen_supplement_runs_only_selected_shell_entries_with_relative_binary_fixtures(self):
        import base64
        import hashlib
        import subprocess
        import tempfile

        script = Path(__file__).resolve().parents[2] / "deployment/local-stand/delivery-test-command.mjs"
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            protected, source, output = (root / name for name in ("protected", "source", "output"))
            for path in (protected, protected / "tests", source, output):
                path.mkdir()
            generated = b"exit 0\n"
            (protected / "tests/checks.sh").write_bytes(generated)
            files = []
            for path, body in (
                ("suite/check.sh", b'size=$(wc -c < "$(dirname "$0")/fixture.bin") || exit 2\ntest "$size" -eq 3 || exit 2\ntest -f "$1/fixed"\n'),
                ("suite/Octafile", b"version: 1\nincludes:\n  Checks: checks.yml\n"),
                ("suite/checks.yml", b'version: 1\ntasks:\n  _Verify:\n    shell: test -f "{{ OCTACITY_TEST_SOURCE }}/fixed"\n'),
                ("suite/fixture.bin", b"\x00\xff\n"),
                ("suite/not-selected.sh", b"exit 2\n"),
            ):
                files.append({"path": path, "content_base64": base64.b64encode(body).decode(),
                              "sha256": hashlib.sha256(body).hexdigest()})
            inventory = {"files": [{"path": "checks.sh", "class": "acceptance",
                "artifact": {"encoded_size": len(generated), "content_digest": hashlib.sha256(generated).hexdigest()}}],
                "supplement": {"files": files, "entries": [{"kind": "shell", "path": "suite/check.sh"}]}}
            digest = "a" * 64
            (protected / "command-input.json").write_text(json.dumps({
                "node": {"input_digest": digest}, "input": {"subject": {"base_revision": "base"}},
                "profile": {"model_or_tool": {"identity": "baseline"}},
            }))
            (protected / "validation-input.json").write_text(json.dumps({
                "input_digest": digest, "classes": ["acceptance"],
                "candidate": {"candidate_revision": "candidate", "changeset_digest": "b" * 64}, "tests": {},
            }))

            runner = root / "octa-runner"
            runner.write_text(f"#!{shutil.which('node')}\n" + """const { readFileSync } = require('node:fs');
const { join } = require('node:path');
const { request, request_id } = JSON.parse(readFileSync(0, 'utf8'));
if (request.commands.join() !== 'Checks:_Verify' || request.octafile !== 'suite/Octafile' ||
    readFileSync(join(request.workspace, request.octafile)).length === 0 ||
    readFileSync(join(request.variables.OCTACITY_TESTS_ROOT, 'suite/fixture.bin')).length !== 3)
  process.exit(2);
const fs = require('node:fs');
const selected = request.variables.OCTACITY_TEST_SOURCE;
const skip = fs.existsSync(join(selected, 'skip'));
const fixed = fs.existsSync(join(selected, 'fixed'));
const status = fixed || skip ? 'succeeded' : 'failed';
const conclusion = { status: skip ? 'skipped' : status };
if (status === 'failed') conclusion.failure = { kind: 'command', exit_code: 1 };
console.log(JSON.stringify({ type: 'finished', request_id, status,
  results: [{ command: 'Checks:_Verify', conclusion }] }));
process.exit(status === 'succeeded' ? 0 : 1);
""")
            runner.chmod(0o700)

            def run(mode):
                (protected / "test-inventory.json").write_text(json.dumps(inventory))
                policy = {"criterion": {}, "environment": {}, "entrypoints": ["checks.sh"]}
                return subprocess.run(["node", str(script), mode, json.dumps(policy), str(protected), str(source),
                    str(output), str(runner)], capture_output=True, check=False)

            result = run("baseline")
            self.assertEqual(result.returncode, 0, result.stderr.decode())
            report = json.loads((output / "baseline.json").read_text())
            self.assertEqual((report["status"], report["finding"]), ("needs_input", "indeterminate"))
            (source / "fixed").touch()
            result = run("candidate")
            self.assertEqual(result.returncode, 0, result.stderr.decode())
            self.assertEqual(json.loads((output / "validation.json").read_text())["classes"],
                             [{"class": "acceptance", "outcome": "passed"}])
            (output / "baseline.json").unlink()
            inventory["supplement"]["entries"] = [{"kind": "octafile", "path": "suite/Octafile", "task": "Checks:_Verify"}]
            result = run("baseline")
            self.assertEqual(result.returncode, 0, result.stderr.decode())
            self.assertEqual(json.loads((output / "baseline.json").read_text())["finding"], "not_matched")
            (output / "validation.json").unlink()
            (output / "baseline.json").unlink()
            (source / "skip").touch()
            result = run("baseline")
            self.assertEqual(result.returncode, 0, result.stderr.decode())
            self.assertEqual(json.loads((output / "baseline.json").read_text())["finding"], "indeterminate")
            files[1]["content_base64"] = base64.b64encode(b"changed").decode()
            self.assertNotEqual(run("candidate").returncode, 0)
            self.assertFalse((output / "validation.json").exists())

    @unittest.skipUnless(
        os.name == "posix" and os.getuid() != 0 and os.environ.get("OCTACITY_DELIVERY_TEST_IMAGE")
        and os.environ.get("OCTACITY_DELIVERY_TEST_NATIVE"),
        "requires a non-root Factory host and the qualified Delivery image and native installation",
    )
    def test_verifier_separates_check_workspaces_and_seals_its_reports(self):
        import base64
        import hashlib
        import subprocess
        import tempfile

        sha = lambda body: hashlib.sha256(body).hexdigest()
        verifier = Path(__file__).resolve().parents[2] / "deployment/local-stand/delivery-test-command.mjs"
        native = Path(os.environ["OCTACITY_DELIVERY_TEST_NATIVE"]).resolve()
        uid, gid = os.getuid(), os.getgid()
        owner = f"uid={uid},gid={gid}"
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            for path in ("source", "output", "scratch", "protected/tests",
                         "anchor/source", "anchor/output", "anchor/scratch"):
                (root / path).mkdir(parents=True)
            generated = b"exit 0\n"
            (root / "protected/tests/checks.sh").write_bytes(generated)
            check = b'''setsid /bin/sh -c 'sleep 0.05; touch /workspace/scratch/escaped' </dev/null >/dev/null 2>&1 &
if (touch /workspace/output/untrusted) 2>/dev/null; then exit 1; fi
printf 'MODE=unselected\n' > "$HOME/shared.env" || exit 2
printf 'MODE=unselected\n' > /tmp/shared.env || exit 2
printf 'MODE=unselected\n' > /dev/shm/octacity-verification.env || exit 2
'''
            wait = b'''sleep 0.2
test ! -e /workspace/scratch/escaped &&
test ! -e "$HOME/shared.env" &&
test ! -e /tmp/shared.env
'''
            files = [{"path": name, "content_base64": base64.b64encode(body).decode(), "sha256": sha(body)}
                     for name, body in [
                         ("check.sh", check), ("wait.sh", wait),
                         ("Octafile.yml", b"version: 1\nincludes:\n  Checks: checks.yml\n"),
                         ("checks.yml", b"version: 1\ntasks:\n  _Verify:\n"
                          b"    shell: test ! -e /dev/shm/octacity-verification.env\n"),
                     ]]
            digest = "a" * 64
            for name, content in [
                ("command-input", {"node": {"input_digest": digest}}),
                ("validation-input", {"input_digest": digest, "classes": ["acceptance"], "candidate": {}}),
                ("test-inventory", {"files": [{"path": "checks.sh", "class": "acceptance",
                    "artifact": {"encoded_size": len(generated), "content_digest": sha(generated)}}],
                    "supplement": {"files": files, "entries": [
                        {"kind": "shell", "path": "check.sh"},
                        {"kind": "shell", "path": "wait.sh"},
                        {"kind": "octafile", "path": "Octafile.yml", "task": "Checks:_Verify"},
                    ]}}),
            ]:
                (root / f"protected/{name}.json").write_text(json.dumps(content))
            result = subprocess.run([str(native / "microsandbox/bin/msb"), "run", "--pull", "never",
                "--no-tty", "--no-stdin", "--timeout", "15s", "--no-net",
                "--security", "restricted", "--user", f"{uid}:{gid}",
                "--mount-dir", f"{native / 'octa'}:/opt/octacity/octa:ro,{owner}",
                "--mount-dir", f"{root / 'anchor'}:/workspace:ro,{owner}",
                "--mount-dir", f"{root / 'source'}:/workspace/source:ro,{owner}",
                "--mount-dir", f"{root / 'scratch'}:/workspace/scratch:{owner}",
                "--mount-dir", f"{root / 'output'}:/workspace/output:{owner}",
                "--mount-dir", f"{root / 'protected'}:/octacity/protected:ro,{owner}",
                "--mount-file", f"{verifier}:/verifier.mjs:ro,{owner}",
                os.environ["OCTACITY_DELIVERY_TEST_IMAGE"], "--", "/usr/local/bin/node", "/verifier.mjs",
                "candidate", json.dumps({"entrypoints": ["checks.sh"], "isolate_checks": True}),
                "/octacity/protected", "/workspace/source", "/workspace/output"],
                capture_output=True, text=True, timeout=30, check=False)
            self.assertEqual(result.returncode, 0, result.stderr)
            report = json.loads((root / "output/validation.json").read_text())
            self.assertEqual(report["classes"], [{"class": "acceptance", "outcome": "passed"}])
            self.assertFalse((root / "output/untrusted").exists())
            self.assertFalse((root / "scratch/escaped").exists())

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
        reference = {"artifact_id": project_id, "content_digest": "f" * 64, "encoded_size": len(pack.program)}
        inventory = {"plan_digest": "a" * 64, "input_digest": "b" * 64, "node_attempt_id": project_id,
                     "files": [{"class": pack.name, "path": "checks.sh", "artifact": reference}],
                     "transcript": {**reference, "encoded_size": 100}}
        self.assertGreaterEqual(author["policy"]["policy"]["max_bytes"],
                                len(pack.program) + len(json.dumps(inventory).encode()))
        for verifier in (base, validation):
            command = json.loads(verifier["profiles"][0]["program"]["octafile"])["tasks"]["test"]["shell"]
            self.assertIn('"entrypoints":["checks.sh"]', command.partition(" <<")[0])
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

    def test_standard_delivery_verifiers_bind_to_the_installed_tester_without_an_imported_pack(self):
        from factory_delivery_setup import install_test_commands, _standard_verifiers_current

        legacy_alias = "delivery-candidate-isolated-tester-assertions-0123456789ab"
        legacy = {"kind": "node_capability", "alias": legacy_alias,
                  "reference": {"identity": legacy_alias, "version": "v1", "digest": "e" * 64}}
        project = {"project_id": "project", "references": [legacy]}
        catalog = {"projects": [project]}
        shell = {"identity": "shell", "version": "v1", "digest": "a" * 64}
        model = {"identity": "tester-model", "version": "v1", "digest": "b" * 64}
        git = {"identity": "git", "version": "v1", "digest": "c" * 64}
        worker = {"build_configuration": {"project_id": project["project_id"]}, "pipeline_node": "worker",
                  "project_permissions": {"mounts": [{"root": "/octacity/protected", "mode": "read_only"}],
                  "resources": {"elapsed_millis": 120000}, "outputs": {"max_report_count": 4, "max_artifact_count": 4}},
                  "local_permissions": {"enforceable": []}}
        inventory = {"kind": "protected_inventory", "output_kind": "artifact",
                     "schema": {"identity": "inventory", "version": "v1", "digest": "d" * 64},
                     "tool": shell, "plugin": shell}
        policy = {"classes": ["acceptance"], "max_files": 1, "max_bytes": 16000, "reserved_inputs": 8,
                  "retention_millis": 3600000}
        tester = {"toolchain": {"model": model}, "protected_tests": {
            "inventory": inventory, "policy": policy,
            "files": [{"output": "tester-checks", "class": "acceptance", "path": "checks.sh"}],
        }}

        install_test_commands(catalog, project, worker, [shell], git, None, author=tester)
        self.assertEqual(catalog.get("protected_test_authors", []), [])
        baseline, candidate = catalog["protected_validation"]
        choices = {choice["alias"]: choice["reference"] for choice in project["references"]}
        self.assertEqual(choices[legacy_alias], legacy["reference"])
        for mode, command in (("baseline", baseline), ("candidate", candidate)):
            executor = command["profiles"][0]["executor"]
            self.assertTrue(executor["identity"].startswith(f"delivery-{mode}-isolated-v2-tester-assertions-"))
            self.assertEqual(choices[executor["identity"]], executor)
        for command in (baseline, candidate):
            self.assertEqual(command["inputs"]["author_profile"], model)
            self.assertEqual(command["inputs"]["inventory"], inventory)
            self.assertEqual(command["inputs"]["policy"], policy)
            profile, = command["profiles"]
            self.assertEqual(profile["project_permissions"]["secret_profiles"], [])
            self.assertEqual(profile["project_permissions"]["network_hosts"], [])
        self.assertTrue(baseline["profiles"][0]["observation_contract"]["requires_protected_tests"])
        self.assertEqual(candidate["inputs"]["capture_tool"], git)

        self.assertTrue(_standard_verifiers_current(catalog, project["project_id"], model))
        candidate["profiles"][0]["program"]["octafile"] = "{}"
        self.assertFalse(_standard_verifiers_current(catalog, project["project_id"], model))

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
