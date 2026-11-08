"""The stand publishes triage through ordinary public management contracts."""
import copy
from contextlib import redirect_stdout
import hashlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import io
import os
import shutil
import socket
import ssl
import subprocess
import tempfile
import tarfile
from pathlib import Path
import sys
import threading
import time
import unittest
import urllib.error
import urllib.request
import uuid
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import bootstrap_local_stand as bootstrap
import factory_local_stand as stand


class FactoryStandTriageTests(unittest.TestCase):
    @unittest.skipUnless(os.name == "posix", "private stand initialization requires POSIX ownership and modes")
    def test_guest_image_contains_only_the_public_ca_and_returns_the_verified_msb_identity(self):
        import factory_triage_setup as setup
        import types
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            stand.initializer.initialize(root)
            calls = []
            digest = "sha256:" + "c" * 64
            missing = {"image": False}

            def execute(arguments, **options):
                calls.append(arguments)
                if "build" in arguments:
                    context = Path(arguments[-1])
                    expected = {"Dockerfile", "local-ca.pem"}
                    if "factory-delivery" in arguments[arguments.index("--tag") + 1]:
                        expected.add("bwrap")
                        self.assertEqual((context / "bwrap").read_bytes(), b"qualified sandbox")
                        self.assertIn("/usr/bin/bwrap --version", (context / "Dockerfile").read_text())
                        self.assertIn("/usr/bin/git --version", (context / "Dockerfile").read_text(),
                            "Delivery must qualify local candidate inspection before publishing its guest")
                    self.assertEqual({path.name for path in context.iterdir()}, expected)
                    self.assertEqual((context / "local-ca.pem").read_bytes(), (root / "pki/local-ca.pem").read_bytes())
                    recipe = (context / "Dockerfile").read_text()
                    self.assertIn("@sha256:", recipe)
                    self.assertIn("update-ca-trust", recipe)
                    self.assertIn("/etc/pki/ca-trust/source/anchors/", recipe)
                    self.assertEqual(options["env"]["MSB_HOME"], str(stand.integration.agent_config.microsandbox_state_root(root) / "microsandbox"))
                if "inspect" in arguments:
                    if missing["image"]:
                        missing["image"] = False
                        raise subprocess.CalledProcessError(1, arguments)
                    return types.SimpleNamespace(stdout=json.dumps({"digest": digest, "os": "linux", "architecture": "arm64"}))
                if "save" in arguments:
                    Path(arguments[arguments.index("--output") + 1]).write_bytes(b"finite-immutable-image-archive")
                return types.SimpleNamespace(stdout="")

            image = setup.prepare_guest_image(root, root / "native", execute=execute)
            self.assertEqual(image, "local.octacity/factory-triage@" + digest)
            self.assertTrue(any("load" in command and "--input" in command for command in calls))
            calls.clear()
            self.assertEqual(setup.prepare_guest_image(root, root / "native", execute=execute), image)
            self.assertFalse(any("build" in command for command in calls), "a retained verified image is reused")
            calls.clear()
            missing["image"] = True
            self.assertEqual(setup.prepare_guest_image(root, root / "native", execute=execute), image)
            self.assertFalse(any("build" in command for command in calls),
                "cache recovery must reimport the retained immutable archive without rebuilding its identity")
            calls.clear()
            legacy = root / "delivery-guest"
            legacy.mkdir(mode=0o700)
            (legacy / "image.docker.tar").write_bytes(b"previously-published-guest")
            (legacy / "inputs.json").write_text('{"recipe":"previous"}')
            def download_sandbox(source, target):
                self.assertEqual(source["tag"], "rust-v0.161.0")
                self.assertEqual(source["sha256"], "1c96e12519c4b6ee1c634c822f8ea2775b006b3d6f4ca16e0648d1dd54f441fe")
                with tarfile.open(target, "w:gz") as archive:
                    member = tarfile.TarInfo("bwrap-aarch64-unknown-linux-musl")
                    member.size = len(b"qualified sandbox")
                    archive.addfile(member, io.BytesIO(b"qualified sandbox"))
            with mock.patch.object(setup.runtime, "download", side_effect=download_sandbox):
                delivery_image = setup.prepare_guest_image(root, root / "native", delivery=True, execute=execute)
            self.assertEqual(delivery_image, "local.octacity/factory-delivery@" + digest)
            self.assertEqual((legacy / "image.docker.tar").read_bytes(), b"previously-published-guest")
            self.assertEqual((legacy / "inputs.json").read_text(), '{"recipe":"previous"}')
            self.assertTrue(any("/usr/local/bin/node --version" in path.read_text()
                for path in (root / "delivery-guest").rglob("Dockerfile")))
            self.assertEqual(setup.prepare_guest_image(root, root / "native", execute=execute), image)
            second_home = root / "separate-worker/microsandbox"
            missing["image"] = True
            calls.clear()
            self.assertEqual(setup.prepare_guest_image(root, root / "native", delivery=True,
                microsandbox_home=second_home, execute=execute), delivery_image)
            self.assertTrue(any("load" in command for command in calls))

    def test_plain_cli_selects_the_triage_installer_instead_of_automatic_demonstration_runs(self):
        expected = {"project": "selected-project", "configuration": {"id": "triage", "version": 1}, "input": {"enabled": False}}
        output = io.StringIO()
        with tempfile.TemporaryDirectory() as temporary, redirect_stdout(output), \
                mock.patch.object(stand, "run_stand", return_value={"runs": {"automatic": "old-demo"}}, create=True), \
                mock.patch.object(stand, "run_triage_stand", return_value=expected, create=True):
            self.assertEqual(stand.main(["--root", temporary]), 0)
        self.assertEqual(json.loads(output.getvalue()), expected)

    def test_delivery_cli_extends_the_retained_project_without_selecting_a_new_stand(self):
        with tempfile.TemporaryDirectory() as temporary, redirect_stdout(io.StringIO()), \
                mock.patch.object(stand, "run_triage_stand", return_value={"project": "retained"}) as install:
            root = Path(temporary)
            self.assertEqual(stand.main(["--root", str(root), "--delivery-test-pack", str(root / "ticket.json"),
                "--delivery-write-directory", "tools"]), 0)
            self.assertEqual(install.call_args.args[0], root)
            self.assertEqual(install.call_args.kwargs["delivery_test_packs"], [root / "ticket.json"])
            self.assertEqual(install.call_args.kwargs["delivery_write_directories"], ["tools"])

    def test_triage_selection_pins_current_source_access_and_unlabeled_scope_without_reporter_writes(self):
        import factory_triage_setup as setup
        namespace = uuid.uuid4()
        project, repository, connection = (str(uuid.uuid4()) for _ in range(3))
        pin = {"identity": "reviewed-reader", "version": "v1", "digest": "a" * 64}
        binding = {"installation": pin, "resource": pin, "capability": pin, "configuration": pin}
        source = {"purpose": "task_read", "binding": binding, "connection_configuration": pin,
            "predicate_states": ["open", "closed"], "predicate_label_presence": ["any", "none"]}
        choices = {"connection_id": connection, "connection_version": 3, "access_version": 7, "items": [source]}
        defaults = {kind: {"project_id": project, "id": str(uuid.uuid4()), "version": version, "digest": "c" * 64}
                    for kind, version in (("launch_policy", 3), ("access_profile", 5))}
        reads = []

        class Management(BaseHTTPRequestHandler):
            def log_message(self, *_):
                pass
            def do_GET(self):
                reads.append(self.path)
                value = choices if "factory-source-choices" in self.path else {"definition_digest": "b" * 64}
                contents = json.dumps(value).encode()
                self.send_response(200)
                self.send_header("Content-Length", str(len(contents)))
                self.end_headers()
                self.wfile.write(contents)

        with ThreadingHTTPServer(("127.0.0.1", 0), Management) as server:
            thread = threading.Thread(target=server.serve_forever, daemon=True)
            thread.start()
            client = bootstrap.BootstrapClient(stand.BootstrapTransport(server.server_port),
                bootstrap.RetryPolicy(attempts=1, timeout_seconds=2, initial_delay_seconds=0))
            try:
                profiles = {"eligibility": {"profile": "reviewed-project"}, "classification": {"profile": "reviewed-analyst"}}
                context = {"project_id": project, "goals": "frozen-goals", "rules": "frozen-rules", "documents": {}}
                selection = setup.pipeline_selection(client, {"project": project, "repository": repository},
                    profiles, context, {"id": connection, "version": 3}, namespace, {"network_hosts": []}, policy_defaults=defaults)
                self.assertEqual(selection["profiles"], profiles)
                self.assertEqual(selection["context"], {stage: context for stage in profiles})
                self.assertEqual(selection["input"]["connection"], {"id": connection, "version": 3,
                    "access_version": 7, "source_configuration": pin, "reporter_configuration": None})
                self.assertEqual(selection["input"]["predicate"], {"state": "open", "label_presence": "none",
                    "all_of": [], "any_of": [], "none_of": []})
                self.assertEqual(selection["input"]["source"], binding)
                self.assertIsNone(selection["input"]["reporter"])
                self.assertEqual(selection["effects"], {})
                self.assertEqual(selection["execution"]["budget"]["max_tokens"], 300000)
                self.assertEqual(selection["configuration"], {
                    "admission_policy": {"project_policy": defaults["launch_policy"]},
                    "permission_ceiling": {"project_policy": defaults["access_profile"]},
                    "wip_limits": {"max_active_runs": 1, "max_active_nodes": 1},
                    "hard_budget": selection["execution"]["budget"],
                })
                self.assertEqual(selection["repository"]["definition_digest"], "b" * 64)
                source["predicate_label_presence"] = ["any"]
                with self.assertRaisesRegex(ValueError, "unlabeled"):
                    setup.pipeline_selection(client, {"project": project, "repository": repository}, profiles,
                        context, {"id": connection, "version": 3}, namespace, {"network_hosts": []}, policy_defaults=defaults)
            finally:
                server.shutdown()
                thread.join(timeout=2)
        self.assertTrue(any(f"connection_id={connection}&connection_version=3" in path for path in reads))

    @unittest.skipUnless(os.name == "posix", "private credential installation requires POSIX ownership and modes")
    def test_native_triage_installation_keeps_model_login_and_project_mcp_separate(self):
        import factory_triage_setup as setup
        import configure_local_stand_agent as agent
        import factory_local_runtime as runtime
        project, configuration, namespace = str(uuid.uuid4()), str(uuid.uuid4()), uuid.uuid4()
        definition = {"enabled": True}

        class Management(BaseHTTPRequestHandler):
            def log_message(self, *_):
                pass
            def do_GET(self):
                contents = json.dumps({"id": configuration, "project_id": project, "version": 1, "definition": definition}).encode()
                self.send_response(200)
                self.send_header("Content-Length", str(len(contents)))
                self.end_headers()
                self.wfile.write(contents)

        with tempfile.TemporaryDirectory() as temporary, ThreadingHTTPServer(("127.0.0.1", 0), Management) as server:
            base = Path(temporary).resolve()
            program = base / "codex"
            program.write_text("#!/bin/sh\necho codex-cli 0.161.0\n")
            program.chmod(0o700)
            native = {"executable": program, "sha256": hashlib.sha256(program.read_bytes()).hexdigest(),
                "policy": json.loads(runtime.POLICY.read_text()),
                "octa": {"version": "0.8.0", "plugin_digests": {"codex": "a" * 64}}}
            helper = base / "runtime-helper"
            helper.write_bytes(b"qualified native companion")
            helper.chmod(0o700)
            native["companions"] = [{"identity":"runtime-helper", "version":"0.161.0", "platform":"linux-aarch64",
                "path":str(helper), "sha256":hashlib.sha256(helper.read_bytes()).hexdigest(), "guest_filename":"runtime-helper"}]
            home = base / "operator"
            home.mkdir(mode=0o700)
            auth = home / "auth.json"
            auth.write_text('{"auth_mode":"chatgpt","tokens":{"access_token":"private-model-canary","refresh_token":"private-refresh-canary"}}')
            auth.chmod(0o600)
            (home / "config.toml").write_text("operator-private-mcp-canary")
            root = base / "stand"
            thread = threading.Thread(target=server.serve_forever, daemon=True)
            thread.start()
            client = bootstrap.BootstrapClient(stand.BootstrapTransport(server.server_port),
                bootstrap.RetryPolicy(attempts=1, timeout_seconds=2, initial_delay_seconds=0))
            try:
                with mock.patch.dict(os.environ, {"CODEX_HOME": str(home)}):
                    result = setup.prepare_runtime(root, native, {"project": project, "configuration": configuration,
                        "pipeline_node": "triage-agent"}, client)
                    again = setup.prepare_runtime(root, native, {"project": project, "configuration": configuration,
                        "pipeline_node": "triage-agent"}, client)
                self.assertEqual(result, again)
                catalog = json.loads(Path(result["profiles"]).read_text())
                compiled, = catalog["codex"]
                installed, = [entry for entry in catalog["projects"][0]["references"]
                    if entry["kind"] == "permission_ceiling"]
                contents = json.dumps(installed["metadata"]["permission_declarations"],
                    sort_keys=True, separators=(",", ":")).encode()
                self.assertEqual(installed["reference"]["digest"], hashlib.sha256(contents).hexdigest())
                self.assertEqual(compiled["authentication"], "native_login_file")
                self.assertEqual(compiled["usage_policy"], {"kind": "subscription"})
                outputs = compiled["project_permissions"]["outputs"]
                self.assertGreaterEqual(outputs["max_artifact_count"], 2)
                self.assertGreaterEqual(outputs["max_report_count"], 1)
                ceiling = catalog["projects"][0]["role_profiles"]["triage-runtime"]["ceiling"]
                self.assertEqual(ceiling["budget"], {"max_attempts": 1, "max_elapsed_millis": 300000,
                    "max_tokens": 150000, "max_cost_micro_units": 1000000, "max_output_bytes": 67108864})
                self.assertLessEqual(outputs["max_artifact_bytes"] + outputs["max_report_bytes"],
                    ceiling["budget"]["max_output_bytes"])
                self.assertEqual(compiled["toolchain"]["model"]["identity"], "gpt-6.1-sol")
                self.assertEqual(compiled["build_configuration"]["id"], configuration)
                self.assertEqual(compiled["build_configuration"]["definition_digest"],
                    "26b3426b2593763c96d0890b4a77a0bbf66d13fc512b0c6b138a23c290f30a2a")
                selected = agent.read_factory_agent_profile(Path(result["agent_profile"]))
                any_arguments = [argument for command in selected["factory_permissions"]["commands"]
                    for argument in command["arguments"] if argument["kind"] == "any"]
                self.assertTrue(any_arguments)
                self.assertTrue(all(argument["max_bytes"] == 16384 for argument in any_arguments),
                    "the Agent must admit the existing common V3 command projection without narrowing its declared bounds")
                login = selected["factory_credential_profiles"]["triage-model"]
                self.assertTrue(login["persist_updates"])
                self.assertEqual(Path(login["path"]).read_bytes(), auth.read_bytes())
                reader = json.loads(Path(result["issue_mcp_readers"]).read_text())["readers"][0]
                self.assertEqual(reader["project_id"], project)
                self.assertNotIn("issue_ids", reader)
                token = Path(reader["credential_file"]).read_text()
                self.assertRegex(token, r"^ocmcp\.[a-z0-9-]+\.[0-9a-f]{64}$")
                mcp = selected["factory_credential_profiles"]["triage-mcp"]
                self.assertEqual(json.loads(Path(mcp).read_text()), {"ISSUE_MCP_TOKEN": token})
                self.assertNotEqual(Path(mcp), Path(login["path"]))
                self.assertEqual(selected["tool_executables"]["codex-cli"]["sha256"], native["sha256"])
                self.assertTrue(selected["tool_executables"]["runtime-helper"]["standalone"])
                self.assertEqual(selected["tool_executables"]["runtime-helper"]["guest_filename"], "runtime-helper")
                companion = {"identity":"runtime-helper", "version":"0.161.0", "digest":native["companions"][0]["sha256"]}
                self.assertIn(companion, compiled["project_permissions"]["executables"])
                self.assertIn(companion, ceiling["permissions"]["executables"])
                self.assertEqual(selected["factory_permissions"]["executables"],
                    sorted(selected["factory_permissions"]["executables"], key=lambda item:item["identity"]))

                self.assertNotIn("model_discovery", catalog["projects"][0]["role_profiles"]["triage-runtime"])
                public = Path(result["profiles"]).read_text() + Path(result["agent_profile"]).read_text()
                self.assertNotIn("private-model-canary", public)
                self.assertNotIn("private-refresh-canary", public)
                self.assertNotIn("operator-private-mcp-canary", public)
                self.assertNotIn(token, public)
                if binary := os.environ.get("OCTACITY_TEST_TRIAGE_SERVER"):
                    stand.initializer.initialize(root)
                    installation = base / "native"
                    installation.mkdir(mode=0o700)
                    job_policy = {"source": {"provider": "git", "plugin_version": "0.1.0",
                        "plugin_sha256": "1" * 64, "repository_parameter": "url"},
                        "octa": {**native["octa"], "runner_sha256": "2" * 64,
                            "runner_protocol": 3, "event_schema": 4, "plugin_protocol": 2}, "validity": 900}
                    policy_file = installation / "job-spec-policy.json"
                    policy_file.write_text(json.dumps(job_policy))
                    policy_file.chmod(0o600)
                    manifest = installation / "installation-manifest.json"
                    manifest.write_text(json.dumps({"schema_version": 1, "files": {
                        "job-spec-policy.json": hashlib.sha256(policy_file.read_bytes()).hexdigest()}}))
                    manifest.chmod(0o600)
                    configured = stand.server_config.generate_server_configuration(root, installation,
                        factory_profiles=Path(result["profiles"]), factory_issue_mcp_readers=Path(result["issue_mcp_readers"]))
                    offline = base / "offline"
                    private = offline / "run/octacity-secrets"
                    shutil.copytree(Path(configured["config"]).parent, private)
                    for filename in stand.initializer.CREDENTIAL_FILES.values():
                        shutil.copyfile(root / "secrets" / filename, private / filename)
                        (private / filename).chmod(0o600)
                    (offline / "run/octacity").mkdir()
                    shutil.copyfile(policy_file, offline / "run/octacity/job-spec-policy.json")
                    (offline / "run/octacity/job-spec-policy.json").chmod(0o600)
                    validated = subprocess.run([binary, "validate", configured["config"], "--file-root", str(offline)],
                        capture_output=True, text=True, timeout=15, env={"PATH": os.defpath})
                    self.assertEqual(validated.returncode, 0, validated.stderr)
                    self.assertIn("server configuration is valid", validated.stdout)
            finally:
                server.shutdown()
                thread.join(timeout=2)

    def test_triage_prerequisites_publish_an_ordinary_worker_without_starting_builds_or_demo_runs(self):
        namespace = uuid.uuid4()
        pool, image = str(uuid.uuid4()), "registry.example/worker@sha256:" + "b" * 64
        calls = []

        class Management(BaseHTTPRequestHandler):
            def log_message(self, *_):
                pass
            def do_POST(self):
                body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
                calls.append((self.path, body))
                contents = json.dumps({"resource": {"id": str(uuid.uuid5(namespace, self.path))}}).encode()
                self.send_response(200)
                self.send_header("Content-Length", str(len(contents)))
                self.end_headers()
                self.wfile.write(contents)

        with ThreadingHTTPServer(("127.0.0.1", 0), Management) as server:
            thread = threading.Thread(target=server.serve_forever, daemon=True)
            thread.start()
            client = bootstrap.BootstrapClient(stand.BootstrapTransport(server.server_port),
                bootstrap.RetryPolicy(attempts=1, timeout_seconds=2, initial_delay_seconds=0))
            try:
                resources = stand.prepare_triage_resources(client, pool, "https://github.com/OctaHive/OctaCity", image, namespace)
            finally:
                server.shutdown()
                thread.join(timeout=2)
        self.assertEqual([path for path, _ in calls], ["/api/v1/projects", "/api/v1/repositories",
            "/api/v1/pipelines", "/api/v1/build-configurations", f"/api/v1/projects/{resources['project']}/policy-versions"])
        self.assertEqual(calls[0][1]["name"], "OctaCity")
        definition = calls[3][1]["definition"]
        self.assertEqual(definition.get("source"), {"kind": "source_free_v1"})
        self.assertIsNone(definition["repository_id"])
        self.assertIsNone(definition["repository_version"])
        self.assertEqual(definition["allowed_pools"], [pool])
        self.assertEqual(definition["runtime"]["immutable_image"], image)
        self.assertEqual(definition["runtime"]["network"], {"mode": "restricted", "allowed_hosts":
            ["auth.openai.com", "chatgpt.com", "github.com", "host.microsandbox.internal"]})
        self.assertEqual(definition["cache"], {"namespace": None, "read": False, "write": False})
        self.assertEqual(definition["runtime"]["timeout_seconds"], 300)
        self.assertEqual(definition["runtime"]["memory_bytes"], 1073741824)
        outputs = definition["artifacts"]
        self.assertGreater(outputs["single_output_bytes"], 0)
        self.assertLessEqual(outputs["single_output_bytes"], min(outputs["artifact_bytes"], outputs["report_bytes"]))
        self.assertEqual(resources["pipeline_node"], calls[2][1]["dag"]["nodes"][0]["id"])
        self.assertEqual(calls[1][1]["definition"]["selection"]["default_reference"], "refs/heads/main")
        self.assertNotIn("trigger", resources)

    def test_publishes_two_separate_reviewed_roles_without_inheriting_investigation_credentials(self):
        project, namespace = str(uuid.uuid4()), uuid.uuid4()
        base = f"/api/v1/projects/{project}"
        calls, reads = [], []
        selected = {"identity": "gpt-6.1-sol", "version": "v1", "digest": "d" * 64}
        capability = {"definition": {"name": "Installed native model", "instructions": [],
            "toolchain": {"model": selected}, "model_parameters": {},
            "context_requirements": ["task", "completion", "project-goals", "project-rules"],
            "execution": {"cli_arguments": ["--ephemeral", "--skip-git-repo-check"],
                "environment": {"NO_PROXY": {"kind": "public", "value": "host.microsandbox.internal"},
                    "ISSUE_MCP_TOKEN": {"kind": "secret", "reference": {"provider": "agent-private", "reference": "ISSUE_MCP_TOKEN"}}},
                "mcp_servers": {"issues": {"configuration": {"identity": "issues", "version": "v1", "digest": "e" * 64}}},
                "integrations": {}},
            "permissions": {"secret_profiles": ["native-model", "project-reader"]}}}

        class Management(BaseHTTPRequestHandler):
            def log_message(self, *_):
                pass
            def reply(self, value):
                contents = json.dumps(value).encode()
                self.send_response(200)
                self.send_header("Content-Length", str(len(contents)))
                self.end_headers()
                self.wfile.write(contents)
            def do_GET(self):
                reads.append(self.path)
                self.reply(capability)
            def do_POST(self):
                body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
                calls.append((self.path, self.headers["Idempotency-Key"], body))
                if self.path == base + "/factory-role-profiles/validate":
                    return self.reply({"incompatibility": None, "profile": {"definition": body["definition"]}})
                reference = lambda identity: {"project_id": project, "id": identity, "version": 1, "digest": "f" * 64}
                self.reply({"role": {"reference": reference(body["role"]["id"])} if "role" in body else None,
                    "profiles": [{"reference": reference(row["id"])} for row in body["profiles"]]})

        with ThreadingHTTPServer(("127.0.0.1", 0), Management) as server:
            thread = threading.Thread(target=server.serve_forever, daemon=True)
            thread.start()
            client = bootstrap.BootstrapClient(stand.BootstrapTransport(server.server_port),
                bootstrap.RetryPolicy(attempts=1, timeout_seconds=2, initial_delay_seconds=0))
            try:
                selections = stand.publish_triage_roles(client, project, namespace,
                    runtime="reviewed-runtime", model_credential_profile="native-model")
                first_calls = copy.deepcopy(calls)
                self.assertEqual(stand.publish_triage_roles(client, project, namespace,
                    runtime="reviewed-runtime", model_credential_profile="native-model"), selections)
                self.assertEqual(calls[:len(first_calls)], calls[len(first_calls):], "restart replays exact publication intents")
            finally:
                server.shutdown()
                thread.join(timeout=2)
        reusable_text = json.dumps([body for _, _, body in first_calls]).lower()
        for assignment in ("duplicate", "fit", "priority_assessment", "recommended_route", "example result"):
            self.assertNotIn(assignment, reusable_text, "step duties and examples must not be embedded in reusable agents")
        profiles = [body["definition"] for path, _, body in first_calls if path.endswith("/validate")]
        self.assertEqual(len(profiles), 2)
        self.assertEqual([profile["context_requirements"] for profile in profiles],
            [["task"], ["task"]])
        self.assertEqual(reads, [base + "/factory-profile-capabilities/reviewed-runtime"] * 2)
        self.assertEqual([profile["toolchain"]["model"] for profile in profiles], [selected, selected])
        self.assertEqual(profiles[0]["execution"], capability["definition"]["execution"])
        self.assertEqual(profiles[1]["execution"]["mcp_servers"], {})
        self.assertEqual(profiles[1]["execution"]["integrations"], {})
        self.assertEqual(profiles[1]["execution"]["environment"], {"NO_PROXY": {"kind": "public", "value": "host.microsandbox.internal"}})
        self.assertEqual(profiles[1]["permissions"]["secret_profiles"], ["native-model"])
        self.assertEqual(capability["definition"]["permissions"]["secret_profiles"], ["native-model", "project-reader"])
        self.assertNotEqual(selections["eligibility"]["role"]["id"], selections["classification"]["role"]["id"])
        self.assertNotEqual(selections["eligibility"]["profile"]["id"], selections["classification"]["profile"]["id"])
        self.assertEqual({path for path, _, _ in first_calls}, {base + "/factory-role-publications", base + "/factory-role-profiles/validate"})

    @unittest.skipUnless(os.name == "posix", "private GitHub installation requires POSIX ownership")
    def test_explicit_catalog_enrolls_the_token_for_its_only_project_after_successful_setup(self):
        project = str(uuid.uuid4())
        calls = []
        resource = {"reference": {"identity": "selected-repository", "version": "v1", "digest": "a" * 64},
            "locator": "Example/Selected", "capabilities": [
                {"purpose": purpose, "credential": {"provider": "installed", "reference": purpose}}
                for purpose in ("task_read", "task_update")]}

        class Management(BaseHTTPRequestHandler):
            def log_message(self, *_):
                pass
            def do_POST(self):
                calls.append((self.path, json.loads(self.rfile.read(int(self.headers["Content-Length"])))))
                self.send_response(200)
                self.send_header("Content-Length", "2")
                self.end_headers()
                self.wfile.write(b"{}")

        with tempfile.TemporaryDirectory() as directory, ThreadingHTTPServer(("127.0.0.1", 0), Management) as server:
            root = Path(directory)
            profile = root / "profiles.json"
            profile.write_text(json.dumps({"version": 1, "projects": [{"project_id": project}], "codex": []}))
            profile.chmod(0o600)
            token = root / "token"
            token.write_text("explicit-private-token-canary")
            token.chmod(0o600)
            catalog = root / "config/integrations/catalog.json"
            catalog.parent.mkdir(parents=True)
            catalog.write_text(json.dumps({"installations": [{"installation": resource["reference"],
                "endpoints": ["https://api.github.com/"], "resources": [resource]}]}))
            catalog.chmod(0o600)
            thread = threading.Thread(target=server.serve_forever, daemon=True)
            thread.start()
            client = bootstrap.BootstrapClient(stand.BootstrapTransport(server.server_port),
                bootstrap.RetryPolicy(attempts=1, timeout_seconds=2, initial_delay_seconds=0))
            try:
                with mock.patch.object(stand.launcher, "main", return_value=0), mock.patch.object(stand.integration, "LocalStandClient", return_value=client):
                    self.assertEqual(stand.main(["--root", str(root), "--factory-profiles", str(profile), "--github-token-file", str(token)]), 0)
                provisioned = [body for path, body in calls if path.endswith("/credentials")]
                self.assertEqual([body["value"] for body in provisioned], ["explicit-private-token-canary"] * 2)
                grants = [body["intent"] for path, body in calls if path.endswith(f"/projects/{project}/access")]
                self.assertEqual(len(grants), 1)
                self.assertEqual(grants[0]["resources"], [{"reference": resource["reference"], "purposes": ["task_read", "task_update"]}])
            finally:
                server.shutdown()
                thread.join(timeout=2)

    @unittest.skipUnless(os.name == "posix", "private Factory bundles require POSIX ownership and modes")
    @unittest.skipUnless(shutil.which("docker"), "Docker Compose is required for effective deployment configuration")
    def test_factory_overlay_publishes_only_a_loopback_mcp_listener_with_an_explicit_route(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            profile = root / "config/server/factory-profiles.json"
            profile.parent.mkdir(parents=True)
            profile.write_text('{"version":1,"projects":[],"codex":[]}')
            profile.chmod(0o600)
            with mock.patch.dict(os.environ, {"OCTACITY_LOCAL_STAND_MCP_PORT": "9444"}):
                rendered = subprocess.run(stand.launcher._compose_command(root, "config", "--format", "json"),
                    env=stand.launcher._compose_environment(root, 8443), check=True, capture_output=True, text=True)
            gateway = json.loads(rendered.stdout)["services"]["gateway"]
            ports = [entry for entry in gateway["ports"] if entry["target"] == 8444]
            self.assertEqual([(entry["host_ip"], str(entry["published"])) for entry in ports], [("127.0.0.1", "9444")])
            route = next(entry for entry in gateway["volumes"] if entry["target"] == "/etc/nginx/octacity-local-extensions/factory-mcp.conf")
            self.assertTrue(route["read_only"])
            self.assertEqual(Path(route["source"]), Path(stand.__file__).parents[1] / "deployment/local-stand/factory-mcp.conf")
            base = (Path(stand.__file__).parents[1] / "deployment/local-stand/nginx.conf").read_text()
            self.assertIn("include /etc/nginx/octacity-local-extensions/*.conf;", base)

    @unittest.skipUnless(shutil.which("nginx"), "nginx is required for the actual gateway boundary")
    def test_microvm_gateway_forwards_only_the_scoped_mcp_route_and_no_management_requests(self):
        token, calls = "Bearer private-mcp-gateway-canary", []

        class Peer(BaseHTTPRequestHandler):
            def log_message(self, *_):
                pass
            def do_POST(self):
                self.rfile.read(int(self.headers["Content-Length"]))
                calls.append((self.path, self.headers.get("Authorization"), self.headers.get("X-Forwarded-User")))
                self.send_response(200 if self.headers.get("Authorization") == token else 403)
                self.end_headers()
                self.wfile.write(b"selected-read-scope")

        with tempfile.TemporaryDirectory() as directory, ThreadingHTTPServer(("127.0.0.1", 0), Peer) as peer:
            root = Path(directory)
            stand.initializer.initialize(root)
            stand.initializer.ensure_gateway_peer(root, "host.microsandbox.internal")
            with socket.socket() as listener:
                listener.bind(("127.0.0.1", 0))
                port = listener.getsockname()[1]
            route = (Path(stand.__file__).parents[1] / "deployment/local-stand/factory-mcp.conf").read_text()
            route = route.replace("listen 8444", f"listen 127.0.0.1:{port}")
            config = root / "nginx.conf"
            config.write_text(f'pid {root}/nginx.pid; error_log {root}/error.log crit; events {{}}\n'
                f'http {{ access_log off; client_body_temp_path {root}/body; '
                f'ssl_certificate {root}/pki/gateway.pem; ssl_certificate_key {root}/pki/gateway-key.pem; '
                f'upstream octacity_management {{ server 127.0.0.1:{peer.server_port}; }}\n{route}\n}}')
            thread = threading.Thread(target=peer.serve_forever, daemon=True)
            thread.start()
            process = subprocess.Popen([shutil.which("nginx"), "-p", str(root), "-c", str(config), "-g", "daemon off;"],
                stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            client = urllib.request.build_opener(urllib.request.ProxyHandler({}),
                urllib.request.HTTPSHandler(context=ssl.create_default_context(cafile=str(root / "pki/local-ca.pem"))))
            origin = f"https://host.microsandbox.internal:{port}"
            resolve = socket.getaddrinfo
            dns = mock.patch.object(socket, "getaddrinfo", side_effect=lambda host, *args, **kwargs:
                resolve("127.0.0.1" if host == "host.microsandbox.internal" else host, *args, **kwargs))
            dns.start()
            try:
                deadline = time.monotonic() + 5
                while True:
                    try:
                        client.open(origin + "/health", timeout=1)
                    except urllib.error.HTTPError as error:
                        self.assertEqual(error.code, 404)
                        error.close()
                        break
                    except urllib.error.URLError as error:
                        if isinstance(error.reason, ssl.SSLError):
                            raise
                        if process.poll() is not None or time.monotonic() >= deadline:
                            self.fail("the MCP-only gateway did not start")
                        time.sleep(0.02)
                request = urllib.request.Request(origin + "/mcp/issues", data=b'{"method":"tools/list"}',
                    headers={"Authorization": token, "X-Forwarded-User": "forged-admin"})
                with client.open(request, timeout=2) as response:
                    self.assertEqual(response.read(), b"selected-read-scope")
                for path in ("/api/v1/projects", "/health", "/", "/mcp/issues/other"):
                    with self.assertRaises(urllib.error.HTTPError) as denied:
                        client.open(origin + path, timeout=2)
                    self.assertEqual(denied.exception.code, 404)
                    denied.exception.close()
                self.assertEqual(calls, [("/mcp/issues", token, None)])
            finally:
                dns.stop()
                process.terminate()
                process.wait(timeout=5)
                peer.shutdown()
                thread.join(timeout=2)

    @unittest.skipUnless(os.name == "posix", "private credential installation requires POSIX ownership")
    def test_explicit_catalog_uses_the_shared_issue_mcp_preflight_before_stand_creation(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            readers = base / "readers.json"
            readers.write_text('{"version":1,"readers":[{"private":"mcp-file-canary"}]}')
            readers.chmod(0o600)
            profiles = base / "profiles.json"
            profiles.write_text('{"version":1,"projects":[],"codex":[]}')
            profiles.chmod(0o600)
            root = base / "stand"
            result = subprocess.run([sys.executable, str(Path(stand.__file__).with_name("factory-stand")),
                "--root", str(root), "--factory-profiles", str(profiles),
                "--factory-issue-mcp-readers", str(readers)], capture_output=True, text=True, timeout=5)
            self.assertEqual(result.returncode, 1, result.stderr)
            self.assertIn("Issue MCP installation is invalid", result.stderr)
            self.assertNotIn("mcp-file-canary", result.stdout + result.stderr)
            self.assertFalse(root.exists())

    @unittest.skipUnless(os.name == "posix", "private credential installation requires POSIX ownership")
    def test_token_file_is_checked_before_stand_setup_without_disclosing_its_value(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            credential = base / "github-token"
            credential.write_text("private-github-installation-canary")
            credential.chmod(0o644)
            root = base / "stand"
            result = subprocess.run([sys.executable, str(Path(stand.__file__).with_name("factory-stand")),
                "--root", str(root), "--github-token-file", str(credential)],
                capture_output=True, text=True, timeout=5)
            self.assertEqual(result.returncode, 1, result.stderr)
            self.assertIn("GitHub credential file is invalid", result.stderr)
            self.assertNotIn("private-github-installation-canary", result.stdout + result.stderr)
            self.assertFalse(root.exists())

    def test_publishes_the_resolved_two_stage_pipeline_without_starting_intake_or_work(self):
        """The example is a common HTTP client; a preset service is deliberately absent."""
        project, namespace = str(uuid.uuid4()), uuid.uuid4()
        base = f"/api/v1/projects/{project}"
        reference = lambda identity: {"project_id": project, "id": identity, "version": 1, "digest": "a" * 64}
        profiles = {stage: {"role": reference(str(uuid.uuid4())), "profile": reference(str(uuid.uuid4())),
                           "capability": "reviewed-runtime"} for stage in ("eligibility", "classification")}
        permissions = json.loads((stand.launcher.REPOSITORY / "config/factory/presets/triage/v1.json").read_text())["execution"]["permissions"]
        budget = json.loads(stand.triage.POLICY.read_text())["flow_budget"]
        input_id = str(uuid.uuid4())
        selected = {"profiles": profiles, "context": {},
            "execution": {"budget": budget, "permissions": permissions, "max_active_nodes": 1},
            "configuration": {"admission_policy": {"project_policy": reference(str(uuid.uuid4()))},
                "permission_ceiling": {"project_policy": reference(str(uuid.uuid4()))},
                "wip_limits": {"max_active_runs": 1, "max_active_nodes": 1}, "hard_budget": budget},
            "input": {"id": input_id, "source": "reviewed-source", "reporter": None,
                "predicate": {"state": "open", "label_presence": "none", "all_of": [], "any_of": [], "none_of": []}},
            "repository": {"project_id": project, "id": str(uuid.uuid4()), "version": 1, "definition_digest": "c" * 64}}
        worker_reference = {"id": str(uuid.uuid4()), "project_id": project, "version": 1, "definition_digest": "b" * 64}
        worker_source = [{"kind": "source_free_v1"}]
        published = {"flows": {}, "configurations": {}, "documents": {}, "inputs": {}}
        calls, reads, replies = [], [], {}
        normalized = {}

        class Management(BaseHTTPRequestHandler):
            def log_message(self, *_):
                pass
            def reply(self, value, status=200):
                data = json.dumps(value).encode()
                self.send_response(status)
                self.send_header("Content-Length", str(len(data)))
                self.end_headers()
                self.wfile.write(data)
            def do_GET(self):
                reads.append(self.path)
                if self.path == f"/api/v1/build-configurations/{worker_reference['id']}/versions/1":
                    return self.reply({"definition": {"source": worker_source[0]}})
                for item in profiles.values():
                    if self.path == f"{base}/factory-role-profiles/{item['profile']['id']}/versions/1":
                        return self.reply({"reference": item["profile"], "role": item["role"],
                            "definition": {"budget": budget, "permissions": permissions, "build_configuration": worker_reference}})
                self.reply({"error": "no preset service or other implicit dependency"}, 404)
            def do_POST(self):
                body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
                key = self.headers.get("Idempotency-Key")
                calls.append((self.path, key, body))
                replay = replies.get((self.path, key))
                if replay is not None:
                    if replay[0] != body:
                        return self.reply({"error": "changed replay"}, 409)
                    return self.reply(replay[1])
                if self.path == base + "/input-documents":
                    artifact = {"artifact_id": str(uuid.uuid4()), "content_digest": "d" * 64,
                                "encoded_size": len(body["content"].encode())}
                    published["documents"][body["name"]] = artifact
                    result = {"project_id": project, "artifact": artifact}
                elif self.path == "/api/v1/flow-data-schemas/validate":
                    result = {"reference": {"identity": body["identity"], "version": body["version"], "digest": "e" * 64},
                              "shape": body["shape"]}
                elif self.path == base + "/factory-execution-bindings/resolve":
                    result = {"build": {"result_outcome": body["result_outcome"], "server_binding": "exact-installed-compilation"},
                              "incompatibility": None}
                elif self.path == base + "/factory-gate-programs/validate":
                    result = {"policy": {"identity": "server-normalized-gate", "version": "v1", "digest": "f" * 64},
                              "parameters": body["program"]}
                elif self.path == "/api/v1/flow-definitions/validate":
                    definition = body["definition"]
                    normalized[body["id"]] = {"closure": {"root": {"id": body["id"], "version": 1, "digest": "b" * 64}},
                        "limits": definition["limits"], "data_schemas": definition["data_schemas"]}
                    result = normalized[body["id"]]
                elif self.path == "/api/v1/flow-definitions":
                    published["flows"][body["id"]] = body["definition"]
                    result = {"resource": {"flow": normalized[body["id"]]}}
                elif self.path == "/api/v1/factory-configurations/validate":
                    result = {"diagnostics": {"errors": []}}
                elif self.path == "/api/v1/factory-configurations":
                    if body["definition"]["enabled"] or body["definition"]["flow"]["closure"]["root"]["id"] not in published["flows"]:
                        return self.reply({"error": "publish the reviewed disabled definition"}, 422)
                    published["configurations"][body["id"]] = body["definition"]
                    result = {"resource": {"configuration": {"reference": reference(body["id"])}}}
                elif self.path == f"{base}/factory-source-inputs/{input_id}/versions":
                    entry, = body["intent"]["inputs"]
                    if self.headers.get("If-None-Match") != "*" or entry["definition"]["enabled"]:
                        return self.reply({"error": "input must remain disabled and use a creation precondition"}, 422)
                    retained = {"id": input_id, "project_id": project, "version": 1, "definition": entry["definition"]}
                    published["inputs"][input_id] = retained
                    result = {"publication": {"inputs": [{"input": retained}]}}
                else:
                    return self.reply({"error": "no legacy authoring, intake, inference or provider effect is authorized"}, 403)
                replies[(self.path, key)] = (body, result)
                self.reply(result)

        with ThreadingHTTPServer(("127.0.0.1", 0), Management) as server:
            worker = threading.Thread(target=server.serve_forever, daemon=True)
            worker.start()
            client = bootstrap.BootstrapClient(stand.BootstrapTransport(server.server_port),
                bootstrap.RetryPolicy(attempts=1, timeout_seconds=2, initial_delay_seconds=0))
            try:
                context = stand.publish_triage_context(client, project, namespace)
                selected["context"] = {stage: context for stage in profiles}
                original = copy.deepcopy(selected)
                receipt = stand.publish_triage(client, project, selected, namespace)
                first = copy.deepcopy(calls)
                self.assertEqual(stand.publish_triage(client, project, selected, namespace), receipt)
                self.assertEqual(calls[len(first):], first[1:], "repeat setup retains exact publication bodies and keys")
                worker_source[0] = {"kind": "repository"}
                # An older launch can lose its final receipt after publishing v1.
                # Preserve the committed Configuration even if the current
                # worker cannot validate that old definition again.
                retained = {**published["configurations"][receipt["configuration"]["id"]],
                            "reference": receipt["configuration"],
                            "admission_policy": {"identity": "legacy-launch", "version": "v1", "digest": "a" * 64},
                            "permission_ceiling": {"identity": "legacy-access", "version": "v1", "digest": "b" * 64}}
                before = len(calls)
                self.assertEqual(stand.publish_triage(client, project, selected, namespace,
                    retained_configuration=retained), receipt)
                self.assertEqual([path for path, _, _ in calls[before:]],
                    [f"{base}/factory-source-inputs/{input_id}/versions"])
                with self.assertRaisesRegex(ValueError, "source-free"):
                    stand.publish_triage(client, project, selected, uuid.uuid4())
            finally:
                server.shutdown()
                worker.join(timeout=3)
        self.assertEqual(selected, original)
        self.assertFalse(any("factory-presets" in path for path in reads))
        self.assertEqual(len(published["flows"]), 1)
        self.assertEqual(len(published["configurations"]), 1)
        configuration = published["configurations"][receipt["configuration"]["id"]]
        for field in ("admission_policy", "permission_ceiling"):
            self.assertEqual(configuration[field], original["configuration"][field])
        flow = next(iter(published["flows"].values()))
        nodes = {node["key"]: node for node in flow["root"]["nodes"]}
        self.assertEqual(set(nodes), {"eligibility", "eligibility_gate", "classification", "classification_gate"})
        for stage in profiles:
            assignment = "\n".join(nodes[stage]["task"])
            self.assertIn("Example result:", assignment)
            self.assertEqual(nodes[stage]["profile_selection"]["candidates"]["selected"]["build"]["server_binding"], "exact-installed-compilation")
            self.assertEqual(nodes[stage]["profile_selection"]["role"], profiles[stage]["role"])
        self.assertIn("duplicate", "\n".join(nodes["eligibility"]["task"]))
        self.assertIn("priority_assessment", "\n".join(nodes["classification"]["task"]))
        self.assertEqual(receipt["configuration"]["id"], next(iter(published["configurations"])))
        self.assertEqual(receipt["input"], {"project_id": project, "id": input_id, "version": 1})
        self.assertFalse(published["inputs"][input_id]["definition"]["enabled"])


if __name__ == "__main__":
    unittest.main()
