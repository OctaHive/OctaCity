"""Opt-in qualification of the default setup against current ordinary Server owners.

Uses an isolated database in the task's disposable PostgreSQL and its S3 peer.
No Agent, inference, Source activation or provider mutation is dispatched here.
"""
from contextlib import contextmanager
import hashlib
import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import patch
from urllib.parse import urlsplit, urlunsplit
import uuid

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import bootstrap_local_stand as bootstrap
import factory_local_runtime as runtime
import factory_local_stand as stand
import factory_triage_setup as triage


@contextmanager
def server(root, installation, binary, environment, database, profiles=None, readers=None):
    configured = stand.server_config.generate_server_configuration(root, installation,
        factory_profiles=profiles, factory_issue_mcp_readers=readers)
    offline = root / ("offline-" + uuid.uuid4().hex)
    private = offline / "secrets"
    shutil.copytree(Path(configured["config"]).parent, private)
    for filename in stand.initializer.CREDENTIAL_FILES.values():
        shutil.copyfile(root / "secrets" / filename, private / filename)
        (private / filename).chmod(0o600)
    url = urlsplit(environment["OCTACITY_POSTGRES_URL"])
    (private / "postgres-url").write_text(urlunsplit((url.scheme, url.netloc, "/" + database, url.query, url.fragment)))
    (private / "object-access-key").write_text(environment["OCTACITY_MINIO_ACCESS_KEY"])
    (private / "object-secret-key").write_text(environment["OCTACITY_MINIO_SECRET_KEY"])
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0))
        port = listener.getsockname()[1]
    text = Path(configured["config"]).read_text()
    text = text.replace('management_bind = "0.0.0.0:8080"', f'management_bind = "127.0.0.1:{port}"')
    text = text.replace('agent_bind = "0.0.0.0:8081"\n', '').replace('cache_bind = "0.0.0.0:8082"\n', '')
    text = text.replace('/run/octacity-secrets/', str(private) + '/')
    text = text.replace('/run/octacity/job-spec-policy.json', str(Path(configured["job_spec_policy"])))
    text = text.replace(stand.initializer.gateway_origin("objects.localhost"), environment["OCTACITY_MINIO_ENDPOINT"])
    config = offline / "server.toml"
    config.write_text(text)
    config.chmod(0o600)
    log = offline / "server.log"
    with log.open("wb") as output:
        log.chmod(0o600)
        process = subprocess.Popen([binary, "run", str(config)], stdout=output, stderr=subprocess.STDOUT,
            env={"PATH": os.defpath})
        client = bootstrap.BootstrapClient(stand.BootstrapTransport(port,
            max_response_bytes=bootstrap.MAX_MANAGEMENT_RESPONSE_BYTES),
            bootstrap.RetryPolicy(attempts=1, timeout_seconds=2, initial_delay_seconds=0))
        try:
            deadline = time.monotonic() + 25
            while True:
                try:
                    client.ready()
                    break
                except (OSError, bootstrap.BootstrapError):
                    if process.poll() is not None or time.monotonic() >= deadline:
                        raise AssertionError("qualification Server did not become ready: " + log.read_text()[-6000:])
                    time.sleep(0.1)
            yield client
        finally:
            if process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=15)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait(timeout=5)


@unittest.skipUnless(os.environ.get("OCTACITY_TEST_TRIAGE_SERVER") and os.environ.get("OCTACITY_TEST_TRIAGE_ENV"),
                     "requires current Server and task-owned PostgreSQL/S3 peers")
class FactoryTriageSetupRestTests(unittest.TestCase):
    def test_default_resources_runtime_roles_context_and_disabled_flow_use_ordinary_owners(self):
        binary = os.environ["OCTACITY_TEST_TRIAGE_SERVER"]
        environment = json.loads(Path(os.environ["OCTACITY_TEST_TRIAGE_ENV"]).read_text())
        database = "triage84_" + uuid.uuid4().hex
        user = urlsplit(environment["OCTACITY_POSTGRES_URL"]).username
        postgres = os.environ["OCTACITY_TEST_TRIAGE_POSTGRES"]
        psql = ["docker", "exec", postgres, "psql", "-U", user, "-d", "postgres", "-v", "ON_ERROR_STOP=1", "-c"]
        subprocess.run([*psql, f'CREATE DATABASE "{database}"'], check=True, capture_output=True)
        try:
            with tempfile.TemporaryDirectory(prefix="oc84-triage-rest-") as temporary:
                base = Path(temporary).resolve()
                root = base / "stand"
                stand.initializer.initialize(root)
                installation = base / "native"
                installation.mkdir(mode=0o700)
                policy = {"source": {"provider": "git", "plugin_version": "0.1.0", "plugin_sha256": "1" * 64,
                    "repository_parameter": "url"}, "octa": {"version": "0.8.0", "runner_sha256": "2" * 64,
                    "runner_protocol": 3, "event_schema": 4, "plugin_protocol": 2, "plugin_digests": {"codex": "a" * 64}}, "validity": 900}
                encoded = json.dumps(policy).encode()
                (installation / "job-spec-policy.json").write_bytes(encoded)
                (installation / "job-spec-policy.json").chmod(0o600)
                (installation / "installation-manifest.json").write_text(json.dumps({"schema_version": 1,
                    "files": {"job-spec-policy.json": hashlib.sha256(encoded).hexdigest()}}))
                (installation / "installation-manifest.json").chmod(0o600)
                namespace = uuid.uuid4()
                program = base / "codex"
                program.write_text("#!/bin/sh\necho codex-cli 0.161.0\n")
                program.chmod(0o700)
                executable = {"executable": program, "sha256": hashlib.sha256(program.read_bytes()).hexdigest(),
                    "policy": json.loads(runtime.POLICY.read_text()), "octa": policy["octa"]}
                home = base / "login"
                home.mkdir(mode=0o700)
                (home / "auth.json").write_text('{"auth_mode":"chatgpt","tokens":{"access_token":"finite-model-only","refresh_token":"finite-refresh-only"}}')
                (home / "auth.json").chmod(0o600)
                with server(root, installation, binary, environment, database) as client:
                    pool = client.post(bootstrap.POOL_PATH, str(namespace) + "-pool", bootstrap.LOCAL_POOL_REQUEST)["resource"]["id"]
                    resources = stand.prepare_triage_resources(client, pool, "https://github.com/OctaHive/OctaCity",
                        "registry.example/worker@sha256:" + "b" * 64, namespace)
                    with patch.dict(os.environ, {"CODEX_HOME": str(home)}):
                        assets = triage.prepare_runtime(root, executable, resources, client)
                configured_root = base / "configured"
                stand.initializer.stage_server_identity(root, configured_root)
                with server(configured_root, installation, binary, environment, database,
                        Path(assets["profiles"]), Path(assets["issue_mcp_readers"])) as client:
                    project = resources["project"]
                    settings = stand._read_json(Path(assets["runtime_settings"]))
                    client.post(f"/api/v1/projects/{project}/factory-runtimes/triage-runtime", str(namespace) + "-runtime",
                        {"expected_version": None, "definition": settings})
                    profiles = stand.publish_triage_roles(client, project, namespace,
                        runtime="triage-runtime", model_credential_profile="triage-model")
                    context = stand.publish_triage_context(client, project, namespace)
                    repository = client.get(f"/api/v1/repositories/{resources['repository']}/versions/1")
                    budget = json.loads(triage.POLICY.read_text())["flow_budget"]
                    selected = {"profiles": profiles, "context": {stage: context for stage in profiles},
                        "repository": {"project_id": project, "id": resources["repository"], "version": 1,
                            "definition_digest": repository["definition_digest"]},
                        "execution": {"budget": budget, "permissions": settings["permissions"], "max_active_nodes": 1},
                        "configuration": {"admission_policy": "triage-admission", "permission_ceiling": "triage-ceiling",
                            "wip_limits": {"max_active_runs": 1, "max_active_nodes": 1}, "hard_budget": budget}, "effects": {}}
                    preset = client.get(f"/api/v1/projects/{project}/factory-presets")["items"][0]["reference"]
                    reviewed = client.post(f"/api/v1/projects/{project}/factory-presets/resolve", str(namespace) + "-review",
                        {**selected, "preset": preset, "flow_id": str(uuid.uuid4()), "configuration_id": str(uuid.uuid4())})
                    self.assertEqual(reviewed["diagnostics"], [], "the installed two-stage defaults must resolve through the ordinary owner")
                    published = stand.publish_triage(client, project, selected, namespace)
                    self.assertEqual(published["configuration"]["version"], 1)
                    self.assertIsNone(published["input"])
                    self.assertEqual(client.get(f"/api/v1/projects/{project}/factory-runs?limit=10")["items"], [])
                    for stage in profiles:
                        retained = client.get(f"/api/v1/projects/{project}/factory-role-profiles/{profiles[stage]['profile']['id']}")
                        self.assertEqual(retained["definition"]["toolchain"]["model"]["identity"], "gpt-6.1-sol")
                    analyst = client.get(f"/api/v1/projects/{project}/factory-role-profiles/{profiles['classification']['profile']['id']}")
                    self.assertEqual(analyst["definition"]["execution"].get("mcp_servers", {}), {})
                    original = client.get(f"/api/v1/projects/{project}/input-documents/{context['goals']['artifact_id']}")
                    self.assertIn("Retain step states and results internally", original["content"])
                    edited = client.post(f"/api/v1/projects/{project}/input-documents", str(namespace) + "-edited-goals",
                        {"name": "OctaCity project goals", "content": "# Updated goals\nAn independently published operator revision."})
                    self.assertNotEqual(edited["artifact"]["artifact_id"], context["goals"]["artifact_id"])
                with server(configured_root, installation, binary, environment, database,
                        Path(assets["profiles"]), Path(assets["issue_mcp_readers"])) as client:
                    retained = client.get(f"/api/v1/projects/{project}/input-documents/{context['goals']['artifact_id']}")
                    self.assertEqual(retained["content"], original["content"])
                    changed = client.get(f"/api/v1/projects/{project}/input-documents/{edited['artifact']['artifact_id']}")
                    self.assertIn("An independently published operator revision", changed["content"])
                    configuration = client.get(f"/api/v1/factory-configurations/{published['configuration']['id']}/versions/1")
                    self.assertFalse(configuration["configuration"]["enabled"])
                    self.assertEqual(client.get(f"/api/v1/projects/{project}/factory-runs?limit=10")["items"], [])
        finally:
            subprocess.run([*psql, f'DROP DATABASE "{database}" WITH (FORCE)'], check=True, capture_output=True)


@unittest.skipUnless(os.environ.get("OCTACITY_TEST_TRIAGE_NATIVE"), "requires verified native Microsandbox installation")
class FactoryTriageGuestTests(unittest.TestCase):
    def test_native_image_contains_the_public_local_ca_and_reuses_its_verified_identity(self):
        native = Path(os.environ["OCTACITY_TEST_TRIAGE_NATIVE"])
        with tempfile.TemporaryDirectory(prefix="oc84-trusted-guest-") as temporary:
            root = Path(temporary).resolve()
            stand.initializer.initialize(root)
            tags = []

            def execute(arguments, **options):
                if "build" in arguments:
                    tags.append(arguments[arguments.index("--tag") + 1])
                return subprocess.run(arguments, **options)

            try:
                image = triage.prepare_guest_image(root, native, execute=execute)
                trusted = subprocess.run(["docker", "run", "--rm", "--network", "none", "--entrypoint", "trust",
                    tags[0], "list", "--filter=ca-anchors"], check=True, capture_output=True, text=True, timeout=30)
                self.assertIn("OctaCity Local Stand CA", trusted.stdout)
                self.assertEqual(triage.prepare_guest_image(root, native, execute=execute), image)
                self.assertEqual(len(tags), 1)
                self.assertTrue((root / "triage-guest/image.docker.tar").is_file(), "immutable image bytes remain available for recovery")
            finally:
                for tag in tags:
                    subprocess.run(["docker", "image", "rm", tag], check=False, capture_output=True, timeout=30)
                state = stand.integration.agent_config.microsandbox_state_root(root)
                if state.exists():
                    shutil.rmtree(state)
