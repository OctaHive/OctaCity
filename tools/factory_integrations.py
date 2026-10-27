"""Explicit private GitHub installation for the personal Factory stand.

This selects native resource ceilings, never remote permissions, Project grants,
Factory inputs or task-card identities. Credentials are provisioned by the UI.
"""
from __future__ import annotations

import hashlib
import json
from pathlib import Path
import re
import subprocess
import unicodedata
import urllib.error
import urllib.request

import local_stand_state as state
import configure_local_stand_server as server_config

API_VERSION = "2026-03-10"  # Same native adapter contract as octacity-github/README.md.
SOURCE_POLICY = Path(__file__).resolve().parents[1] / "deployment/local-stand/source-policy.json"
SOCKET = "/run/octacity-management/management.sock"
STATE_ROOT = "/var/lib/octacity-integrations"
MAX_REPOSITORY_LABELS = 128  # Installed Reporter and native mapping ceilings.


class _NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None


def install_repository(root: Path, repository: str) -> Path:
    """Reuse retained identity; resolve a fresh selection without ambient tokens or CLI sessions."""
    if not re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", repository) or any(p in {".", ".."} for p in repository.split("/")):
        raise ValueError("GitHub task repository must be owner/repository")
    retained = _retained_repository(root, repository)
    if retained is None:
        try:
            metadata = _public_json(f"repos/{repository}")
            if (not isinstance(metadata, dict) or type(metadata.get("id")) is not int or metadata["id"] <= 0
                    or not isinstance(metadata.get("full_name"), str) or metadata["full_name"].casefold() != repository.casefold()):
                raise ValueError("repository identity differs")
            native_id = str(metadata["id"])
        except (ValueError, urllib.error.URLError):
            raise ValueError("cannot resolve the selected public GitHub repository without authentication; no local tokens were used") from None
        labels = []
        for page in (1, 2):
            try:
                items = _public_json(f"repos/{repository}/labels?per_page=100&page={page}")
                if not isinstance(items, list) or len(items) > 100:
                    raise ValueError("invalid repository labels")
                labels.extend(item["name"] for item in items)
                labels = _checked_labels(labels)
            except (ValueError, TypeError, KeyError, urllib.error.URLError):
                raise ValueError("cannot install the selected GitHub repository labels; expected at most 128 unique bounded names from a public repository") from None
            if len(items) < 100:
                break
    else:
        native_id, labels = retained
    result = subprocess.run(["docker", "run", "--rm", "--pull", "never", "--network", "none", "--read-only",
        "--entrypoint", "sha256sum", "octacity/server:local", "/usr/local/bin/octacity-github"],
        check=True, capture_output=True, text=True, timeout=30)
    digest = result.stdout.split()[0] if result.stdout.split() else ""
    # Rebuild the expected bytes and use the existing immutable-bundle validator;
    # retained metadata alone cannot authorize altered profiles or a different adapter.
    return configure_integrations(root, repository, native_repository_id=native_id, adapter_digest=digest,
                                  repository_labels=labels)


def _public_json(path: str):
    """Bounded public metadata read; redirects and ambient credentials are never used."""
    request = urllib.request.Request(f"https://api.github.com/{path}", headers={
        "Accept": "application/vnd.github+json", "User-Agent": "OctaCity-local-stand", "X-GitHub-Api-Version": API_VERSION,
    })
    with urllib.request.build_opener(_NoRedirect).open(request, timeout=10) as response:
        payload = response.read(65537)
    if len(payload) > 65536:
        raise ValueError("oversized repository metadata")
    return json.loads(payload)


def _checked_labels(labels) -> list[str]:
    if (len(labels) > MAX_REPOSITORY_LABELS
            or any(not isinstance(label, str) or not label.strip() or label in {".", ".."}
                   or len(label.encode("utf-8")) > 256
                   or any(unicodedata.category(char) == "Cc" for char in label) for label in labels)
            or len({label.casefold() for label in labels}) != len(labels)):
        raise ValueError("invalid repository labels")
    return sorted(labels, key=str.casefold)


def _retained_repository(root: Path, repository: str) -> tuple[str, list[str]] | None:
    bundle = root / "config/integrations"
    if not bundle.exists() and not bundle.is_symlink():
        return None
    try:
        state.validate_private_directory(bundle)
        catalog = json.loads(state.read_regular_file(bundle / "catalog.json", "installed integration catalog",
                            max_bytes=4 * 1024 * 1024, modes=frozenset({0o600})))
        installation, = catalog["installations"]
        resource, = installation["resources"]
        if resource["locator"] != repository or not isinstance(resource["native_id"], str):
            raise ValueError("retained repository differs")
        profiles = json.loads(state.read_regular_file(bundle / "source-profiles.json", "installed Source profiles",
                             max_bytes=4 * 1024 * 1024, modes=frozenset({0o600})))
        reporter, = profiles["reporters"]
        return resource["native_id"], _checked_labels(reporter["reporting"]["editable"]["labels"])
    except (OSError, ValueError, KeyError, TypeError, state.StateError):
        raise ValueError("existing integration installation differs or is incomplete; retain it and use another stand root") from None


def _json(value) -> bytes:
    return (json.dumps(value, sort_keys=True, separators=(",", ":")) + "\n").encode()


def _digest(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def configure_integrations(root: Path, repository: str, *, native_repository_id: str, adapter_digest: str,
                           repository_labels: tuple[str, ...] | list[str] = ()) -> Path:
    """Publish a separate immutable installation, retaining the existing server bundle and Runs."""
    if (not re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", repository)
            or any(part in {".", ".."} for part in repository.split("/"))
            or not re.fullmatch(r"[1-9][0-9]{0,19}", native_repository_id)
            or not re.fullmatch(r"[0-9a-f]{64}", adapter_digest)):
        raise ValueError("invalid explicit GitHub installation")
    owner, name = repository.split("/")
    labels = _checked_labels(repository_labels)
    native = f'''config_version = 1
api_base = "https://api.github.com/"
api_version = "{API_VERSION}"
credential_directory = "credentials"
selection_directory = "selections"
mapping_directory = "mappings"
[capabilities]
source_repositories = [{json.dumps(repository)}]
reporter_repositories = [{json.dumps(repository)}]
[repositories.{json.dumps(repository)}]
repository_id = "{native_repository_id}"
owner = "{owner}"
name = "{name}"
'''.encode()
    profile_digest = _digest(native)
    identity = {"repository": repository, "native_id": native_repository_id,
                "adapter": adapter_digest, "profile": profile_digest}
    if labels:
        identity["labels"] = labels
    key = _digest(_json(identity))
    installed_root = f"{STATE_ROOT}/{key}"
    provider_root = f"{installed_root}/adapters/github"

    def exact(identity: str, digest: str = key):
        return {"identity": identity, "version": "v1", "digest": digest}

    capabilities = []
    targets = []
    for purpose, handle in (("task_read", "issue-reader"), ("task_update", "issue-reporter")):
        reference = {"provider": "installed", "reference": f"github-{key[:16]}-{handle}"}
        capabilities.append({"purpose": purpose, "capability": exact(handle),
                             "configuration": exact(f"{handle}-profile", profile_digest), "credential": reference})
        targets.append({"reference": reference, "handle": handle, "purpose": purpose, "resources": [repository]})
    catalog = {"version": 1, "registry": f"{installed_root}/adapters", "installations": [{
        "installation": exact("github-installation"), "provider": "github", "adapter": exact("github", adapter_digest),
        "endpoints": ["https://api.github.com/"], "authentication": ["token"], "resources": [{
            "reference": exact("github-issues"), "native_id": native_repository_id,
            "locator": repository, "capabilities": capabilities}]}],
        "credential_publishers": [{"kind": "github_private_files", "root": provider_root,
                                  "directory": "credentials", "targets": targets}]}
    catalog["checks"] = [{
        "installation": exact("github-installation"), "resource_reference": exact("github-issues"),
        "capability": capability, "plugin": exact("github", adapter_digest), "provider_configuration": profile_digest,
        "resource": {"resource_id": native_repository_id, "resource_locator": repository, "credential": {
            "class": "work_source_read" if target["purpose"] == "task_read" else "work_reporter_write", "handle": target["handle"]}},
        "network_hosts": ["api.github.com"],
    } for capability, target in zip(capabilities, targets)]
    policy = json.loads(SOURCE_POLICY.read_text())
    policy["reporter"]["reporting"]["editable"]["labels"] = labels
    # Repository labels are independent fields, not one mutually exclusive vocabulary.
    policy["mapping"]["definition"]["managed_label_groups"] = {
        f"repository-{index:03d}": [label] for index, label in enumerate(labels)}
    reader = policy["reader"]
    reader["read"] = catalog["checks"][0]
    reader["selection_labels"] = policy["selector"]["definition"]
    read = reader["read"]
    reader["preparation"]["mapping"]["source"] = {
        "installation": read["installation"], "capability": read["capability"]["capability"],
        "configuration": read["capability"]["configuration"], "resource": read["resource_reference"]}
    reader["selection"] = {"identity": policy["selector"]["identity"],
                           "sha256": _digest(_json(policy["selector"]["definition"]))}
    reporter = policy["reporter"]
    reporter["write"] = catalog["checks"][1]
    reporter["credential_profile"] = reporter["write"]["resource"]["credential"]["handle"]
    reporter["mapping"] = {"identity": policy["mapping"]["identity"],
                           "sha256": _digest(_json(policy["mapping"]["definition"]))}
    reporter["reporting"]["mapping"] = {"statuses": [
        {"status": status, **effects}
        for status, effects in policy["mapping"]["definition"]["statuses"].items()]}
    catalog["source_profiles_file"] = f"{installed_root}/source-profiles.json"
    manifest = f'''manifest_version = 1
adapter_id = "github"
executable = "plugin"
executable_sha256 = "{adapter_digest}"
[protocol.source]
min = 1
max = 1
[protocol.reporter]
min = 1
max = 2
[capabilities.source]
check = true
authorize = true
preview = true
cancel = false
credential_class = "work_source_read"
configuration_digest = "{profile_digest}"
[capabilities.reporter]
check = true
custom_effects = true
observe = true
project = true
cancel = false
idempotency = "observe_before_retry"
credential_class = "work_reporter_write"
configuration_digest = "{profile_digest}"
'''.encode()
    baseline = state.read_regular_file(root / "config/server/server.toml", "installed server configuration",
                                       max_bytes=65536, modes=frozenset({0o600}))
    config = baseline + f'''
[integration_management]
socket_path = "{SOCKET}"
peer_uid = 101
principal = "local-stand-gateway"
catalog_file = "{installed_root}/catalog.json"
'''.encode()
    bundle = root / "config/integrations"
    mapping_bytes = _json(policy["mapping"]["definition"])
    if len(mapping_bytes) > 64 * 1024:
        raise ValueError("installed GitHub label mapping is too large")
    files = {"server.toml": config, "catalog.json": _json(catalog), "github-work.toml": native,
             "source-profiles.json": _json({"version": 1, "readers": [reader], "reporters": [reporter]}),
             "selections": None,
             f"selections/{_digest(policy['selector']['identity'].encode())}.json": _json(policy["selector"]["definition"]),
             "mappings": None,
             f"mappings/{_digest(policy['mapping']['identity'].encode())}.json": mapping_bytes,
             "adapter.toml": manifest, "installation-key": (key + "\n").encode(),
             "management-upstream.conf": f"server unix:{SOCKET};\n".encode()}
    try:
        server_config._publish_bundle(bundle, files)
    except server_config.ServerConfigurationError:
        raise ValueError("existing integration installation differs or is incomplete; retain it and use another stand root") from None
    return bundle
