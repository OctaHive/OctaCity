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
import time
import uuid
from urllib.parse import urlencode

import configure_local_stand_agent as agent
import factory_local_runtime as runtime
import local_stand_state as state
import local_stand_build_inputs as build_inputs

POLICY = Path(__file__).resolve().parents[1] / "deployment/local-stand/factory-triage.json"


def prepare_guest_image(root: Path, native: Path, *, execute=subprocess.run) -> str:
    """Qualify an immutable guest with the public stand CA in its normal trust store.

    Only a public certificate enters the build context. Model credentials,
    gateway keys and workstation configuration never enter image layers.
    """
    inputs = build_inputs.load_build_inputs(POLICY.parent / "inputs.json", POLICY.parents[2])
    base = inputs["native"]["microsandbox"]["guest_image"]
    certificate = state.read_regular_file(root / "pki/local-ca.pem", "guest trust anchor",
        max_bytes=65536, modes=frozenset({0o644}))
    recipe = (f"FROM {base}\nCOPY local-ca.pem /etc/pki/ca-trust/source/anchors/octacity-local-ca.pem\n"
              "RUN update-ca-trust\n")
    identity = {"base": base, "ca_sha256": hashlib.sha256(certificate).hexdigest(),
                "recipe_sha256": hashlib.sha256(recipe.encode()).hexdigest()}
    work = root / "triage-guest"
    _publish(work / "inputs.json", identity)
    home = agent.microsandbox_state_root(root) / "microsandbox"
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
            if retained["reference"] == "local.octacity/factory-triage@" + inspect(retained["reference"]):
                return retained["reference"]
        except subprocess.CalledProcessError:
            # A rebuild can produce another image digest. Recovery must keep the
            # immutable reference already pinned by ordinary Configurations.
            descriptor, _ = state.open_regular_file(archive, "retained triage guest archive", max_bytes=2 * 1024**3,
                modes=frozenset({0o600, 0o644}))
            os.close(descriptor)
            run([msb, "image", "load", "--input", str(archive), "--tag", retained["reference"]])
            if retained["reference"] != "local.octacity/factory-triage@" + inspect(retained["reference"]):
                raise ValueError("restored triage guest differs from its retained identity")
            return retained["reference"]
    context = work / "context"
    state.ensure_private_directory(context)
    (context / "Dockerfile").write_text(recipe)
    (context / "local-ca.pem").write_bytes(certificate)
    tag = "local.octacity/factory-triage:" + hashlib.sha256(json.dumps(identity, sort_keys=True).encode()).hexdigest()
    run(["docker", "build", "--network", "none", "--platform=linux/arm64", "--tag", tag, str(context)])
    if archive.is_symlink() or archive.exists() and not archive.is_file():
        raise ValueError("triage guest archive path is invalid")
    run(["docker", "image", "save", "--output", str(archive), tag])
    run([msb, "image", "load", "--input", str(archive), "--tag", tag])
    reference = "local.octacity/factory-triage@" + inspect(tag)
    run([msb, "image", "load", "--input", str(archive), "--tag", reference])
    if reference != "local.octacity/factory-triage@" + inspect(reference):
        raise ValueError("triage guest import changed image identity")
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


def preset_selection(client, resources: dict, profiles: dict, context: dict,
                     connection: dict, namespace: uuid.UUID, permissions: dict) -> dict:
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
    return {"profiles": profiles, "context": {stage: copy.deepcopy(context) for stage in profiles},
        "repository": {"project_id": project, "id": resources["repository"], "version": 1,
            "definition_digest": repository["definition_digest"]},
        "execution": {"budget": budget, "permissions": permissions, "max_active_nodes": 1},
        "configuration": {"admission_policy": "triage-admission", "permission_ceiling": "triage-ceiling",
            "wip_limits": {"max_active_runs": 1, "max_active_nodes": 1}, "hard_budget": budget}, "effects": {},
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
        if state.read_regular_file(path, "triage installation", max_bytes=131072,
                modes=frozenset({0o600})) != contents:
            raise ValueError("triage installation differs; use the explicit installation updater")


def _reference(identity: str, version: str = "v1", digest: str | None = None) -> dict:
    return {"identity": identity, "version": version,
            "digest": digest or hashlib.sha256(identity.encode()).hexdigest()}


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
    permissions.update(plugins=[plugin], executables=[executable],
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
        "budget": native["policy"]["budget"], "execution": {"cli_arguments": ["--ephemeral", "--skip-git-repo-check"],
            "environment": {"ISSUE_MCP_TOKEN": {"kind": "secret", "reference": logical}}}}
    references = [{"kind": "node_capability", "alias": pin["identity"], "reference": pin}
        for pin in toolchain.values()]
    references += [{"kind": kind, "alias": name, "reference": _reference(name)} for kind, name in
        (("admission_policy", "triage-admission"), ("permission_ceiling", "triage-ceiling"),
         ("secret_profile", "triage-model"), ("secret_profile", "triage-mcp"))]
    references.append({"kind": "node_capability", "alias": "triage-runtime", "reference": executable})
    references.append({"kind": "node_capability", "alias": policy["mcp_evidence_verifier"]["identity"],
                       "reference": policy["mcp_evidence_verifier"]})
    for reference in references:
        if reference["kind"] == "permission_ceiling":
            reference["metadata"] = {"node_kinds": [], "signal_purposes": [], "enforcement": [],
                "compatible_choices": [], "permission_declarations": permissions}
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
    contents = ("\n".join(f"{key} = {agent._toml_value(value)}" for key, value in agent_policy.items()) + "\n").encode()
    if not state.publish_exclusive_private_file(agent_profile, contents):
        if state.read_regular_file(agent_profile, "triage Agent policy", max_bytes=262144, modes=frozenset({0o600})) != contents:
            raise ValueError("triage Agent installation differs; use the explicit installation updater")
    return {"profiles": str(profiles), "agent_profile": str(agent_profile), "issue_mcp_readers": str(readers),
        "runtime_settings": str(root / "triage/runtime-settings.json")}
