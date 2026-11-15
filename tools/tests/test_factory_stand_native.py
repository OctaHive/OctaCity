"""Opt-in stand qualification with explicitly packaged development inputs.

Artifact producers supply exact checked images/binaries. All runtime services,
management handlers, enrollment, private installers and Agent remain real.
The default-command case starts no inference or remote business mutation. The
separate UI case requires an explicitly selected repository and private inputs;
it executes real triage/Delivery and creates a draft PR for review.
"""
from contextlib import redirect_stdout
import base64
import hashlib
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import shutil
import unittest
import uuid
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import factory_local_stand as stand
import local_stand_state as state


@unittest.skipUnless(os.environ.get("OCTACITY_TEST_FACTORY_STAND_INPUT") or os.environ.get("OCTACITY_TEST_FACTORY_STAND_UI_INPUT"), "requires explicit packaged development inputs and private credentials")
class FactoryStandNativeTests(unittest.TestCase):
    @unittest.skipUnless(os.environ.get("OCTACITY_TEST_FACTORY_STAND_UI_INPUT"), "requires an explicitly selected disposable UI delivery target and credentials")
    def test_console_authors_triage_and_delivery_without_api_pipeline_preparation(self):
        inputs = json.loads(Path(os.environ["OCTACITY_TEST_FACTORY_STAND_UI_INPUT"]).read_text())
        project = "octacity-ui-" + uuid.uuid4().hex[:12]
        with patch.object(stand.launcher, "PROJECT_NAME", project), \
                patch.object(stand.launcher, "NAMED_VOLUMES", tuple(f"{project}_{name}" for name in ("postgres-data", "minio-data"))):
            self._installed_stand(inputs, ui_authoring=True)

    @unittest.skipUnless(os.environ.get("OCTACITY_TEST_FACTORY_STAND_INPUT"), "requires explicit packaged development inputs and private credentials")
    def test_default_command_installs_disabled_triage_and_restarts_without_republishing_operator_state(self):
        inputs = json.loads(Path(os.environ["OCTACITY_TEST_FACTORY_STAND_INPUT"]).read_text())
        self._installed_stand(inputs)

    def _installed_stand(self, inputs, *, ui_authoring=False):
        project_filter = "label=com.docker.compose.project=" + stand.launcher.PROJECT_NAME
        for kind in ("ps", "volume"):
            arguments = ["docker", "ps", "--all", "--quiet", "--filter", project_filter] if kind == "ps" else [
                "docker", "volume", "ls", "--quiet", "--filter", project_filter]
            if subprocess.check_output(arguments).strip():
                self.skipTest("the shared local-stand Compose project already owns services or volumes")
        native = Path(inputs["native_installation"])
        document = stand.launcher.load_build_inputs(stand.launcher.MANIFEST, stand.launcher.REPOSITORY)
        stand.launcher.native_stager.verify_agent_bundle(native / "agent", document, inputs["source_tree"])
        previous = {}
        for key, target in (("server_image", stand.launcher.SERVER_IMAGE), ("gateway_image", stand.launcher.GATEWAY_IMAGE)):
            inspected = subprocess.run(["docker", "image", "inspect", target, "--format", "{{.Id}}"], capture_output=True, text=True)
            previous[target] = inspected.stdout.strip() if inspected.returncode == 0 else None
            subprocess.run(["docker", "tag", inputs[key], target], check=True)
        # macOS Unix-domain lifecycle sockets require a short resolved root.
        with tempfile.TemporaryDirectory(prefix="oc84-stand-", dir="/tmp") as temporary:
            root = Path(temporary).resolve() / "stand"
            if state.DEFAULT_ROOT.exists():
                stand.initializer.stage_server_identity(state.DEFAULT_ROOT, root)
            installed = stand.launcher.NativeInstallation(native, document)
            try:
                with patch.object(stand.launcher, "_prepare_native_installation", return_value=installed), \
                        patch.object(stand.launcher, "git_revision", return_value=inputs["source_tree"]):
                    output = io.StringIO()
                    with redirect_stdout(output):
                        arguments = ["--root", str(root), "--github-token-file", inputs["github_token_file"]]
                        if ui_authoring:
                            arguments += ["--no-example-pipeline", "--source-repository", inputs["source_repository"],
                                "--source-revision", inputs["source_revision"], "--delivery-write-directory", inputs["delivery_write_directory"],
                                "--delivery-target-branch", inputs["delivery_target_branch"], "--delivery-token-file", inputs["delivery_token_file"]]
                        self.assertEqual(stand.main(arguments), 0)
                    receipt = stand._read_json(root / "evidence/factory-triage.json")
                    self.assertNotIn("runs", receipt)
                    self.assertEqual(set(receipt["profiles"]), {"eligibility", "classification"})
                    self.assertLess((root / "evidence/factory-triage.json").stat().st_size, 4096)
                    client = stand.integration.LocalStandClient(root, receipt["origin"])
                    project = receipt["project"]
                    if ui_authoring:
                        self.assertIsNone(receipt["configuration"])
                        self.assertIsNone(receipt["flow"])
                        self.assertIsNone(receipt["input"])
                        for collection in ("flow-definitions", "factory-configurations", "factory-runs"):
                            self.assertEqual(client.get(f"/api/v1/projects/{project}/{collection}?limit=100")["items"], [])
                        public_key = subprocess.run(["openssl", "x509", "-in", str(root / "pki/gateway.pem"),
                            "-pubkey", "-noout"], capture_output=True, check=True).stdout
                        der = subprocess.run(["openssl", "pkey", "-pubin", "-outform", "DER"], input=public_key,
                            capture_output=True, check=True).stdout
                        subprocess.run(["pnpm", "exec", "playwright", "test", "--config", "playwright.released.config.ts",
                            "issue-delivery-authoring.spec.ts"], cwd=stand.launcher.REPOSITORY / "ui", check=True,
                            env={**os.environ, "OCTACITY_RELEASE_CONSOLE_ORIGIN": receipt["origin"],
                                "OCTACITY_RELEASE_PROJECT_ID": project, "OCTACITY_RELEASE_CODE_REVISION": inputs["source_revision"],
                                "OCTACITY_RELEASE_SOURCE_REPOSITORY": inputs["source_repository"],
                                "OCTACITY_RELEASE_DELIVERY_DIRECTORY": inputs["delivery_write_directory"],
                                "OCTACITY_RELEASE_DELIVERY_FILE": inputs["delivery_file"],
                                "OCTACITY_RELEASE_TLS_SPKI": base64.b64encode(hashlib.sha256(der).digest()).decode(),
                                "NODE_EXTRA_CA_CERTS": str(root / "pki/local-ca.pem")}, timeout=960)
                        return
                    configuration = client.get(f"/api/v1/factory-configurations/{receipt['configuration']['id']}/versions/1")
                    self.assertFalse(configuration["configuration"]["enabled"])
                    record = client.get(f"/api/v1/projects/{project}/factory-source-inputs/{receipt['input']['id']}/versions/1")
                    self.assertEqual(record["kind"], "input")
                    self.assertFalse(record["definition"]["definition"]["enabled"])
                    self.assertEqual(record["definition"]["definition"]["predicate"]["label_presence"], "none")
                    self.assertEqual(client.get(f"/api/v1/projects/{project}/factory-runs?limit=10")["items"], [])
                    goals = client.get(f"/api/v1/projects/{project}/input-documents/{receipt['context']['goals']['artifact_id']}")
                    self.assertIn("Retain step states and results internally", goals["content"])
                    public_key = subprocess.run(["openssl", "x509", "-in", str(root / "pki/gateway.pem"),
                        "-pubkey", "-noout"], capture_output=True, check=True).stdout
                    der = subprocess.run(["openssl", "pkey", "-pubin", "-outform", "DER"], input=public_key,
                        capture_output=True, check=True).stdout
                    subprocess.run(["pnpm", "exec", "playwright", "test", "--config", "playwright.released.config.ts",
                        "factory-stand-documents.spec.ts"], cwd=stand.launcher.REPOSITORY / "ui", check=True,
                        env={**os.environ, "OCTACITY_RELEASE_CONSOLE_ORIGIN": receipt["origin"],
                            "OCTACITY_RELEASE_PROJECT_ID": project,
                            "OCTACITY_RELEASE_TLS_SPKI": base64.b64encode(hashlib.sha256(der).digest()).decode(),
                            "NODE_EXTRA_CA_CERTS": str(root / "pki/local-ca.pem")}, timeout=180)
                    edited = client.post(f"/api/v1/projects/{project}/input-documents", "native-operator-goals-edit",
                        {"name": "OctaCity project goals", "content": "# Operator goals\nAn explicit later revision."})
                    connection = stand._read_json(root / "evidence/factory-github-connection.json")
                    connection_path = f"/api/v1/integrations/{connection['id']}/state"
                    administration = client.get(connection_path)
                    self.assertTrue(administration["enabled"])
                    client.post(connection_path, "native-operator-disable-connection", {"intent": {
                        "id": connection["id"], "expected_metadata_version": connection["version"],
                        "expected_revision": administration["revision"], "action": {"operation": "disable"}}},
                        expected_version=administration["revision"])
                    # Reconnect advances the registration ID through the normal
                    # protocol. Stable Agent identity and credential secret,
                    # rather than that session-specific token text, must survive.
                    credential_digest = hashlib.sha256((root / "agent/credential").read_text().strip().split(".")[-1].encode()).digest()
                    agent_ids = [item["id"] for item in client.get("/api/v1/agents")["items"]]
                    self.assertEqual(len(agent_ids), 1)
                    private_files = [root / "secrets/factory-model-login/auth.json", root / "secrets/triage-issue-mcp-token"]
                    private_digests = [hashlib.sha256(path.read_bytes()).digest() for path in private_files]
                    stand.launcher.run(stand.launcher.parse_arguments(["down", "--root", str(root)]))
                    with redirect_stdout(io.StringIO()):
                        self.assertEqual(stand.main(["--root", str(root)]), 0)
                    self.assertEqual(stand._read_json(root / "evidence/factory-triage.json"), receipt)
                    self.assertEqual(hashlib.sha256((root / "agent/credential").read_text().strip().split(".")[-1].encode()).digest(), credential_digest)
                    self.assertEqual([item["id"] for item in client.get("/api/v1/agents")["items"]], agent_ids)
                    self.assertEqual([hashlib.sha256(path.read_bytes()).digest() for path in private_files], private_digests)
                    self.assertFalse(client.get(connection_path)["enabled"], "restart must preserve operator revocation")
                    self.assertEqual(client.get(f"/api/v1/projects/{project}/input-documents/{receipt['context']['goals']['artifact_id']}")["content"], goals["content"])
                    self.assertEqual(client.get(f"/api/v1/projects/{project}/input-documents/{edited['artifact']['artifact_id']}")["content"], "# Operator goals\nAn explicit later revision.")
                    self.assertEqual(client.get(f"/api/v1/projects/{project}/factory-runs?limit=10")["items"], [])
            finally:
                original_error = sys.exception()
                cleanup_errors = []
                try:
                    if ui_authoring and inputs.get("evidence_directory"):
                        evidence = Path(inputs["evidence_directory"])
                        evidence.mkdir(mode=0o700, parents=True, exist_ok=True)
                        for relative in ["factory-bootstrap.log", "logs/agent.log", "logs/validation-agent.log", "logs/microsandbox-preflight.log",
                                         "evidence/factory-triage.json", "evidence/factory-triage-resources.json"]:
                            source = root / relative
                            if source.is_file():
                                target = evidence / relative.replace("/", "-")
                                shutil.copyfile(source, target)
                                target.chmod(0o600)
                        for service in ("server", "gateway"):
                            result = subprocess.run(stand.launcher._compose_command(root, "logs", "--no-color", "--tail", "1000", service),
                                env=stand.launcher._compose_environment(root, 8443), capture_output=True, timeout=30)
                            target = evidence / (service + ".log")
                            target.write_bytes(result.stdout[-1024*1024:] + result.stderr[-65536:])
                            target.chmod(0o600)
                except Exception as error:
                    cleanup_errors.append(error)
                # The precondition above proves this invocation created all shared
                # Compose volumes. Reset only this task-owned disposable root.
                try:
                    stand.launcher.run(stand.launcher.parse_arguments(["reset", "--root", str(root),
                        "--confirm", stand.launcher.RESET_CONFIRMATION]))
                except Exception as error:
                    cleanup_errors.append(error)
                for target, identity in previous.items():
                    try:
                        if identity:
                            subprocess.run(["docker", "tag", identity, target], check=True)
                        else:
                            subprocess.run(["docker", "image", "rm", target], check=True, capture_output=True)
                    except Exception as error:
                        cleanup_errors.append(error)
                if cleanup_errors:
                    if original_error is None:
                        raise ExceptionGroup("Native stand evidence/cleanup failed", cleanup_errors)
                    # Cleanup failures supplement the original qualification failure.
                    for error in cleanup_errors:
                        original_error.add_note(f"Native stand evidence/cleanup: {type(error).__name__}: {error}")
