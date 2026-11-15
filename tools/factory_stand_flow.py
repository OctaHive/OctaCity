"""Resolve the configured stand example through ordinary read-only Factory authoring."""
from __future__ import annotations

import copy
import json
from pathlib import Path
import uuid

POLICY = Path(__file__).resolve().parents[1] / "deployment/local-stand/factory-triage.json"


def completion_document() -> dict:
    """Read the example's configured process criteria from the reviewed repository assets."""
    repository = Path(__file__).resolve().parents[1]
    path = (repository / json.loads(POLICY.read_text())["completion_document"]).resolve()
    if not path.is_relative_to(repository):
        raise ValueError("example completion document must belong to the repository")
    return json.loads(path.read_text())


def prepare_project_fields(client, project: str, namespace) -> None:
    """Seed the configured example definitions once; published operator catalogs are authoritative."""
    path = f"/api/v1/projects/{project}/issue-property-definitions"
    catalog = client.get(path)
    if catalog["version"] != 0:
        return
    configured = json.loads(POLICY.read_text())["project_fields"]
    definition = {**catalog["definition"], **configured}
    key = uuid.uuid5(namespace, "example-fields/" + json.dumps(definition, sort_keys=True))
    client.post(path, str(key), {"expected_version": 0, "definition": definition})


def flow_draft(client, project: str, selection: dict, namespace, criteria: dict) -> dict:
    """The server derives forms and bindings; installation only reviews the resulting draft."""
    if selection.get("effects"):
        raise ValueError("stand reporting requires separately reviewed ordinary action settings")
    policy = json.loads(POLICY.read_text())
    configured = policy["example_preset"]
    base = f"/api/v1/projects/{project}/factory-presets"
    page = client.get(base)
    preset, = [item["reference"] for item in page["items"]
        if all(item["reference"][key] == value for key, value in configured.items())]
    request = {"preset": preset, "flow_id": str(uuid.uuid5(namespace,
            f"flow/{configured['identity']}/manual-v1")), "flow_only": True,
        "profiles": {alias: {key: entry[key] for key in ("role", "profile")}
            for alias, entry in selection["profiles"].items()},
        "context": copy.deepcopy(selection["context"]), "execution": copy.deepcopy(selection["execution"])}
    resolved = client.post(base + "/resolve", f"{namespace}-manual-v1-template", request)
    if resolved["diagnostics"] or resolved.get("flow") is None or resolved.get("dependencies"):
        raise ValueError("stand template requires review of its explicit dependencies before publication")
    flow = copy.deepcopy(resolved["flow"])
    # The template supplies the graph; installation retains its reviewed ceilings.
    flow["limits"] = {**policy["flow_limits"],
        "budget": copy.deepcopy(selection["execution"]["budget"]),
        "permissions": copy.deepcopy(selection["execution"]["permissions"])}
    flow["root"]["process"] = {"criteria": {"artifact": criteria, "recipients": list(selection["profiles"])}}
    return flow
