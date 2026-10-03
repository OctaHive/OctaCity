#!/usr/bin/env python3
"""Generate and validate the native Agent configuration for the local stand."""

from __future__ import annotations

import argparse
import base64
from dataclasses import dataclass
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import stat
import subprocess
import tempfile
from typing import Any

import init_local_stand as initializer
import local_stand_state as state
import verify_local_stand_context as build_context


REPOSITORY = Path(__file__).resolve().parents[1]
DEFAULT_ROOT = state.DEFAULT_ROOT
DIRECTORY_MODE = state.DIRECTORY_MODE
PRIVATE_FILE_MODE = state.PRIVATE_FILE_MODE
MAX_MANIFEST_BYTES = 4 * 1024 * 1024
MAX_CREDENTIAL_BYTES = 64 * 1024
MAX_PUBLIC_KEY_BYTES = 1024
MAX_CA_BYTES = 1024 * 1024
# These ceilings equal the aggregate extraction policy in the staging
# allowlist. Configuration verification must never turn a malformed bundle
# into an unbounded filesystem walk or whole-file allocation.
INSTALLATION_FILE_MODES = frozenset({0o600, 0o644, 0o755})
HASH_CHUNK_BYTES = 1024 * 1024
VERSION_PATTERN = re.compile(r"^[0-9]+\.[0-9]+\.[0-9]+(?:[-+][A-Za-z0-9.-]+)?$")

# This is a workstation profile, not protocol policy. Values are collected in
# one place so future hardware profiles can replace the renderer input without
# editing TOML text or silently diverging between related watermarks.
LOCAL_AGENT_LIMITS = {
    "host_accounting_max_entries": 100_000,
    "max_archive_entries": 10_000,
    "upload_timeout_seconds": 120,
    "max_workspace_bytes": 4 * 1024**3,
    "max_spool_bytes": 256 * 1024**2,
    "max_spool_records": 25_000,
    "event_batch_max_bytes": 1024**2,
    "event_batch_max_records": 256,
    "event_channel_capacity": 128,
    "poll_timeout_seconds": 30,
    "coordinator_request_timeout_seconds": 10,
    "coordinator_max_body_bytes": 4 * 1024**2,
    "retry_initial_delay_milliseconds": 100,
    "retry_max_delay_seconds": 10,
    "retry_max_attempts": 4,
    "heartbeat_interval_seconds": 5,
    "lease_safety_margin_seconds": 15,
    "graceful_cancel_timeout_seconds": 10,
    "cleanup_timeout_seconds": 30,
    "runner_hello_timeout_seconds": 5,
    "resource_sample_interval_seconds": 5,
    "resource_sample_timeout_seconds": 1,
    "max_accounting_failures": 3,
}
LOCAL_OUTPUT_LIMITS = {
    "artifact_count": 32,
    "artifact_bytes": 1024**3,
    "report_count": 32,
    "report_bytes": 256 * 1024**2,
    "single_output_bytes": 256 * 1024**2,
}
LOCAL_CACHE_LIMITS = {
    "max_scopes": 4,
    "request_timeout_seconds": 30,
    "max_parallel_transfers": 4,
}
LOCAL_CACHE_CAPACITY = {
    "max_bytes": 512 * 1024**2,
    "high_watermark_bytes": 448 * 1024**2,
    "low_watermark_bytes": 384 * 1024**2,
}
LOCAL_STORAGE_RESERVES = {
    "work_reserve_bytes": 1024**3,
    "state_reserve_bytes": 256 * 1024**2,
    "cache_reserve_bytes": 256 * 1024**2,
    "disk_check_interval_seconds": 30,
}


@dataclass(frozen=True)
class InstallationLimits:
    """Aggregate bounds derived from the reviewed native staging allowlist."""

    entries: int
    single_file_bytes: int
    total_bytes: int


class AgentConfigurationError(RuntimeError):
    """The native local-stand Agent configuration cannot be trusted."""


def _regular_file(
    path: Path,
    description: str,
    *,
    max_bytes: int,
    modes: frozenset[int] | None = None,
) -> bytes:
    try:
        return state.read_regular_file(
            path,
            description,
            max_bytes=max_bytes,
            modes=modes if modes is not None else INSTALLATION_FILE_MODES,
        )
    except state.StateError as error:
        raise AgentConfigurationError(str(error)) from error


def _json_object(contents: bytes, description: str) -> dict[str, Any]:
    try:
        return state.load_json_object(contents, description)
    except state.StateError as error:
        raise AgentConfigurationError(str(error)) from error


def _verified_file_digest(
    path: Path,
    description: str,
    maximum: int,
    single_file_bytes: int,
) -> tuple[str, int]:
    if maximum <= 0:
        raise AgentConfigurationError("native installation exceeds its aggregate byte limit")
    try:
        descriptor, metadata = state.open_regular_file(
            path,
            description,
            max_bytes=min(maximum, single_file_bytes),
            modes=INSTALLATION_FILE_MODES,
        )
    except state.StateError as error:
        raise AgentConfigurationError(str(error)) from error
    digest = hashlib.sha256()
    read_bytes = 0
    try:
        with os.fdopen(descriptor, "rb", closefd=False) as source:
            while chunk := source.read(min(HASH_CHUNK_BYTES, maximum - read_bytes + 1)):
                read_bytes += len(chunk)
                if read_bytes > maximum:
                    raise AgentConfigurationError(
                        "native installation exceeds its aggregate byte limit"
                    )
                digest.update(chunk)
        current = os.fstat(descriptor)
    finally:
        if descriptor >= 0:
            os.close(descriptor)
    if (
        state.file_state(current) != state.file_state(metadata)
        or read_bytes != current.st_size
    ):
        raise AgentConfigurationError(f"{description} changed while it was verified")
    return digest.hexdigest(), read_bytes


def _installation_limits(repository: Path) -> InstallationLimits:
    try:
        entries = build_context.load_staging_allowlist(
            repository / "deployment/local-stand/staging-allowlist.json",
            repository / "deployment/local-stand/inputs.json",
        )
    except (OSError, build_context.ContextError) as error:
        raise AgentConfigurationError(f"cannot load native staging limits: {error}") from error
    # The generated installation manifest is the sole entry not extracted
    # from one of the bounded native archives.
    return InstallationLimits(
        entries=1 + sum(entry["max_members"] for entry in entries.values()),
        single_file_bytes=max(entry["max_expanded_bytes"] for entry in entries.values()),
        total_bytes=sum(entry["max_expanded_bytes"] for entry in entries.values()),
    )


def _verified_installation(
    installation: Path,
    limits: InstallationLimits,
) -> tuple[dict[str, Any], dict[str, Path]]:
    if not installation.is_absolute():
        raise AgentConfigurationError("native installation must be an absolute path")
    try:
        metadata = installation.lstat()
    except FileNotFoundError as error:
        raise AgentConfigurationError(f"native installation is absent: {installation}") from error
    if stat.S_ISLNK(metadata.st_mode) or not stat.S_ISDIR(metadata.st_mode):
        raise AgentConfigurationError("native installation must be a real directory")
    if metadata.st_uid != state.current_uid() or stat.S_IMODE(metadata.st_mode) != DIRECTORY_MODE:
        raise AgentConfigurationError("native installation must be owner-owned with mode 0700")

    manifest_path = installation / "installation-manifest.json"
    manifest = _json_object(
        _regular_file(
            manifest_path,
            "native installation manifest",
            max_bytes=MAX_MANIFEST_BYTES,
            modes=frozenset({0o600, 0o644}),
        ),
        "native installation manifest",
    )
    if manifest.get("schema_version") != 1:
        raise AgentConfigurationError("native installation manifest has an unsupported schema version")
    for field in ("agent_version", "microsandbox_version"):
        value = manifest.get(field)
        if not isinstance(value, str) or not VERSION_PATTERN.fullmatch(value):
            raise AgentConfigurationError(f"native installation manifest has an invalid {field}")
    files = manifest.get("files")
    if not isinstance(files, dict) or not files:
        raise AgentConfigurationError("native installation manifest has no file inventory")

    actual: dict[str, str] = {}
    entry_count = 0
    total_bytes = 0
    for path in sorted(installation.rglob("*")):
        entry_count += 1
        if entry_count > limits.entries:
            raise AgentConfigurationError("native installation contains too many entries")
        entry = path.lstat()
        if stat.S_ISLNK(entry.st_mode) or not (stat.S_ISDIR(entry.st_mode) or stat.S_ISREG(entry.st_mode)):
            raise AgentConfigurationError(f"native installation contains an unsafe entry: {path}")
        if entry.st_uid != state.current_uid():
            raise AgentConfigurationError(f"native installation entry is not owner-owned: {path}")
        if stat.S_ISREG(entry.st_mode) and path != manifest_path:
            relative = path.relative_to(installation).as_posix()
            digest, file_bytes = _verified_file_digest(
                path,
                f"native installation file {relative}",
                limits.total_bytes - total_bytes,
                limits.single_file_bytes,
            )
            total_bytes += file_bytes
            actual[relative] = digest
    if files != actual:
        raise AgentConfigurationError("native installation file inventory differs from its manifest")

    required = {
        "agent": installation / "agent/bin/octacity-agent",
        "octa": installation / "octa",
        "source_plugins": installation / "agent/source-plugins",
        "msb": installation / "microsandbox/bin/msb",
        "libkrunfw": installation / "microsandbox/lib/libkrunfw.5.dylib",
    }
    for name in ("octa", "source_plugins"):
        path = required[name]
        if not path.is_dir() or path.is_symlink():
            raise AgentConfigurationError(f"native installation lacks a real {name} directory")
    for name in ("agent", "msb", "libkrunfw"):
        if required[name].relative_to(installation).as_posix() not in files:
            raise AgentConfigurationError(f"native installation lacks verified {name}")
    return manifest, required


def _run_version(executable: Path, expected: str, description: str) -> None:
    try:
        result = subprocess.run(
            [str(executable), "--version"],
            check=False,
            capture_output=True,
            text=True,
            timeout=15,
        )
    except (OSError, subprocess.TimeoutExpired) as error:
        raise AgentConfigurationError(f"cannot inspect {description}: {error}") from error
    if result.returncode != 0 or result.stdout.strip() != expected:
        raise AgentConfigurationError(f"{description} version differs from the verified manifest")


def _toml_string(value: Path | str) -> str:
    return json.dumps(str(value))


def _toml_integer_lines(values: dict[str, int]) -> str:
    """Render an internal, identifier-keyed integer profile as TOML fields."""

    return "\n".join(f"{name} = {value}" for name, value in values.items())


def _toml_inline_integer_table(values: dict[str, int]) -> str:
    return "{ " + ", ".join(f"{name} = {value}" for name, value in values.items()) + " }"


def _render_config(
    *,
    root: Path,
    installation: Path,
    credential: Path,
    ca_certificate: Path,
    signing_public_key: str,
    microsandbox_version: str,
) -> bytes:
    agent_root = root / "agent"
    work_root = agent_root / "work"
    state_root = agent_root / "state"
    cache_root = agent_root / "cache"
    values = {
        "credential": _toml_string(credential),
        "ca": _toml_string(ca_certificate),
        "work": _toml_string(work_root),
        "state": _toml_string(state_root),
        "cache": _toml_string(cache_root),
        "octa": _toml_string(installation / "octa"),
        "plugins": _toml_string(installation / "agent/source-plugins"),
        "msb": _toml_string(installation / "microsandbox/bin/msb"),
        "libkrunfw": _toml_string(installation / "microsandbox/lib/libkrunfw.5.dylib"),
        "identity": _toml_string(f"microsandbox-{microsandbox_version}-linux-arm64-v1"),
        "signing_key": _toml_string(signing_public_key),
    }
    agent_limits = _toml_integer_lines(LOCAL_AGENT_LIMITS)
    output_limits = _toml_inline_integer_table(LOCAL_OUTPUT_LIMITS)
    cache_limits = _toml_integer_lines(LOCAL_CACHE_LIMITS)
    cache_capacity = _toml_integer_lines(LOCAL_CACHE_CAPACITY)
    storage_reserves = _toml_integer_lines(LOCAL_STORAGE_RESERVES)
    return f'''agent_id = "local-stand-agent"
server_url = "https://agent.localhost"
credential_file = {values["credential"]}
tls_ca_certificate_file = {values["ca"]}
work_root = {values["work"]}
state_root = {values["state"]}
octa_release_root = {values["octa"]}
source_plugins_dir = {values["plugins"]}
workload_identity_profiles = {{}}
enabled_runtime_modes = []
allow_native_execution = false
native_linux_pids_limit = 0
allow_host_execution = false
allow_unrestricted_network = false
allowed_network_hosts = []
allowed_upload_origins = ["https://objects.localhost"]
{agent_limits}
max_output_limits = {output_limits}
oci_engines = []
isolation_providers = []

[cache]
root = {values["cache"]}
allow_read = true
allow_write = true
allowed_remote_origins = ["https://cache.localhost"]
ca_certificate_file = {values["ca"]}
native_environment_identities = {{}}
{cache_limits}

[cache.capacity]
{cache_capacity}

[maintenance]
{storage_reserves}

[[virtualization_providers]]
provider = "microsandbox"
environment_identity = {values["identity"]}
executable = {values["msb"]}
libkrunfw = {values["libkrunfw"]}
metrics_sample_interval_seconds = 1

[server_signing_keys]
local-stand = {values["signing_key"]}

[labels]
os = "macos"
arch = "arm64"
guest = "linux-arm64"
environment = "local-stand"
'''.encode("utf-8")


def _ensure_exact_private_file(path: Path, contents: bytes, description: str) -> None:
    if path.exists() or path.is_symlink():
        if _regular_file(
            path,
            description,
            max_bytes=max(len(contents), 1),
            modes=frozenset({PRIVATE_FILE_MODE}),
        ) != contents:
            raise AgentConfigurationError(f"{description} differs from the verified input")
        return
    if state.publish_exclusive_private_file(path, contents):
        return
    if _regular(
        path,
        description,
        max_bytes=max(len(contents), 1),
        modes=frozenset({PRIVATE_FILE_MODE}),
    ) != contents:
        raise AgentConfigurationError(f"{description} differs from the verified input")


def _validate_with_agent(agent: Path, configuration: Path) -> None:
    environment = {
        **os.environ,
        "HTTP_PROXY": "http://127.0.0.1:9",
        "HTTPS_PROXY": "http://127.0.0.1:9",
        "ALL_PROXY": "http://127.0.0.1:9",
        "NO_PROXY": "",
    }
    try:
        result = subprocess.run(
            [str(agent), "validate", str(configuration)],
            check=False,
            capture_output=True,
            text=True,
            env=environment,
            timeout=30,
        )
    except (OSError, subprocess.TimeoutExpired) as error:
        raise AgentConfigurationError(f"cannot validate Agent configuration: {error}") from error
    if result.returncode != 0 or "configuration is valid" not in result.stdout:
        diagnostic = (result.stderr or result.stdout).strip()[-2048:]
        raise AgentConfigurationError(f"octacity-agent rejected generated configuration: {diagnostic}")


def _require_apple_silicon(system: str | None, machine: str | None) -> None:
    resolved_system = system or platform.system()
    resolved_machine = (machine or platform.machine()).lower()
    if resolved_system != "Darwin" or resolved_machine not in {"arm64", "aarch64"}:
        raise AgentConfigurationError("native Agent configuration requires Apple Silicon macOS")


def generate_agent_configuration(
    root: Path,
    native_installation: Path,
    *,
    repository: Path = REPOSITORY,
    home: Path = Path.home(),
    openssl: str = "openssl",
    host_system: str | None = None,
    host_machine: str | None = None,
) -> dict[str, str]:
    """Validate all trust inputs and publish one fail-closed Agent configuration."""

    _require_apple_silicon(host_system, host_machine)
    try:
        initialized = initializer.validate_initialized_state(
            root,
            repository=repository,
            home=home,
            openssl=openssl,
        )
    except state.StateError as error:
        raise AgentConfigurationError(str(error)) from error
    installation = native_installation.expanduser()
    manifest, installed = _verified_installation(
        installation,
        _installation_limits(repository),
    )
    installation = installation.resolve()
    _run_version(
        installed["agent"],
        f"octacity-agent {manifest['agent_version']}",
        "OctaCity Agent",
    )
    _run_version(
        installed["msb"],
        f"msb {manifest['microsandbox_version']}",
        "Microsandbox",
    )

    root = Path(initialized["root"])
    agent_root = root / "agent"
    with state.private_umask():
        state.ensure_private_directory(agent_root)
        credential = agent_root / "credential"
        _regular_file(
            credential,
            "Agent enrollment or registration credential",
            max_bytes=MAX_CREDENTIAL_BYTES,
            modes=frozenset({PRIVATE_FILE_MODE}),
        )
        for path in (
            agent_root / "work",
            agent_root / "state",
            agent_root / "cache",
            agent_root / "trust",
            root / "logs",
            root / "logs/agent",
            root / "config",
        ):
            state.ensure_private_directory(path)

        pki = {name: Path(path) for name, path in initialized["pki"].items()}
        source_ca = _regular_file(
            pki["ca_certificate"],
            "local stand CA certificate",
            max_bytes=MAX_CA_BYTES,
            modes=frozenset({0o644}),
        )
        trusted_ca = agent_root / "trust/local-ca.pem"
        _ensure_exact_private_file(trusted_ca, source_ca, "Agent CA certificate")

        encoded_key = _regular_file(
            pki["signing_public_key"],
            "server signing public key",
            max_bytes=MAX_PUBLIC_KEY_BYTES,
            modes=frozenset({0o644}),
        ).decode("ascii").strip()
        try:
            decoded_key = base64.b64decode(encoded_key, validate=True)
        except ValueError as error:
            raise AgentConfigurationError("server signing public key is not valid base64") from error
        if len(decoded_key) != 32:
            raise AgentConfigurationError("server signing public key is not an Ed25519 public key")

        contents = _render_config(
            root=root,
            installation=installation,
            credential=credential,
            ca_certificate=trusted_ca,
            signing_public_key=encoded_key,
            microsandbox_version=manifest["microsandbox_version"],
        )
        destination = root / "config/agent.toml"
        if destination.exists() or destination.is_symlink():
            _ensure_exact_private_file(destination, contents, "generated Agent configuration")
            _validate_with_agent(installed["agent"], destination)
        else:
            descriptor, temporary_name = tempfile.mkstemp(
                prefix=f".{destination.name}.staging-", dir=destination.parent
            )
            temporary = Path(temporary_name)
            try:
                os.fchmod(descriptor, PRIVATE_FILE_MODE)
                with os.fdopen(descriptor, "wb") as output:
                    output.write(contents)
                    output.flush()
                    os.fsync(output.fileno())
                descriptor = -1
                _validate_with_agent(installed["agent"], temporary)
                try:
                    os.link(temporary, destination, follow_symlinks=False)
                except FileExistsError:
                    _ensure_exact_private_file(
                        destination, contents, "generated Agent configuration"
                    )
            finally:
                if descriptor >= 0:
                    os.close(descriptor)
                temporary.unlink(missing_ok=True)

    return {
        "config": str(destination),
        "credential": str(credential),
        "work_root": str(agent_root / "work"),
        "state_root": str(agent_root / "state"),
        "cache_root": str(agent_root / "cache"),
        "log_root": str(root / "logs/agent"),
        "ca_certificate": str(trusted_ca),
    }


def parse_arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--root",
        type=Path,
        default=Path(os.environ.get("OCTACITY_LOCAL_STAND_ROOT", DEFAULT_ROOT)),
        help="initialized private host state root (default: %(default)s)",
    )
    parser.add_argument("--native-installation", required=True, type=Path)
    parser.add_argument("--openssl", default="openssl", help="OpenSSL executable")
    return parser.parse_args()


def main() -> int:
    arguments = parse_arguments()
    try:
        receipt = generate_agent_configuration(
            arguments.root,
            arguments.native_installation,
            openssl=arguments.openssl,
        )
    except (AgentConfigurationError, OSError, UnicodeError, ValueError) as error:
        raise SystemExit(f"local stand Agent configuration failed: {error}") from error
    print(json.dumps(receipt, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
