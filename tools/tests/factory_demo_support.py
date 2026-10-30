"""Legacy automatic/human-gate examples used only by regression qualification.

The production factory-stand command installs triage and never seeds these Runs.
"""
from __future__ import annotations
import copy
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
import time
import uuid
import factory_local_stand as stand
from factory_local_stand import (_private_json, _read_json, _prepare_bootstrap_services, bootstrap_server,
    install_repository, installed_github_resource, ensure_github_connection)
from factory_local_runtime import install_runtime
state, bootstrap = stand.state, stand.bootstrap
launcher, initializer, server_config, integration = stand.launcher, stand.initializer, stand.server_config, stand.integration

def publish_runtime_profiles(root: Path, native: Path, path: Path, document: dict,
                             bundle: Path, *, previous: dict) -> None:
    """Upgrade recognized generated metadata only; PostgreSQL and operator state are untouched."""
    if not path.exists() or _read_json(path) == document:
        _private_json(path, document)
        return
    before_support = copy.deepcopy(document)
    for project in before_support.get("projects", []):
        for declaration in project.get("role_profiles", {}).values():
            declaration.pop("settings_support", None)
    if _read_json(path) not in (previous, before_support):
        raise ValueError("Factory profiles were customized; refusing to replace them")
    # Prove ownership/content before preparing or stopping anything.
    destination = root / "config/server"
    if destination.exists():
        server_config.generate_server_configuration(root, native, factory_profiles=path)
    with tempfile.TemporaryDirectory(prefix=".runtime-upgrade-", dir=root) as temporary:
        staging = Path(temporary)
        initializer.stage_server_identity(root, staging)
        reviewed = staging / "profiles.json"
        _private_json(reviewed, document)
        server_config.generate_server_configuration(staging, native, factory_profiles=reviewed,
                                                   factory_runtime_bundle=bundle)
        backup = staging / "previous-server"
        if destination.exists():
            launcher.run(launcher.parse_arguments(["down", "--root", str(root)]))
            destination.rename(backup)
        try:
            (staging / "config/server").rename(destination)
            reviewed.replace(path)
        except BaseException:
            if destination.exists():
                destination.rename(staging / "uncommitted-server")
            if backup.exists():
                backup.rename(destination)
            raise


def run_stand(root: Path, source: str, revision: str | None, *, github_repository: str | None = None, github_token_file: Path | None = None) -> dict:
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
    runtime = None
    if not prepared_file.exists():
        _prepare_bootstrap_services(root)
        print("Publishing ordinary prerequisite resources through the bootstrap API", flush=True)
        with bootstrap_server(root, native.path) as client:
            pool = client.post(bootstrap.POOL_PATH, bootstrap._stable_key(b"pool"), bootstrap.LOCAL_POOL_REQUEST)["resource"]["id"]
            resources = prepare_resources(client, pool, source, integration._guest_image(), str(namespace))
            runtime = install_runtime(root, native.path, resources, client)
        _private_json(prepared_file, resources)
    resources = _read_json(prepared_file)
    if runtime is None:
        if (root / "factory-runtime/project.json").exists():
            runtime = install_runtime(root, native.path, resources, None)
        else:
            _prepare_bootstrap_services(root)
            with bootstrap_server(root, native.path) as client:
                runtime = install_runtime(root, native.path, resources, client)
    profiles = root / "factory-demo/factory-profiles.json"
    project = {"project_id":resources["project"], **runtime["project"]}
    project["references"] = [*fixture["references"], *project["references"]]
    previous = {"version":1,"projects":[{"project_id":resources["project"],
        "references":fixture["references"],"build_configurations":[]}],"codex":[]}
    publish_runtime_profiles(root, native.path, profiles, {"version":1,"projects":[project],"codex":[]},
                             runtime["bundle"], previous=previous)
    if github_repository is not None:
        # Install against the final profile before starting the worker: a later
        # server/gateway recreation would interrupt its outstanding lease poll.
        server_config.generate_server_configuration(root, native.path, factory_profiles=profiles, factory_runtime_bundle=runtime["bundle"])
        install_repository(root, github_repository)
    print("Starting the standard stand with Factory enabled", flush=True)
    launcher.run(launcher.parse_arguments(["up", "--root", str(root), "--factory-profiles", str(profiles), "--factory-runtime-bundle", str(runtime["bundle"])]))
    if github_repository is not None or (root / "config/integrations/server.toml").exists():
        probe = integration.LocalStandClient(root, bootstrap.DEFAULT_ORIGIN)
        for endpoint in ("/api/v1/integration-installations", "/api/v1/integrations"):
            probe.get(endpoint)
    if github_token_file is not None:
        ensure_github_connection(integration.LocalStandClient(root, bootstrap.DEFAULT_ORIGIN), root,
            resources["project"], installed_github_resource(root), github_token_file, namespace)
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
