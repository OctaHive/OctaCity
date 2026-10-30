#!/usr/bin/env python3
"""Generate the private server configuration bundle for the local stand."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import tempfile
from typing import Any
import uuid
from urllib.parse import quote

import init_local_stand as initializer
import local_stand_state as state


REPOSITORY = Path(__file__).resolve().parents[1]
DEFAULT_ROOT = state.DEFAULT_ROOT
DIRECTORY_MODE = state.DIRECTORY_MODE
PRIVATE_FILE_MODE = state.PRIVATE_FILE_MODE
MAX_POLICY_BYTES = 1024 * 1024
MAX_MANIFEST_BYTES = 1024 * 1024
MAX_FACTORY_PROFILE_BYTES = 4 * 1024 * 1024
SERVER_BUNDLE_FILES = {
    "config": "server.toml",
    "postgres_url": "postgres-url",
    "job_spec_policy": "job-spec-policy.json",
}


class ServerConfigurationError(RuntimeError):
    """The local stand server bundle cannot be generated or trusted safely."""


def _validate_regular_file(
    path: Path,
    description: str,
    max_bytes: int,
    allowed_modes: frozenset[int],
) -> bytes:
    try:
        return state.read_regular_file(
            path, description, max_bytes=max_bytes, modes=allowed_modes
        )
    except state.StateError as error:
        raise ServerConfigurationError(str(error)) from error


def _load_json_object(contents: bytes, description: str) -> dict[str, Any]:
    try:
        return state.load_json_object(contents, description)
    except state.StateError as error:
        raise ServerConfigurationError(str(error)) from error


def _verified_policy(installation: Path) -> bytes:
    if not installation.is_absolute():
        raise ServerConfigurationError("native installation must be an absolute path")
    try:
        metadata = installation.lstat()
    except FileNotFoundError as error:
        raise ServerConfigurationError(
            f"native installation is absent: {installation}"
        ) from error
    if installation.is_symlink() or not installation.is_dir():
        raise ServerConfigurationError(
            "native installation must be a real directory"
        )
    if metadata.st_uid != state.current_uid():
        raise ServerConfigurationError(
            "native installation is not owned by the invoking user"
        )
    if (metadata.st_mode & 0o777) != DIRECTORY_MODE:
        raise ServerConfigurationError("native installation must have mode 0700")

    policy_path = installation / "job-spec-policy.json"
    policy = _validate_regular_file(
        policy_path,
        "generated JobSpec policy",
        MAX_POLICY_BYTES,
        frozenset({PRIVATE_FILE_MODE}),
    )
    document = _load_json_object(policy, "generated JobSpec policy")
    if set(document) != {"source", "octa", "validity"}:
        raise ServerConfigurationError(
            "generated JobSpec policy has unexpected top-level fields"
        )
    if (
        not isinstance(document["source"], dict)
        or not isinstance(document["octa"], dict)
        or not isinstance(document["validity"], int)
        or isinstance(document["validity"], bool)
        or document["validity"] <= 0
    ):
        raise ServerConfigurationError("generated JobSpec policy has invalid structure")

    manifest_path = installation / "installation-manifest.json"
    manifest = _load_json_object(
        _validate_regular_file(
            manifest_path,
            "native installation manifest",
            MAX_MANIFEST_BYTES,
            frozenset({0o600, 0o644}),
        ),
        "native installation manifest",
    )
    if manifest.get("schema_version") != 1:
        raise ServerConfigurationError(
            "native installation manifest has an unsupported schema version"
        )
    files = manifest.get("files")
    expected_digest = (
        files.get("job-spec-policy.json") if isinstance(files, dict) else None
    )
    actual_digest = hashlib.sha256(policy).hexdigest()
    if expected_digest != actual_digest:
        raise ServerConfigurationError(
            "generated JobSpec policy differs from its installation manifest digest"
        )
    return policy


def _postgres_url(password: str) -> bytes:
    encoded = quote(password, safe="")
    return f"postgresql://octacity:{encoded}@postgres:5432/octacity\n".encode("ascii")


def _server_toml(*, factory: bool = False, signal_connectors: bool = False) -> bytes:
    profile = 'factory_profiles_file = "/run/octacity-secrets/factory-profiles.json"\n' if factory else ""
    if signal_connectors:
        profile += 'factory_signal_connectors_file = "/run/octacity-secrets/factory-signal-connectors.json"\n'
    workers = '''
[factory]
poll_interval_milliseconds = 250
claim_lifetime_milliseconds = 30000
batch_size = 8
concurrency = 2
''' if factory else ""
    return f'''{profile}management_bind = "0.0.0.0:8080"
agent_bind = "0.0.0.0:8081"
cache_bind = "0.0.0.0:8082"
acknowledge_unauthenticated_management = true
supported_pipeline_capabilities = ["oci.hypervisor", "shell"]

[postgres]
    url_file = "/run/octacity-secrets/postgres-url"
max_connections = 10

[object_storage]
endpoint = "{initializer.gateway_origin("objects.localhost")}"
region = "us-east-1"
bucket = "octacity-artifacts"
force_path_style = true
    access_key_file = "/run/octacity-secrets/object-access-key"
    secret_key_file = "/run/octacity-secrets/object-secret-key"

[signing]
key_id = "local-stand"
key_file = "/run/octacity-secrets/signing-key"

[agent_credentials]
enrollment_key_file = "/run/octacity-secrets/agent-enrollment-key"

[cache]
endpoint = "{initializer.gateway_origin("cache.localhost")}"
credential_key_file = "/run/octacity-secrets/cache-credential-key"
session_lifetime_milliseconds = 300000

[job_spec]
policy_file = "/run/octacity/job-spec-policy.json"
{workers}
'''.encode("utf-8")


def _expected_bundle(password: str, policy: bytes, profiles: bytes | None, *, signal_connectors: bool = False) -> dict[str, bytes]:
    expected = {
        SERVER_BUNDLE_FILES["config"]: _server_toml(factory=profiles is not None, signal_connectors=signal_connectors),
        SERVER_BUNDLE_FILES["postgres_url"]: _postgres_url(password),
        SERVER_BUNDLE_FILES["job_spec_policy"]: policy,
    }
    if profiles is not None:
        expected["factory-profiles.json"] = profiles
    return expected


def _connector_files(index: int) -> tuple[tuple[str, str, int], ...]:
    return (
        ("configuration_file", f"factory-signal-{index:04d}-configuration", MAX_FACTORY_PROFILE_BYTES),
        ("credential_file", f"factory-signal-{index:04d}-credential", 64 * 1024),
    )


def preflight_issue_mcp_readers(catalog: Path) -> None:
    """Validate the explicit scope and private credential references before creating a stand."""
    _issue_mcp_bundle(catalog.expanduser())


def _issue_mcp_bundle(catalog: Path) -> tuple[bytes, dict[str, bytes]]:
    """Translate explicit reader references into the existing private server bundle."""
    try:
        document = _load_json_object(_validate_regular_file(catalog, "Issue MCP installation",
            MAX_FACTORY_PROFILE_BYTES, frozenset({PRIVATE_FILE_MODE})), "Issue MCP installation")
        if set(document) != {"version", "readers"} or type(document["version"]) is not int or document["version"] != 1:
            raise ValueError("catalog")
        readers = document["readers"]
        if not isinstance(readers, list) or len(readers) > 128:
            raise ValueError("readers")
        files, installed, names = {}, [], set()
        for index, reader in enumerate(readers):
            required = {"id", "project_id", "credential_file", "expires_at_unix_ms"}
            if not isinstance(reader, dict) or not required <= set(reader) or set(reader) - required - {"issue_ids"}:
                raise ValueError("reader")
            identity = reader["id"]
            if not isinstance(identity, str) or not re.fullmatch(r"[a-z0-9_-]{1,64}", identity) or identity in names:
                raise ValueError("identity")
            names.add(identity)
            project = str(uuid.UUID(reader["project_id"]))
            expiry = reader["expires_at_unix_ms"]
            if type(expiry) is not int or not 0 < expiry <= 9223372036854775807:
                raise ValueError("expiry")
            source = reader["credential_file"]
            if not isinstance(source, str) or not Path(source).is_absolute():
                raise ValueError("credential reference")
            credential = _validate_regular_file(Path(source), "Issue MCP credential", 65536,
                frozenset({PRIVATE_FILE_MODE}))
            if not re.fullmatch(r"ocmcp\." + re.escape(identity) + r"\.[a-f0-9]{64}", credential.decode().rstrip("\r\n")):
                raise ValueError("credential")
            name = f"factory-issue-mcp-{index:04d}-credential"
            item = {"id": identity, "project_id": project, "credential_file": f"/run/octacity-secrets/{name}",
                    "expires_at_unix_ms": expiry}
            if "issue_ids" in reader:
                issues = reader["issue_ids"]
                if not isinstance(issues, list) or len(issues) > 256:
                    raise ValueError("issue scope")
                item["issue_ids"] = sorted({str(uuid.UUID(issue)) for issue in issues})
            installed.append(item)
            files[name] = credential
        files["factory-issue-mcp-readers.json"] = (json.dumps({"version": 1, "readers": installed}, sort_keys=True) + "\n").encode()
        suffix = "".join("\n[[issue_mcp_readers]]\n" + "".join(f"{key} = {json.dumps(value)}\n" for key, value in reader.items()) for reader in installed)
        return suffix.encode(), files
    except (ValueError, TypeError, AttributeError, OSError, ServerConfigurationError):
        raise ServerConfigurationError("Issue MCP installation is invalid") from None


def export_installed_issue_mcp_readers(bundle: Path, destination: Path) -> Path | None:
    """Restore private input references for the existing installation transaction.

    Only this configurator translates its runtime file names back into host
    references. Export contains no token bytes and cannot adopt another path.
    """
    catalog = bundle / "factory-issue-mcp-readers.json"
    if not catalog.exists() and not catalog.is_symlink():
        return None
    document = _load_json_object(_validate_regular_file(catalog, "installed Issue MCP readers",
        MAX_FACTORY_PROFILE_BYTES, frozenset({PRIVATE_FILE_MODE})), "installed Issue MCP readers")
    for index, reader in enumerate(document.get("readers", [])):
        name = f"factory-issue-mcp-{index:04d}-credential"
        if reader.get("credential_file") != f"/run/octacity-secrets/{name}":
            raise ServerConfigurationError("installed Issue MCP reader reference is not recognized")
        reader["credential_file"] = str(bundle / name)
    state.publish_exclusive_private_file(destination, (json.dumps(document, sort_keys=True) + "\n").encode())
    return destination


def export_installed_signal_connectors(bundle: Path, destination: Path) -> Path | None:
    """Export installed connector inputs for a state-preserving reconfiguration.

    This configurator owns both directions of its private file-name and runtime
    reference encoding. The lifecycle only chooses the private staging target.
    """
    catalog = bundle / "factory-signal-connectors.json"
    if not catalog.exists():
        return None
    document = state.load_json_object(state.read_regular_file(
        catalog, "installed signal connectors", max_bytes=MAX_FACTORY_PROFILE_BYTES,
        modes=frozenset({state.PRIVATE_FILE_MODE}),
    ), "installed signal connectors")
    for index, connector in enumerate(document.get("connectors", [])):
        for field, name, _ in _connector_files(index):
            if connector.get(field) != f"/run/octacity-secrets/{name}":
                raise state.StateError("installed signal connector reference is not recognized")
            connector[field] = str(bundle / name)
    destination.write_text(json.dumps(document))
    destination.chmod(PRIVATE_FILE_MODE)
    return destination


def _connector_bundle(catalog: Path) -> dict[str, bytes]:
    """Copy opaque adapter files while rewriting only trusted runtime references."""
    try:
        document = _load_json_object(_validate_regular_file(
            catalog, "Factory connector catalog", MAX_FACTORY_PROFILE_BYTES,
            frozenset({PRIVATE_FILE_MODE}),
        ), "Factory connector catalog")
        connectors = document.get("connectors")
        if (set(document) != {"version", "connectors"}
                or type(document["version"]) is not int or document["version"] != 1
                or not isinstance(connectors, list) or len(connectors) > 64):
            raise ServerConfigurationError("invalid connector catalog")
        files: dict[str, bytes] = {}
        installed = []
        for index, connector in enumerate(connectors):
            if (not isinstance(connector, dict)
                    or set(connector) != {"kind", "credential_profile", "configuration_file", "credential_file"}
                    or not all(isinstance(value, str) and value for value in connector.values())):
                raise ServerConfigurationError("invalid connector files")
            rewritten = {"kind": connector["kind"], "credential_profile": connector["credential_profile"]}
            for field, name, limit in _connector_files(index):
                source = Path(connector[field])
                if not source.is_absolute():
                    raise ServerConfigurationError("connector files must be absolute")
                files[name] = _validate_regular_file(
                    source, "Factory connector input", limit,
                    frozenset({PRIVATE_FILE_MODE}),
                )
                rewritten[field] = f"/run/octacity-secrets/{name}"
            installed.append(rewritten)
        files["factory-signal-connectors.json"] = (
            json.dumps({"version": 1, "connectors": installed}, sort_keys=True) + "\n"
        ).encode("utf-8")
        return files
    except (OSError, UnicodeError, ValueError, ServerConfigurationError):
        raise ServerConfigurationError("Factory connector inputs are invalid") from None


def _executable_bundle(root: Path, name: str) -> tuple[dict[str, bytes | None], frozenset[str]]:
    """Copy a bounded opaque executable installation, never interpreting its protocol."""
    try:
        records = state.map_private_tree(root, lambda _name, source, size: source.read(size + 1),
            max_depth=4, max_entries=1024, max_bytes=512 * 1024 * 1024)
        files: dict[str, bytes | None] = {}
        executables = set()
        for relative, record in records.items():
            target = (Path(name) / relative).as_posix()
            files[target] = None if record is None else record[1]
            if record is not None and record[0] & 0o111:
                executables.add(target)
        return files, frozenset(executables)
    except (OSError, ValueError, state.StateError):
        raise ServerConfigurationError("Factory executable installation is invalid") from None


def _validate_existing_bundle(destination: Path, expected: dict[str, bytes | None], executables: frozenset[str] = frozenset()) -> None:
    initializer.validate_private_directory(destination)
    entries = {path.name for path in destination.iterdir()}
    if ("factory-profiles.json" in entries) != ("factory-profiles.json" in expected):
        raise ServerConfigurationError(
            "Factory installation cannot change an existing server bundle; "
            "reset the disposable stand before changing its Factory mode"
        )
    if entries != {Path(name).parts[0] for name in expected}:
        raise ServerConfigurationError(
            "generated server bundle contains missing or unexpected files"
        )
    for name, contents in expected.items():
        path = destination / name
        if contents is None:
            state.validate_private_directory(path)
            if {entry.name for entry in path.iterdir()} != {Path(child).name for child in expected if Path(child).parent.as_posix() == name}:
                raise ServerConfigurationError("generated server bundle contains missing or unexpected files")
            continue
        if _validate_regular_file(
            path,
            f"generated server {name}",
            len(contents),
            frozenset({0o700 if name in executables else PRIVATE_FILE_MODE}),
        ) != contents:
            raise ServerConfigurationError(
                f"generated server {name} differs from the verified inputs"
            )


def _publish_bundle(destination: Path, expected: dict[str, bytes | None], executables: frozenset[str] = frozenset()) -> None:
    if destination.exists() or destination.is_symlink():
        _validate_existing_bundle(destination, expected, executables)
        return
    staging = Path(
        tempfile.mkdtemp(prefix=".server.staging-", dir=destination.parent)
    )
    staging.chmod(DIRECTORY_MODE)
    try:
        for name, contents in expected.items():
            path = staging / name
            if contents is None:
                state.ensure_private_directory(path)
                continue
            path.write_bytes(contents)
            path.chmod(0o700 if name in executables else PRIVATE_FILE_MODE)
        _validate_existing_bundle(staging, expected, executables)
        try:
            staging.rename(destination)
        except OSError:
            if not destination.exists() and not destination.is_symlink():
                raise
            _validate_existing_bundle(destination, expected, executables)
    finally:
        if staging.exists():
            shutil.rmtree(staging)


def generate_server_configuration(
    root: Path,
    native_installation: Path,
    *,
    repository: Path = REPOSITORY,
    home: Path = Path.home(),
    openssl: str = "openssl",
    factory_profiles: Path | None = None,
    factory_signal_connectors: Path | None = None,
    factory_delivery_bundle: Path | None = None,
    factory_runtime_bundle: Path | None = None,
    factory_issue_mcp_readers: Path | None = None,
) -> dict[str, str]:
    """Validate inputs and atomically publish an idempotent server bundle."""

    if (factory_signal_connectors is not None or factory_delivery_bundle is not None or factory_runtime_bundle is not None or factory_issue_mcp_readers is not None) and factory_profiles is None:
        raise ServerConfigurationError("Factory connectors require an explicit Factory profile")
    try:
        initialized = initializer.validate_initialized_state(
            root,
            repository=repository,
            home=home,
            openssl=openssl,
        )
    except state.StateError as error:
        raise ServerConfigurationError(str(error)) from error
    credentials = {name: Path(path) for name, path in initialized["credentials"].items()}
    password = initializer.read_credential(
        "database_password",
        credentials["database_password"],
    ).decode("ascii")
    policy = _verified_policy(native_installation.expanduser())
    profiles = None
    if factory_profiles is not None:
        profiles = _validate_regular_file(
            factory_profiles.expanduser(), "Factory profile catalog",
            MAX_FACTORY_PROFILE_BYTES, frozenset({PRIVATE_FILE_MODE}),
        )
        document = _load_json_object(profiles, "Factory profile catalog")
        if type(document.get("version")) is not int or document["version"] != 1:
            raise ServerConfigurationError("Factory profile catalog schema version is unsupported")
    connector_files = None if factory_signal_connectors is None else _connector_bundle(factory_signal_connectors.expanduser())
    expected = _expected_bundle(password, policy, profiles, signal_connectors=connector_files is not None)
    if connector_files is not None:
        expected.update(connector_files)
    if factory_issue_mcp_readers is not None:
        suffix, files = _issue_mcp_bundle(factory_issue_mcp_readers.expanduser())
        expected[SERVER_BUNDLE_FILES["config"]] += suffix
        expected.update(files)
    executables = frozenset()
    for name, source in (("factory-delivery", factory_delivery_bundle), ("factory-runtime", factory_runtime_bundle)):
        if source is not None:
            files, programs = _executable_bundle(source.expanduser(), name)
            expected.update(files)
            executables |= programs

    root = Path(initialized["root"])
    with state.private_umask():
        config_root = root / "config"
        state.ensure_private_directory(config_root)
        destination = config_root / "server"
        _publish_bundle(destination, expected, executables)
    receipt = {
        name: str(destination / filename)
        for name, filename in SERVER_BUNDLE_FILES.items()
    }
    if profiles is not None:
        receipt["factory_profiles"] = str(destination / "factory-profiles.json")
    if connector_files is not None:
        receipt["factory_signal_connectors"] = str(destination / "factory-signal-connectors.json")
    if factory_delivery_bundle is not None:
        receipt["factory_delivery_bundle"] = str(destination / "factory-delivery")
    return receipt


def parse_arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--root",
        type=Path,
        default=Path(os.environ.get("OCTACITY_LOCAL_STAND_ROOT", DEFAULT_ROOT)),
        help="initialized private host state root (default: %(default)s)",
    )
    parser.add_argument(
        "--native-installation",
        required=True,
        type=Path,
        help="verified native installation containing job-spec-policy.json",
    )
    parser.add_argument("--openssl", default="openssl", help="OpenSSL executable")
    parser.add_argument("--factory-profiles", type=Path, help="explicit private native Factory profile catalog; omitted disables Factory")
    parser.add_argument("--factory-signal-connectors", type=Path, help="separate private Decision Signal connector catalog")
    parser.add_argument("--factory-delivery-bundle", type=Path, help="separate private installed Delivery adapter directory")
    parser.add_argument("--factory-runtime-bundle", type=Path, help="separate private installed model runtime directory")
    parser.add_argument("--factory-issue-mcp-readers", type=Path, help="explicit Project-scoped Issue MCP reader installations")
    return parser.parse_args()


def main() -> int:
    arguments = parse_arguments()
    try:
        receipt = generate_server_configuration(
            arguments.root,
            arguments.native_installation,
            openssl=arguments.openssl,
            factory_profiles=arguments.factory_profiles,
            factory_signal_connectors=arguments.factory_signal_connectors,
            factory_delivery_bundle=arguments.factory_delivery_bundle,
            factory_runtime_bundle=arguments.factory_runtime_bundle,
            factory_issue_mcp_readers=arguments.factory_issue_mcp_readers,
        )
    except (OSError, UnicodeError, ValueError, ServerConfigurationError) as error:
        raise SystemExit(f"local stand server configuration failed: {error}") from error
    print(json.dumps(receipt, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
