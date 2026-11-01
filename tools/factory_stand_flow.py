"""Author the opt-in stand example with ordinary public Factory contracts.

Shared reviewed schema/routing fragments remain data. Only the Server normalizes
schemas, evaluator policies and exact installed execution bindings. This module
owns neither retained definitions nor runtime state, and is not a prerequisite
of the blank Console journey.
"""
from __future__ import annotations

import copy
import json
from pathlib import Path


CONFIG = Path(__file__).resolve().parents[1] / "config/factory"
EXAMPLE = CONFIG.parents[1] / "deployment/local-stand"
MAX_FRAGMENT_EXPANSION_DEPTH = 64


def _read(path: str) -> dict:
    return json.loads((CONFIG / path).read_text())


def _shape(value, schemas: dict, depth: int = 0):
    """Inline this fixed example's named shape fragments before public validation."""
    if depth > MAX_FRAGMENT_EXPANSION_DEPTH:
        raise ValueError("stand schema fragments exceed their expansion bound")
    if isinstance(value, dict):
        if value.get("type") == "reference":
            return _shape(schemas[value["schema"]]["shape"], schemas, depth + 1)
        return {key: _shape(item, schemas, depth + 1) for key, item in value.items()}
    if isinstance(value, list):
        return [_shape(item, schemas, depth + 1) for item in value]
    return value


def flow_draft(client, project: str, selection: dict, namespace, criteria: dict) -> dict:
    """Compose this explicit four-step example; no preset or prior Flow is read."""
    if selection.get("effects"):
        raise ValueError("stand reporting requires separately reviewed ordinary action settings")
    schemas = {}
    for path in ("flows/base-flow/schemas/artifact_reference.json", "flows/base-flow/schemas/work.json",
                 "presets/triage/inputs-v1.json", "presets/triage/eligibility-results-v1.json",
                 "presets/triage/classification-results-v1.json"):
        schemas.update(_read(path)["schemas"])
    normalized = {alias: client.post("/api/v1/flow-data-schemas/validate", f"{namespace}-manual-v1-schema-{alias}",
        {"project_id": project, "identity": schema["identity"], "version": schema["version"],
         "shape": _shape(schema["shape"], schemas)}) for alias, schema in schemas.items()}
    tasks = json.loads((EXAMPLE / "factory-triage-tasks.json").read_text())
    deny = _read("presets/triage/v1.json")["execution"]["permissions"]
    nodes = []
    for stage in ("eligibility", "eligibility_gate", "classification", "classification_gate"):
        node = _read(f"presets/triage/{stage.replace('_', '-')}-v1.json")["nodes"][0]
        node["input_schema"] = normalized[node["input_schema"]]["reference"]
        node["outcomes"] = [{**outcome, "schema": normalized[outcome["schema"]]["reference"]}
                            for outcome in node["outcomes"]]
        node["permissions"] = copy.deepcopy(deny)
        node["code"] = {"kind": "none"}
        if "gate" in node:
            node["gate"] = client.post(f"/api/v1/projects/{project}/factory-gate-programs/validate",
                f"{namespace}-manual-v1-gate-{stage}", {"program": node["gate"]})
        else:
            selected = selection["profiles"][stage]
            ref = selected["profile"]
            profile = client.get(f"/api/v1/projects/{project}/factory-role-profiles/{ref['id']}/versions/{ref['version']}")
            if profile["reference"] != ref or profile["role"] != selected["role"]:
                raise ValueError("stand selected Profile identity changed")
            worker = profile["definition"]["build_configuration"]
            configuration = client.get(
                f"/api/v1/build-configurations/{worker['id']}/versions/{worker['version']}")
            if configuration["definition"].get("source") != {"kind": "source_free_v1"}:
                raise ValueError("stand requires an exact source-free worker")
            outcome, = node["outcomes"]
            binding = client.post(f"/api/v1/projects/{project}/factory-execution-bindings/resolve",
                f"{namespace}-manual-v1-binding-{stage}", {"profile": ref,
                    "capability": selected["capability"], "result_outcome": outcome["key"]})
            if binding["incompatibility"] is not None or binding["build"] is None:
                raise ValueError("stand selected execution binding is unavailable")
            node.pop("build_profile")
            node.update(task=tasks[stage], budget=profile["definition"]["budget"],
                permissions=profile["definition"]["permissions"], profile_selection={
                    "role": selected["role"], "candidates": {"selected": {"profile": profile, "build": binding["build"]}},
                    "policy": {"mode": "manual", "candidate": "selected"}, "failure": {"kind": "manual_selection"},
                    "selection_budget": {"attempts": 0, "elapsed_millis": 0, "tokens": 0, "cost_micro_units": 0, "output_bytes": 0}})
            context = selection["context"][stage]
            documents = {"project-goals": context["goals"], **context.get("documents", {})}
            node["context_binding"] = {"work_task": "task", "entries": {
                name: {"source_kind": "specification", "source": {"value": {"mapping": "constant",
                       "value": {"kind": "artifact", "value": artifact}}}} for name, artifact in documents.items()}}
        nodes.append(node)
    routes = _read("presets/triage/routing-v1.json")
    policy = json.loads((EXAMPLE / "factory-triage.json").read_text())
    return {"root": {"input_schema": normalized["project_input"]["reference"], "entry": "eligibility",
        "nodes": nodes, "transitions": routes["transitions"] + _read("presets/triage/direct-classification-v1.json")["transitions"],
        "terminals": [{"key": key, "schema": normalized[schema]["reference"]} for key, schema in routes["terminals"].items()],
        "context_projections": [], "data_projections": [], "code_sources": {},
        "process": {"criteria": {"artifact": criteria, "recipients": list(tasks)}}, "execution": copy.deepcopy(selection["execution"])},
        "data_schemas": list(normalized.values()), "pools": {},
        "limits": {**policy["flow_limits"], "budget": selection["execution"]["budget"],
                   "permissions": selection["execution"]["permissions"]}}
