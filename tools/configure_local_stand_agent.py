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
import tomllib
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
MAX_FACTORY_AGENT_PROFILE_BYTES = 256 * 1024
# These ceilings equal the aggregate extraction policy in the staging
# allowlist. Configuration verification must never turn a malformed bundle
# into an unbounded filesystem walk or whole-file allocation.
INSTALLATION_FILE_MODES = frozenset({0o600, 0o644, 0o700, 0o755})
HASH_CHUNK_BYTES = 1024 * 1024
VERSION_PATTERN = re.compile(r"^[0-9]+\.[0-9]+\.[0-9]+(?:[-+][A-Za-z0-9.-]+)?$")
LOCAL_AGENT_NAME = "local-stand-agent"
LOCAL_AGENT_LABELS = {
    "os": "macos",
    "arch": "arm64",
    "guest": "linux-arm64",
    "environment": "local-stand",
}
LOCAL_HOST_PLATFORM = {"os": "macos", "architecture": "arm64"}
LOCAL_CODEX_VERSION = "0.161.0"
LOCAL_CODEX_PLATFORM = "linux-aarch64"
MICROSANDBOX_STATE_PARENT = Path("/tmp").resolve()

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


@dataclass(frozen=True)
class AgentLaunchPlan:
    """Validated executable and state paths owned by the generated profile."""

    configuration: Path
    state_root: Path
    agent: Path
    msb: Path
    libkrunfw: Path


class AgentConfigurationError(RuntimeError):
    """The native local-stand Agent configuration cannot be trusted."""


def _verify_factory_executable(tool: Any, *, allow_standalone: bool = False) -> None:
    """Verify one immutable operator executable, independently of its purpose."""
    required = {"path", "version", "platform", "sha256"}
    if not isinstance(tool, dict) or not required <= set(tool) or set(tool) - required - ({"standalone", "guest_filename"} if allow_standalone else set()):
        raise ValueError("tool")
    if "standalone" in tool and type(tool["standalone"]) is not bool:
        raise ValueError("tool kind")
    if "guest_filename" in tool and (not isinstance(tool["guest_filename"], str) or not tool["guest_filename"]):
        raise ValueError("tool guest filename")
    if not all(isinstance(tool[key], str) and tool[key] for key in required) or not VERSION_PATTERN.fullmatch(tool["version"]) or not re.fullmatch("[a-z0-9_-]+", tool["platform"]) or not re.fullmatch("[0-9a-f]{64}", tool["sha256"]):
        raise ValueError("tool")
    executable = Path(tool["path"])
    if not executable.is_absolute():
        raise ValueError("path")
    digest, _ = _verified_file_digest(executable, "Factory Agent executable", 256 * 1024 * 1024, 256 * 1024 * 1024)
    if digest != tool["sha256"] or not os.access(executable, os.X_OK):
        raise ValueError("identity")


def read_factory_agent_profile(path: Path) -> dict[str, Any]:
    """Read a separate operator policy and verify its selected executable bytes."""
    try:
        contents = state.read_regular_file(
            path, "Factory Agent profile", max_bytes=MAX_FACTORY_AGENT_PROFILE_BYTES,
            modes=frozenset({PRIVATE_FILE_MODE}),
        )
        document = tomllib.loads(contents.decode("utf-8"))
        required = {"version", "factory_permissions", "tool_executables", "allowed_network_hosts"}
        if not required <= set(document) or set(document) - required - {"change_set_capture", "factory_credential_profiles", "protected_reader"} or type(document["version"]) is not int or document["version"] != 1:
            raise ValueError("shape")
        grants, tools, hosts = document["factory_permissions"], document["tool_executables"], document["allowed_network_hosts"]
        if not isinstance(grants, dict) or not isinstance(tools, dict) or not 1 <= len(tools) <= 32 or not isinstance(hosts, list) or len(hosts) > 128:
            raise ValueError("shape")
        if any(not isinstance(host, str) or not host or host.strip() != host or any(ord(char) < 32 for char in host) for host in hosts) or len(hosts) != len(set(hosts)):
            raise ValueError("hosts")
        for product, tool in tools.items():
            if not product:
                raise ValueError("tool")
            _verify_factory_executable(tool, allow_standalone=True)
        if "change_set_capture" in document:
            capture = document["change_set_capture"]
            if not isinstance(capture, dict) or set(capture) != {"provider", "executable"} or capture["provider"] != "git":
                raise ValueError("capture")
            _verify_factory_executable(capture["executable"])
        for reference in grants.get("executables", []):
            tool = tools.get(reference.get("identity"))
            if tool is None or reference.get("version") != tool["version"] or reference.get("sha256") != tool["sha256"]:
                raise ValueError("identity")
        credentials = document.get("factory_credential_profiles", {})
        if not isinstance(credentials, dict) or len(credentials) > 32 or any(
            not isinstance(name, str) or not name or not _credential_reference(reference)
            for name, reference in credentials.items()
        ):
            raise ValueError("credential references")
        if "protected_reader" in document and not isinstance(document["protected_reader"], dict):
            raise ValueError("reader")
        _toml_value(grants)
        return document
    except (OSError, UnicodeError, ValueError, TypeError, AttributeError, RecursionError, state.StateError, AgentConfigurationError) as error:
        raise AgentConfigurationError("Factory Agent profile is invalid") from error


def _credential_reference(reference: Any) -> bool:
    """Keep existing variable files and explicit native JSON references distinct."""
    if isinstance(reference, str):
        return Path(reference).is_absolute()
    if not isinstance(reference, dict) or not {"path", "destination"} <= set(reference) or set(reference) - {"path", "destination", "sensitive_fields", "persist_updates"}:
        return False
    path, destination = reference["path"], reference["destination"]
    if "persist_updates" in reference and type(reference["persist_updates"]) is not bool:
        return False
    fields = reference.get("sensitive_fields")
    if "sensitive_fields" in reference and not (
        isinstance(fields, list) and 0 < len(fields) <= 64
        and all(isinstance(field, str) and len(field.encode()) <= 1024
                and (not field or field.startswith("/")) for field in fields)
    ):
        return False
    return (
        isinstance(path, str) and Path(path).is_absolute()
        and isinstance(destination, str) and 0 < len(destination.encode()) <= 1024
        and not any(character in destination for character in ("\\", ":", "\0"))
        and not destination.startswith("/")
        and all(part not in ("", ".", "..") for part in destination.split("/"))
    )


def _factory_policy_matches(document: dict[str, Any], profile: dict[str, Any]) -> bool:
    """Compare the complete installed local policy, including optional capture."""
    return all(document.get(name) == profile.get(name) for name in (
        "factory_permissions", "tool_executables", "allowed_network_hosts", "change_set_capture", "factory_credential_profiles", "protected_reader",
    ))


def _toml_value(value: Any) -> str:
    """Render bounded protocol policy values as one TOML inline value."""
    if isinstance(value, dict):
        return "{ " + ", ".join(f"{json.dumps(key)} = {_toml_value(item)}" for key, item in value.items()) + " }"
    if isinstance(value, list):
        return "[" + ", ".join(_toml_value(item) for item in value) + "]"
    if isinstance(value, (str, bool)) or type(value) is int:
        return json.dumps(value)
    raise ValueError("unsupported policy value")


def microsandbox_state_root(root: Path) -> Path:
    """Return a private, stable path short enough for macOS Unix sockets."""

    identity = hashlib.sha256(os.fsencode(root.resolve(strict=False))).hexdigest()[:12]
    return MICROSANDBOX_STATE_PARENT / f"ocm-{state.current_uid()}-{identity}"


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


def _environment_identity(microsandbox_version: str) -> str:
    return f"microsandbox-{microsandbox_version}-linux-arm64-v1"


def _virtualization_provider(
    installation: Path, microsandbox_version: str
) -> dict[str, object]:
    return {
        "provider": "microsandbox",
        "environment_identity": _environment_identity(microsandbox_version),
        "executable": str(installation / "microsandbox/bin/msb"),
        "libkrunfw": str(installation / "microsandbox/lib/libkrunfw.5.dylib"),
        "metrics_sample_interval_seconds": 1,
    }


def _render_config(
    *,
    root: Path,
    installation: Path,
    credential: Path,
    ca_certificate: Path,
    signing_public_key: str,
    microsandbox_version: str,
    factory_profile: dict[str, Any] | None = None,
    temporary_state_root: Path | None = None,
) -> bytes:
    agent_root = root / "agent"
    work_root = agent_root / "work"
    state_root = temporary_state_root or microsandbox_state_root(root)
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
        "identity": _toml_string(_environment_identity(microsandbox_version)),
        "signing_key": _toml_string(signing_public_key),
    }
    codex_fixture = installation / "agent/share/local-stand-codex-fixture"
    codex_contents = _regular_file(
        codex_fixture,
        "local-stand Codex fixture",
        max_bytes=4 * 1024 * 1024,
        modes=frozenset({0o755}),
    )
    codex_digest = hashlib.sha256(codex_contents).hexdigest()
    factory_prefix = ""
    selected_hosts = "[]"
    tool_toml = f'''[tool_executables.codex-cli]
path = {_toml_string(codex_fixture)}
version = "{LOCAL_CODEX_VERSION}"
platform = "{LOCAL_CODEX_PLATFORM}"
sha256 = "{codex_digest}"
'''
    if factory_profile is not None:
        factory_prefix = f'factory_permissions = {_toml_value(factory_profile["factory_permissions"])}\ntool_executables = {_toml_value(factory_profile["tool_executables"])}\n'
        if "change_set_capture" in factory_profile:
            factory_prefix += f'change_set_capture = {_toml_value(factory_profile["change_set_capture"])}\n'
        if "factory_credential_profiles" in factory_profile:
            factory_prefix += f'factory_credential_profiles = {_toml_value(factory_profile["factory_credential_profiles"])}\n'
        if "protected_reader" in factory_profile:
            factory_prefix += f'protected_reader = {_toml_value(factory_profile["protected_reader"])}\n'
        selected_hosts = _toml_value(factory_profile["allowed_network_hosts"])
        tool_toml = ""
    agent_limits = _toml_integer_lines(LOCAL_AGENT_LIMITS)
    output_limits = _toml_inline_integer_table(LOCAL_OUTPUT_LIMITS)
    cache_limits = _toml_integer_lines(LOCAL_CACHE_LIMITS)
    cache_capacity = _toml_integer_lines(LOCAL_CACHE_CAPACITY)
    storage_reserves = _toml_integer_lines(LOCAL_STORAGE_RESERVES)
    labels = "\n".join(
        f"{name} = {json.dumps(value)}" for name, value in LOCAL_AGENT_LABELS.items()
    )
    return f'''{factory_prefix}agent_id = "{LOCAL_AGENT_NAME}"
server_url = "{initializer.gateway_origin("agent.localhost")}"
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
allowed_network_hosts = {selected_hosts}
allowed_upload_origins = ["{initializer.gateway_origin("objects.localhost")}"]
{agent_limits}
max_output_limits = {output_limits}
oci_engines = []
isolation_providers = []

[cache]
root = {values["cache"]}
allow_read = true
allow_write = true
allowed_remote_origins = ["{initializer.gateway_origin("cache.localhost")}"]
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

{tool_toml}

[labels]
{labels}
'''.encode("utf-8")


def validated_launch_plan(
    root: Path, installation: Path, receipt: dict[str, str]
) -> AgentLaunchPlan:
    """Validate the generated profile once at its owning module boundary."""

    expected_config = root / "config/agent.toml"
    expected_state = microsandbox_state_root(root)
    if receipt.get("config") != str(expected_config) or receipt.get("state_root") != str(
        expected_state
    ):
        raise AgentConfigurationError("Agent configuration receipt targets unexpected state")
    try:
        document = tomllib.loads(
            _regular_file(
                expected_config,
                "generated Agent configuration",
                max_bytes=256 * 1024,
                modes=frozenset({PRIVATE_FILE_MODE}),
            ).decode("utf-8")
        )
    except (UnicodeError, tomllib.TOMLDecodeError) as cause:
        raise AgentConfigurationError("generated Agent configuration is invalid") from cause
    disabled = {
        "enabled_runtime_modes": [],
        "allow_native_execution": False,
        "allow_host_execution": False,
        "oci_engines": [],
        "isolation_providers": [],
    }
    providers = document.get("virtualization_providers")
    if any(document.get(name) != value for name, value in disabled.items()):
        raise AgentConfigurationError("Agent configuration enables a fallback execution capability")
    if not isinstance(providers, list) or len(providers) != 1:
        raise AgentConfigurationError("Agent configuration must enable exactly one virtualization provider")
    if document.get("agent_id") != LOCAL_AGENT_NAME or document.get("labels") != LOCAL_AGENT_LABELS:
        raise AgentConfigurationError("Agent configuration advertises an unexpected identity")

    installation = installation.resolve()
    codex_fixture = installation / "agent/share/local-stand-codex-fixture"
    expected_tool = {
        "codex-cli": {
            "path": str(codex_fixture),
            "version": LOCAL_CODEX_VERSION,
            "platform": LOCAL_CODEX_PLATFORM,
            "sha256": hashlib.sha256(
                _regular_file(
                    codex_fixture,
                    "local-stand Codex fixture",
                    max_bytes=4 * 1024 * 1024,
                    modes=frozenset({0o755}),
                )
            ).hexdigest(),
        }
    }
    profile_path = receipt.get("factory_agent_profile")
    if profile_path is not None:
        if profile_path != str(root / "config/factory-agent-profile.toml"):
            raise AgentConfigurationError("Factory Agent profile targets unexpected state")
        profile = read_factory_agent_profile(Path(profile_path))
        expected_tool = profile["tool_executables"]
        if not _factory_policy_matches(document, profile):
            raise AgentConfigurationError("Factory Agent configuration differs from its installed profile")
    elif any(document.get(name) is not None for name in ("factory_permissions", "change_set_capture", "factory_credential_profiles", "protected_reader")):
        raise AgentConfigurationError("Factory Agent configuration has no independently installed profile")
    if document.get("tool_executables") != expected_tool:
        raise AgentConfigurationError("Agent configuration has an unexpected tool executable")
    manifest = _json_object(
        _regular_file(
            installation / "installation-manifest.json",
            "native installation manifest",
            max_bytes=MAX_MANIFEST_BYTES,
            modes=frozenset({0o600, 0o644}),
        ),
        "native installation manifest",
    )
    version = manifest.get("microsandbox_version")
    if not isinstance(version, str):
        raise AgentConfigurationError("native installation manifest has no Microsandbox version")
    expected_provider = _virtualization_provider(installation, version)
    if providers[0] != expected_provider:
        raise AgentConfigurationError("Agent configuration differs from the pinned Microsandbox provider")
    return AgentLaunchPlan(
        expected_config,
        expected_state,
        installation / "agent/bin/octacity-agent",
        installation / "microsandbox/bin/msb",
        installation / "microsandbox/lib/libkrunfw.5.dylib",
    )


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


def _validate_with_agent(agent: Path, configuration: Path, *, private_factory: bool = False) -> None:
    """Validate unchanged settings without opening a running Agent's mutable stores."""
    environment = {
        **os.environ,
        "HTTP_PROXY": "http://127.0.0.1:9",
        "HTTPS_PROXY": "http://127.0.0.1:9",
        "ALL_PROXY": "http://127.0.0.1:9",
        "NO_PROXY": "",
    }
    try:
        with tempfile.TemporaryDirectory(prefix="ocv-", dir=MICROSANDBOX_STATE_PARENT) as temporary:
            root = Path(temporary)
            document = tomllib.loads(configuration.read_text())
            for field in ("work_root", "state_root"):
                path = root / field
                state.ensure_private_directory(path)
                document[field] = str(path)
            cache = root / "cache"
            state.ensure_private_directory(cache)
            document["cache"]["root"] = str(cache)
            credential = root / "credential"
            state.publish_exclusive_private_file(credential, _regular_file(Path(document["credential_file"]),
                "validation credential", max_bytes=MAX_CREDENTIAL_BYTES, modes=frozenset({PRIVATE_FILE_MODE})))
            document["credential_file"] = str(credential)
            candidate = root / "agent.toml"
            state.publish_exclusive_private_file(candidate,
                ("\n".join(f"{key} = {_toml_value(value)}" for key, value in document.items()) + "\n").encode())
            result = subprocess.run(
                [str(agent), "validate", str(candidate)], check=False,
                capture_output=True, text=True, env=environment, timeout=30,
            )
    except (OSError, subprocess.TimeoutExpired) as error:
        raise AgentConfigurationError(f"cannot validate Agent configuration: {error}") from error
    if result.returncode != 0 or "configuration is valid" not in result.stdout:
        if private_factory:
            raise AgentConfigurationError("octacity-agent rejected the Factory configuration")
        diagnostic = (result.stderr or result.stdout).strip()[-2048:]
        raise AgentConfigurationError(f"octacity-agent rejected generated configuration: {diagnostic}")


def preflight_factory_agent_profile(root: Path, installation: Path, profile_path: Path) -> None:
    """Validate installed authority with the real Agent before enrollment or launch.

    The CLI receives only disposable owned work, state, cache and credential
    paths. Public trust inputs and immutable installed executables are reused.
    """
    profile = read_factory_agent_profile(profile_path)
    existing = root / "config/agent.toml"
    if existing.exists() or existing.is_symlink():
        try:
            document = tomllib.loads(_regular_file(existing, "installed Agent configuration", max_bytes=256 * 1024, modes=frozenset({PRIVATE_FILE_MODE})).decode("utf-8"))
        except (OSError, UnicodeError, ValueError) as error:
            raise AgentConfigurationError("installed Agent configuration is invalid") from error
        if not _factory_policy_matches(document, profile):
            raise AgentConfigurationError("Factory Agent installation cannot change an existing profile; reset the disposable stand first")
    manifest, installed = _verified_installation(installation, _installation_limits(REPOSITORY))
    with tempfile.TemporaryDirectory(prefix="ocf-", dir=MICROSANDBOX_STATE_PARENT) as temporary:
        validation_root = Path(temporary).resolve()
        for relative in ("agent", "agent/work", "agent/cache", "agent/state", "agent/trust"):
            state.ensure_private_directory(validation_root / relative)
        credential = validation_root / "agent/credential"
        credential.write_text("disposable-preflight-credential\n")
        credential.chmod(PRIVATE_FILE_MODE)
        ca = validation_root / "agent/trust/local-ca.pem"
        ca.write_bytes(_regular_file(root / "pki/local-ca.pem", "stand CA", max_bytes=MAX_CA_BYTES, modes=frozenset({0o644})))
        ca.chmod(PRIVATE_FILE_MODE)
        signing_key = _regular_file(root / "pki/server-signing-public-key", "stand signing public key", max_bytes=MAX_PUBLIC_KEY_BYTES, modes=frozenset({0o644})).decode("ascii").strip()
        contents = _render_config(root=validation_root, installation=installation.resolve(), credential=credential,
            ca_certificate=ca, signing_public_key=signing_key, microsandbox_version=manifest["microsandbox_version"],
            factory_profile=profile, temporary_state_root=validation_root / "agent/state")
        config = validation_root / "agent.toml"
        config.write_bytes(contents)
        config.chmod(PRIVATE_FILE_MODE)
        _validate_with_agent(installed["agent"], config, private_factory=True)


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
    factory_agent_profile: Path | None = None,
    staging_directory: Path | None = None,
) -> dict[str, str]:
    """Validate and publish a configuration, optionally staging it for an update.

    Staging changes only the output directory; credential, trust and work paths
    always refer to the existing installation.
    """

    _require_apple_silicon(host_system, host_machine)
    factory_profile = read_factory_agent_profile(factory_agent_profile) if factory_agent_profile is not None else None
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
    configuration_directory = root / "config"
    if staging_directory is not None:
        state.validate_private_directory(staging_directory)
        staging_directory = staging_directory.resolve()
        if not staging_directory.is_relative_to(root) or staging_directory == root:
            raise AgentConfigurationError("Agent staging directory must be a private directory inside the stand root")
        configuration_directory = staging_directory
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
            agent_root / "cache",
            agent_root / "trust",
            microsandbox_state_root(root),
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
            factory_profile=factory_profile,
        )
        destination = configuration_directory / "agent.toml"
        if destination.exists() or destination.is_symlink():
            _ensure_exact_private_file(destination, contents, "generated Agent configuration")
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
                _validate_with_agent(installed["agent"], temporary, private_factory=factory_profile is not None)
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
        if factory_profile is not None:
            normalized = "\n".join(f"{json.dumps(name)} = {_toml_value(value)}" for name, value in factory_profile.items()).encode("utf-8") + b"\n"
            _ensure_exact_private_file(configuration_directory / "factory-agent-profile.toml", normalized, "installed Factory Agent profile")

    receipt = {
        "config": str(destination),
        "credential": str(credential),
        "work_root": str(agent_root / "work"),
        "state_root": str(microsandbox_state_root(root)),
        "cache_root": str(agent_root / "cache"),
        "log_root": str(root / "logs/agent"),
        "ca_certificate": str(trusted_ca),
    }
    if factory_profile is not None:
        receipt["factory_agent_profile"] = str(configuration_directory / "factory-agent-profile.toml")
    return receipt


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
    parser.add_argument("--factory-agent-profile", type=Path, help="separate private local Factory Agent policy")
    return parser.parse_args()


def main() -> int:
    arguments = parse_arguments()
    try:
        receipt = generate_agent_configuration(
            arguments.root,
            arguments.native_installation,
            openssl=arguments.openssl,
            factory_agent_profile=arguments.factory_agent_profile,
        )
    except (AgentConfigurationError, OSError, UnicodeError, ValueError) as error:
        raise SystemExit(f"local stand Agent configuration failed: {error}") from error
    print(json.dumps(receipt, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
