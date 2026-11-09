"""Compose the default triage installation through existing private owners.

This module installs no OAuth client and owns no Run or step state. Native
credential refresh belongs to Codex/Agent; accepted content belongs to Server.
"""
from __future__ import annotations

import copy
import hashlib
import json
import os
from pathlib import Path
import secrets
import re
import subprocess
import tempfile
import time
import uuid
from urllib.parse import urlencode

import configure_local_stand_agent as agent
import configure_local_stand_server as server
import factory_local_runtime as runtime
import local_stand_state as state
import local_stand_build_inputs as build_inputs

POLICY = Path(__file__).resolve().parents[1] / "deployment/local-stand/factory-triage.json"
# Installed identities are retained for compatibility with earlier stand publications.
_INSTALLED_ADMISSION = "triage-admission"
_INSTALLED_ACCESS_DEFAULT = "triage-ceiling"


def prepare_guest_image(root: Path, native: Path, *, delivery: bool = False,
                        microsandbox_home: Path | None = None, execute=subprocess.run) -> str:
    """Qualify an immutable guest with the public stand CA in its normal trust store.

    Only a public certificate and reviewed runtime executables enter the build
    context. Model credentials, gateway keys and workstation configuration never
    enter image layers.
    """
    inputs = build_inputs.load_build_inputs(POLICY.parent / "inputs.json", POLICY.parents[2])
    base = inputs["native"]["microsandbox"]["guest_image"]
    certificate = state.read_regular_file(root / "pki/local-ca.pem", "guest trust anchor",
        max_bytes=65536, modes=frozenset({0o644}))
    recipe = (f"FROM {base}\nCOPY local-ca.pem /etc/pki/ca-trust/source/anchors/octacity-local-ca.pem\n"
              "RUN update-ca-trust\n")
    name = "triage"
    sandbox = None
    if delivery:
        name = "delivery"
        sandbox = json.loads((POLICY.parent / "factory-runtime-sandbox.json").read_text())
        if sandbox["version"] != json.loads(runtime.POLICY.read_text())["archive"]["version"]:
            raise ValueError("guest sandbox differs from the installed Codex runtime")
        node = inputs["images"]["node"]["reference"]
        build_tools = inputs["images"]["rust"]["reference"]
        recipe = (f"FROM {node} AS node\nFROM {build_tools} AS build-tools\n" + recipe
            + "COPY --from=node /usr/local/bin/node /usr/local/bin/node\n"
            + "COPY --from=node /usr/lib/aarch64-linux-gnu/libstdc++.so.6 /usr/local/lib/libstdc++.so.6\n"
            + "COPY --from=node /lib/aarch64-linux-gnu/libgcc_s.so.1 /usr/local/lib/libgcc_s.so.1\n"
            + "COPY --from=node /usr/bin/cmp /usr/bin/cmp\n"
            + "COPY --from=build-tools /usr/bin/git /usr/bin/git\n"
            + "COPY --from=build-tools /usr/lib/aarch64-linux-gnu/libpcre2-8.so.0 /usr/local/lib/libpcre2-8.so.0\n"
            + "COPY --chmod=0555 bwrap /usr/bin/bwrap\n"
            + "RUN ldconfig && /usr/local/bin/node --version && /usr/bin/bwrap --version && /usr/bin/cmp --version"
            + " && /usr/bin/git --version && /usr/bin/git diff --no-index /dev/null /dev/null\n")
    repository = f"local.octacity/factory-{name}"
    identity = {"base": base, "ca_sha256": hashlib.sha256(certificate).hexdigest(),
                "recipe_sha256": hashlib.sha256(recipe.encode()).hexdigest()}
    work = root / f"{name}-guest"
    if sandbox is not None:
        identity["sandbox_sha256"] = sandbox["sha256"]
        # Preserve archives already pinned by published Build configurations.
        # New dependency recipes get a separate immutable installation.
        work = work / hashlib.sha256(json.dumps(identity, sort_keys=True).encode()).hexdigest()
    _publish(work / "inputs.json", identity)
    home = microsandbox_home or agent.microsandbox_state_root(root) / "microsandbox"
    state.ensure_private_directory(home.parent)
    state.ensure_private_directory(home)
    environment = {"PATH": os.environ.get("PATH", os.defpath), "HOME": str(Path.home()), "MSB_HOME": str(home),
        "MSB_LIBKRUNFW_PATH": str(native / "microsandbox/lib" / inputs["native"]["microsandbox"]["firmware"])}
    msb = str(native / "microsandbox/bin/msb")

    def run(arguments):
        return execute(arguments, check=True, capture_output=True, text=True, timeout=180, env=environment)

    def inspect(reference):
        value = json.loads(run([msb, "image", "inspect", reference, "--format", "json"]).stdout)
        if (value.get("os") != "linux" or value.get("architecture") != "arm64"
                or not re.fullmatch(r"sha256:[0-9a-f]{64}", value.get("digest", ""))):
            raise ValueError("triage guest image identity is invalid")
        return value["digest"]

    receipt = work / "image.json"
    archive = work / "image.docker.tar"
    if receipt.exists():
        retained = state.load_json_object(state.read_regular_file(receipt, "triage guest image",
            max_bytes=4096, modes=frozenset({0o600})), "triage guest image")
        try:
            if retained["reference"] == repository + "@" + inspect(retained["reference"]):
                return retained["reference"]
        except subprocess.CalledProcessError:
            # A rebuild can produce another image digest. Recovery must keep the
            # immutable reference already pinned by ordinary Configurations.
            descriptor, _ = state.open_regular_file(archive, "retained triage guest archive", max_bytes=2 * 1024**3,
                modes=frozenset({0o600, 0o644}))
            os.close(descriptor)
            run([msb, "image", "load", "--input", str(archive), "--tag", retained["reference"]])
            if retained["reference"] != repository + "@" + inspect(retained["reference"]):
                raise ValueError("restored triage guest differs from its retained identity")
            return retained["reference"]
    context = work / "context"
    state.ensure_private_directory(context)
    (context / "Dockerfile").write_text(recipe)
    (context / "local-ca.pem").write_bytes(certificate)
    if sandbox is not None:
        with tempfile.TemporaryDirectory(prefix=".sandbox-", dir=work) as temporary:
            staging = Path(temporary)
            runtime.download(sandbox, staging / "sandbox.tar.gz")
            runtime.extract_tar_archive(staging / "sandbox.tar.gz", staging / "release",
                max_expanded_bytes=sandbox["expanded_max_bytes"],
                max_file_bytes=sandbox["file_max_bytes"], max_members=sandbox["member_max_count"])
            (context / "bwrap").write_bytes((staging / "release" / sandbox["executable"]).read_bytes())
    tag = repository + ":" + hashlib.sha256(json.dumps(identity, sort_keys=True).encode()).hexdigest()
    if archive.is_symlink() or archive.exists() and not archive.is_file():
        raise ValueError("triage guest archive path is invalid")
    run(["docker", "build", "--network", "none", "--platform=linux/arm64", "--tag", tag, str(context)])
    run(["docker", "image", "save", "--output", str(archive), tag])
    run([msb, "image", "load", "--input", str(archive), "--tag", tag])
    reference = repository + "@" + inspect(tag)
    run([msb, "image", "load", "--input", str(archive), "--tag", reference])
    if reference != repository + "@" + inspect(reference):
        raise ValueError(f"{name} guest import changed image identity")
    _publish(receipt, {"reference": reference})
    return reference


def preparation_mapping(project: str) -> dict:
    """Use the released process-completion document, preserving missing ticket details."""
    repository = POLICY.parents[2]
    source = json.loads((POLICY.parent / "source-policy.json").read_text())
    mapping = source["reader"]["preparation"]["mapping"]
    completion = json.loads((repository / "config/factory/presets/triage/completion-v2.json").read_text())
    document = {"kind": "draft", "project_id": str(uuid.UUID(project)), **completion}
    mapping.update(require_body=False, acceptance={"kind": "process_completion", "value": document})
    return mapping


def pipeline_selection(client, resources: dict, profiles: dict, context: dict,
                     connection: dict, namespace: uuid.UUID, permissions: dict, *, policy_defaults: dict | None) -> dict:
    """Pin current public Source choices; unsupported narrowing cannot fall back."""
    project = resources["project"]
    query = urlencode({"connection_id": connection["id"], "connection_version": connection["version"]})
    choices = client.get(f"/api/v1/projects/{project}/factory-source-choices?{query}")
    readers = [entry for entry in choices["items"] if entry["purpose"] == "task_read"]
    if len(readers) != 1:
        raise ValueError("triage requires exactly one reviewed Source reader")
    source, = readers
    if "none" not in source.get("predicate_label_presence", []) or "open" not in source.get("predicate_states", []):
        raise ValueError("triage Source does not support the reviewed open unlabeled scope")
    repository = client.get(f"/api/v1/repositories/{resources['repository']}/versions/1")
    budget = json.loads(POLICY.read_text())["flow_budget"]
    configuration = None
    if policy_defaults is not None:
        launch, access = (policy_defaults[key] for key in ("launch_policy", "access_profile"))
        if any(not pin or pin["project_id"] != project for pin in (launch, access)):
            raise ValueError("stand Configuration requires exact Project launch and access defaults")
        configuration = {"admission_policy": {"project_policy": launch},
            "permission_ceiling": {"project_policy": access},
            "wip_limits": {"max_active_runs": 1, "max_active_nodes": 1}, "hard_budget": budget}
    return {"profiles": profiles, "context": {stage: copy.deepcopy(context) for stage in profiles},
        "repository": {"project_id": project, "id": resources["repository"], "version": 1,
            "definition_digest": repository["definition_digest"]},
        "execution": {"budget": budget, "permissions": permissions, "max_active_nodes": 1},
        "configuration": configuration, "effects": {},
        "input": {"id": str(uuid.uuid5(namespace, "input/triage")), "source": source["binding"],
            "reporter": None, "reporting": None,
            "connection": {"id": choices["connection_id"], "version": choices["connection_version"],
                "access_version": choices["access_version"], "source_configuration": source["connection_configuration"],
                "reporter_configuration": None},
            "predicate": {"state": "open", "label_presence": "none", "all_of": [], "any_of": [], "none_of": []}}}


def _publish(path: Path, value: dict) -> None:
    state.ensure_private_directory(path.parent)
    contents = (json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False) + "\n").encode()
    if not state.publish_exclusive_private_file(path, contents):
        # An immutable replay may only match the bytes just generated.
        if state.read_regular_file(path, "triage installation", max_bytes=len(contents),
                modes=frozenset({0o600})) != contents:
            raise ValueError("triage installation differs; use the explicit installation updater")


def _reference(identity: str, version: str = "v1", digest: str | None = None) -> dict:
    return {"identity": identity, "version": version,
            "digest": digest or hashlib.sha256(identity.encode()).hexdigest()}


def provision_project_policies(client, project: str, namespace: uuid.UUID, catalog: Path,
                               retained: dict | None = None) -> dict:
    """Designate the reviewed installation ceiling and import only exact retained legacy pins."""
    legacy = [
        {"kind": kind, "reference": retained[field]}
        for kind, field in (("launch", "admission_policy"), ("access", "permission_ceiling"))
        if retained is not None and "project_policy" not in retained[field]
    ]
    installed = state.load_json_object(state.read_regular_file(catalog, "Factory profile catalog",
        max_bytes=server.MAX_FACTORY_PROFILE_BYTES, modes=frozenset({state.PRIVATE_FILE_MODE})),
        "Factory profile catalog")
    designated = [entry["reference"] for owner in installed["projects"] if owner["project_id"] == project
        for entry in owner["references"]
        if entry["kind"] == "permission_ceiling" and entry["alias"] == _INSTALLED_ACCESS_DEFAULT]
    if len(designated) > 1:
        raise ValueError("stand Factory access default is ambiguous")
    base = f"/api/v1/projects/{project}/factory-policies"
    request = {"designated_access": next(iter(designated), None), "legacy_references": legacy}
    key = uuid.uuid5(namespace, "factory-policies/" + json.dumps(request, sort_keys=True, separators=(",", ":")))
    receipt = client.post(f"{base}/provision", str(key), request)
    unresolved = receipt["unresolved"]
    if unresolved and receipt["disposition"] == "replayed":
        # The original receipt is historical. Only recheck references still missing.
        unresolved = [item for item in unresolved if client.post(
            f"{base}/legacy/resolve", str(uuid.uuid4()), item["legacy"])["mapping"] is None]
        if unresolved:
            receipt = client.post(f"{base}/provision", str(uuid.uuid4()), request)
            unresolved = receipt["unresolved"]
    if unresolved:
        pending = ", ".join(f"{item['legacy']['reference']['identity']} "
            f"({item['reason']})" for item in unresolved)
        raise ValueError(f"unresolved Factory policies: {pending}. Restore the exact installed declarations and retry.")
    original = receipt.get("provisioning", {}).get("standard_access")
    if original is not None:
        _name_triage_access(client, base, namespace, original)
        receipt["defaults"] = client.get(f"{base}/defaults")["defaults"]
    return receipt


def project_policies(client, base: str) -> list[dict]:
    """Read current policy heads without assuming a single catalog page."""
    records, cursor = [], None
    for _ in range(100):
        page = client.get(base + ("?" + urlencode({"after": json.dumps(cursor)}) if cursor else ""))
        records.extend(page["items"])
        following = page.get("next_cursor")
        if following is None:
            return records
        if following == cursor:
            raise ValueError("Factory policy catalog repeated its cursor")
        cursor = following
    raise ValueError("Factory policy catalog exceeded its page limit")


def publish_access_profile(client, base: str, namespace: uuid.UUID, draft: dict) -> dict:
    """Validate and publish through the ordinary versioned policy owner."""
    key = str(uuid.uuid5(namespace, "access-profile/" + json.dumps(draft, sort_keys=True)))
    client.post(f"{base}/validate", key, draft)
    return client.post(base, key, draft)["policy"]["policy"]["reference"]


def _name_triage_access(client, base: str, namespace: uuid.UUID, original: dict) -> None:
    current = next((row["reference"]["reference"] for row in project_policies(client, base)
        if row["reference"]["kind"] == "access" and row["reference"]["reference"]["id"] == original["id"]), None)
    if current is None:
        raise ValueError("provisioned triage access profile is unavailable")
    path = f"{base}/access/{current['id']}/versions/"
    record = client.get(path + str(current["version"]) + "?" + urlencode({"digest": current["digest"]}))["policy"]["policy"]
    if current == original and record["definition"]["name"] == "Standard":
        current = publish_access_profile(client, base, namespace, {"kind": "access", "id": current["id"],
            "expected_current": current, "definition": {**record["definition"], "name": "Triage"}})
    elif record["definition"]["name"] != "Triage":
        return
    else:
        previous = client.get(path + str(original["version"]) + "?" + urlencode({"digest": original["digest"]}))["policy"]["policy"]
        if record["definition"]["permissions"] != previous["definition"]["permissions"]:
            return
    defaults = client.get(f"{base}/defaults")["defaults"]
    if defaults is not None and defaults["access_profile"] == original and current != original:
        request = {"expected_version": defaults["version"], "launch_policy": defaults["launch_policy"],
            "access_profile": current}
        key = str(uuid.uuid5(namespace, "triage-access-default/" + json.dumps(request, sort_keys=True)))
        client.post(f"{base}/defaults", key, request)


def _issue_reader(root: Path, project: str, lifetime: int) -> tuple[Path, Path]:
    """Retain the explicitly scoped credential; restart cannot extend its lifetime."""
    metadata = root / "triage/issue-mcp-readers.json"
    token_file = root / "secrets/triage-issue-mcp-token"
    variables = root / "secrets/triage-issue-mcp.json"
    if metadata.exists() or metadata.is_symlink():
        installed = state.load_json_object(state.read_regular_file(metadata, "triage MCP scope",
            max_bytes=65536, modes=frozenset({0o600})), "triage MCP scope")
        reader, = installed["readers"]
        if reader["project_id"] != project or reader["credential_file"] != str(token_file) or "issue_ids" in reader:
            raise ValueError("triage MCP installation belongs to another scope")
    else:
        identity = "triage-" + uuid.UUID(project).hex
        if not token_file.exists():
            state.publish_exclusive_private_file(token_file, f"ocmcp.{identity}.{secrets.token_hex(32)}".encode())
        _publish(metadata, {"version": 1, "readers": [{"id": identity, "project_id": project,
            "credential_file": str(token_file), "expires_at_unix_ms": time.time_ns() // 1000000 + lifetime}]})
    token = state.read_regular_file(token_file, "triage MCP token", max_bytes=65536,
        modes=frozenset({0o600})).decode()
    _publish(variables, {"ISSUE_MCP_TOKEN": token})
    return metadata, variables


def _job_permissions(permissions: dict, argument_bytes: int) -> dict:
    """Translate installed domain declarations to the existing Agent wire shape.

    Both owners validate the result. This adapter grants no additional entries;
    protocol Any arguments use the reviewed installed V3 contract ceiling.
    """
    projected = permissions.copy()
    wire_reference = lambda reference: {"identity": reference["identity"], "version": reference["version"],
                                        "sha256": reference["digest"]}
    for category in ("plugins", "executables", "tools"):
        projected[category] = [wire_reference(reference) for reference in permissions[category]]
    projected["commands"] = [{"executable": wire_reference(command["executable"]), "arguments": [
        {"kind": "any", "max_bytes": argument_bytes} if argument["kind"] == "any" else argument.copy()
        for argument in command["arguments"]]} for command in permissions["commands"]]
    # V3 represents sets as strictly ordered arrays. Domain declarations are
    # normalized by Server; the independently installed Agent needs the same
    # protocol ordering (Exact precedes Any), without changing any grant.
    for category in ("plugins", "executables", "tools"):
        projected[category].sort(key=lambda reference: reference["identity"])
    projected["commands"].sort(key=lambda command: (
        tuple(command["executable"][key] for key in ("identity", "version", "sha256")),
        tuple((0, argument["value"]) if argument["kind"] == "exact" else (1, argument["max_bytes"])
              for argument in command["arguments"])))
    projected["mounts"] = sorted(permissions["mounts"], key=lambda mount: mount["root"])
    for category in ("network_hosts", "secret_profiles"):
        projected[category] = sorted(permissions[category])
    projected["outputs"] = {**permissions["outputs"], "kinds": sorted(permissions["outputs"]["kinds"])}
    return projected


def prepare_runtime(root: Path, native: dict, resources: dict, client) -> dict:
    """Create one reviewed native Runtime and independent Agent/MCP installation."""
    policy = json.loads(POLICY.read_text())
    root = root.expanduser().resolve()
    login = runtime.install_native_login(root)
    readers, mcp_variables = _issue_reader(root, resources["project"], policy["mcp_credential_lifetime_millis"])
    plugin = _reference("codex", native["octa"]["version"], native["octa"]["plugin_digests"]["codex"])
    executable = _reference("codex-cli", native["policy"]["archive"]["version"], native["sha256"])
    companions = native.get("companions", [])
    companion_references = [_reference(item["identity"], item["version"], item["sha256"]) for item in companions]
    toolchain = {"plugin": plugin, "executable": executable, "model": _reference(policy["model_identifier"])}
    published = client.get(f"/api/v1/build-configurations/{resources['configuration']}/versions/1")
    build = {name: published[name] for name in ("id", "project_id", "version")}
    # The ordinary REST definition is a JSON value, as in the packaged installer;
    # object members are sorted and Unicode is preserved before content SHA-256.
    build["definition_digest"] = hashlib.sha256(json.dumps(published["definition"],
        sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode()).hexdigest()
    exact = lambda value: {"kind": "exact", "value": value}
    arguments = [exact(value) for value in ("--ask-for-approval=never", "exec", "--json",
        "--sandbox=workspace-write", f"--model={policy['model_identifier']}")]
    arguments += [{"kind": "any"}]  # Compiler-owned protected output-schema path.
    arguments += [exact(value) for value in ("--ephemeral", "--skip-git-repo-check", "--ignore-user-config")]
    permissions = copy.deepcopy(native["policy"]["permissions"])
    outputs = policy["outputs"]
    permissions.update(plugins=[plugin], executables=[executable, *companion_references],
        commands=[{"executable": executable, "arguments": [exact("--version")]},
            {"executable": executable, "arguments": [*arguments, exact("-")]},
            {"executable": executable, "arguments": [*arguments, {"kind": "any"}, exact("-")]}],
        mounts=[{"root": "/octacity/protected", "mode": "read_only"},
            {"root": "/workspace/scratch", "mode": "read_write"}, {"root": "/workspace/output", "mode": "read_write"}],
        secret_profiles=["triage-model", "triage-mcp"], network_hosts=policy["network_hosts"],
        resources=policy["resources"], max_descendants=policy["resources"]["process_count"] - 1,
        outputs={"kinds": ["codex-run-result", "codex-run-provenance", "codex-run-trace"],
            **{f"max_{key}": outputs[key] for key in ("artifact_count", "artifact_bytes", "report_count", "report_bytes")}})
    port = int(os.environ.get("OCTACITY_LOCAL_STAND_MCP_PORT", "8444"))
    if not 1 <= port <= 65535 or port == 8443:
        raise ValueError("triage MCP listener port is invalid")
    server = {"transport": "http", "url": f"https://host.microsandbox.internal:{port}/mcp/issues",
        "bearer_token_env_var": "ISSUE_MCP_TOKEN", "enabled_tools": ["search_issues", "read_issue"]}
    logical = {"provider": "agent-private", "reference": "ISSUE_MCP_TOKEN"}
    authorization = {"credential_profile": "triage-mcp", "secret": logical, "environment": "ISSUE_MCP_TOKEN",
        "purpose": "mcp_read", "resource": _reference(f"project-{resources['project']}")}
    settings = {"toolchain": toolchain, "permissions": permissions, "integrations": {},
        "mcp_servers": {"issues": {"server": server, "authorization": authorization}}}
    ceiling = {"name": "GPT-6.1 Sol triage", "instructions": [], "toolchain": toolchain,
        "model_parameters": {}, "skills": {}, "context_requirements": ["task", "completion", "project-goals", "project-rules"],
        "build_configuration": build, "credential_purpose": "evaluator", "permissions": permissions,
        "budget": policy["stage_budget"], "execution": {"cli_arguments": ["--ephemeral", "--skip-git-repo-check"],
            "environment": {"ISSUE_MCP_TOKEN": {"kind": "secret", "reference": logical}}}}
    references = [{"kind": "node_capability", "alias": pin["identity"], "reference": pin}
        for pin in toolchain.values()]
    references += [{"kind":"node_capability", "alias":pin["identity"], "reference":pin} for pin in companion_references]
    references += [{"kind": kind, "alias": name, "reference": _reference(name)} for kind, name in
        (("admission_policy", _INSTALLED_ADMISSION),
         ("secret_profile", "triage-model"), ("secret_profile", "triage-mcp"))]
    references.append({"kind": "node_capability", "alias": "triage-runtime", "reference": executable})
    references.append({"kind": "node_capability", "alias": policy["mcp_evidence_verifier"]["identity"],
                       "reference": policy["mcp_evidence_verifier"]})
    permission_digest = hashlib.sha256(json.dumps(permissions, sort_keys=True, separators=(",", ":")).encode()).hexdigest()
    references.append({"kind": "permission_ceiling", "alias": _INSTALLED_ACCESS_DEFAULT,
        "reference": _reference(_INSTALLED_ACCESS_DEFAULT, digest=permission_digest),
        "metadata": {"node_kinds": [], "signal_purposes": [], "enforcement": [],
            "compatible_choices": [], "permission_declarations": permissions}})
    catalog = {"version": 1, "projects": [{"project_id": resources["project"], "references": references,
        "build_configurations": [{"alias": "triage-worker", "reference": build}],
        "role_profiles": {"triage-runtime": {"ceiling": ceiling, "parameters": native["policy"]["parameters"],
            "public_environment": {}, "enforcement": native["policy"]["enforcement"],
            "settings_support": {"mcp_transports": ["http"], "mcp_executables": {}, "authenticated_mcp": {
                "issues": {"identity": "issues", "version": "v1", "server": server, "authorization": authorization}}}}}}],
        "codex": [{"authentication": "native_login_file", "toolchain": toolchain, "build_configuration": build,
            "instructions": "Use only the frozen role, selected Context and protected result contract.",
            "credentials": {"model": "triage-author", "evaluator": "triage-model", "source": "triage-source", "delivery": "triage-delivery"},
            "credential_purpose": "evaluator", "usage_policy": {"kind": "subscription"},
            "pipeline_node": resources["pipeline_node"], "parameters": {}, "project_permissions": permissions,
            "local_permissions": {"grants": permissions, "enforceable": native["policy"]["enforcement"]}}]}
    profiles, agent_profile = root / "triage/factory-profiles.json", root / "triage/agent.toml"
    _publish(profiles, catalog)
    _publish(root / "triage/runtime-settings.json", settings)
    agent_policy = {"version": 1, "factory_permissions": _job_permissions(permissions, policy["job_command_argument_max_bytes"]),
        "allowed_network_hosts": policy["network_hosts"], "tool_executables": {"codex-cli": {
            "path": str(native["executable"]), "version": executable["version"], "platform": "linux-aarch64", "sha256": native["sha256"]}},
        "factory_credential_profiles": {"triage-model": login, "triage-mcp": str(mcp_variables)}}
    for item in companions:
        agent_policy["tool_executables"][item["identity"]] = {
            key:item[key] for key in ("path", "version", "platform", "sha256", "guest_filename")}
        agent_policy["tool_executables"][item["identity"]]["standalone"] = True
    contents = ("\n".join(f"{key} = {agent._toml_value(value)}" for key, value in agent_policy.items()) + "\n").encode()
    if not state.publish_exclusive_private_file(agent_profile, contents):
        if state.read_regular_file(agent_profile, "triage Agent policy", max_bytes=262144, modes=frozenset({0o600})) != contents:
            raise ValueError("triage Agent installation differs; use the explicit installation updater")
    return {"profiles": str(profiles), "agent_profile": str(agent_profile), "issue_mcp_readers": str(readers),
        "runtime_settings": str(root / "triage/runtime-settings.json")}
