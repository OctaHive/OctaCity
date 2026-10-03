#!/usr/bin/env python3
"""Generate the private server configuration bundle for the local stand."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import tempfile
from typing import Any
from urllib.parse import quote

import init_local_stand as initializer
import local_stand_state as state


REPOSITORY = Path(__file__).resolve().parents[1]
DEFAULT_ROOT = state.DEFAULT_ROOT
DIRECTORY_MODE = state.DIRECTORY_MODE
PRIVATE_FILE_MODE = state.PRIVATE_FILE_MODE
MAX_POLICY_BYTES = 1024 * 1024
MAX_MANIFEST_BYTES = 1024 * 1024
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


def _server_toml() -> bytes:
    return f'''management_bind = "0.0.0.0:8080"
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
'''.encode("utf-8")


def _expected_bundle(password: str, policy: bytes) -> dict[str, bytes]:
    return {
        SERVER_BUNDLE_FILES["config"]: _server_toml(),
        SERVER_BUNDLE_FILES["postgres_url"]: _postgres_url(password),
        SERVER_BUNDLE_FILES["job_spec_policy"]: policy,
    }


def _validate_existing_bundle(destination: Path, expected: dict[str, bytes]) -> None:
    initializer.validate_private_directory(destination)
    entries = {path.name for path in destination.iterdir()}
    if entries != set(expected):
        raise ServerConfigurationError(
            "generated server bundle contains missing or unexpected files"
        )
    for name, contents in expected.items():
        path = destination / name
        if _validate_regular_file(
            path,
            f"generated server {name}",
            len(contents),
            frozenset({PRIVATE_FILE_MODE}),
        ) != contents:
            raise ServerConfigurationError(
                f"generated server {name} differs from the verified inputs"
            )


def _publish_bundle(destination: Path, expected: dict[str, bytes]) -> None:
    if destination.exists() or destination.is_symlink():
        _validate_existing_bundle(destination, expected)
        return
    staging = Path(
        tempfile.mkdtemp(prefix=".server.staging-", dir=destination.parent)
    )
    staging.chmod(DIRECTORY_MODE)
    try:
        for name, contents in expected.items():
            path = staging / name
            path.write_bytes(contents)
            path.chmod(PRIVATE_FILE_MODE)
        _validate_existing_bundle(staging, expected)
        try:
            staging.rename(destination)
        except OSError:
            if not destination.exists() and not destination.is_symlink():
                raise
            _validate_existing_bundle(destination, expected)
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
) -> dict[str, str]:
    """Validate inputs and atomically publish an idempotent server bundle."""

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
    expected = _expected_bundle(password, policy)

    root = Path(initialized["root"])
    with state.private_umask():
        config_root = root / "config"
        state.ensure_private_directory(config_root)
        destination = config_root / "server"
        _publish_bundle(destination, expected)
    return {
        name: str(destination / filename)
        for name, filename in SERVER_BUNDLE_FILES.items()
    }


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
    return parser.parse_args()


def main() -> int:
    arguments = parse_arguments()
    try:
        receipt = generate_server_configuration(
            arguments.root,
            arguments.native_installation,
            openssl=arguments.openssl,
        )
    except (OSError, UnicodeError, ValueError, ServerConfigurationError) as error:
        raise SystemExit(f"local stand server configuration failed: {error}") from error
    print(json.dumps(receipt, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
