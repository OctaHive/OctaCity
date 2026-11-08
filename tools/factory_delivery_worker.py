"""Independent ordinary Agent capacity for a waiting Delivery Tester.

The primary Pool stays single-slot. Only the verifier Build configuration uses
this Pool, so the waiting model cannot consume the verifier's worker.
"""
from __future__ import annotations
import copy
import json
import hashlib
from pathlib import Path

import bootstrap_local_stand as bootstrap
import local_stand_state as state
import verify_local_stand_integration as integration

PROCESS_NAME = "validation-agent"
AGENT_NAME = "local-stand-delivery-validation"


def configuration(root: Path, primary: dict) -> Path:
    """Render an independently registered slot from the same verified installation."""
    import configure_local_stand_agent as agent
    document = copy.deepcopy(primary)
    directory = root / "agent-validation"
    private_state = agent.microsandbox_state_root(directory)
    with state.private_umask():
        for path in (directory, directory / "work", directory / "cache", private_state):
            state.ensure_private_directory(path)
    document.update(agent_id=AGENT_NAME, credential_file=str(directory / "credential"),
                    work_root=str(directory / "work"), state_root=str(private_state))
    if document.get("cache") is not None:
        document["cache"]["root"] = str(directory / "cache")
    contents = ("\n".join(f"{key} = {agent._toml_value(value)}" for key, value in document.items()) + "\n").encode()
    path = directory / ("config-" + hashlib.sha256(contents).hexdigest() + ".toml")
    if not state.publish_exclusive_private_file(path, contents) and state.read_regular_file(
        path, "Delivery worker configuration", max_bytes=4 * 1024**2, modes=frozenset({0o600})
    ) != contents:
        raise ValueError("Delivery worker configuration differs from its verified inputs")
    return path


def start(root: Path, owner, plan, client, timeout_seconds: float) -> None:
    """Join the standard lifecycle and wait for the separate Pool registration."""
    import time
    import tomllib
    import local_stand_lifecycle as lifecycle
    import start_local_stand_agent as starter
    marker = root / "agent-validation/worker.json"
    if not marker.exists():
        return
    receipt = state.load_json_object(state.read_regular_file(marker, "Delivery worker receipt", max_bytes=4096,
        modes=frozenset({0o600})), "Delivery worker receipt")
    if set(receipt) != {"pool_id", "agent_name"} or receipt["agent_name"] != AGENT_NAME:
        raise starter.AgentStartupError("Delivery worker receipt is invalid")
    primary = tomllib.loads(state.read_regular_file(plan.configuration, "primary Agent configuration",
        max_bytes=4 * 1024**2, modes=frozenset({0o600})).decode())
    selected = configuration(root, primary)
    # Each Agent owns a distinct Microsandbox home. Reimport the same retained
    # archive there rather than rebuilding or substituting the selected image.
    import factory_triage_setup as triage
    triage.prepare_guest_image(root, plan.msb.parent.parent.parent, delivery=True,
        microsandbox_home=Path(tomllib.loads(selected.read_text())["state_root"]) / "microsandbox",
        execute=starter.subprocess.run)
    launched = False
    try:
        disposition = owner.reconcile_before_start(PROCESS_NAME)
        not_before = None
        if disposition is not lifecycle.StartDisposition.ALREADY_RUNNING:
            not_before = int(time.time() * 1000)
            environment = starter._child_environment()
            if plan.guest_agent is not None:
                environment["MSB_AGENTD_PATH"] = str(plan.guest_agent)
            owner.launch(PROCESS_NAME, [str(plan.agent), "--log-format", "json", "run", str(selected)],
                         environment=environment, working_directory=root)
            launched = True
        owner.wait_until_ready(PROCESS_NAME,
            starter.RegistrationObserver(client, not_before, require_factory=True, name=AGENT_NAME,
                                         pool_id=receipt["pool_id"]).registered,
            timeout_seconds=timeout_seconds, poll_interval_seconds=0.25)
    except (starter.AgentStartupError, lifecycle.LifecycleError):
        if launched:
            owner.shutdown(PROCESS_NAME, graceful_timeout_seconds=starter.SHUTDOWN_GRACE_SECONDS,
                           forced_timeout_seconds=starter.SHUTDOWN_FORCE_SECONDS)
        raise


def prepare(root: Path, client, project: str, repository: str, definition: dict, namespace) -> str:
    """Install a distinct enrollment and extend only the known stand seed policy.

    The version precondition rejects a concurrently edited Project policy.
    Replays return the original publication, preserving later operator changes.
    """
    request = copy.deepcopy(bootstrap.LOCAL_POOL_REQUEST)
    request["name"] += "-delivery-validation"
    pool = client.post(bootstrap.POOL_PATH, f"{namespace}-validation-pool", request)["resource"]
    if pool["name"] != request["name"] or pool["definition"] != request["definition"]:
        raise ValueError("Delivery validation Pool differs from the reviewed single-slot policy")
    pool_id, version = pool["id"], pool["version"]
    policy = json.loads((Path(__file__).resolve().parents[1] / "deployment/local-stand/factory-triage.json").read_text())
    replace = lambda value: {"mode": "replace", "value": value}
    client.post(f"/api/v1/projects/{project}/policy-versions", f"{namespace}-delivery-validation-capacity-v1", {"policy": {
        "pools": replace(sorted(set(definition["allowed_pools"]) | {pool_id})),
        "repositories": replace([repository]), "secret_profiles": replace([]), "identity_profiles": replace([]),
        "runtimes": replace([definition["runtime"]["class"]]), "execution_targets": replace([integration._execution_target()]),
        "cache": replace({"namespaces": [], "read": False, "write": False, "max_bytes": 0}),
        "artifacts": replace(definition["artifacts"]), "retention": replace(policy["retention"]),
        "concurrency": replace({"active_builds": 2, "active_jobs": 2}),
    }}, expected_version=1)
    directory = root / "agent-validation"
    with state.private_umask():
        state.ensure_private_directory(directory)
    credential = directory / "credential"
    if bootstrap._existing_agent_credential(credential) is None:
        document = client.post(bootstrap.ENROLLMENT_PATH, f"{namespace}-validation-enrollment", {
            "pool_id": pool_id, "pool_version": version,
            "expected_platform": {"operating_system": "macos", "architecture": "arm64"},
        })
        token = bootstrap._enrollment(document, pool_id, version)
        state.publish_exclusive_private_file(credential, (token + "\n").encode())
    marker = {"pool_id": pool_id, "agent_name": AGENT_NAME}
    path = directory / "worker.json"
    encoded = json.dumps(marker, sort_keys=True).encode()
    if not state.publish_exclusive_private_file(path, encoded) and state.read_regular_file(
        path, "Delivery worker receipt", max_bytes=4096, modes=frozenset({0o600})
    ) != encoded:
        raise ValueError("Delivery worker receipt differs from its original Pool")
    return pool_id
