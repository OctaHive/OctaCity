"""Compose Delivery capabilities for the existing personal Factory stand."""
from __future__ import annotations

import base64
import copy
from dataclasses import dataclass
import hashlib
import json
from pathlib import Path
import re
import shlex
from urllib.parse import urlsplit

import local_stand_state as state

import factory_triage_setup as triage

REPOSITORY = Path(__file__).resolve().parents[1]
MODEL_CREDENTIAL = "delivery-model"
CAPTURE_OUTPUT = "candidate_facts"


def _json(value) -> bytes:
    return json.dumps(value, ensure_ascii=False, separators=(",", ":")).encode()


def _digest(domain: str, value) -> str:
    digest = hashlib.sha256()
    for field in (domain.encode(), _json(value)):
        digest.update(len(field).to_bytes(8, "big"))
        digest.update(field)
    return digest.hexdigest()


def _schema_pin(identity: str, version: str, shape: dict) -> dict:
    """Match the released FlowDataSchema wire and its length-framed digest."""
    return triage._reference(identity, version,
        _digest("octacity.factory.flow-data-schema.v1", [identity, version, shape]))


SCHEMAS = REPOSITORY / "config/factory/flows/base-flow/schemas"


def _schema(name: str, path: Path | None = None, *, include_shape: bool = False) -> dict:
    definitions = json.loads((path or (SCHEMAS / f"{name}.json")).read_text())["schemas"]
    def expand(shape):
        match shape["type"]:
            case "reference":
                reference = shape["schema"]
                definition = definitions.get(reference)
                if definition is None:
                    definition = json.loads((SCHEMAS / f"{reference}.json").read_text())["schemas"][reference]
                return expand(definition["shape"])
            case "object":
                return {"type": "object", "fields": {key: {"required": field["required"],
                    "value": expand(field["value"])} for key, field in sorted(shape["fields"].items())}}
            case "array":
                return {"type": "array", "item": expand(shape["item"]),
                    "min_items": shape["min_items"], "max_items": shape["max_items"]}
            case _:
                return shape

    definition = definitions[name]
    shape = expand(definition["shape"])
    reference = _schema_pin(definition["identity"], definition["version"], shape)
    return {"reference": reference, "shape": shape} if include_shape else reference


def _add(entries: list, key, value: dict) -> None:
    matches = [entry for entry in entries if key(entry) == key(value)]
    if not matches:
        entries.append(value)
    elif matches != [value]:
        raise ValueError("Delivery conflicts with an existing installed capability")


def delivery_models(catalog: dict, project_id: str, capture_tool: dict, write_directories: list[str], *,
                    code_build: dict) -> dict:
    """Extend the effective catalog with three model compilers and Runtime choices."""
    catalog = copy.deepcopy(catalog)
    project, = (entry for entry in catalog["projects"] if entry["project_id"] == project_id)
    runtimes = project["role_profiles"]
    original = runtimes["triage-runtime"]
    ceiling = original["ceiling"]
    observer, = (entry for entry in catalog["codex"] if entry["toolchain"] == ceiling["toolchain"]
                 and entry["build_configuration"] == ceiling["build_configuration"]
                 and entry["credential_purpose"] == "evaluator")
    permissions = observer["project_permissions"]
    model_credential = observer["credentials"]["evaluator"]
    mcp_hosts = {urlsplit(entry["server"]["url"]).hostname
                 for entry in original.get("settings_support", {}).get("authenticated_mcp", {}).values()}
    hosts = [host for host in permissions["network_hosts"] if host not in mcp_hosts]
    for role, purpose, source_mode in (
        ("coder", "model", "read_write"),
        ("reviewer", "evaluator", "read_only"),
        ("devops", "evaluator", None),
    ):
        alias = f"delivery-{role}"
        grants = copy.deepcopy(permissions)
        grants["secret_profiles"] = [MODEL_CREDENTIAL if role == "coder" else model_credential]
        grants["network_hosts"] = hosts
        if source_mode:
            grants["mounts"].append({"root": "/workspace/source", "mode": source_mode})
        # The reference version distinguishes compilers for the same provider model.
        toolchain = {**observer["toolchain"], "model": {**observer["toolchain"]["model"], "version": alias + "-v1"}}
        entry = {**copy.deepcopy(observer), "toolchain": toolchain, "credential_purpose": purpose,
            "credentials": {**observer["credentials"], "model": MODEL_CREDENTIAL},
            "project_permissions": grants, "local_permissions": {**observer["local_permissions"], "grants": grants}}
        entry.pop("mcp_configurations", None)
        if source_mode:
            entry.update(build_configuration=code_build, candidate_capture_tool=capture_tool)
        if role == "coder":
            schema = _schema("candidate_capture_facts")
            grants["outputs"]["kinds"].extend(["change-set.bundle", "change-set-manifest.json", CAPTURE_OUTPUT])
            entry["capture"] = {"tool": capture_tool, "instruction": {
                "author_name": "OctaCity Factory", "author_email": "factory@octacity.local",
                "committed_at": 1, "policy": {
                    "allowed_path_prefixes": sorted(set(write_directories)),
                    "forbidden_control_paths": [".github", ".octa", "Octafile.yml"],
                    "max_changed_paths": 32, "max_file_bytes": 1024 * 1024, "max_total_bytes": 4 * 1024 * 1024,
                    "allow_empty": False, "binary_policy": "reject",
                    "forbidden_secret_patterns": ["pem_private_key", "aws_access_key_id", "git_hub_token", "open_ai_api_key"]},
                "observation": {"name": CAPTURE_OUTPUT, "schema": {
                    **{key: value for key, value in schema.items() if key != "digest"},
                    "sha256": schema["digest"]}}}}
        _add(catalog["codex"], lambda item: item["toolchain"]["model"], entry)
        runtime = {key: copy.deepcopy(value) for key, value in original.items()
                   if key not in {"model_discovery", "settings_support"}}
        runtime["ceiling"].update(name=f"Delivery {role.title()}", toolchain=toolchain, credential_purpose=purpose,
            build_configuration=entry["build_configuration"], permissions=grants, context_requirements=["task"],
            execution={**ceiling["execution"], "environment": {}, "integrations": {}, "mcp_servers": {}})
        if alias in runtimes and runtimes[alias] != runtime:
            raise ValueError("Delivery conflicts with an existing installed Runtime")
        runtimes[alias] = runtime
        _choice(project, alias, toolchain["executable"])
        _choice(project, alias + "-model", toolchain["model"])
    _add(project["references"], lambda item: (item["kind"], item["alias"]),
        {"kind": "secret_profile", "alias": MODEL_CREDENTIAL, "reference": triage._reference(MODEL_CREDENTIAL)})
    return catalog


@dataclass(frozen=True)
class TestPack:
    name: str
    task_sha256: str
    acceptance_sha256: str
    program: bytes

    @property
    def digest(self) -> str:
        return hashlib.sha256(_json([self.name, self.task_sha256, self.acceptance_sha256,
                                    hashlib.sha256(self.program).hexdigest()])).hexdigest()


def read_test_pack(path: Path, *, client=None, project: str | None = None) -> TestPack:
    """Read operator-reviewed inputs once and bind the exact task, acceptance and suite."""
    try:
        data = state.load_json_object(state.read_regular_file(path, "Delivery test pack", max_bytes=4096,
            modes=frozenset({0o600})), "Delivery test pack")
        if (set(data) not in ({"name", "task_sha256", "acceptance_sha256", "script"}, {"name", "issue_id", "script"})
                or not re.fullmatch(r"[a-z][a-z0-9-]{0,31}", data["name"])
                or not re.fullmatch(r"[a-zA-Z0-9][a-zA-Z0-9_.-]{0,95}\.sh", data["script"])):
            raise ValueError("invalid pack declaration")
        program = state.read_regular_file(path.parent / data["script"], "reviewed Delivery checks",
            max_bytes=16384, modes=frozenset({0o600, 0o700}))
        if not program.strip() or b"\x00" in program:
            raise ValueError("invalid check program")
        if "issue_id" in data:
            import uuid
            if client is None or project is None:
                raise ValueError("Issue lookup requires the retained stand")
            issue_id = str(uuid.UUID(data["issue_id"]))
            issue = client.get(f"/api/v1/projects/{project}/issues/{issue_id}")
            content = issue["content"]
            if not issue["content_available"] or not isinstance(content["acceptance"], str) or not content["acceptance"].strip():
                raise ValueError("Issue must have available acceptance criteria")
            task = f"{content['title']}\n\n{content['description']}".encode()
            return TestPack(data["name"], hashlib.sha256(task).hexdigest(),
                            hashlib.sha256(content["acceptance"].encode()).hexdigest(), program)
        if any(not re.fullmatch(r"[a-f0-9]{64}", data[key]) for key in ("task_sha256", "acceptance_sha256")):
            raise ValueError("invalid pack digests")
        return TestPack(data["name"], data["task_sha256"], data["acceptance_sha256"], program)
    except (OSError, ValueError, KeyError, TypeError, state.StateError):
        raise ValueError("Delivery test pack is invalid") from None


def _object(fields: dict, *, optional: tuple[str, ...] = ()) -> dict:
    return {"type": "object", "fields": {key: {"required": key not in optional, "value": value}
        for key, value in sorted(fields.items())}}


def _text(minimum: int, maximum: int) -> dict:
    return {"type": "string", "min_bytes": minimum, "max_bytes": maximum}


def _enum(*values) -> dict:
    return {"type": "enum", "values": list(values)}


def _observation(pack: TestPack, worker: dict, *, protected: bool) -> tuple[dict, dict]:
    shape = _object({"failed": {"type": "boolean"}})
    identity, version = "delivery-test-observation", "v1"
    contract = {"environment": triage._reference("delivery-guest", digest=worker["build_configuration"].get("definition_digest")),
        "criterion": triage._reference("delivery-tests-" + pack.name, digest=pack.digest),
        "observations": {"reference": _schema_pin(identity, version, shape), "shape": shape},
        "completion": {"operation": "exists", "operand": {"source": "path", "path": "/failed"}},
        "symptom": {"operation": "compare", "left": {"source": "path", "path": "/failed"},
                    "comparison": "equal", "right": {"source": "literal", "value": True}},
        "requires_protected_tests": protected}
    reference = _object({"identity": _text(1, 128), "version": _text(1, 128), "digest": _text(64, 64)})
    fields = {
        "command": reference, "criterion": reference, "environment": reference, "input_digest": _text(64, 64),
        "finding": _enum("matched", "not_matched", "indeterminate"), "observations": shape,
        "status": _enum("completed", "environment_unavailable", "needs_input"),
        "subject": _object({"project_id": _text(36, 36), "repository_id": _text(36, 36), "base_revision": _text(1, 512)}),
    }
    if protected:
        fields["test_classes"] = {"type": "array", "item": _object({"class": _text(1, 128),
            "outcome": _enum("passed", "failed", "indeterminate")}), "min_items": 1, "max_items": 64}
    report = _object(fields, optional=("observations", "test_classes"))
    return contract, _schema_pin("factory.command-observation", "v1", report)


def _choice(project: dict, alias: str, reference: dict, *, node_kind: str | None = None,
            command_output: dict | None = None) -> None:
    item = {"kind": "node_capability", "alias": alias, "reference": reference}
    if node_kind is not None:
        item["metadata"] = {"node_kinds": [node_kind], "signal_purposes": [], "enforcement": [], "compatible_choices": []}
        if command_output is not None:
            item["metadata"]["command_output"] = command_output
    _add(project["references"], lambda row: (row["kind"], row["alias"]), item)


def install_test_commands(catalog: dict, project: dict, worker: dict, plugins: list[dict], capture: dict,
                          pack: TestPack, *, author: dict) -> None:
    """Bind one reviewed suite to an exact author, baseline and candidate verifier."""
    shell, = (plugin for plugin in plugins if plugin["identity"] == "shell")
    baseline, baseline_schema = _observation(pack, worker, protected=True)
    reproduction, reproduction_schema = _observation(pack, worker, protected=False)
    observations = {"baseline": baseline, "reproduction": reproduction}
    runner = (REPOSITORY / "deployment/local-stand/delivery-test-command.mjs").read_text()
    digest = hashlib.sha256(pack.program).hexdigest()
    generation = hashlib.sha256(_json([pack.digest, runner, worker["build_configuration"]])).hexdigest()[:12]
    task = "test"
    private = {"test-script": "checks.sh", "test-transcript": "author-transcript.txt"}
    definition = {"class": pack.name, "digest": digest,
                  "timeout": min(60000, worker["project_permissions"]["resources"]["elapsed_millis"] // 2)}
    checked = {**definition, "task": pack.task_sha256, "acceptance": pack.acceptance_sha256,
               "program": base64.b64encode(pack.program).decode()}
    config = {
        "author": (checked, {}),
        "baseline": ({**definition, **{key: baseline[key] for key in ("environment", "criterion")}},
                    {"baseline.report": ("baseline.json", baseline_schema)}),
        "reproduction": ({**checked, **{key: reproduction[key] for key in ("environment", "criterion")}},
                         {"reproduction.report": ("baseline.json", reproduction_schema)}),
        "candidate": (definition, {"validation_report": ("validation.json", _schema("protected_validation_report")),
                        "validation_facts": ("validation-facts.json", _schema("protected_validation_facts"))}),
    }
    commands = {}
    for mode, (parameters, reports) in config.items():
        binding = author if mode == "author" else worker
        alias = f"delivery-{mode}-{pack.name}-{generation}"
        arguments = shlex.join([mode, _json(parameters).decode(), "/octacity/protected", "/workspace/source", "/workspace/output"])
        command = {"dir": "/workspace", "shell": f"/usr/local/bin/node --input-type=module - {arguments} <<'DELIVERY_COMMAND'\n{runner}\nDELIVERY_COMMAND"}
        if reports:
            command["reports"] = [{"name": name, "path": f"output/{filename}", "format": schema["identity"]}
                                  for name, (filename, schema) in reports.items()]
        else:
            command["artifacts"] = [{"name": name, "path": f"output/{filename}", "content_type": "application/octet-stream"}
                                    for name, filename in private.items()]
        octafile = _json({"version": 1, "tasks": {task: command}}).decode()
        executor = triage._reference(alias, digest=hashlib.sha256(octafile.encode()).hexdigest())
        def output(name, kind, schema):
            return {"kind": name, "output_kind": kind, "schema": schema, "tool": executor, "plugin": shell}
        outputs = [output(name, "report", schema) for name, (_, schema) in reports.items()]
        grants = copy.deepcopy(worker["project_permissions"])
        grants.update(plugins=plugins, executables=[], tools=[], commands=[], secret_profiles=[], network_hosts=[],
                      workload_identity_profiles=[])
        grants["mounts"] = [item for item in grants["mounts"] if item["root"] != "/workspace/source"]
        if mode != "author":
            grants["mounts"].append({"root": "/workspace/source", "mode": "read_only"})
        else:
            inventory = output("protected_inventory", "artifact", triage._reference("delivery-test-inventory"))
            outputs = [output("protected_handoff", "report", _schema("protected_test_handoff")), inventory]
            policy = {"policy": {"classes": [pack.name], "max_files": 1, "max_bytes": len(pack.program),
                                 "reserved_inputs": 8, "retention_millis": 3600000},
                      "inventory": inventory, "files": [{"output": "test-script", "class": pack.name, "path": "checks.sh"}],
                      "transcript": "test-transcript"}
        grants["outputs"]["kinds"] = [item["kind"] for item in outputs] + (list(private) if mode == "author" else [])
        identity = triage._reference(alias + "-policy", digest=hashlib.sha256(_json(policy)).hexdigest()
                                     if mode == "author" else executor["digest"])
        commands[mode] = entry = {"identity": identity, "plugin": shell, "executor": executor,
            "build_configuration": binding["build_configuration"], "pipeline_node": binding["pipeline_node"],
            "program": {"octafile": octafile, "tasks": [task], "workspace_root": "/workspace", "credential_profile": None},
            "input_destination": "/octacity/protected/command-input.json", "parameters": {},
            "project_permissions": grants, "local_permissions": {"grants": grants, "enforceable": worker["local_permissions"]["enforceable"]},
            "outputs": outputs}
        if mode in observations:
            entry["observation_contract"] = observations[mode]
        if mode == "candidate":
            entry["candidate_capture_tool"] = capture
        command_output = {"author": "protected_test_handoff", "candidate": "protected_validation_report"}.get(mode)
        _choice(project, alias, executor, node_kind="reasoning" if mode == "author" else "build_command",
                command_output=_schema(command_output, include_shape=True) if command_output else None)
        _choice(project, alias + "-policy", identity)
    for plugin in plugins:
        if not any(item["kind"] == "node_capability" and item["reference"] == plugin for item in project["references"]):
            _choice(project, "delivery-plugin-" + plugin["identity"], plugin)
    _add(catalog.setdefault("protected_test_authors", []), lambda item: item["command"]["identity"],
         {"command": commands["author"], "policy": policy})
    _add(catalog.setdefault("commands", []), lambda item: item["identity"], commands["reproduction"])
    shared_inputs = {"author_profile": commands["author"]["identity"], "policy": policy["policy"], "inventory": inventory}
    baseline, candidate = commands["baseline"], commands["candidate"]
    for entry in ({"profiles": [baseline], "inputs": shared_inputs, "report": baseline["outputs"][0]},
                  {"profiles": [candidate], "inputs": {**shared_inputs, "capture_tool": capture},
                   "report": candidate["outputs"][0], "facts": candidate["outputs"][1]}):
        _add(catalog.setdefault("protected_validation", []), lambda item: item["profiles"][0]["identity"], entry)


def install_worker(client, project: dict, worker: dict, image: str, namespace, *, repository_id: str | None = None) -> dict:
    """Create an ordinary Build bound to either no code or the selected Repository."""
    source = worker["build_configuration"]
    current = client.get(f"/api/v1/build-configurations/{source['id']}/versions/{source['version']}")
    definition = copy.deepcopy(current["definition"])
    definition["runtime"]["immutable_image"] = image
    name = "Factory Delivery checks"
    if repository_id is not None:
        repository = client.get(f"/api/v1/repositories/{repository_id}/versions/1")
        definition.update(source=None, repository_id=repository_id, repository_version=repository["version"])
        name = "Factory Delivery source"
    identity = hashlib.sha256(json.dumps(definition, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode()).hexdigest()
    created = client.post("/api/v1/build-configurations", f"{namespace}-delivery-worker-{identity}",
        {"project_id": project["project_id"], "name": name, "definition": definition})["resource"]
    build = {name: created[name] for name in ("id", "project_id", "version")}
    build["definition_digest"] = hashlib.sha256(json.dumps(created["definition"], sort_keys=True,
        separators=(",", ":"), ensure_ascii=False).encode()).hexdigest()
    _add(project["build_configurations"], lambda item: item["alias"],
         {"alias": f"delivery-checks-{identity[:12]}", "reference": build})
    return {**worker, "build_configuration": build}


def _capture_policy(root: Path, agent_policy: dict) -> dict:
    import shutil
    import subprocess
    if "change_set_capture" not in agent_policy:
        git = shutil.which("git")
        if git is None:
            raise ValueError("Delivery requires a local Git executable")
        program = state.read_regular_file(Path(git).resolve(strict=True), "Delivery Git executable",
            max_bytes=256 * 1024**2, modes=frozenset({0o555, 0o700, 0o755}))
        digest = hashlib.sha256(program).hexdigest()
        directory = root / "delivery-tools" / digest
        state.ensure_private_directory(root / "delivery-tools")
        state.ensure_private_directory(directory)
        path = directory / "git"
        if state.publish_exclusive_private_file(path, program):
            path.chmod(0o700)
        elif state.read_regular_file(path, "retained Delivery Git", max_bytes=256 * 1024**2,
                                    modes=frozenset({0o700})) != program:
            raise ValueError("retained Delivery Git differs from its approved bytes")
        result = subprocess.run([str(path), "--version"], capture_output=True, text=True, check=True, timeout=5)
        match = re.fullmatch(r"git version ([a-zA-Z0-9.\-_]+)\s*", result.stdout)
        if match is None:
            raise ValueError("Delivery requires Git with a stable executable version")
        agent_policy["change_set_capture"] = {"provider": "git", "executable": {
            "path": str(path), "version": match[1], "platform": "macos-aarch64", "sha256": digest}}
    executable = agent_policy["change_set_capture"]["executable"]
    return triage._reference("git", executable["version"], executable["sha256"])


def _extend_agent(agent_policy: dict, catalog: dict, project: dict, plugins: list[dict]) -> None:
    grants = agent_policy["factory_permissions"]
    runtimes = project["role_profiles"]
    login = next(row["credentials"]["evaluator"] for row in catalog["codex"]
                 if row["toolchain"] == runtimes["triage-runtime"]["ceiling"]["toolchain"])
    agent_policy["factory_credential_profiles"].setdefault(MODEL_CREDENTIAL, agent_policy["factory_credential_profiles"][login])
    grants["secret_profiles"] = sorted(set(grants["secret_profiles"]) | {MODEL_CREDENTIAL})
    if not any(item["root"] == "/workspace/source" for item in grants["mounts"]):
        grants["mounts"].append({"root": "/workspace/source", "mode": "read_write"})
        grants["mounts"].sort(key=lambda item: item["root"])
    for plugin in plugins:
        if not any(item["identity"] == plugin["identity"] for item in grants["plugins"]):
            grants["plugins"].append({"identity": plugin["identity"], "version": plugin["version"], "sha256": plugin["digest"]})
    grants["plugins"].sort(key=lambda item: item["identity"])
    required = [runtimes[f"delivery-{role}"]["ceiling"]["permissions"] for role in ("coder", "reviewer", "devops")]
    required += [item["command"]["project_permissions"] for item in catalog["protected_test_authors"]
                 if item["command"]["build_configuration"]["project_id"] == project["project_id"]]
    required += [profile["project_permissions"] for item in catalog["protected_validation"]
                 for profile in item["profiles"] if profile["build_configuration"]["project_id"] == project["project_id"]]
    grants["outputs"]["kinds"] = sorted(set(grants["outputs"]["kinds"]).union(
        *(item["outputs"]["kinds"] for item in required)))


def stage(root: Path, directory: Path, installed: dict, project_id: str, repository_id: str, namespace, native: Path, client,
          packs: list[TestPack], write_directories: list[str], *,
          publication: tuple[str, str, str] | None = None) -> dict[str, Path]:
    """Prepare additive immutable inputs for the ordinary installation updater."""
    import configure_local_stand_agent as agent
    import configure_local_stand_server as server
    if not write_directories or any(not re.fullmatch(r"[a-zA-Z0-9_][a-zA-Z0-9_.-]*(/[a-zA-Z0-9_][a-zA-Z0-9_.-]*)*",
                                                    path) for path in write_directories):
        raise ValueError("Delivery write directories must be explicit repository-relative paths")
    source = installed["factory_profiles"]
    catalog = state.load_json_object(state.read_regular_file(source, "Factory profile catalog",
        max_bytes=server.MAX_FACTORY_PROFILE_BYTES, modes=frozenset({0o600})), "Factory profile catalog")
    agent_policy = agent.read_factory_agent_profile(installed["factory_agent_profile"])
    capture = _capture_policy(root, agent_policy)
    project, = (row for row in catalog["projects"] if row["project_id"] == project_id)
    ceiling = project["role_profiles"]["triage-runtime"]["ceiling"]
    template, = (row for row in catalog["codex"] if row["toolchain"] == ceiling["toolchain"]
                 and row["build_configuration"] == ceiling["build_configuration"] and row["credential_purpose"] == "evaluator")
    octa = json.loads((native / "job-spec-policy.json").read_text())["octa"]
    plugins = [triage._reference(name, octa["version"], digest) for name, digest in sorted(octa["plugin_digests"].items())]
    image = triage.prepare_guest_image(root, native, delivery=True)
    author = install_worker(client, project, template, image, namespace)
    worker = install_worker(client, project, template, image, namespace, repository_id=repository_id)
    catalog = delivery_models(catalog, project_id, capture, write_directories, code_build=worker["build_configuration"])
    project, = (row for row in catalog["projects"] if row["project_id"] == project_id)
    for pack in packs:
        install_test_commands(catalog, project, worker, plugins, capture, pack, author=author)
    _extend_agent(agent_policy, catalog, project, plugins)
    result = {}
    if publication is not None:
        repository_id, branch, token = publication
        repository = client.get(f"/api/v1/repositories/{repository_id}/versions/1")
        plugin, git = _packaged_github()
        result["factory_delivery_bundle"] = stage_publication(directory, catalog, project, repository,
            branch, token, plugin, git, installed["factory_delivery_bundle"])
    profiles = directory / "delivery-profiles.json"
    policy = directory / "delivery-agent.toml"
    state.publish_exclusive_private_file(profiles, _json(catalog))
    state.publish_exclusive_private_file(policy, ("\n".join(
        f"{key} = {agent._toml_value(value)}" for key, value in agent_policy.items()) + "\n").encode())
    return {**result, "factory_profiles": profiles, "factory_agent_profile": policy}


def _packaged_github() -> tuple[bytes, str]:
    import subprocess
    from local_stand_launcher import SERVER_IMAGE
    def read(entrypoint: str, path: str) -> bytes:
        return subprocess.run(["docker", "run", "--rm", "--network", "none", "--read-only",
            "--entrypoint", entrypoint, SERVER_IMAGE, path], check=True, capture_output=True, timeout=30).stdout
    git = read("/usr/bin/sha256sum", "/usr/bin/git").split()
    if not git or not re.fullmatch(rb"[a-f0-9]{64}", git[0]):
        raise ValueError("Delivery image Git executable identity is invalid")
    return read("/bin/cat", "/usr/local/bin/octacity-github"), git[0].decode()


def publish_roles(client, project: str, namespace) -> None:
    """Publish reusable roles with fixed initial keys through the Project owner."""
    import uuid
    base = f"/api/v1/projects/{project}"
    for name, runtime, description in (
        ("Coder", "delivery-coder", "Implement the explicitly assigned change within the approved source directories."),
        ("Tester", "delivery-devops", "Explain selected test evidence and distinguish setup errors from confirmed failures."),
        ("Reviewer", "delivery-reviewer", "Review the selected candidate against the accepted requirements and record findings."),
        ("DevOps", "delivery-devops", "Prepare a short review title and summary from the selected accepted evidence."),
    ):
        key = f"stand-delivery-v1-{name.lower()}"
        role_id = str(uuid.uuid5(namespace, key + "-role"))
        profile_id = str(uuid.uuid5(namespace, key + "-profile"))
        ceiling = client.get(f"{base}/factory-profile-capabilities/{runtime}")["definition"]
        settings = {field: ceiling[field] for field in ("toolchain", "permissions")}
        client.post(f"{base}/factory-runtimes/{runtime}", f"{namespace}-{runtime}",
            {"expected_version": None, "definition": {**settings, "integrations": {}, "mcp_servers": {}}})
        role = client.post(f"{base}/factory-role-publications", f"{namespace}-{key}-role", {
            "role": {"id": role_id, "expected_current_version": None, "definition": {
                "name": name, "icon": "bot", "description": description,
                "instructions": ["Follow the reviewed step assignment. Use only the selected inputs and granted access."]}},
            "profiles": []})["role"]["reference"]
        definition = {**ceiling, "name": f"{name} · Delivery"}
        reviewed = client.post(f"{base}/factory-role-profiles/validate", f"{namespace}-{key}-validate",
            {"id": profile_id, "role": role, "capability": runtime, "definition": definition})
        if reviewed["incompatibility"] is not None:
            raise ValueError(f"Delivery {name} profile is incompatible with its installed Runtime")
        client.post(f"{base}/factory-role-publications", f"{namespace}-{key}-profile", {"profiles": [{
            "id": profile_id, "role": role, "expected_current_version": None,
            "definition": reviewed["profile"]["definition"]}]})


def stage_publication(directory: Path, catalog: dict, project: dict, repository: dict, branch: str,
                      token: str, plugin_bytes: bytes, git_digest: str, inherited: Path | None) -> Path:
    """Install a separate GitHub draft-PR authority for the selected code repository."""
    from factory_integrations import API_VERSION
    from local_stand_installation_update import _snapshot
    locator = repository["definition"]["repository_locator"]
    match = re.fullmatch(r"https://github\.com/([A-Za-z0-9_.-]+)/([A-Za-z0-9_.-]+?)(?:\.git)?", locator)
    if not match or not re.fullmatch(r"[A-Za-z0-9_][A-Za-z0-9_/-]{0,127}", branch):
        raise ValueError("Delivery needs a GitHub code repository and an explicit base branch")
    owner, name = match.groups()
    remote = f"https://github.com/{owner}/{name}.git"
    root = directory / "delivery-adapters"
    if inherited is not None:
        state.ensure_private_directory(root)
        _snapshot(inherited, copy_to=root)
    adapter = root / "github"
    credentials, inputs = adapter / "credentials", adapter / "inputs"
    for path in (root, adapter, credentials, inputs):
        state.ensure_private_directory(path)
    plugin = triage._reference("github", digest=hashlib.sha256(plugin_bytes).hexdigest())
    configuration = (f'config_version = 1\napi_base = "https://api.github.com/"\napi_version = "{API_VERSION}"\n'
        f'git_path = "/usr/bin/git"\ngit_sha256 = "{git_digest}"\n'
        f'credential_directory = "credentials"\ninput_directory = "inputs"\n'
        f'[repositories.{json.dumps(remote)}]\nrepository_id = "{repository["id"]}"\n'
        f'owner = "{owner}"\nname = "{name}"\nremote = "{remote}"\n').encode()
    digest = hashlib.sha256(configuration).hexdigest()
    manifest = (f'manifest_version = 1\nadapter_id = "github"\nexecutable = "octacity-github"\n'
        f'executable_sha256 = "{plugin["digest"]}"\n[protocol]\nmin = 1\nmax = 1\n[capabilities]\n'
        f'observe = true\npublish = true\ncancel = true\nidempotency = "observe_before_retry"\n'
        f'credential_class = "delivery_write"\nconfiguration_digest = "{digest}"\n').encode()
    for filename, content, mode in (("octacity-github", plugin_bytes, 0o700), ("github-plugin.toml", configuration, 0o600),
                                    ("adapter.toml", manifest, 0o600)):
        path = adapter / filename
        if not state.publish_exclusive_private_file(path, content):
            if state.read_regular_file(path, "installed Delivery adapter", max_bytes=256 * 1024**2,
                                       modes=frozenset({mode})) != content:
                raise ValueError("Delivery conflicts with an existing GitHub adapter")
        path.chmod(mode)
    credential = "stand-delivery-github"
    handle = "delivery-" + repository["id"]
    path = credentials / (hashlib.sha256(handle.encode()).hexdigest() + ".json")
    path.unlink(missing_ok=True)
    state.publish_exclusive_private_file(path, _json({"class": "delivery_write", "token": token, "repositories": [remote]}))
    draft = {"plugin": plugin, "configuration_digest": digest, "project_id": project["project_id"],
        "repository_id": repository["id"], "repository_locator": remote, "credential_profile": credential,
        "credential_handle": handle, "network_hosts": ["api.github.com", "github.com"],
        "target": {"base_reference": "refs/heads/" + branch, "candidate_reference_prefix": "refs/heads/octacity/delivery/"},
        "review": {"title": "OctaCity candidate", "summary": "", "draft": True}}
    target = triage._reference("github-draft-pr", digest=_digest("octacity.factory.delivery-policy.v1", draft))
    action = triage._reference("stand-delivery-publish", digest=plugin["digest"])
    receipt = _schema("pull_request_receipt", REPOSITORY / "config/factory/presets/ticket-delivery/pull-request-v1.json",
                      include_shape=True)
    profile = {"identity": action, "outcome": {"key": "published", "kind": "success", "schema": receipt["reference"]},
        "receipt_schema": receipt, "adapter_root": "/run/octacity-delivery",
        "policies": [{"reference": target, "draft": draft, "input_directory": "/run/octacity-delivery/github/inputs",
                      "review_links": "github"}], "timeout_milliseconds": 60000, "termination_grace_milliseconds": 1000}
    _add(catalog.setdefault("delivery", []), lambda item: item["identity"], profile)
    _choice(project, action["identity"], action, node_kind="trusted_action")
    _choice(project, "delivery-github-provider", plugin)
    _add(project["references"], lambda item: (item["kind"], item["alias"]),
         {"kind": "secret_profile", "alias": credential, "reference": triage._reference(credential)})
    return root
