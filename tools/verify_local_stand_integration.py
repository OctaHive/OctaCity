#!/usr/bin/env python3
"""Verify the running hybrid local stand through its public HTTPS gateway."""

from __future__ import annotations

import argparse
import hashlib
from http import client as http_client
import json
from pathlib import Path
import re
import ssl
import subprocess
import sys
import time
from typing import Callable, Sequence
from urllib import parse
import uuid

import bootstrap_local_stand as bootstrap
import configure_local_stand_agent as agent_config
import init_local_stand as initializer
import local_stand_state as state
import verify_local_stand_inputs as inputs


REPOSITORY = Path(__file__).resolve().parents[1]
AGENTS_PATH = "/api/v1/agents?limit=100"
POOLS_PATH = "/api/v1/agent-pools?limit=100"
FIXTURE_OCTAFILE = (
    "server/tests/octacity-release-harness/fixtures/release-matrix/Octafile.yml"
)
EXPECTED_ARTIFACT_NAME = "release-result"
EXPECTED_REPORT_NAME = "release-junit"
CACHE_NAMESPACE = "local-stand-integration"
EXPECTED_ARTIFACT_BYTES = 1024 * 1024
MAX_DOWNLOAD_BYTES = 4 * 1024 * 1024
DEFAULT_BUILD_TIMEOUT_SECONDS = 600
POLL_INTERVAL_SECONDS = 0.5
RECEIPT_MAX_BYTES = 64 * 1024
GIT_REVISION = re.compile(r"[0-9a-f]{40}")
GITHUB_COMPONENT = re.compile(r"[A-Za-z0-9_.-]+")


class IntegrationError(RuntimeError):
    """The running stand does not satisfy its integration contract."""


def _require_object(value: object, description: str) -> dict[str, object]:
    if not isinstance(value, dict):
        raise IntegrationError(f"{description} must be an object")
    return value


def _require_array(value: object, description: str) -> list[object]:
    if not isinstance(value, list):
        raise IntegrationError(f"{description} must be an array")
    return value


def _require_string(value: object, description: str) -> str:
    if not isinstance(value, str) or not value or value != value.strip():
        raise IntegrationError(f"{description} must be a non-empty trimmed string")
    return value


def _canonical_uuid(value: object, description: str) -> str:
    text = _require_string(value, description)
    try:
        parsed = uuid.UUID(text)
    except ValueError as cause:
        raise IntegrationError(f"{description} must be a canonical UUID") from cause
    if str(parsed) != text:
        raise IntegrationError(f"{description} must be a canonical UUID")
    return text


def _resource_id(response: dict[str, object], description: str) -> str:
    resource = _require_object(response.get("resource"), f"{description} resource")
    return _canonical_uuid(resource.get("id"), f"{description} identity")


def _validated_source_repository(value: str) -> str:
    parsed = parse.urlsplit(value)
    components = [component for component in parsed.path.split("/") if component]
    if (
        parsed.scheme != "https"
        or parsed.hostname != "github.com"
        or parsed.port is not None
        or parsed.username is not None
        or parsed.password is not None
        or parsed.query
        or parsed.fragment
        or len(components) != 2
    ):
        raise IntegrationError(
            "source repository must be an HTTPS GitHub owner/repository URL"
        )
    repository = components[1].removesuffix(".git")
    if (
        not repository
        or GITHUB_COMPONENT.fullmatch(components[0]) is None
        or GITHUB_COMPONENT.fullmatch(repository) is None
        or components[0] in {".", ".."}
        or repository in {".", ".."}
    ):
        raise IntegrationError("source repository path is invalid")
    return f"https://github.com/{components[0]}/{repository}.git"


def _validated_revision(value: str) -> str:
    if GIT_REVISION.fullmatch(value) is None:
        raise IntegrationError("source revision must be a lowercase Git SHA-1")
    return value


def _guest_image(repository: Path = REPOSITORY) -> str:
    document = inputs.load_manifest(repository / "deployment/local-stand/inputs.json")
    revision = inputs.require_match(
        _git_revision(repository), inputs.GIT_SHA1, "OctaCity revision"
    )
    tags = inputs.validate_manifest(document, revision)
    inputs.validate_repository_pins(document, tags, repository)
    native = inputs.require_object(document["native"], "native")
    microsandbox = inputs.require_object(native["microsandbox"], "native.microsandbox")
    return inputs.immutable_image(
        microsandbox["guest_image"], "native.microsandbox.guest_image"
    )


def _git_revision(repository: Path) -> str:
    result = subprocess.run(
        ["git", "-C", str(repository), "rev-parse", "HEAD"],
        check=True,
        capture_output=True,
        text=True,
        timeout=10,
    )
    return result.stdout.strip()


class LocalStandClient:
    """Bounded client for the trusted local management and object origins."""

    def __init__(self, root: Path, origin: str):
        ca = state.read_regular_file(
            root / "pki/local-ca.pem",
            "local stand CA certificate",
            max_bytes=64 * 1024,
            modes=frozenset({state.PUBLIC_FILE_MODE}),
        )
        self._transport = bootstrap.HttpsTransport(origin, ca)
        self._management = bootstrap.BootstrapClient(
            self._transport,
            bootstrap.RetryPolicy(
                attempts=5,
                timeout_seconds=5,
                initial_delay_seconds=0.2,
                maximum_delay_seconds=2,
            ),
        )
        try:
            self._ssl_context = ssl.create_default_context(cadata=ca.decode("ascii"))
        except (UnicodeDecodeError, ssl.SSLError) as cause:
            raise IntegrationError("local stand CA certificate is invalid") from cause

    def verify_ui_and_readiness(self) -> None:
        response = self._transport.request(
            "GET",
            "/",
            {
                "Accept": "text/html",
                "User-Agent": "octacity-local-stand-integration/1",
            },
            None,
            5,
        )
        content_type = response.headers.get("content-type", "").split(";", 1)[0]
        if (
            response.status != 200
            or content_type != "text/html"
            or b'<div id="root"></div>' not in response.body
        ):
            raise IntegrationError("gateway did not serve the built operator console")
        self._management.ready()

    def get(self, path: str) -> dict[str, object]:
        return self._management.get(path)

    def post(
        self, path: str, key: str, document: dict[str, object]
    ) -> dict[str, object]:
        return self._management.post(path, key, document)

    def post_without_body(self, path: str) -> dict[str, object]:
        response = self._transport.request(
            "POST",
            path,
            {
                "Accept": "application/json",
                "User-Agent": "octacity-local-stand-integration/1",
            },
            None,
            5,
        )
        if response.status != 200:
            raise IntegrationError(
                f"management download authorization returned HTTP {response.status}"
            )
        try:
            return state.load_json_object(response.body, "download authorization")
        except state.StateError as cause:
            raise IntegrationError(str(cause)) from cause

    def download(self, url: str) -> bytes:
        parsed = parse.urlsplit(url)
        if (
            parsed.scheme != "https"
            or parsed.hostname != "objects.localhost"
            or parsed.username is not None
            or parsed.password is not None
            or parsed.fragment
            or parsed.port != initializer.GATEWAY_HTTPS_PORT
            or not parsed.path.startswith("/")
            or parsed.path.startswith("//")
        ):
            raise IntegrationError("artifact download URL left the local object origin")
        target = parsed.path + (f"?{parsed.query}" if parsed.query else "")
        connection = http_client.HTTPSConnection(
            "objects.localhost", parsed.port, timeout=30, context=self._ssl_context
        )
        try:
            connection.request(
                "GET",
                target,
                headers={"User-Agent": "octacity-local-stand-integration/1"},
            )
            response = connection.getresponse()
            contents = response.read(MAX_DOWNLOAD_BYTES + 1)
        except (http_client.HTTPException, TimeoutError, OSError) as cause:
            raise IntegrationError("artifact download failed") from cause
        finally:
            connection.close()
        if response.status != 200:
            raise IntegrationError(f"artifact download returned HTTP {response.status}")
        if len(contents) > MAX_DOWNLOAD_BYTES:
            raise IntegrationError("artifact download exceeded its byte limit")
        return contents


def verify_registered_topology(client: LocalStandClient) -> tuple[str, str]:
    """Require one online Agent admitted by the exact local virtualization Pool."""

    pools = client.get(POOLS_PATH)
    pool_items = _require_array(pools.get("items"), "Agent Pool page items")
    matching_pools = [
        _require_object(item, "Agent Pool")
        for item in pool_items
        if isinstance(item, dict) and item.get("name") == bootstrap.LOCAL_POOL_REQUEST["name"]
    ]
    if len(matching_pools) != 1:
        raise IntegrationError("expected exactly one local stand Agent Pool")
    pool = matching_pools[0]
    definition = _require_object(pool.get("definition"), "Agent Pool definition")
    expected_definition = _require_object(
        bootstrap.LOCAL_POOL_REQUEST["definition"], "expected Agent Pool definition"
    )
    if definition != expected_definition:
        raise IntegrationError("local Agent Pool policy differs from the declared topology")
    pool_id = _canonical_uuid(pool.get("id"), "Agent Pool identity")

    agents = client.get(AGENTS_PATH)
    agent_items = _require_array(agents.get("items"), "Agent page items")
    matching_agents = [
        _require_object(item, "Agent")
        for item in agent_items
        if isinstance(item, dict) and item.get("name") == agent_config.LOCAL_AGENT_NAME
    ]
    if len(matching_agents) != 1:
        raise IntegrationError("expected exactly one local stand Agent")
    agent = matching_agents[0]
    inventory = _require_object(agent.get("inventory"), "Agent inventory")
    capacity = _require_object(agent.get("capacity"), "Agent capacity")
    if (
        agent.get("status") != "online"
        or agent.get("pool_id") != pool_id
        or inventory.get("host_platform") != agent_config.LOCAL_HOST_PLATFORM
        or inventory.get("labels") != agent_config.LOCAL_AGENT_LABELS
        or capacity.get("virtualization_available") is not True
    ):
        raise IntegrationError(
            "registered Agent does not provide the declared macOS ARM64 Microsandbox target"
        )
    return pool_id, _canonical_uuid(agent.get("id"), "Agent identity")


def _execution_target() -> dict[str, object]:
    definition = _require_object(
        bootstrap.LOCAL_POOL_REQUEST["definition"], "local Agent Pool definition"
    )
    admission = _require_object(definition["admission_policy"], "Pool admission policy")
    targets = _require_array(admission["execution_targets"], "Pool execution targets")
    if len(targets) != 1:
        raise IntegrationError("local Agent Pool must declare exactly one execution target")
    return _require_object(targets[0], "Pool execution target")


def _create_resources(
    client: LocalStandClient,
    *,
    pool_id: str,
    source_repository: str,
    source_revision: str,
    guest_image: str,
    scope: str,
) -> tuple[str, str]:
    project_id = _resource_id(
        client.post(
            "/api/v1/projects",
            f"{scope}-project",
            {"parent_id": None, "name": f"local-stand-{scope}"},
        ),
        "Project",
    )
    repository_id = _resource_id(
        client.post(
            "/api/v1/repositories",
            f"{scope}-repository",
            {
                "project_id": project_id,
                "name": "local-stand-integration",
                "definition": {
                    "vcs_integration_id": str(uuid.uuid4()),
                    "repository_locator": source_repository,
                    "selection": {
                        "allowed_references": [],
                        "default_reference": None,
                        "allow_exact_revision": True,
                    },
                },
            },
        ),
        "Repository",
    )
    pipeline_id = _resource_id(
        client.post(
            "/api/v1/pipelines",
            f"{scope}-pipeline",
            {
                "project_id": project_id,
                "name": "local-stand-integration",
                "dag": {
                    "nodes": [
                        {
                            "id": "cacheable",
                            "name": "cacheable",
                            "dependency_policy": "all_succeeded",
                            "required_capabilities": ["shell"],
                            "execution": {
                                "octafile": FIXTURE_OCTAFILE,
                                "commands": ["cacheable"],
                                "parallel": False,
                                "failfast": True,
                            },
                        }
                    ],
                    "edges": [],
                },
            },
        ),
        "Pipeline",
    )
    configuration_id = _resource_id(
        client.post(
            "/api/v1/build-configurations",
            f"{scope}-configuration",
            {
                "project_id": project_id,
                "name": "local-stand-integration",
                "definition": {
                    "enabled": True,
                    "job_concurrency_limit": 1,
                    "repository_id": repository_id,
                    "repository_version": 1,
                    "pipeline_id": pipeline_id,
                    "pipeline_version": 1,
                    "parameters": {"parameters": {}, "deny_unknown": True},
                    "triggers": ["manual"],
                    "agent_requirements": {
                        "capabilities": ["shell"],
                        "labels": agent_config.LOCAL_AGENT_LABELS,
                        "minimum_cpu_millis": 1000,
                        "minimum_memory_bytes": 512 * 1024 * 1024,
                        "minimum_disk_bytes": 1024 * 1024 * 1024,
                    },
                    "allowed_pools": [pool_id],
                    "runtime": {
                        "class": "virtualization",
                        "operating_system": "linux",
                        "architecture": "arm64",
                        "host_platform": agent_config.LOCAL_HOST_PLATFORM,
                        "required_guarantees": _execution_target()[
                            "required_guarantees"
                        ],
                        "immutable_image": guest_image,
                        "cpu_millis": 1000,
                        "memory_bytes": 512 * 1024 * 1024,
                        "writable_disk_bytes": 1024 * 1024 * 1024,
                        "timeout_seconds": 300,
                        "network": {"mode": "disabled"},
                        "workload_identity_profile": None,
                    },
                    "cache": {
                        "namespace": CACHE_NAMESPACE,
                        "read": True,
                        "write": True,
                    },
                    "artifacts": {
                        "artifact_count": 1,
                        "artifact_bytes": MAX_DOWNLOAD_BYTES,
                        "report_count": 1,
                        "report_bytes": MAX_DOWNLOAD_BYTES,
                        "single_output_bytes": MAX_DOWNLOAD_BYTES,
                    },
                    "retry": {"max_attempts": 1, "retry_on": []},
                },
            },
        ),
        "Build Configuration",
    )
    target = _execution_target()
    client.post(
        f"/api/v1/projects/{project_id}/policy-versions",
        f"{scope}-policy",
        {
            "policy": {
                "pools": {"mode": "replace", "value": [pool_id]},
                "repositories": {"mode": "replace", "value": [repository_id]},
                "secret_profiles": {"mode": "replace", "value": []},
                "identity_profiles": {"mode": "replace", "value": []},
                "runtimes": {"mode": "replace", "value": ["virtualization"]},
                "execution_targets": {"mode": "replace", "value": [target]},
                "cache": {
                    "mode": "replace",
                    "value": {
                        "namespaces": [CACHE_NAMESPACE],
                        "read": True,
                        "write": True,
                        "max_bytes": MAX_DOWNLOAD_BYTES,
                    },
                },
                "artifacts": {
                    "mode": "replace",
                    "value": {
                        "artifact_count": 1,
                        "artifact_bytes": MAX_DOWNLOAD_BYTES,
                        "report_count": 1,
                        "report_bytes": MAX_DOWNLOAD_BYTES,
                        "single_output_bytes": MAX_DOWNLOAD_BYTES,
                    },
                },
                "concurrency": {
                    "mode": "replace",
                    "value": {"active_builds": 1, "active_jobs": 1},
                },
                "retention": {
                    "mode": "replace",
                    "value": {
                        "build_seconds": 86400,
                        "log_seconds": 86400,
                        "artifact_seconds": 86400,
                        "cache_seconds": 86400,
                    },
                },
            }
        },
    )
    trigger_id = _resource_id(
        client.post(
            "/api/v1/trigger-definitions/manual",
            f"{scope}-trigger-definition",
            {
                "configuration_id": configuration_id,
                "configuration_version": 1,
                "enabled": True,
                "definition": {},
            },
        ),
        "Trigger Definition",
    )
    trigger = client.post(
        "/api/v1/triggers/manual",
        f"{scope}-trigger",
        {
            "trigger_id": trigger_id,
            "trigger_version": 1,
            "configuration_id": configuration_id,
            "configuration_version": 1,
            "deduplication_identity": f"{scope}-trigger",
            "source": {"kind": "exact_revision", "value": source_revision},
            "parameters": {},
            "priority": 50,
        },
    )
    if trigger.get("outcome") != "accepted":
        raise IntegrationError("manual integration trigger was not accepted")
    return project_id, _canonical_uuid(trigger.get("build_id"), "Build identity")


def _wait_for_successful_build(
    client: LocalStandClient,
    build_id: str,
    timeout_seconds: int,
    *,
    monotonic: Callable[[], float] = time.monotonic,
    sleep: Callable[[float], None] = time.sleep,
) -> dict[str, object]:
    deadline = monotonic() + timeout_seconds
    while True:
        build = client.get(f"/api/v1/builds/{build_id}")
        build_state = build.get("state")
        if build_state == "succeeded":
            return build
        if build_state in {"failed", "cancelled"}:
            raise IntegrationError(f"integration Build ended in state {build_state}")
        if monotonic() >= deadline:
            raise IntegrationError("timed out waiting for the integration Build")
        sleep(POLL_INTERVAL_SECONDS)


def _verify_job(client: LocalStandClient, build: dict[str, object]) -> str:
    attempt = _require_object(build.get("current_attempt"), "current Build attempt")
    attempt_id = _canonical_uuid(attempt.get("id"), "Attempt identity")
    detail = client.get(f"/api/v1/attempts/{attempt_id}")
    jobs = _require_array(detail.get("jobs"), "Attempt jobs")
    if len(jobs) != 1:
        raise IntegrationError("integration Build must contain exactly one Job")
    job = _require_object(jobs[0], "Job")
    event_cursor = job.get("event_cursor")
    if (
        job.get("state") != "succeeded"
        or not isinstance(event_cursor, int)
        or isinstance(event_cursor, bool)
    ):
        raise IntegrationError("integration Job did not publish a successful event stream")
    if event_cursor <= 0:
        raise IntegrationError("integration Job event stream is empty")
    return _canonical_uuid(job.get("id"), "Job identity")


def _verify_artifact(
    client: LocalStandClient, build_id: str
) -> tuple[str, str, int]:
    page = client.get(f"/api/v1/builds/{build_id}/artifacts?limit=10")
    artifacts = [
        _require_object(item, "Build output")
        for item in _require_array(page.get("items"), "Build outputs")
    ]
    names = [item.get("name") for item in artifacts]
    if any(not isinstance(name, str) for name in names) or sorted(names) != [
        EXPECTED_REPORT_NAME,
        EXPECTED_ARTIFACT_NAME,
    ]:
        raise IntegrationError("integration Build did not publish its artifact and report")
    artifact = next(
        item for item in artifacts if item.get("name") == EXPECTED_ARTIFACT_NAME
    )
    artifact_id = _canonical_uuid(artifact.get("id"), "Artifact identity")
    authorization = client.post_without_body(f"/api/v1/artifacts/{artifact_id}/download")
    metadata = _require_object(authorization.get("artifact"), "Artifact metadata")
    expected_digest = _require_string(metadata.get("sha256"), "Artifact digest")
    contents = client.download(
        _require_string(authorization.get("get_url"), "Artifact download URL")
    )
    actual_digest = hashlib.sha256(contents).hexdigest()
    if (
        len(contents) != EXPECTED_ARTIFACT_BYTES
        or not any(contents)
        or actual_digest != expected_digest
    ):
        raise IntegrationError("downloaded MinIO artifact failed integrity validation")
    return artifact_id, actual_digest, len(contents)


def run_vertical_slice(
    client: LocalStandClient,
    *,
    source_repository: str,
    source_revision: str,
    guest_image: str,
    build_timeout_seconds: int,
) -> dict[str, object]:
    """Create and run one real PostgreSQL/MinIO-backed Microsandbox Build."""

    client.verify_ui_and_readiness()
    pool_id, agent_id = verify_registered_topology(client)
    scope = uuid.uuid4().hex
    project_id, build_id = _create_resources(
        client,
        pool_id=pool_id,
        source_repository=_validated_source_repository(source_repository),
        source_revision=_validated_revision(source_revision),
        guest_image=guest_image,
        scope=scope,
    )
    build = _wait_for_successful_build(client, build_id, build_timeout_seconds)
    job_id = _verify_job(client, build)
    artifact_id, artifact_sha256, artifact_bytes = _verify_artifact(client, build_id)
    return {
        "schema_version": 1,
        "pool_id": pool_id,
        "agent_id": agent_id,
        "project_id": project_id,
        "build_id": build_id,
        "job_id": job_id,
        "artifact_id": artifact_id,
        "artifact_sha256": artifact_sha256,
        "artifact_bytes": artifact_bytes,
    }


def observe_persisted_slice(
    client: LocalStandClient, receipt: dict[str, object]
) -> dict[str, object]:
    """Verify topology uniqueness and durable Build plus object state after restart."""

    if receipt.get("schema_version") != 1:
        raise IntegrationError("integration receipt has an unsupported schema version")
    client.verify_ui_and_readiness()
    pool_id, agent_id = verify_registered_topology(client)
    if pool_id != receipt.get("pool_id") or agent_id != receipt.get("agent_id"):
        raise IntegrationError("stand restart created a duplicate Pool or Agent identity")
    build_id = _canonical_uuid(receipt.get("build_id"), "persisted Build identity")
    build = client.get(f"/api/v1/builds/{build_id}")
    if build.get("state") != "succeeded":
        raise IntegrationError("persisted PostgreSQL Build is no longer successful")
    artifact_id, artifact_sha256, artifact_bytes = _verify_artifact(client, build_id)
    if (
        artifact_id != receipt.get("artifact_id")
        or artifact_sha256 != receipt.get("artifact_sha256")
        or artifact_bytes != receipt.get("artifact_bytes")
    ):
        raise IntegrationError("persisted MinIO artifact identity changed after restart")
    return {
        "schema_version": 1,
        "state": "verified",
        "pool_id": pool_id,
        "agent_id": agent_id,
        "build_id": build_id,
        "artifact_id": artifact_id,
        "artifact_sha256": artifact_sha256,
        "artifact_bytes": artifact_bytes,
    }


def _write_receipt(path: Path, receipt: dict[str, object]) -> None:
    with state.private_umask():
        state.ensure_private_directory(path.parent)
    contents = json.dumps(receipt, sort_keys=True, separators=(",", ":")).encode(
        "utf-8"
    ) + b"\n"
    if not state.publish_exclusive_private_file(path, contents):
        raise IntegrationError("integration receipt already exists")


def _read_receipt(path: Path) -> dict[str, object]:
    try:
        return state.load_json_object(
            state.read_regular_file(
                path,
                "integration receipt",
                max_bytes=RECEIPT_MAX_BYTES,
                modes=frozenset({state.PRIVATE_FILE_MODE}),
            ),
            "integration receipt",
        )
    except state.StateError as cause:
        raise IntegrationError(str(cause)) from cause


def _bounded_seconds(value: str) -> int:
    parsed = int(value)
    if not 1 <= parsed <= DEFAULT_BUILD_TIMEOUT_SECONDS:
        raise argparse.ArgumentTypeError(
            f"timeout must be between 1 and {DEFAULT_BUILD_TIMEOUT_SECONDS} seconds"
        )
    return parsed


def parse_arguments(arguments: Sequence[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", required=True, type=Path)
    parser.add_argument("--origin", default=bootstrap.DEFAULT_ORIGIN)
    commands = parser.add_subparsers(dest="command", required=True)
    run = commands.add_parser("run", help="run a new vertical slice")
    run.add_argument("--source-repository", required=True)
    run.add_argument("--source-revision", required=True)
    run.add_argument("--receipt", required=True, type=Path)
    run.add_argument(
        "--build-timeout-seconds",
        type=_bounded_seconds,
        default=DEFAULT_BUILD_TIMEOUT_SECONDS,
    )
    observe = commands.add_parser("observe", help="verify a persisted vertical slice")
    observe.add_argument("--receipt", required=True, type=Path)
    return parser.parse_args(arguments)


def main(arguments: Sequence[str] | None = None) -> int:
    parsed = parse_arguments(arguments)
    try:
        root = state.resolve_safe_root(parsed.root, REPOSITORY, Path.home())
        state.validate_private_directory(root)
        client = LocalStandClient(root, parsed.origin)
        if parsed.command == "run":
            receipt = run_vertical_slice(
                client,
                source_repository=parsed.source_repository,
                source_revision=parsed.source_revision,
                guest_image=_guest_image(),
                build_timeout_seconds=parsed.build_timeout_seconds,
            )
            _write_receipt(parsed.receipt.resolve(), receipt)
        else:
            receipt = observe_persisted_slice(client, _read_receipt(parsed.receipt.resolve()))
    except (
        IntegrationError,
        bootstrap.BootstrapError,
        inputs.InputError,
        state.StateError,
        OSError,
        ValueError,
    ) as cause:
        print(f"local stand integration verification failed: {cause}", file=sys.stderr)
        return 1
    print(json.dumps(receipt, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
