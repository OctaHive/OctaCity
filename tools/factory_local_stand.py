"""Install the reviewed two-stage triage pipeline without activating intake."""
from __future__ import annotations

import copy
import argparse
from contextlib import contextmanager
import http.client
import json
import os
import subprocess
import sys
import tempfile
import time
from pathlib import Path
import uuid

import bootstrap_local_stand as bootstrap
import configure_local_stand_server as server_config
import init_local_stand as initializer
import local_stand_installation_update as installation_update
import local_stand_launcher as launcher
import local_stand_state as state
import verify_local_stand_integration as integration
from factory_integrations import install_repository, read_github_token, ensure_github_connection, installed_github_resource
from factory_local_runtime import install_runtime_executable
import factory_triage_setup as triage


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


def _prepare_bootstrap_services(root: Path) -> None:
    launcher._run_compose(root, 8443, ("up", "--detach", "--wait", "postgres", "minio"), timeout_seconds=180)
    launcher._run_compose(root, 8443, ("run", "--rm", "--no-deps", "minio-init"), timeout_seconds=60)
    launcher._run_compose(root, 8443, ("run", "--rm", "--no-deps", "--entrypoint", "/bin/true", "gateway"), timeout_seconds=30)


def run_triage_stand(root: Path, source: str | None, revision: str | None, *,
                     github_repository: str | None = None, github_token_file: Path | None = None) -> dict:
    """Install the actual two-stage pipeline, leaving activation to ordinary review.

    Local files retain only installation receipts. Server remains the owner of
    roles, immutable context, workflow inputs, Runs, step state and results.
    """
    root = state.resolve_safe_root(root, launcher.REPOSITORY, Path.home())
    resources_file = root / "evidence/factory-triage-resources.json"
    assets_file = root / "evidence/factory-triage-assets.json"
    receipt_file = root / "evidence/factory-triage.json"
    if (root / "config/server").exists() and not resources_file.exists():
        raise ValueError("this stand has another server installation; use a clean disposable stand root")
    if github_token_file is None and not (root / "evidence/factory-github-connection.json").exists():
        raise ValueError("the first triage installation requires --github-token-file")
    if source is not None:
        source = integration._validated_source_repository(source)
    if revision is not None:
        revision = integration._validated_revision(revision)
    initializer.initialize(root)
    initializer.ensure_gateway_peer(root, "host.microsandbox.internal")
    context_file = root / "evidence/factory-triage-installation.json"
    if context_file.exists():
        installation = _read_json(context_file)
        if ((source is not None and installation["source"] != source)
                or (revision is not None and installation["revision"] != revision)):
            raise ValueError("the triage installation has another selected code source")
        source = integration._validated_source_repository(installation["source"])
    else:
        source = source or "https://github.com/OctaHive/OctaCity"
        installation = {"namespace": str(uuid.uuid4()), "source": source, "revision": revision}
        _private_json(context_file, installation)
    namespace = uuid.UUID(installation["namespace"])
    print("Preparing verified native installation and stand images", flush=True)
    native = launcher._prepare_native_installation(root)
    launcher._build_images(native)
    image = triage.prepare_guest_image(root, native.path)
    if receipt_file.exists():
        # Bootstrap receipts are immutable seeds, not the current installation.
        # Resume through the same owner used by explicit installation updates.
        repository = installed_github_resource(root)["resource"]["locator"]
        if github_repository is not None and github_repository != repository:
            raise ValueError("the triage installation has another selected GitHub repository")
        with tempfile.TemporaryDirectory(prefix=".factory-resume-", dir=root) as temporary:
            inputs = installation_update.installed_factory_inputs(root, Path(temporary))
            if inputs["factory_profiles"] is None or inputs["factory_agent_profile"] is None:
                raise ValueError("the retained triage installation is incomplete")
            arguments = ["up", "--root", str(root)]
            for name, path in inputs.items():
                if path is not None:
                    arguments.extend(("--" + name.replace("_", "-"), str(path)))
            launcher.run(launcher.parse_arguments(arguments))
        return _read_json(receipt_file)
    executable = install_runtime_executable(root, native.path)
    if not resources_file.exists() or not assets_file.exists():
        _prepare_bootstrap_services(root)
        with bootstrap_server(root, native.path) as client:
            if resources_file.exists():
                resources = _read_json(resources_file)
            else:
                pool = client.post(bootstrap.POOL_PATH, bootstrap._stable_key(b"pool"),
                    bootstrap.LOCAL_POOL_REQUEST)["resource"]["id"]
                resources = prepare_triage_resources(client, pool, source, image, namespace)
                _private_json(resources_file, resources)
            assets = triage.prepare_runtime(root, executable, resources, client)
            _private_json(assets_file, assets)
    resources, assets = _read_json(resources_file), _read_json(assets_file)
    server_config.generate_server_configuration(root, native.path, factory_profiles=Path(assets["profiles"]),
        factory_issue_mcp_readers=Path(assets["issue_mcp_readers"]))
    repository = github_repository or installed_github_resource(root)["resource"]["locator"]
    frozen_source = None if installation["revision"] is None else {"kind": "exact_revision", "value": installation["revision"]}
    install_repository(root, repository, preparation_mapping=triage.preparation_mapping(resources["project"]), preparation_source=frozen_source)
    print("Starting the standard stand with isolated triage agents", flush=True)
    launcher.run(launcher.parse_arguments(["up", "--root", str(root),
        "--factory-profiles", assets["profiles"], "--factory-agent-profile", assets["agent_profile"],
        "--factory-issue-mcp-readers", assets["issue_mcp_readers"]]))
    client = integration.LocalStandClient(root, bootstrap.DEFAULT_ORIGIN)
    connection = ensure_github_connection(client, root, resources["project"], installed_github_resource(root),
        github_token_file, namespace)
    settings = _read_json(Path(assets["runtime_settings"]))
    client.post(f"/api/v1/projects/{resources['project']}/factory-runtimes/triage-runtime",
        f"{namespace}-triage-runtime", {"expected_version": None, "definition": settings})
    profiles = publish_triage_roles(client, resources["project"], namespace,
        runtime="triage-runtime", model_credential_profile="triage-model")
    context = publish_triage_context(client, resources["project"], namespace)
    selection = triage.preset_selection(client, resources, profiles, context, connection, namespace, settings["permissions"])
    published = publish_triage(client, resources["project"], selection, namespace)
    receipt = {"schema_version": 1, "origin": bootstrap.DEFAULT_ORIGIN, "project": resources["project"],
        "profiles": profiles, "context": context, **published}
    _private_json(receipt_file, receipt)
    return receipt


def prepare_triage_resources(client, pool: str, source: str, image: str, namespace: uuid.UUID) -> dict:
    """Publish a dedicated ordinary worker; no Trigger, Build or Work is created."""
    policy = json.loads((launcher.REPOSITORY / "deployment/local-stand/factory-triage.json").read_text())
    limits = policy["resources"]

    def create(path, suffix, body):
        return client.post(path, f"{namespace}-triage-{suffix}", body)["resource"]["id"]

    project = create("/api/v1/projects", "project", {"parent_id": None, "name": policy["project_name"]})
    repository = create("/api/v1/repositories", "repository", {"project_id": project, "name": "triage-code",
        "definition": {"vcs_integration_id": str(uuid.uuid5(namespace, "source")), "repository_locator": source,
            "selection": {"allowed_references": [policy["source_reference"]],
                "default_reference": policy["source_reference"], "allow_exact_revision": True}}})
    pipeline = create("/api/v1/pipelines", "pipeline", {"project_id": project, "name": "Factory triage worker",
        "dag": {"nodes": [{"id": policy["pipeline_node"], "name": "Factory agent", "dependency_policy": "all_succeeded",
            "required_capabilities": ["shell"], "execution": {"octafile": "Octafile.yml", "commands": ["factory"],
                "parallel": False, "failfast": True}}], "edges": []}})
    request = integration._configuration_request(project_id=project, repository_id=repository,
        pipeline_id=pipeline, pool_id=pool, guest_image=image, name="Factory triage worker", publishes_outputs=True)
    definition = request["definition"]
    definition["cache"] = {"namespace": None, "read": False, "write": False}
    definition["artifacts"] = policy["outputs"]
    definition["agent_requirements"].update(minimum_cpu_millis=limits["cpu_millis"],
        minimum_memory_bytes=limits["memory_bytes"], minimum_disk_bytes=limits["disk_bytes"])
    definition["runtime"].update(cpu_millis=limits["cpu_millis"], memory_bytes=limits["memory_bytes"],
        writable_disk_bytes=limits["disk_bytes"], timeout_seconds=limits["elapsed_millis"] // 1000,
        network={"mode": "restricted", "allowed_hosts": policy["network_hosts"]})
    configuration = create("/api/v1/build-configurations", "configuration", request)
    replace = lambda value: {"mode": "replace", "value": value}
    client.post(f"/api/v1/projects/{project}/policy-versions", f"{namespace}-triage-policy", {"policy": {
        "pools": replace([pool]), "repositories": replace([repository]), "secret_profiles": replace([]),
        "identity_profiles": replace([]), "runtimes": replace([definition["runtime"]["class"]]),
        "execution_targets": replace([integration._execution_target()]),
        "cache": replace({"namespaces": [], "read": False, "write": False, "max_bytes": 0}),
        "artifacts": replace(definition["artifacts"]), "concurrency": replace({"active_builds": 1, "active_jobs": 1}),
        "retention": replace(policy["retention"])}})
    return {"project": project, "repository": repository, "configuration": configuration,
            "pipeline_node": policy["pipeline_node"]}


@contextmanager
def bootstrap_server(root: Path, native: Path):
    """Publish prerequisite resources without replacing an installed server bundle."""
    initializer.validate_initialized_state(root)
    with tempfile.TemporaryDirectory(prefix=".factory-bootstrap-", dir=root) as temporary:
        staging = Path(temporary)
        initializer.stage_server_identity(root, staging)
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

    def __init__(self, port: int, *, max_response_bytes: int = bootstrap.MAX_RESPONSE_BYTES):
        if type(max_response_bytes) is not int or not 1 <= max_response_bytes <= bootstrap.MAX_MANAGEMENT_RESPONSE_BYTES:
            raise bootstrap.BootstrapError("management response limit is invalid")
        self.port = port
        self.max_response_bytes = max_response_bytes

    def request(self, method, path, headers, body, timeout_seconds):
        connection = http.client.HTTPConnection("127.0.0.1", self.port, timeout=timeout_seconds)
        try:
            connection.request(method, path, body=body, headers=headers)
            response = connection.getresponse()
            payload = response.read(self.max_response_bytes + 1)
            if len(payload) > self.max_response_bytes:
                raise bootstrap.BootstrapError("bootstrap response exceeds its bound")
            return bootstrap.HttpResponse(response.status, dict(response.getheaders()), payload)
        finally:
            connection.close()


def publish_triage_context(client, project: str, namespace: uuid.UUID) -> dict:
    """Publish initial editable Project documents through their existing owner.

    These are immutable publication references, not mutable launcher-owned
    state. The Console publishes later revisions through the same contract.
    """
    directory = launcher.REPOSITORY / "deployment/local-stand"
    context = {"project_id": project, "documents": {}}
    for kind, name, filename in (
        ("goals", "OctaCity project goals", "project-goals.md"),
        ("rules", "OctaCity triage rules", "triage-rules.md"),
    ):
        published = client.post(f"/api/v1/projects/{project}/input-documents",
            f"{namespace}-triage-{kind}", {"name": name, "content": (directory / filename).read_text()})
        context[kind] = published["artifact"]
    return context


def publish_triage_roles(client, project: str, namespace: uuid.UUID, *,
                         runtime: str, model_credential_profile: str) -> dict:
    """Copy reviewed role content and qualify separate profiles with existing owners.

    Installed Runtime settings supply exact MCP pins. Publication and validation
    retain server-derived role/profile digests; the launcher does not hash them.
    """
    base = f"/api/v1/projects/{project}"
    ceiling = client.get(f"{base}/factory-profile-capabilities/{runtime}")["definition"]
    if model_credential_profile not in ceiling["permissions"]["secret_profiles"]:
        raise ValueError("triage Runtime lacks the selected model credential profile")
    templates = launcher.REPOSITORY / "config/factory/presets/triage"
    selections = {}
    for stage, filename in (("eligibility", "project-role-v1.json"),
                             ("classification", "analyst-role-v1.json")):
        seed = json.loads((templates / filename).read_text())
        role_id = str(uuid.uuid5(namespace, f"role/triage/{stage}"))
        profile_id = str(uuid.uuid5(namespace, f"profile/triage/{stage}"))
        role = client.post(f"{base}/factory-role-publications", f"{namespace}-triage-{stage}-role",
            {"role": {"id": role_id, "expected_current_version": None, "definition": seed["role"]},
             "profiles": []})["role"]["reference"]
        definition = copy.deepcopy(ceiling)
        definition.update(seed["profiles"]["default"])
        if stage == "classification":
            execution = definition["execution"]
            execution["mcp_servers"] = {}
            execution["integrations"] = {}
            execution["environment"] = {name: value for name, value in execution["environment"].items()
                                        if value["kind"] == "public"}
            definition["permissions"]["secret_profiles"] = [model_credential_profile]
        validated = client.post(f"{base}/factory-role-profiles/validate",
            f"{namespace}-triage-{stage}-review", {"id": profile_id, "role": role,
                "capability": runtime, "definition": definition})
        if validated["incompatibility"] is not None:
            raise ValueError("triage profile requires review against its installed Runtime")
        published = client.post(f"{base}/factory-role-publications",
            f"{namespace}-triage-{stage}-profile", {"profiles": [{"id": profile_id, "role": role,
                "expected_current_version": None, "definition": validated["profile"]["definition"]}]})
        selections[stage] = {"role": role, "profile": published["profiles"][0]["reference"]}
    return selections


def publish_triage(client, project: str, selection: dict, namespace: uuid.UUID) -> dict:
    """Publish reviewed preset drafts through ordinary management; do not start intake."""
    presets = client.get(f"/api/v1/projects/{project}/factory-presets")["items"]
    preset = next(item["reference"] for item in presets if item["reference"]["identity"] == "triage")
    flow_id = str(uuid.uuid5(namespace, "flow/triage"))
    configuration_id = str(uuid.uuid5(namespace, "configuration/triage"))
    resolved = client.post(f"/api/v1/projects/{project}/factory-presets/resolve",
        f"{namespace}-triage-review", {**copy.deepcopy(selection), "preset": preset,
        "flow_id": flow_id, "configuration_id": configuration_id})
    if resolved["diagnostics"] or resolved["flow"] is None or resolved["configuration"] is None:
        raise ValueError("triage prerequisites require explicit review before publication")
    for child in resolved["dependencies"]:
        client.post("/api/v1/flow-definitions", f"{namespace}-triage-child-{child['id']}",
            {"id": child["id"], "project_id": project, "definition": child["definition"]})
    flow = client.post("/api/v1/flow-definitions", f"{namespace}-triage-flow",
        {"id": flow_id, "project_id": project, "definition": resolved["flow"]})["resource"]["flow"]
    definition = {**resolved["configuration"], "flow": flow, "enabled": False}
    configuration = client.post("/api/v1/factory-configurations", f"{namespace}-triage-configuration",
        {"id": configuration_id, "project_id": project, "definition": definition})["resource"]["configuration"]
    installed_input = None
    if resolved["input"] is not None:
        source = copy.deepcopy(resolved["input"])
        source["definition"].update(target=configuration["reference"], enabled=False)
        published = client.post(f"/api/v1/projects/{project}/factory-source-inputs/{source['id']}/versions",
            f"{namespace}-triage-input", {"intent": {"project_id": project, "group": None, "inputs": [source]}},
            expected_version=0)
        retained = published["publication"]["inputs"][0]["input"]
        installed_input = {key: retained[key] for key in ("project_id", "id", "version")}
    return {"flow": flow["closure"]["root"], "configuration": configuration["reference"], "input": installed_input}


def main(arguments=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path,
                        default=Path(os.environ.get("OCTACITY_LOCAL_STAND_ROOT", state.DEFAULT_ROOT)))
    parser.add_argument("--source-repository", help="code repository; defaults to https://github.com/OctaHive/OctaCity")
    parser.add_argument("--source-revision", help="published exact revision; omission uses the reviewed default reference")
    parser.add_argument("--github-repository", help="explicit owner/repository to install as a ticket source")
    parser.add_argument("--github-token-file", type=Path,
                        help="explicit private PAT file for the selected GitHub integration")
    parser.add_argument("--factory-profiles", type=Path,
                        help="complete private operator catalog; uses ordinary setup without seeding Runs")
    parser.add_argument("--factory-runtime-bundle", type=Path, help="private installed AI Runtime directory")
    parser.add_argument("--factory-agent-profile", type=Path, help="independently approved private Agent policy")
    parser.add_argument("--factory-issue-mcp-readers", type=Path, help="explicit Project-scoped Issue MCP installation")
    parser.add_argument("--update-factory-installation", action="store_true", help="update an existing installation without resetting stand data")
    args = parser.parse_args(arguments)
    if args.github_token_file is not None:
        try:
            read_github_token(args.github_token_file)
        except ValueError as error:
            print(f"Factory stand failed: {error}", file=sys.stderr)
            return 1
    if args.update_factory_installation and (args.factory_profiles is None or args.factory_agent_profile is None):
        parser.error("--update-factory-installation requires --factory-profiles and --factory-agent-profile")
    if args.factory_profiles is None and (args.factory_runtime_bundle is not None or args.factory_agent_profile is not None or args.factory_issue_mcp_readers is not None):
        parser.error("explicit Factory runtime, Agent and MCP inputs require --factory-profiles")
    if args.factory_profiles is not None:
        if args.source_repository is not None or args.source_revision is not None:
            parser.error("default triage source options cannot be combined with --factory-profiles")
        target_project = None
        if args.github_token_file is not None:
            try:
                catalog = state.load_json_object(state.read_regular_file(args.factory_profiles,
                    "Factory profile catalog", max_bytes=server_config.MAX_FACTORY_PROFILE_BYTES,
                    modes=frozenset({0o600})), "Factory profile catalog")
                project, = catalog["projects"]
                target_project = uuid.UUID(project["project_id"])
            except (ValueError, KeyError, TypeError, OSError, state.StateError):
                print("Factory stand failed: GitHub enrollment requires exactly one explicit Project", file=sys.stderr)
                return 1
        # The explicit catalog owns its complete installation and authority.
        selected = ["up", "--root", str(args.root), "--factory-profiles", str(args.factory_profiles)]
        if args.update_factory_installation:
            selected.append("--update-factory-installation")
        for option in ("factory_runtime_bundle", "factory_agent_profile", "factory_issue_mcp_readers", "github_repository"):
            value = getattr(args, option)
            if value is not None:
                selected.extend(("--" + option.replace("_", "-"), str(value)))
        result = launcher.main(selected)
        if result == 0 and target_project is not None:
            try:
                ensure_github_connection(integration.LocalStandClient(args.root, bootstrap.DEFAULT_ORIGIN),
                    args.root, str(target_project), installed_github_resource(args.root),
                    args.github_token_file, target_project)
            except (ValueError, OSError, RuntimeError) as error:
                print(f"Factory stand failed: {error}", file=sys.stderr)
                return 1
        return result
    try:
        installed = args.root / "config/integrations/server.toml"
        if args.github_repository is None and not (installed.exists() or installed.is_symlink()):
            policy = json.loads(triage.POLICY.read_text())
            args.github_repository = policy["github_repository"]
        receipt = run_triage_stand(args.root, args.source_repository,
                            args.source_revision, github_repository=args.github_repository,
                            **({"github_token_file": args.github_token_file} if args.github_token_file is not None else {}))
    except (ValueError, OSError, RuntimeError, subprocess.SubprocessError) as error:
        print(f"Factory stand failed: {error}", file=sys.stderr)
        return 1
    print(json.dumps(receipt, sort_keys=True))
    return 0
