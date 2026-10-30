"""Stage an explicit Factory installation update under the ordinary lifecycle lock.

Only generated configuration changes owners here. PostgreSQL, object storage,
Agent credentials, work directories and demonstration receipts stay in place.
The config directory is exchanged atomically so a process interruption cannot
leave the server and Agent on different installation generations.
"""

from __future__ import annotations

from contextlib import contextmanager, nullcontext
import ctypes
import hashlib
import os
from pathlib import Path
import shutil
import sys
import subprocess
import tempfile
import tomllib
from typing import BinaryIO, Callable, Iterator

import configure_local_stand_agent as agent
import configure_local_stand_server as server
import init_local_stand as initializer
import local_stand_lifecycle as lifecycle
import local_stand_state as state


def validate_factory_installation(staging: Path, image: str) -> None:
    """Run the packaged server's full file validator without network or volumes.

    The ordinary image entrypoint materializes only ephemeral secrets and
    runtime files. No enrollment, schema migration or model invocation occurs.
    Diagnostics can contain private installation data, so report only status.
    """
    with tempfile.TemporaryDirectory(prefix=".factory-preflight-", dir=staging) as temporary:
        secrets = Path(temporary)
        for name in ("object-access-key", "object-secret-key", "signing-key", "agent-enrollment-key", "cache-credential-key"):
            shutil.copyfile(staging / "secrets" / name, secrets / name)
            (secrets / name).chmod(state.PRIVATE_FILE_MODE)
        for name in ("postgres-url", "factory-profiles.json"):
            shutil.copyfile(staging / "config/server" / name, secrets / name)
            (secrets / name).chmod(state.PRIVATE_FILE_MODE)
        container = "octacity-factory-preflight-" + secrets.name.removeprefix(".factory-preflight-")
        command = ["docker", "run", "--rm", "--network", "none", "--read-only", "--user", "0:0", "--name", container]
        for destination, size in (("secrets", "320m"), ("runtime", "512m"), ("delivery", "512m")):
            command.extend(("--tmpfs", f"/run/octacity-{destination}:size={size},mode=0700"))
        for source, destination in ((secrets, "/run/secrets"),
                                    (staging / "config/server", "/run/octacity"),
                                    (staging / "config/server", "/run/octacity-factory-connectors")):
            command.extend(("--mount", f"type=bind,src={source},dst={destination},readonly"))
        command.extend((image, "validate", "/run/octacity/server.toml"))
        try:
            result = subprocess.run(command, check=False, capture_output=True, text=True, timeout=120)
        except subprocess.TimeoutExpired:
            # Killing the Docker client does not stop its daemon-owned container.
            subprocess.run(["docker", "rm", "--force", container], check=False,
                capture_output=True, text=True, timeout=30)
            raise state.StateError(f"packaged server preflight timed out (temporary validator: {container})") from None
        if result.returncode != 0 or "server configuration is valid" not in result.stdout:
            raise state.StateError("packaged server rejected the Factory installation")


def _exchange(left: Path, right: Path) -> None:
    """Exchange two owned directories on one filesystem, or fail closed."""
    library = ctypes.CDLL(None, use_errno=True)
    if sys.platform == "darwin":
        rename = library.renamex_np
        rename.argtypes = [ctypes.c_char_p, ctypes.c_char_p, ctypes.c_uint]
        result = rename(os.fsencode(left), os.fsencode(right), 2)  # RENAME_SWAP
    elif sys.platform.startswith("linux"):
        rename = library.renameat2
        rename.argtypes = [ctypes.c_int, ctypes.c_char_p, ctypes.c_int, ctypes.c_char_p, ctypes.c_uint]
        result = rename(-100, os.fsencode(left), -100, os.fsencode(right), 2)  # AT_FDCWD, RENAME_EXCHANGE
    else:
        raise state.StateError("atomic installation updates are unavailable on this host")
    if result != 0:
        raise OSError(ctypes.get_errno(), "cannot atomically exchange the Factory installation")


def _snapshot(directory: Path, *, copy_to: Path | None = None) -> dict[str, tuple[int, str] | None]:
    """Fingerprint configuration and optionally stream-copy it to private staging."""
    def fingerprint(relative: Path, source: BinaryIO, size: int) -> str:
        target = copy_to / relative if copy_to is not None else None
        if target is not None:
            for parent in reversed(relative.parents):
                state.ensure_private_directory(copy_to / parent)
        with target.open("xb") if target is not None else nullcontext() as output:
            if target is not None:
                target.chmod(state.PRIVATE_FILE_MODE)
            digest = hashlib.sha256()
            consumed = 0
            while chunk := source.read(64 * 1024):
                consumed += len(chunk)
                if consumed > size:
                    raise state.StateError("Factory configuration changed while being read")
                digest.update(chunk)
                if output is not None:
                    output.write(chunk)
            return digest.hexdigest()

    # config/server adds two levels to each executable bundle's depth bound.
    files = state.map_private_tree(directory, fingerprint,
        max_depth=6, max_entries=4096, max_bytes=2 * 1024**3)
    if copy_to is not None:
        for name, record in files.items():
            target = copy_to / name
            if record is None:
                state.ensure_private_directory(target)
            else:
                target.chmod(record[0])
    return files


def installed_factory_inputs(root: Path, exports: Path) -> dict[str, Path | None]:
    """Select the effective installation, independently of its original seed files.

    The caller supplies private temporary storage for reference-only exports.
    Configurators retain validation and ownership of their file encodings;
    this selection neither updates configuration nor renews credentials.
    """
    bundle = root / "config/server"
    paths = {"factory_profiles": bundle / "factory-profiles.json",
        "factory_agent_profile": root / "config/factory-agent-profile.toml",
        "factory_runtime_bundle": bundle / "factory-runtime",
        "factory_delivery_bundle": bundle / "factory-delivery"}
    selected = {name: path if path.exists() or path.is_symlink() else None for name, path in paths.items()}
    selected["factory_signal_connectors"] = server.export_installed_signal_connectors(bundle, exports / "signal-connectors.json")
    selected["factory_issue_mcp_readers"] = server.export_installed_issue_mcp_readers(bundle, exports / "issue-mcp-readers.json")
    return selected


@contextmanager
def factory_installation_update(
    root: Path,
    installation: Path,
    *,
    owner: lifecycle.LifecycleState,
    factory_profiles: Path,
    factory_agent_profile: Path,
    preflight: Callable[[Path], None],
    stop: Callable[[], None],
    factory_runtime_bundle: Path | None = None,
    factory_delivery_bundle: Path | None = None,
    factory_signal_connectors: Path | None = None,
    factory_issue_mcp_readers: Path | None = None,
    openssl: str = "openssl",
) -> Iterator[None]:
    """Preflight before stopping; restore the old configuration on startup failure.

    The caller owns ordinary shutdown/startup and must stop failed candidate
    services before leaving this context. No mutation API or Work admission is
    invoked here. Omitted optional bundles retain their installed contents.
    """
    owner.require_exclusive_lock()
    root = root.resolve()
    initialized = initializer.validate_initialized_state(root, openssl=openssl)
    original = _snapshot(root / "config")
    installed_config = tomllib.loads(state.read_regular_file(
        root / "config/agent.toml", "installed Agent configuration", max_bytes=256 * 1024,
        modes=frozenset({state.PRIVATE_FILE_MODE}),
    ).decode())
    old_installation = Path(installed_config["octa_release_root"]).parent
    old_policy = root / "config/factory-agent-profile.toml"
    # An explicit update is not permission to overwrite unrecognized edits.
    agent.generate_agent_configuration(root, old_installation, openssl=openssl,
        factory_agent_profile=old_policy if old_policy.exists() else None)
    with tempfile.TemporaryDirectory(prefix=".factory-update-", dir=root) as temporary:
        staging = Path(temporary)
        inherited = installed_factory_inputs(root, staging)
        server.generate_server_configuration(root, old_installation, openssl=openssl,
            **{name: path for name, path in inherited.items() if name != "factory_agent_profile"})
        for group in ("credentials", "pki"):
            for source in initialized[group].values():
                source = Path(source)
                destination = staging / source.relative_to(root)
                state.ensure_private_directory(destination.parent)
                shutil.copyfile(source, destination)
                destination.chmod(source.stat().st_mode & 0o777)
        if _snapshot(root / "config", copy_to=staging / "config") != original:
            raise state.StateError("installation changed during staging; retry with reviewed inputs")
        shutil.rmtree(staging / "config/server")
        (staging / "config/agent.toml").unlink()
        (staging / "config/factory-agent-profile.toml").unlink(missing_ok=True)
        server.generate_server_configuration(staging, installation, openssl=openssl,
            factory_profiles=factory_profiles,
            factory_runtime_bundle=factory_runtime_bundle or inherited["factory_runtime_bundle"],
            factory_delivery_bundle=factory_delivery_bundle or inherited["factory_delivery_bundle"],
            factory_signal_connectors=factory_signal_connectors or inherited["factory_signal_connectors"],
            factory_issue_mcp_readers=factory_issue_mcp_readers or inherited["factory_issue_mcp_readers"])
        integrated = staging / "config/integrations/server.toml"
        if integrated.exists():
            previous_base = (root / "config/server/server.toml").read_bytes()
            contents = integrated.read_bytes()
            suffix = contents[len(previous_base):]
            if not contents.startswith(previous_base) or set(tomllib.loads(suffix.decode())) != {"integration_management"}:
                raise state.StateError("integration server configuration contains unrecognized edits")
            integrated.write_bytes((staging / "config/server/server.toml").read_bytes() + suffix)
        agent.preflight_factory_agent_profile(staging, installation, factory_agent_profile)
        agent.generate_agent_configuration(root, installation, openssl=openssl,
            factory_agent_profile=factory_agent_profile, staging_directory=staging / "config")
        preflight(staging)
        if _snapshot(root / "config") != original:
            raise state.StateError("installation changed during preflight; retry with reviewed inputs")
        changed = _snapshot(staging / "config") != original
        if changed:
            # Probe filesystem support before stopping any usable service.
            for name in ("exchange-left", "exchange-right"):
                state.ensure_private_directory(staging / name)
            _exchange(staging / "exchange-left", staging / "exchange-right")
            stop()
            if _snapshot(root / "config") != original:
                raise state.StateError("installation changed during preflight; retry with reviewed inputs")
            _exchange(root / "config", staging / "config")
        try:
            yield
        except BaseException:
            if changed:
                _exchange(root / "config", staging / "config")
            raise
