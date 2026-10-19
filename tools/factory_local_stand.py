"""Prepare a hermetic Factory stand and publish declarative demonstration data."""
from __future__ import annotations

import copy
import argparse
from contextlib import contextmanager
import http.client
import json
import os
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path
import uuid

import bootstrap_local_stand as bootstrap
import configure_local_stand_server as server_config
import init_local_stand as initializer
import local_stand_launcher as launcher
import local_stand_state as state
import verify_local_stand_integration as integration
from factory_integrations import configure_integrations, install_repository


def _private_json(path: Path, value: dict) -> None:
    state.ensure_private_directory(path.parent)
    payload = (json.dumps(value, sort_keys=True, separators=(",", ":")) + "\n").encode()
    if not state.publish_exclusive_private_file(path, payload):
        if state.read_regular_file(path, "Factory fixture receipt", max_bytes=131072,
                                   modes=frozenset({0o600})) != payload:
            raise ValueError("existing Factory fixture receipt differs from the selected installation")


def _read_json(path: Path) -> dict:
    return state.load_json_object(state.read_regular_file(path, "Factory fixture receipt", max_bytes=131072,
                                  modes=frozenset({0o600})), "Factory fixture receipt")


def run_stand(root: Path, source: str, revision: str | None, *, github_repository: str | None = None) -> dict:
    """Start the standard stand with explicit Factory profiles and retain real demo Runs."""
    root = state.resolve_safe_root(root, launcher.REPOSITORY, Path.home())
    prepared_file = root / "evidence/factory-demo-resources.json"
    if (root / "config/server").exists() and not prepared_file.exists():
        raise ValueError("this stand has another server installation; use a clean disposable stand root")
    source = integration._validated_source_repository(source)
    initializer.initialize(root)
    fixture = json.loads((launcher.REPOSITORY / "deployment/local-stand/fixtures/factory-demo.json").read_text())
    context_file = root / "evidence/factory-demo-input.json"
    if context_file.exists():
        context = _read_json(context_file)
        if context["source"] != source or revision is not None and context["revision"] != revision:
            raise ValueError("the prepared Factory demo has another frozen source")
    else:
        if revision is None:
            revision = subprocess.run(["git", "ls-remote", source, "HEAD"], check=True,
                                      capture_output=True, text=True, timeout=30).stdout.split()[0]
        context = {"namespace": str(uuid.uuid4()), "source": source,
                   "revision": integration._validated_revision(revision)}
        _private_json(context_file, context)
    namespace = uuid.UUID(context["namespace"])
    print("Preparing verified native installation and stand images", flush=True)
    native = launcher._prepare_native_installation(root)
    launcher._build_images(native)
    if not prepared_file.exists():
        launcher._run_compose(root, 8443, ("up", "--detach", "--wait", "postgres", "minio"), timeout_seconds=180)
        launcher._run_compose(root, 8443, ("run", "--rm", "--no-deps", "minio-init"), timeout_seconds=60)
        launcher._run_compose(root, 8443, ("run", "--rm", "--no-deps", "--entrypoint", "/bin/true", "gateway"), timeout_seconds=30)
        print("Publishing ordinary prerequisite resources through the bootstrap API", flush=True)
        with bootstrap_server(root, native.path) as client:
            pool = client.post(bootstrap.POOL_PATH, bootstrap._stable_key(b"pool"), bootstrap.LOCAL_POOL_REQUEST)["resource"]["id"]
            resources = prepare_resources(client, pool, source, integration._guest_image(), str(namespace))
        _private_json(prepared_file, resources)
    resources = _read_json(prepared_file)
    profiles = root / "factory-demo/factory-profiles.json"
    _private_json(profiles, {"version": 1, "projects": [{"project_id": resources["project"],
        "references": fixture["references"], "build_configurations": []}], "codex": []})
    if github_repository is not None:
        # Install against the final profile before starting the worker: a later
        # server/gateway recreation would interrupt its outstanding lease poll.
        server_config.generate_server_configuration(root, native.path, factory_profiles=profiles)
        install_repository(root, github_repository)
    print("Starting the standard stand with Factory enabled", flush=True)
    launcher.run(launcher.parse_arguments(["up", "--root", str(root), "--factory-profiles", str(profiles)]))
    if github_repository is not None or (root / "config/integrations/server.toml").exists():
        probe = integration.LocalStandClient(root, bootstrap.DEFAULT_ORIGIN)
        for endpoint in ("/api/v1/integration-installations", "/api/v1/integrations"):
            probe.get(endpoint)
    receipt_file = root / "evidence/factory-demo.json"
    if receipt_file.exists():
        return _read_json(receipt_file)
    client = integration.LocalStandClient(root, bootstrap.DEFAULT_ORIGIN)
    published = publish_flows(client, resources["project"], fixture, namespace)
    artifacts = []
    for purpose in ("task", "acceptance"):
        print(f"Producing the retained {purpose} input in an ordinary Build", flush=True)
        build, _ = integration._trigger_build(client, trigger_id=resources["trigger"],
            configuration_id=resources["configuration"], source_revision=context["revision"],
            identity=f"{namespace}-{purpose}-input")
        integration._wait_for_successful_build(client, build, 600)
        artifact, digest, size = integration._verify_artifact(client, build)
        artifacts.append({"artifact_id": artifact, "content_digest": digest, "encoded_size": size})
    runs = {}
    for case in fixture["cases"]:
        admitted = client.post(f"/api/v1/projects/{resources['project']}/factory-work", f"{namespace}-{case['key']}", {
            "configuration_id": published[case["flow"]]["configuration"], "configuration_version": 1,
            "repository_id": resources["repository"], "repository_version": 1,
            "external_identity": f"demo/{namespace}/{case['key']}",
            "source": {"kind": "exact_revision", "value": context["revision"]},
            "artifacts": {"task": artifacts[0], "acceptance": artifacts[1], "specifications": []},
            "classification": {"priority": 40, "risk": case["risk"], "metadata": {"fixture": case["key"]}}})
        run = admitted["factory_run_id"]
        runs[case["key"]] = run
        _wait_for_demo(client, run, case["expect"])
    receipt = {"schema_version": 1, "origin": bootstrap.DEFAULT_ORIGIN, "project": resources["project"],
               "profiles": str(profiles), "source_revision": context["revision"], "flows": published, "runs": runs}
    _private_json(receipt_file, receipt)
    return receipt


def _wait_for_demo(client, run, expected):
    deadline = time.monotonic() + 90
    while time.monotonic() < deadline:
        detail = client.get(f"/api/v1/factory-runs/{run}")
        if expected == "completed" and detail["state"] == "completed":
            return
        if detail["state"] in {"failed", "cancelled", "completed"}:
            raise RuntimeError(f"Factory fixture stopped in {detail['state']} before its declared expectation")
        if expected == "human_gate":
            attempts = client.get(f"/api/v1/factory-runs/{run}/node-attempts?limit=100")
            if any(row["record"]["node_kind"] == "human_gate" for row in attempts["items"]):
                return
        time.sleep(0.5)
    raise RuntimeError("Factory fixture did not reach its declared expectation")


def prepare_resources(client, pool: str, source: str, image: str, identity: str) -> dict:
    """Create ordinary immutable producer resources before installing the Factory catalogue."""
    def create(path, suffix, body):
        return client.post(path, f"{identity}-{suffix}", body)["resource"]["id"]

    project = create("/api/v1/projects", "project", {"parent_id": None, "name": "Factory demonstration"})
    repository = create("/api/v1/repositories", "repository", {"project_id": project, "name": "demo-source",
        "definition": {"vcs_integration_id": str(uuid.uuid5(uuid.UUID(identity), "source")),
                       "repository_locator": source,
                       "selection": {"allowed_references": [], "default_reference": None, "allow_exact_revision": True}}})
    pipeline = create("/api/v1/pipelines", "producer-pipeline", integration._pipeline_request(project, "cacheable"))
    request = integration._configuration_request(project_id=project, repository_id=repository,
        pipeline_id=pipeline, pool_id=pool, guest_image=image, name="demo-input-producer", publishes_outputs=True)
    configuration = create("/api/v1/build-configurations", "producer-configuration", request)
    definition = request["definition"]
    replace = lambda value: {"mode": "replace", "value": value}
    client.post(f"/api/v1/projects/{project}/policy-versions", f"{identity}-policy", {"policy": {
        "pools": replace([pool]), "repositories": replace([repository]), "secret_profiles": replace([]),
        "identity_profiles": replace([]), "runtimes": replace([definition["runtime"]["class"]]),
        "execution_targets": replace([integration._execution_target()]),
        "cache": replace({"namespaces": [integration.CACHE_NAMESPACE], "read": True, "write": True,
                          "max_bytes": integration.MAX_DOWNLOAD_BYTES}),
        "artifacts": replace(definition["artifacts"]),
        "concurrency": replace({"active_builds": 1, "active_jobs": 1}),
        "retention": replace({"build_seconds": 86400, "log_seconds": 86400, "artifact_seconds": 86400, "cache_seconds": 86400})}})
    trigger = create("/api/v1/trigger-definitions/manual", "producer-trigger", {
        "configuration_id": configuration, "configuration_version": 1, "enabled": True, "definition": {}})
    return {"project": project, "repository": repository, "configuration": configuration, "trigger": trigger}


@contextmanager
def bootstrap_server(root: Path, native: Path):
    """Publish prerequisite resources without replacing an installed server bundle."""
    initializer.validate_initialized_state(root)
    with tempfile.TemporaryDirectory(prefix=".factory-bootstrap-", dir=root) as temporary:
        staging = Path(temporary)
        for name in ("secrets", "pki"):
            shutil.copytree(root / name, staging / name)
        receipt = server_config.generate_server_configuration(staging, native)
        configuration = Path(receipt["config"])
        mounts = []
        for source, destination in ((configuration, "/run/octacity/server.toml"),
                                    (Path(receipt["job_spec_policy"]), "/run/octacity/job-spec-policy.json")):
            source.chmod(0o444)
            mounts.extend(["--mount", f"type=bind,src={source},dst={destination},readonly"])
        secrets = {name: staging / "secrets" / name for name in initializer.CREDENTIAL_FILES.values()}
        secrets["postgres-url"] = Path(receipt["postgres_url"])
        secrets.pop("postgres-password")
        for name, source in secrets.items():
            mounts.extend(["--mount", f"type=bind,src={source},dst=/run/secrets/{name},readonly"])
        mounts.extend(["--mount", f"type=bind,src={staging / 'pki/local-ca.pem'},dst=/etc/ssl/certs/octacity-local-ca.pem,readonly"])
        started = subprocess.run([
            "docker", "run", "--detach", "--user", "0:0", "--network", "name=octacity-local_stand,alias=server",
            "--network", "octacity-local_edge",
            "--publish", "127.0.0.1::8080", "--read-only", "--tmpfs", "/run/octacity-secrets:mode=0700",
            "--security-opt", "no-new-privileges:true", "--cap-drop", "ALL",
            "--cap-add", "CHOWN", "--cap-add", "DAC_OVERRIDE", "--cap-add", "SETGID", "--cap-add", "SETUID",
            *mounts, "octacity/server:local", "run", "/run/octacity/server.toml"],
            check=True, capture_output=True, text=True, timeout=30)
        container = started.stdout.strip()
        gateway = None
        try:
            gateway_mounts = []
            for name in ("gateway.pem", "gateway-key.pem"):
                gateway_mounts.extend(["--mount", f"type=bind,src={staging / 'pki' / name},dst=/run/secrets/{name},readonly"])
            gateway = subprocess.run([
                "docker", "run", "--detach", "--network", "octacity-local_stand", "--network-alias", "objects.localhost",
                "--read-only", "--tmpfs", "/tmp:mode=1777", "--cap-drop", "ALL",
                "--security-opt", "no-new-privileges:true", *gateway_mounts, "octacity/gateway:local"],
                capture_output=True, text=True, check=True, timeout=30).stdout.strip()
            port = subprocess.run(["docker", "port", container, "8080/tcp"], check=True,
                                  capture_output=True, text=True, timeout=10).stdout.strip().rsplit(":", 1)[1]
            client = bootstrap.BootstrapClient(BootstrapTransport(int(port)), bootstrap.RetryPolicy())
            deadline = time.monotonic() + 45
            while True:
                try:
                    client.ready()
                    break
                except (OSError, bootstrap.BootstrapError):
                    if time.monotonic() >= deadline:
                        raise RuntimeError("Factory bootstrap server did not become ready") from None
                    time.sleep(0.25)
            yield client
        finally:
            log = subprocess.run(["docker", "logs", container], capture_output=True, timeout=10, check=False)
            log_path = root / "factory-bootstrap.log"
            with open(log_path, "wb") as stream:
                log_path.chmod(0o600)
                stream.write(log.stdout + log.stderr)
            subprocess.run(["docker", "stop", "--time", "10", container], capture_output=True, timeout=20, check=False)
            subprocess.run(["docker", "rm", "--force", container], capture_output=True, timeout=10, check=False)
            if gateway:
                subprocess.run(["docker", "rm", "--force", gateway], capture_output=True, timeout=10, check=False)


class BootstrapTransport:
    """Private bootstrap API, reachable only through a Docker-assigned loopback port."""

    def __init__(self, port: int):
        self.port = port

    def request(self, method, path, headers, body, timeout_seconds):
        connection = http.client.HTTPConnection("127.0.0.1", self.port, timeout=timeout_seconds)
        try:
            connection.request(method, path, body=body, headers=headers)
            response = connection.getresponse()
            payload = response.read(bootstrap.MAX_RESPONSE_BYTES + 1)
            if len(payload) > bootstrap.MAX_RESPONSE_BYTES:
                raise bootstrap.BootstrapError("bootstrap response exceeds its bound")
            return bootstrap.HttpResponse(response.status, dict(response.getheaders()), payload)
        finally:
            connection.close()


def _bind(value, bindings):
    if isinstance(value, dict):
        if set(value) == {"$fixture"}:
            selected = bindings
            for part in value["$fixture"].split("."):
                selected = selected[part]
            return copy.deepcopy(selected)
        return {key: _bind(item, bindings) for key, item in value.items()}
    if isinstance(value, list):
        return [_bind(item, bindings) for item in value]
    return value


def publish_flows(client, project: str, fixture: dict, namespace: uuid.UUID) -> dict:
    """Publish configured graphs, preserving server-owned schemas and closure digests."""
    bindings = {"schemas": {}, "flows": {}, "settings": fixture.get("settings", {})}
    for key, schema in fixture["schemas"].items():
        bindings["schemas"][key] = client.post(
            "/api/v1/flow-data-schemas/validate", f"{namespace}-schema-{key}", {"project_id": project, **schema})
    published = {}
    for flow in fixture["flows"]:
        flow_id = str(uuid.uuid5(namespace, "flow/" + flow["key"]))
        configuration_id = str(uuid.uuid5(namespace, "configuration/" + flow["key"]))
        resource = client.post("/api/v1/flow-definitions", f"{namespace}-flow-{flow['key']}", {
            "id": flow_id, "project_id": project, "definition": _bind(flow["definition"], bindings)})["resource"]
        bindings["flows"][flow["key"]] = resource["flow"]
        client.post("/api/v1/factory-configurations", f"{namespace}-configuration-{flow['key']}", {
            "id": configuration_id, "project_id": project,
            "definition": {**_bind(flow["configuration"], bindings), "flow": resource["flow"]}})
        published[flow["key"]] = {"flow": flow_id, "configuration": configuration_id}
    return published


def main(arguments=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path,
                        default=Path(os.environ.get("OCTACITY_LOCAL_STAND_ROOT", state.DEFAULT_ROOT)))
    parser.add_argument("--source-repository", default="https://github.com/OctaHive/OctaCity")
    parser.add_argument("--source-revision", help="published exact revision; defaults to the remote HEAD")
    parser.add_argument("--github-repository", help="explicit owner/repository to install as a ticket source; credentials are entered in Integrations")
    args = parser.parse_args(arguments)
    try:
        installed = args.root / "config/integrations/server.toml"
        if args.github_repository is None and not (installed.exists() or installed.is_symlink()):
            fixture = json.loads((launcher.REPOSITORY / "deployment/local-stand/fixtures/factory-demo.json").read_text())
            args.github_repository = fixture["github_repository"]
        receipt = run_stand(args.root, args.source_repository, args.source_revision, github_repository=args.github_repository)
    except (ValueError, OSError, RuntimeError, subprocess.SubprocessError) as error:
        print(f"Factory stand failed: {error}", file=sys.stderr)
        return 1
    print(json.dumps(receipt, sort_keys=True))
    return 0
