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
TEST_INVENTORY_BYTE_RESERVE = 1024  # One file and its optional transcript reference.
TEST_SCRIPT_MAX_BYTES = 16384
# The assertion protocol changes the installed instructions. Keep the original
# Tester Runtime and its Profile pins usable by already published Flows.
TEST_AUTHOR_RUNTIME = "delivery-tester-assertions"
TEST_VERIFIER_RUNTIME = "delivery-tester-verification"
TESTER_PREPARE_MODE = "delivery-tester-prepare"
TESTER_ANALYZE_MODE = "delivery-tester-analyze"
TEST_VERIFIER_INSTRUCTIONS = (
    "Factory starts the mandatory verification Build before this step. Read its retained result "
    "using the installed verify_candidate tool with action status. Summarize the runner verdict; "
    "a missing, failed or indeterminate check is not a pass. Your response cannot replace that verdict. "
    "The server selects the retained checks and reviewed candidate; do not replace either."
)
# Microsandbox allocates whole vCPUs to each isolated VM.
READER_CPU_MILLIS = 1000
SCOPED_TESTER_CPU_MILLIS = 2 * READER_CPU_MILLIS


def install_controller(root: Path, agent_policy: dict, *, read=None) -> None:
    """Pin the packaged guest controller independently of model-selected tools."""
    if read is None:
        import subprocess
        from local_stand_launcher import SERVER_IMAGE
        def read(name):
            return subprocess.run(["docker", "run", "--rm", "--network", "none", "--read-only",
                "--entrypoint", "/bin/cat", SERVER_IMAGE, "/usr/local/lib/octacity/factory-tools/" + name],
                check=True, capture_output=True, timeout=30).stdout
    programs = {name: read(name) for name in ("octacity-protected-reader", "octacity-codex-reader")}
    if any(not contents or len(contents) > 64 * 1024**2 for contents in programs.values()):
        raise ValueError("packaged Factory controller is outside its byte bounds")
    digests = {name: hashlib.sha256(contents).hexdigest() for name, contents in programs.items()}
    identity = hashlib.sha256(_json(digests)).hexdigest()
    directory = root / "delivery-tools" / identity
    with state.private_umask():
        state.ensure_private_directory(root / "delivery-tools")
        state.ensure_private_directory(directory)
    for name, contents in programs.items():
        path = directory / name
        if state.publish_exclusive_private_file(path, contents):
            path.chmod(0o500)
        elif state.read_regular_file(path, "retained Factory controller", max_bytes=64 * 1024**2,
                                     modes=frozenset({0o500})) != contents:
            raise ValueError("retained Factory controller differs from its original bytes")
    agent_policy["protected_reader"] = {
        "release_root": str(directory), "entrypoint": "octacity-protected-reader", "platform": "linux-aarch64",
        "tool": {"identity": "factory-scoped-validation", "version": "v1", "sha256": triage._reference("factory-scoped-validation")["digest"]},
        "executable": {"identity": "octacity-protected-reader", "version": identity[:16], "sha256": digests["octacity-protected-reader"]},
        "bridge": {"entrypoint": "octacity-codex-reader", "sha256": digests["octacity-codex-reader"],
                   "hook_tool_name": "mcp__factory__read_file"},
        "resources": {"cpu_millis": READER_CPU_MILLIS, "memory_bytes": 64 * 1024**2, "disk_bytes": 16 * 1024**2,
                      "process_count": 8, "elapsed_millis": 300000},
        "outputs": {"kinds": [], "max_artifact_count": 0, "max_artifact_bytes": 0, "max_report_count": 0, "max_report_bytes": 0},
    }


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


def with_bootstrap_compiler(catalog: dict, project: dict, root: Path) -> dict:
    """Reuse the exact bootstrap template only when its active compiler was retired."""
    ceiling = project["role_profiles"]["triage-runtime"]["ceiling"]
    def matches(row):
        return (row["toolchain"] == ceiling["toolchain"] and
                row["build_configuration"] == ceiling["build_configuration"] and
                row["credential_purpose"] == "evaluator")
    if any(matches(row) for row in catalog["codex"]):
        return catalog
    seed = state.load_json_object(state.read_regular_file(root / "triage/factory-profiles.json",
        "Factory bootstrap compiler", max_bytes=triage.server.MAX_FACTORY_PROFILE_BYTES,
        modes=frozenset({0o600})), "Factory bootstrap compiler")
    candidates = [row for row in seed["codex"] if matches(row)]
    if len(candidates) != 1:
        raise ValueError("the retained bootstrap compiler does not match the installed declaration")
    catalog = copy.deepcopy(catalog)
    catalog["codex"].extend(candidates)
    return catalog


def current_runtime_catalog(catalog: dict, plugins: list[dict]) -> dict:
    """Activate only compilers qualified for this native plugin inventory.

    Published Role/Profile and Flow definitions stay in Server storage. Their
    installed choices remain visible for inspection, but a historical compiler
    cannot acquire a current native binding by changing its pinned dependency.
    The caller retains the full installation input before this projection.
    """
    active = copy.deepcopy(catalog)
    matches = lambda grants: all(plugin in plugins for plugin in grants["plugins"])
    active["codex"] = [row for row in active.get("codex", []) if row["toolchain"]["plugin"] in plugins]
    for kind in ("protected_validation", "promotions"):
        if kind in active:
            active[kind] = [row for row in active[kind]
                if all(matches(profile["project_permissions"]) for profile in row["profiles"])]
    if "commands" in active:
        active["commands"] = [row for row in active["commands"] if matches(row["project_permissions"])]
    if "protected_test_authors" in active:
        active["protected_test_authors"] = [row for row in active["protected_test_authors"]
            if matches(row["command"]["project_permissions"])]
    return active


def delivery_models(catalog: dict, project_id: str, capture_tool: dict, write_directories: list[str], *,
                    code_build: dict, scoped_build: dict | None = None, plugin: dict | None = None) -> dict:
    """Install reusable runtimes and keep any previously approved Coder scope."""
    catalog = copy.deepcopy(catalog)
    project, = (entry for entry in catalog["projects"] if entry["project_id"] == project_id)
    runtimes = project["role_profiles"]
    original = runtimes["triage-runtime"]
    ceiling = original["ceiling"]
    observer, = (entry for entry in catalog["codex"] if entry["toolchain"] == ceiling["toolchain"]
                 and entry["build_configuration"] == ceiling["build_configuration"]
                 and entry["credential_purpose"] == "evaluator")
    permissions = observer["project_permissions"]
    selected_plugin = plugin or observer["toolchain"]["plugin"]
    model_credential = observer["credentials"]["evaluator"]
    mcp_hosts = {urlsplit(entry["server"]["url"]).hostname
                 for entry in original.get("settings_support", {}).get("authenticated_mcp", {}).values()}
    hosts = [host for host in permissions["network_hosts"] if host not in mcp_hosts]
    if not write_directories and "delivery-coder" in runtimes:
        retained = [row["ceiling"] for alias, row in runtimes.items()
            if alias == "delivery-coder" or re.fullmatch(r"delivery-coder-[0-9a-f]{12}", alias)]
        scopes = {tuple(row["capture"]["instruction"]["policy"]["allowed_path_prefixes"])
            for row in catalog["codex"] if row.get("capture") and any(
                row["toolchain"] == ceiling["toolchain"]
                and row["build_configuration"] == ceiling["build_configuration"] for ceiling in retained)}
        if len(scopes) != 1:
            raise ValueError("retained Delivery Coder write scope is unavailable or ambiguous")
        write_directories = list(scopes.pop())
    roles = []
    if write_directories:
        roles.append(("delivery-coder", "Coder", "model", "read_write"))
    elif "delivery-coder" not in runtimes:
        roles.append(("delivery-coder-readonly", "Coder", "evaluator", "read_only"))
    roles.extend((("delivery-reviewer", "Reviewer", "evaluator", "read_only"),
                  ("delivery-devops", "DevOps", "evaluator", None),
                  (TEST_AUTHOR_RUNTIME, "Tester", "evaluator", "read_only"),
                  (TEST_VERIFIER_RUNTIME, "Tester", "model", None)))
    tester_modes = []
    for alias, role, purpose, source_mode in roles:
        runtime_alias = alias
        selected_build = (scoped_build or observer["build_configuration"]) if alias == TEST_VERIFIER_RUNTIME else code_build
        plugin_changed = alias in runtimes and runtimes[alias]["ceiling"]["toolchain"]["plugin"] != selected_plugin
        cpu_changed = alias == TEST_VERIFIER_RUNTIME and alias in runtimes and (
            runtimes[alias]["ceiling"]["permissions"].get("resources", {}).get("cpu_millis", 0)
            < SCOPED_TESTER_CPU_MILLIS)
        instructions_changed = alias == TEST_VERIFIER_RUNTIME and alias in runtimes and any(
            row["toolchain"] == runtimes[alias]["ceiling"]["toolchain"]
            and row.get("instructions") != TEST_VERIFIER_INSTRUCTIONS for row in catalog["codex"])
        if alias in runtimes and (plugin_changed or cpu_changed or instructions_changed or
                (source_mode or alias == TEST_VERIFIER_RUNTIME) and runtimes[alias]["ceiling"]["build_configuration"] != selected_build):
            # Published profiles retain their exact compiler and guest. Install
            # a separate capability rather than silently replacing that binding.
            identity = [selected_build, selected_plugin] if plugin_changed else selected_build
            if cpu_changed:
                identity = [identity, {"cpu_millis": SCOPED_TESTER_CPU_MILLIS}]
            if instructions_changed:
                identity = [identity, {"instructions": TEST_VERIFIER_INSTRUCTIONS}]
            runtime_alias += "-" + hashlib.sha256(_json(identity)).hexdigest()[:12]
        grants = copy.deepcopy(permissions)
        if plugin is not None:
            grants["plugins"] = [selected_plugin]
        grants["secret_profiles"] = [MODEL_CREDENTIAL if purpose == "model" else model_credential]
        grants["network_hosts"] = hosts
        if source_mode:
            grants["mounts"].append({"root": "/workspace/source", "mode": source_mode})
        # The reference version distinguishes compilers for the same provider model.
        toolchain = {**observer["toolchain"], "plugin": selected_plugin, "model": {**observer["toolchain"]["model"], "version": runtime_alias + "-v1"}}
        entry = {**copy.deepcopy(observer), "toolchain": toolchain, "credential_purpose": purpose,
            "credentials": {**observer["credentials"], "model": MODEL_CREDENTIAL},
            "project_permissions": grants, "local_permissions": {**observer["local_permissions"], "grants": grants}}
        entry.pop("mcp_configurations", None)
        if source_mode:
            entry.update(build_configuration=code_build, candidate_capture_tool=capture_tool)
            _choice(project, "delivery-capture-" + capture_tool["digest"][:12], capture_tool)
        if alias == TEST_AUTHOR_RUNTIME:
            entry["instructions"] = (
                "Author a POSIX shell check for the accepted behavior. The candidate source directory is the first "
                "argument. Exit 0 for a verified pass. After a completed failing assertion, print the exact line "
                "octacity:assertion:failed to stdout as the final line and exit 1. Exit 2 if the harness or "
                "environment is unavailable. The isolated verifier runs the check."
            )
            entry["protected_tests"] = {
                "policy": {"classes": ["acceptance"], "max_files": 1, "reserved_inputs": 8,
                           "max_bytes": TEST_SCRIPT_MAX_BYTES + TEST_INVENTORY_BYTE_RESERVE,
                           "retention_millis": 3600000},
                "inventory": {"kind": "protected_inventory", "output_kind": "artifact",
                    "schema": triage._reference("delivery-test-inventory-assertions"),
                    "tool": toolchain["executable"], "plugin": toolchain["plugin"]},
                "files": [{"output": "tester-checks", "class": "acceptance", "path": "checks.sh"}],
                "transcript": "codex-run-trace",
            }
            grants["outputs"]["kinds"].extend(["tester-checks", "protected_inventory", "protected_handoff"])
            grants["outputs"]["max_artifact_count"] = max(4, grants["outputs"]["max_artifact_count"])
            grants["outputs"]["max_report_count"] = max(2, grants["outputs"]["max_report_count"])
        if alias == TEST_VERIFIER_RUNTIME:
            resources = grants.setdefault("resources", {})
            resources["cpu_millis"] = max(resources.get("cpu_millis", 0), SCOPED_TESTER_CPU_MILLIS)
            entry["build_configuration"] = selected_build
            entry["execution_mode"] = "scoped_validation"
            entry["tool_response_limits"] = {"per_call_bytes": 16384, "total_bytes": 262144}
            entry["instructions"] = TEST_VERIFIER_INSTRUCTIONS
            grants["tools"] = [triage._reference("factory-scoped-validation")]
            _choice(project, "factory-scoped-validation", grants["tools"][0])
        elif purpose == "model":
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
        if runtime_alias in runtimes and runtimes[runtime_alias] != runtime:
            raise ValueError("Delivery conflicts with an existing installed Runtime")
        runtimes[runtime_alias] = runtime
        _choice(project, runtime_alias, toolchain["executable"])
        _choice(project, runtime_alias + "-model", toolchain["model"])
        if alias in (TEST_AUTHOR_RUNTIME, TEST_VERIFIER_RUNTIME):
            tester_modes.append((entry, runtime))
    _add(project["references"], lambda item: (item["kind"], item["alias"]),
        {"kind": "secret_profile", "alias": MODEL_CREDENTIAL, "reference": triage._reference(MODEL_CREDENTIAL)})
    _install_tester_modes(catalog, project, tester_modes)
    return catalog


def _install_tester_modes(catalog: dict, project: dict, modes: list) -> None:
    """Share AI settings while retaining independent, content-pinned step compilers."""
    identity = hashlib.sha256(_json(modes)).hexdigest()
    permissions = _merge_permissions(*(runtime["ceiling"]["permissions"] for _, runtime in modes))
    model = {**modes[0][0]["toolchain"]["model"], "version": "delivery-tester-shared-" + identity[:16]}
    for alias, (compiler, runtime) in zip((TESTER_PREPARE_MODE, TESTER_ANALYZE_MODE), modes, strict=True):
        compiler, runtime = copy.deepcopy((compiler, runtime))
        compiler["toolchain"]["model"] = model
        runtime["ceiling"].update(toolchain=compiler["toolchain"], execution_binding="step", permissions=permissions)
        runtime["ceiling"]["name"] = ("Prepare protected tests" if alias == TESTER_PREPARE_MODE else "Analyze CI results")
        _add(catalog["codex"], lambda row: (row["toolchain"], row["build_configuration"]), compiler)
        previous = project["role_profiles"].get(alias)
        if previous is not None and previous != runtime:
            retained = alias + "-" + hashlib.sha256(_json(previous)).hexdigest()[:12]
            project["role_profiles"].setdefault(retained, previous)
            _choice(project, retained, previous["ceiling"]["toolchain"]["executable"])
        project["role_profiles"][alias] = runtime
        _choice(project, alias, compiler["toolchain"]["executable"])
        _choice(project, alias + "-" + identity[:12], model)


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
            max_bytes=TEST_SCRIPT_MAX_BYTES, modes=frozenset({0o600, 0o700}))
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


def _observation(criterion: dict, worker: dict, *, protected: bool) -> tuple[dict, dict]:
    shape = _object({"failed": {"type": "boolean"}})
    identity, version = "delivery-test-observation", "v1"
    contract = {"environment": triage._reference("delivery-guest", digest=worker["build_configuration"].get("definition_digest")),
        "criterion": criterion,
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


def _verification_runner_source() -> str:
    return (REPOSITORY / "deployment/local-stand/delivery-test-command.mjs").read_text()


def _standard_verifiers_current(catalog: dict, project_id: str, author_profile: dict) -> bool:
    """Keep old immutable commands usable, while installing the current verifier for new Flows."""
    script = "\n" + _verification_runner_source() + "\nDELIVERY_COMMAND"
    modes = set()
    for validator in catalog.get("protected_validation", []):
        inputs = validator["inputs"]
        if inputs["author_profile"] != author_profile or not inputs.get("accepts_supplement"):
            continue
        for profile in validator["profiles"]:
            if profile["build_configuration"]["project_id"] != project_id:
                continue
            try:
                program = json.loads(profile["program"]["octafile"])
            except json.JSONDecodeError:
                continue
            if any(task.get("shell", "").endswith(script) for task in program.get("tasks", {}).values()):
                modes.add("candidate" if "facts" in validator else "baseline")
    return modes == {"baseline", "candidate"}


def install_test_commands(catalog: dict, project: dict, worker: dict, plugins: list[dict], capture: dict,
                          pack: TestPack | None, *, author: dict) -> None:
    """Bind the baseline and candidate verifier to one exact author."""
    shell, = (plugin for plugin in plugins if plugin["identity"] == "shell")
    runner = _verification_runner_source()
    private = {"test-script": "checks.sh", "test-transcript": "author-transcript.txt"}
    if pack is None:
        author_profile = author["toolchain"]["model"]
        policy = author["protected_tests"]
        name, identity = "tester-assertions", policy
        entrypoints = [file["path"] for file in policy["files"]]
        criterion = triage._reference("delivery-tester-acceptance", digest=hashlib.sha256(_json(policy)).hexdigest())
    else:
        name, identity = pack.name, pack.digest
        entrypoints = [private["test-script"]]
        criterion = triage._reference("delivery-tests-" + name, digest=identity)
    baseline, baseline_schema = _observation(criterion, worker, protected=True)
    observations = {"baseline": baseline}
    timeout = min(60000, worker["project_permissions"]["resources"]["elapsed_millis"] // 2)
    definition = {"entrypoints": entrypoints, "timeout": timeout, "isolate_checks": True}
    if pack is None:
        definition["explicit_assertion_failure"] = True
    config = {
        "baseline": ({**definition, **{key: baseline[key] for key in ("environment", "criterion")}},
                    {"baseline.report": ("baseline.json", baseline_schema)}),
        "candidate": (definition, {"validation_report": ("validation.json", _schema("protected_validation_report")),
                        "validation_facts": ("validation-facts.json", _schema("protected_validation_facts"))}),
    }
    bundle_bytes = 0
    if pack is not None:
        bundle_bytes = len(pack.program) + TEST_INVENTORY_BYTE_RESERVE
        checked = {"class": name, "digest": hashlib.sha256(pack.program).hexdigest(), "timeout": timeout,
                   "task": pack.task_sha256, "acceptance": pack.acceptance_sha256,
                   "program": base64.b64encode(pack.program).decode()}
        reproduction, reproduction_schema = _observation(criterion, worker, protected=False)
        observations["reproduction"] = reproduction
        config["author"] = (checked, {})
        config["reproduction"] = ({**checked, "isolate_checks": True,
                                  **{key: reproduction[key] for key in ("environment", "criterion")}},
                                  {"reproduction.report": ("baseline.json", reproduction_schema)})
    generation = hashlib.sha256(_json([identity, runner, worker["build_configuration"], bundle_bytes])).hexdigest()[:12]
    task = "test"
    commands = {}
    for mode, (parameters, reports) in config.items():
        binding = author if mode == "author" else worker
        isolation = "-isolated-v2" if parameters.get("isolate_checks") else ""
        alias = f"delivery-{mode}{isolation}-{name}-{generation}"
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
            policy = {"policy": {"classes": [name], "max_files": 1, "max_bytes": bundle_bytes,
                                 "reserved_inputs": 8, "retention_millis": 3600000},
                      "inventory": inventory, "files": [{"output": "test-script", "class": name, "path": private["test-script"]}],
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
            alias = "delivery-plugin-" + plugin["identity"]
            if any(item["alias"] == alias for item in project["references"]):
                alias += "-" + plugin["digest"][:12]
            _choice(project, alias, plugin)
    if pack is not None:
        author_profile = commands["author"]["identity"]
        _add(catalog.setdefault("protected_test_authors", []), lambda item: item["command"]["identity"],
             {"command": commands["author"], "policy": policy})
        _add(catalog.setdefault("commands", []), lambda item: item["identity"], commands["reproduction"])
    shared_inputs = {"author_profile": author_profile, "policy": policy["policy"], "inventory": policy["inventory"],
                     "accepts_supplement": True}
    baseline, candidate = commands["baseline"], commands["candidate"]
    for entry in ({"profiles": [baseline], "inputs": shared_inputs, "report": baseline["outputs"][0]},
                  {"profiles": [candidate], "inputs": {**shared_inputs, "capture_tool": capture},
                   "report": candidate["outputs"][0], "facts": candidate["outputs"][1]}):
        _add(catalog.setdefault("protected_validation", []), lambda item: item["profiles"][0]["identity"], entry)


def install_worker(client, project: dict, worker: dict, image: str, namespace, *, repository_id: str | None = None,
                   pool_id: str | None = None, cpu_millis: int | None = None) -> dict:
    """Create an ordinary Build bound to either no code or the selected Repository."""
    source = worker["build_configuration"]
    current = client.get(f"/api/v1/build-configurations/{source['id']}/versions/{source['version']}")
    definition = copy.deepcopy(current["definition"])
    definition["runtime"]["immutable_image"] = image
    if cpu_millis is not None:
        definition["runtime"]["cpu_millis"] = cpu_millis
    if pool_id is not None:
        definition["allowed_pools"] = [pool_id]
    name = "Factory Delivery checks"
    if repository_id is not None:
        repository = client.get(f"/api/v1/repositories/{repository_id}/versions/1")
        definition.update(source=None, repository_id=repository_id, repository_version=repository["version"])
        name = "Factory Delivery source"
    identity = hashlib.sha256(json.dumps(definition, sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode()).hexdigest()
    if pool_id is not None:
        name = "Factory Delivery validation"
    name += " · " + identity[:12]
    created = client.post("/api/v1/build-configurations", f"{namespace}-delivery-worker-v2-{identity}",
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
    writable = "delivery-coder" in runtimes
    if writable or TEST_VERIFIER_RUNTIME in runtimes:
        agent_policy["factory_credential_profiles"].setdefault(MODEL_CREDENTIAL, agent_policy["factory_credential_profiles"][login])
        grants["secret_profiles"] = sorted(set(grants["secret_profiles"]) | {MODEL_CREDENTIAL})
    source = next((item for item in grants["mounts"] if item["root"] == "/workspace/source"), None)
    if source is None:
        grants["mounts"].append({"root": "/workspace/source", "mode": "read_write" if writable else "read_only"})
        grants["mounts"].sort(key=lambda item: item["root"])
    elif writable:
        source["mode"] = "read_write"
    for plugin in plugins:
        grants["plugins"] = [item for item in grants["plugins"] if item["identity"] != plugin["identity"]]
        grants["plugins"].append({"identity": plugin["identity"], "version": plugin["version"], "sha256": plugin["digest"]})
    grants["plugins"].sort(key=lambda item: item["identity"])
    required = [runtime["ceiling"]["permissions"] for alias, runtime in runtimes.items()
                if alias in ("delivery-coder", "delivery-coder-readonly", "delivery-reviewer", "delivery-devops",
                             TEST_AUTHOR_RUNTIME, TEST_VERIFIER_RUNTIME)
                or alias.startswith(TEST_VERIFIER_RUNTIME + "-")]
    for permissions in required:
        if "resources" in permissions:
            resources = grants.setdefault("resources", {})
            resources["cpu_millis"] = max(resources.get("cpu_millis", 0),
                                         permissions["resources"].get("cpu_millis", 0))
        for tool in permissions.get("tools", []):
            item = {"identity": tool["identity"], "version": tool["version"], "sha256": tool["digest"]}
            _add(grants.setdefault("tools", []), lambda entry: entry["identity"], item)
    grants["tools"].sort(key=lambda item: item["identity"])
    required += [item["command"]["project_permissions"] for item in catalog.get("protected_test_authors", [])
                 if item["command"]["build_configuration"]["project_id"] == project["project_id"]]
    required += [profile["project_permissions"] for item in catalog.get("protected_validation", [])
                 for profile in item["profiles"] if profile["build_configuration"]["project_id"] == project["project_id"]]
    grants["outputs"]["kinds"] = sorted(set(grants["outputs"]["kinds"]).union(
        *(item["outputs"]["kinds"] for item in required)))


def _merge_permissions(left, right):
    if isinstance(left, dict):
        return {key: _merge_permissions(left[key], right[key]) if key in left and key in right
                else copy.deepcopy(left.get(key, right.get(key))) for key in sorted(left.keys() | right.keys())}
    if isinstance(left, list):
        if left + right and all(isinstance(row, dict) and "root" in row for row in left + right):
            mounts = {}
            for row in left + right:
                if row["root"] not in mounts or row["mode"] == "read_write":
                    mounts[row["root"]] = copy.deepcopy(row)
            return [mounts[key] for key in sorted(mounts)]
        return [json.loads(value) for value in sorted({json.dumps(row, sort_keys=True) for row in left + right})]
    return max(left, right)


def install_access_ceiling(catalog: dict, project: dict) -> None:
    """Declare the complete installed Delivery authority without changing Project policies.

    One content-pinned declaration covers the selected runtimes, ordinary verifiers
    and trusted publisher. Project owners still choose and publish their own
    narrower access profile; an existing Standard profile is never replaced.
    """
    required = [row["ceiling"]["permissions"] for alias, row in project["role_profiles"].items()
                if alias.startswith("delivery-")]
    required += [row["project_permissions"] for row in catalog.get("commands", [])
                 if row["build_configuration"]["project_id"] == project["project_id"]]
    required += [row["command"]["project_permissions"] for row in catalog.get("protected_test_authors", [])
                 if row["command"]["build_configuration"]["project_id"] == project["project_id"]]
    required += [row["project_permissions"] for entry in catalog.get("protected_validation", [])
                 for row in entry["profiles"] if row["build_configuration"]["project_id"] == project["project_id"]]
    if not required:
        return

    permissions = copy.deepcopy(required[0])
    for grants in required[1:]:
        permissions = _merge_permissions(permissions, grants)
    for entry in catalog.get("delivery", []):
        for policy in entry["policies"]:
            draft = policy["draft"]
            if draft["project_id"] != project["project_id"]:
                continue
            permissions = _merge_permissions(permissions, {"plugins": [entry["identity"], draft["plugin"]],
                "secret_profiles": [draft["credential_profile"]], "network_hosts": draft["network_hosts"]})
    digest = hashlib.sha256(_json(permissions)).hexdigest()
    alias = "delivery-ceiling-" + digest[:16]
    _add(project["references"], lambda row: (row["kind"], row["alias"]), {
        "kind": "permission_ceiling", "alias": alias, "reference": triage._reference(alias, digest=digest),
        "metadata": {"node_kinds": [], "signal_purposes": [], "enforcement": [],
                     "compatible_choices": [], "permission_declarations": permissions}})


def stage(root: Path, directory: Path, installed: dict, project_id: str, repository_id: str, namespace, native: Path, client,
          packs: list[TestPack], write_directories: list[str], *,
          publication: tuple[str, str, str] | None = None) -> dict[str, Path]:
    """Prepare additive immutable inputs for the ordinary installation updater."""
    import configure_local_stand_agent as agent
    import configure_local_stand_server as server
    if any(not re.fullmatch(r"[a-zA-Z0-9_][a-zA-Z0-9_.-]*(/[a-zA-Z0-9_][a-zA-Z0-9_.-]*)*",
                                                    path) for path in write_directories):
        raise ValueError("Delivery write directories must be explicit repository-relative paths")
    source = installed["factory_profiles"]
    catalog = state.load_json_object(state.read_regular_file(source, "Factory profile catalog",
        max_bytes=server.MAX_FACTORY_PROFILE_BYTES, modes=frozenset({0o600})), "Factory profile catalog")
    project, = (row for row in catalog["projects"] if row["project_id"] == project_id)
    runtimes = project["role_profiles"]
    image = triage.prepare_guest_image(root, native, delivery=True)
    octa = json.loads((native / "job-spec-policy.json").read_text())["octa"]
    installed_plugin = triage._reference("codex", octa["version"], octa["plugin_digests"]["codex"])
    current_authors = [row for row in catalog["codex"] if row.get("protected_tests")
        and row["toolchain"].get("plugin") == installed_plugin
        and row["build_configuration"]["project_id"] == project_id
        and client.get(f"/api/v1/build-configurations/{row['build_configuration']['id']}/versions/"
                       f"{row['build_configuration']['version']}")["definition"]["runtime"]["immutable_image"] == image]
    if not (packs or write_directories or publication) and (
            {"delivery-reviewer", "delivery-devops", TEST_AUTHOR_RUNTIME, TEST_VERIFIER_RUNTIME,
             TESTER_PREPARE_MODE, TESTER_ANALYZE_MODE} <= runtimes.keys()
            and {"delivery-coder", "delivery-coder-readonly"} & runtimes.keys()
            and any(row["kind"] == "permission_ceiling" and row["alias"].startswith("delivery-ceiling-")
                    for row in project["references"])
            and (root / "agent-validation/worker.json").is_file()
            and any(row.get("execution_mode") == "scoped_validation"
                    and row.get("instructions") == TEST_VERIFIER_INSTRUCTIONS
                    and row["toolchain"].get("plugin") == installed_plugin
                    and row["build_configuration"]["project_id"] == project_id
                    and row["project_permissions"].get("resources", {}).get("cpu_millis", 0)
                        >= SCOPED_TESTER_CPU_MILLIS
                    and client.get(f"/api/v1/build-configurations/{row['build_configuration']['id']}/versions/"
                                   f"{row['build_configuration']['version']}")["definition"]["runtime"].get("cpu_millis", 0)
                        >= SCOPED_TESTER_CPU_MILLIS for row in catalog["codex"])
            and all(any(choice["kind"] == "node_capability" and choice.get("reference") == row["capture"]["tool"]
                        for choice in project["references"])
                    for row in catalog["codex"] if row.get("capture")
                    and row["build_configuration"]["project_id"] == project_id)
            and any(_standard_verifiers_current(catalog, project_id, row["toolchain"]["model"])
                    for row in current_authors)):
        return {}
    agent_policy = agent.read_factory_agent_profile(installed["factory_agent_profile"])
    install_controller(root, agent_policy)
    capture = _capture_policy(root, agent_policy)
    catalog = with_bootstrap_compiler(catalog, project, root)
    project, = (row for row in catalog["projects"] if row["project_id"] == project_id)
    runtimes = project["role_profiles"]
    ceiling = runtimes["triage-runtime"]["ceiling"]
    template, = (row for row in catalog["codex"] if row["toolchain"] == ceiling["toolchain"]
                 and row["build_configuration"] == ceiling["build_configuration"] and row["credential_purpose"] == "evaluator")
    import factory_delivery_worker
    original = template["build_configuration"]
    original_definition = client.get(f"/api/v1/build-configurations/{original['id']}/versions/{original['version']}")["definition"]
    verification_pool = factory_delivery_worker.prepare(root, client, project_id, repository_id, original_definition, namespace)
    plugins = [triage._reference(name, octa["version"], digest) for name, digest in sorted(octa["plugin_digests"].items())]
    worker = install_worker(client, project, template, image, namespace, repository_id=repository_id)
    verification_worker = install_worker(client, project, template, image, namespace,
                                        repository_id=repository_id, pool_id=verification_pool)
    if packs:
        author = install_worker(client, project, template, image, namespace)
    scoped_worker = install_worker(client, project, template, image, namespace, cpu_millis=SCOPED_TESTER_CPU_MILLIS)
    catalog = delivery_models(catalog, project_id, capture, write_directories, code_build=worker["build_configuration"],
                              scoped_build=scoped_worker["build_configuration"], plugin=installed_plugin)
    project, = (row for row in catalog["projects"] if row["project_id"] == project_id)
    tester, = (row for row in catalog["codex"] if row.get("protected_tests")
               and row["build_configuration"] == worker["build_configuration"]
               and row["toolchain"] == project["role_profiles"][TESTER_PREPARE_MODE]["ceiling"]["toolchain"])
    install_test_commands(catalog, project, verification_worker, plugins, capture, None, author=tester)
    for pack in packs:
        install_test_commands(catalog, project, verification_worker, plugins, capture, pack, author=author)
    _extend_agent(agent_policy, catalog, project, plugins)
    result = {}
    if publication is not None:
        repository_id, branch, token = publication
        repository = client.get(f"/api/v1/repositories/{repository_id}/versions/1")
        plugin, git = _packaged_github()
        result["factory_delivery_bundle"] = stage_publication(directory, catalog, project, repository,
            branch, token, plugin, git, installed["factory_delivery_bundle"])
    install_access_ceiling(catalog, project)
    # Keep the complete prior installation input as bounded audit evidence.
    # Historical Server profiles/Flows are never rewritten to a different plugin.
    retained = _json(catalog)
    triage._publish(root / "evidence" / ("factory-runtime-catalog-" + hashlib.sha256(retained).hexdigest() + ".json"), catalog)
    catalog = current_runtime_catalog(catalog, plugins)
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


def publish_roles(client, project: str, namespace, catalog_path: Path) -> None:
    """Publish reusable roles with fixed initial keys through the Project owner."""
    import uuid
    import configure_local_stand_server as server
    catalog = state.load_json_object(state.read_regular_file(catalog_path, "Factory profile catalog",
        max_bytes=server.MAX_FACTORY_PROFILE_BYTES, modes=frozenset({0o600})), "Factory profile catalog")
    entry, = (row for row in catalog["projects"] if row["project_id"] == project)
    runtimes = entry["role_profiles"]
    base = f"/api/v1/projects/{project}"
    declarations = (
        ("Coder", "delivery-coder", "Implement the explicitly assigned change within the approved source directories."),
        ("Coder", "delivery-coder-readonly", "Implement the explicitly assigned change within the approved source directories."),
        ("Tester", TESTER_PREPARE_MODE, "Explain selected test evidence and distinguish setup errors from confirmed failures."),
        ("Reviewer", "delivery-reviewer", "Review the selected candidate against the accepted requirements and record findings."),
        ("DevOps", "delivery-devops", "Prepare a short review title and summary from the selected accepted evidence."),
    )
    for name, kind, runtime, description in (
        (name, kind, alias, description) for name, kind, description in declarations
        for alias in sorted(runtimes) if alias == kind or
        name != "Tester" and alias.startswith(kind + "-") and re.fullmatch(r"[0-9a-f]{12}", alias[len(kind) + 1:])
    ):
        key = f"stand-delivery-v1-{name.lower()}"
        role_id = str(uuid.uuid5(namespace, key + "-role"))
        profile_key = key + {"delivery-coder-readonly": "-readonly", TEST_AUTHOR_RUNTIME: "-author-v2", TEST_VERIFIER_RUNTIME: "-verification"}.get(kind, "")
        if runtime != kind:
            profile_key += "-" + runtime[len(kind) + 1:]
        profile_id = str(uuid.uuid5(namespace, profile_key + "-profile"))
        ceiling = client.get(f"{base}/factory-profile-capabilities/{runtime}")["definition"]
        if name == "Tester":
            # One shared AI profile, with immutable earlier versions left available to retained Flows.
            profile_key = key + "-shared-" + hashlib.sha256(_json(ceiling)).hexdigest()[:12]
            profile_id = str(uuid.uuid5(namespace, profile_key + "-profile"))
        else:
            settings = {field: ceiling[field] for field in ("toolchain", "permissions")}
            client.post(f"{base}/factory-runtimes/{runtime}", f"{namespace}-{runtime}",
                {"expected_version": None, "definition": {**settings, "integrations": {}, "mcp_servers": {}}})
        role = client.post(f"{base}/factory-role-publications", f"{namespace}-{key}-role", {
            "role": {"id": role_id, "expected_current_version": None, "definition": {
                "name": name, "icon": "bot", "description": description,
                "instructions": ["Follow the reviewed step assignment. Use only the selected inputs and granted access."]}},
            "profiles": []})["role"]["reference"]
        suffix = {"delivery-coder-readonly": " (read-only)", TEST_AUTHOR_RUNTIME: " (test author v2)", TEST_VERIFIER_RUNTIME: " (verification)"}.get(kind, "")
        if runtime != kind:
            suffix += " · " + runtime[len(kind) + 1:]
        definition = {**ceiling, "name": f"{name} · Delivery{suffix}"}
        reviewed = client.post(f"{base}/factory-role-profiles/validate", f"{namespace}-{profile_key}-validate",
            {"id": profile_id, "role": role, "capability": runtime, "definition": definition})
        if reviewed["incompatibility"] is not None:
            raise ValueError(f"Delivery {name} profile is incompatible with its installed Runtime")
        client.post(f"{base}/factory-role-publications", f"{namespace}-{profile_key}-profile", {"profiles": [{
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
    state.ensure_private_directory(root)
    if inherited is not None:
        _snapshot(inherited, copy_to=root)
    plugin = triage._reference("github", digest=hashlib.sha256(plugin_bytes).hexdigest())
    legacy = root / "github/octacity-github"
    legacy_digest = None
    if legacy.exists():
        legacy_digest = hashlib.sha256(state.read_regular_file(legacy, "installed Delivery provider",
            max_bytes=256 * 1024**2, modes=frozenset({0o700}))).hexdigest()
    if legacy_digest is not None and legacy_digest != plugin["digest"]:
        # Move only the staged copy. Historical policy identities and provider
        # bytes remain intact under their own registry, without duplicate IDs.
        retained = root / legacy_digest
        state.ensure_private_directory(retained)
        for item in list(root.iterdir()):
            if item != retained:
                item.rename(retained / item.name)
        previous = "/run/octacity-delivery"
        for entry in catalog.get("delivery", []):
            if entry["adapter_root"] == previous:
                entry["adapter_root"] = previous + "/" + retained.name
                for policy in entry["policies"]:
                    policy["input_directory"] = entry["adapter_root"] + policy["input_directory"][len(previous):]
    registry = root if (root / "github").exists() or not any(root.iterdir()) else root / plugin["digest"]
    runtime_root = "/run/octacity-delivery" + ("/" + registry.name if registry != root else "")
    suffix = "-" + plugin["digest"][:16] if registry != root else ""
    adapter = registry / "github"
    credentials, inputs = adapter / "credentials", adapter / "inputs"
    for path in (root, registry, adapter, credentials, inputs):
        state.ensure_private_directory(path)
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
        "receipt_schema": receipt, "adapter_root": runtime_root,
        "policies": [{"reference": target, "draft": draft, "input_directory": runtime_root + "/github/inputs",
                      "review_links": "github"}], "timeout_milliseconds": 60000, "termination_grace_milliseconds": 1000}
    _add(catalog.setdefault("delivery", []), lambda item: item["identity"], profile)
    _choice(project, action["identity"] + suffix, action, node_kind="trusted_action")
    _choice(project, "delivery-github-provider" + suffix, plugin)
    _add(project["references"], lambda item: (item["kind"], item["alias"]),
         {"kind": "secret_profile", "alias": credential, "reference": triage._reference(credential)})
    return root
