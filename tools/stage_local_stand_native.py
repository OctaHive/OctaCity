#!/usr/bin/env python3
"""Prepare and atomically install the pinned local-stand native bundle."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import platform
import shutil
import stat
from string import Template
import subprocess
import tempfile
import tomllib
from collections.abc import Callable
from typing import Any

from local_stand_build_inputs import (
    git_revision,
    stage_octacity_source,
    stage_octa_source,
)
from pinned_source_archive import (
    SourceArchiveError,
    download,
    extract_tar_archive,
    verify_github_release_asset,
    verify_release_ref,
)
import verify_local_stand_context as context
import verify_local_stand_inputs as inputs
import validate_octa_release_contract as release_contract


REPOSITORY = Path(__file__).resolve().parents[1]
DEFAULT_MANIFEST = REPOSITORY / "deployment/local-stand/inputs.json"
DEFAULT_ALLOWLIST = REPOSITORY / "deployment/local-stand/staging-allowlist.json"
DEFAULT_AGENT_FIXTURE = REPOSITORY / "deployment/local-stand/fixtures/agent.toml.in"
MAX_RELEASE_FILES = 4096
MAX_CHECKSUM_BYTES = 4 * 1024 * 1024
MAX_JOB_SPEC_POLICY_BYTES = 1024 * 1024
DEFAULT_JOB_SPEC_VALIDITY_SECONDS = 900
POLICY_HELPER_ROLE = "octacity_release_harness"


class NativeStageError(RuntimeError):
    """A native input set or installation cannot be prepared safely."""


def sha256(path: Path) -> str:
    """Return the lowercase SHA-256 digest of one regular file."""

    digest = hashlib.sha256()
    with path.open("rb") as source:
        while chunk := source.read(1024 * 1024):
            digest.update(chunk)
    return digest.hexdigest()


def load_policy(
    manifest: Path, allowlist: Path, repository: Path, revision: str
) -> tuple[dict[str, Any], dict[str, dict[str, Any]]]:
    """Load and cross-check immutable inputs and the native staging allowlist."""

    document = inputs.load_manifest(manifest)
    tags = inputs.validate_manifest(document, revision)
    inputs.validate_repository_pins(document, tags, repository)
    entries = context.load_staging_allowlist(allowlist, manifest)
    return document, entries


def input_set_name(document: dict[str, Any], revision: str) -> str:
    """Return the immutable cache key for all native input archives."""

    native = document["native"]
    return (
        f"inputs-agent-{native['octacity_agent']['version']}-{revision[:12]}"
        f"-octa-{native['octa']['version']}-msb-{native['microsandbox']['version']}"
    )


def installation_name(document: dict[str, Any], revision: str) -> str:
    """Return the versioned native installation directory name."""

    return input_set_name(document, revision).removeprefix("inputs-")


def _atomic_directory(destination: Path, populate: Callable[[Path], None]) -> Path:
    """Populate a private sibling directory and publish it with one rename."""

    if destination.exists() or destination.is_symlink():
        raise NativeStageError(f"destination already exists: {destination}")
    destination.parent.mkdir(parents=True, exist_ok=True)
    temporary = Path(
        tempfile.mkdtemp(prefix=f".{destination.name}.staging-", dir=destination.parent)
    )
    temporary.chmod(0o700)
    try:
        populate(temporary)
        os.replace(temporary, destination)
    except Exception:
        shutil.rmtree(temporary, ignore_errors=True)
        raise
    return destination.resolve()


def _download_archive(source: dict[str, Any], destination: Path) -> None:
    policy = {
        "archive_url": source["release_url"],
        "sha256": source["sha256"],
        "archive_max_bytes": source["archive_max_bytes"],
        "tag": source.get("release_name", source.get("tag", source["version"])),
    }
    download(policy, destination)


def _build_agent_archive(
    destination: Path,
    policy_helper: Path,
    document: dict[str, Any],
    revision: str,
    repository: Path,
) -> None:
    """Build the Agent and source plugin from an exact archived Git revision."""

    native = document["native"]
    with tempfile.TemporaryDirectory(prefix="octacity-native-build-") as value:
        build_root = Path(value)
        checkout = build_root / "octacity"
        stage_octacity_source(repository, checkout, revision)
        octa = native["octa"]
        stage_octa_source(document, build_root / "octa")
        target = repository / "target/local-stand-native"
        environment = {**os.environ, "CARGO_TARGET_DIR": str(target)}
        subprocess.run(
            [
                "cargo",
                "build",
                "--locked",
                "--release",
                "-p",
                "octacity-agent",
                "-p",
                "octacity-source-git",
                "-p",
                "octacity-release-harness",
            ],
            cwd=checkout,
            env=environment,
            check=True,
        )
        metadata = build_root / "source-plugin-package.json"
        result = subprocess.run(
            [str(target / "release/octacity-source-git"), "package-metadata"],
            check=True,
            capture_output=True,
        )
        metadata.write_bytes(result.stdout)
        subprocess.run(
            [
                "python3",
                str(checkout / "tools/package_release.py"),
                "--repository",
                str(checkout),
                "--platform",
                native["octacity_agent"]["platform"],
                "--version",
                native["octacity_agent"]["version"],
                "--agent",
                str(target / "release/octacity-agent"),
                "--source-git",
                str(target / "release/octacity-source-git"),
                "--source-metadata",
                str(metadata),
                "--octacity-revision",
                revision,
                "--octa-revision",
                octa["source_revision"],
                "--codex-fixture",
                str(checkout / "deployment/local-stand/fixtures/codex-fixture.sh"),
                "--output",
                str(destination),
            ],
            check=True,
        )
        shutil.copyfile(
            target / "release/octacity-release-harness",
            policy_helper,
        )
        policy_helper.chmod(0o755)
        policy_helper.with_name(f"{policy_helper.name}.sha256").write_text(
            f"{sha256(policy_helper)}  {policy_helper.name}\n",
            encoding="utf-8",
        )


def prepare_inputs(
    destination: Path,
    document: dict[str, Any],
    entries: dict[str, dict[str, Any]],
    revision: str,
    repository: Path,
) -> Path:
    """Build/download the exact input set without exposing the API token to builds."""

    if destination.is_dir() and not destination.is_symlink():
        context.validate_staging_directory(destination, entries, document, revision)
        return destination.resolve()

    github_token = os.environ.pop("GITHUB_TOKEN", None)

    def populate(root: Path) -> None:
        msb_policy = dict(document["native"]["microsandbox"])
        msb_policy["archive_max_bytes"] = entries["microsandbox"]["max_bytes"]
        verify_release_ref(msb_policy)
        verify_github_release_asset(msb_policy, github_token=github_token)

        agent = root / entries["octacity_agent"]["filename"]
        policy_helper = root / entries[POLICY_HELPER_ROLE]["filename"]
        _build_agent_archive(agent, policy_helper, document, revision, repository)
        octa = root / entries["octa"]["filename"]
        octa_policy = dict(document["native"]["octa"])
        octa_policy["archive_max_bytes"] = entries["octa"]["max_bytes"]
        _download_archive(octa_policy, octa)
        octa.with_name(f"{octa.name}.sha256").write_text(
            f"{sha256(octa)}  {octa.name}\n", encoding="utf-8"
        )
        microsandbox = root / entries["microsandbox"]["filename"]
        _download_archive(msb_policy, microsandbox)
        context.validate_staging_directory(root, entries, document, revision)

    try:
        return _atomic_directory(destination, populate)
    finally:
        if github_token is not None:
            os.environ["GITHUB_TOKEN"] = github_token


def _safe_relative(value: object) -> Path:
    if not isinstance(value, str):
        raise NativeStageError("release path must be a string")
    path = PurePosixPath(value)
    if path.is_absolute() or not path.parts or any(
        part in {"", ".", ".."} for part in path.parts
    ):
        raise NativeStageError(f"unsafe release path: {value!r}")
    return Path(*path.parts)


def verify_checksum_inventory(root: Path) -> dict[Path, str]:
    """Verify SHA256SUMS covers the exact regular-file release contents."""

    checksum = root / "SHA256SUMS"
    if (
        not checksum.is_file()
        or checksum.is_symlink()
        or checksum.stat().st_size > MAX_CHECKSUM_BYTES
    ):
        raise NativeStageError(f"release checksum inventory is missing or oversized: {root}")
    expected: dict[Path, str] = {}
    for line in checksum.read_text(encoding="utf-8").splitlines():
        digest, separator, name = line.partition("  ")
        relative = _safe_relative(name) if separator else None
        if (
            relative is None
            or len(digest) != 64
            or any(character not in "0123456789abcdef" for character in digest)
            or relative == Path("SHA256SUMS")
            or relative in expected
        ):
            raise NativeStageError(f"invalid release checksum inventory: {root}")
        expected[relative] = digest
        if len(expected) > MAX_RELEASE_FILES:
            raise NativeStageError(f"release contains too many files: {root}")
    if not expected:
        raise NativeStageError(f"release checksum inventory is empty: {root}")
    actual: set[Path] = set()
    for path in root.rglob("*"):
        metadata = path.lstat()
        if stat.S_ISLNK(metadata.st_mode) or not (
            stat.S_ISDIR(metadata.st_mode) or stat.S_ISREG(metadata.st_mode)
        ):
            raise NativeStageError(f"release contains an unsafe entry: {path}")
        if stat.S_ISREG(metadata.st_mode) and path != checksum:
            actual.add(path.relative_to(root))
    if set(expected) != actual:
        raise NativeStageError(f"release checksum inventory does not cover its exact files: {root}")
    for relative, digest in expected.items():
        if sha256(root / relative) != digest:
            raise NativeStageError(f"release checksum mismatch: {relative}")
    return expected


def _json_object(path: Path) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
        raise NativeStageError(f"invalid JSON manifest {path}: {error}") from error
    if not isinstance(value, dict):
        raise NativeStageError(f"JSON manifest must be an object: {path}")
    return value


def verify_agent_bundle(root: Path, document: dict[str, Any], revision: str) -> None:
    """Verify the Agent manifest, component digests, and source-plugin identity."""

    inventory = verify_checksum_inventory(root)
    native = document["native"]
    manifest = _json_object(root / "release-manifest.json")
    expected = {
        "format_version": 1,
        "product": "octacity-agent",
        "version": native["octacity_agent"]["version"],
        "platform": native["octacity_agent"]["platform"],
        "build_inputs": {
            "octacity_revision": revision,
            "octa_revision": native["octa"]["source_revision"],
        },
    }
    if {key: manifest.get(key) for key in expected} != expected:
        raise NativeStageError("installed Agent manifest differs from immutable inputs")
    components = manifest.get("components")
    if not isinstance(components, dict) or set(components) != {"agent", "source_git"}:
        raise NativeStageError("installed Agent component manifest is invalid")
    for name, raw in components.items():
        if not isinstance(raw, dict) or set(raw) != {"path", "sha256"}:
            raise NativeStageError(f"installed Agent component is invalid: {name}")
        relative = _safe_relative(raw["path"])
        if inventory.get(relative) != raw["sha256"]:
            raise NativeStageError(f"installed Agent component digest differs: {name}")

    plugin_path = root / "source-plugins/git/plugin.toml"
    try:
        plugin = tomllib.loads(plugin_path.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, tomllib.TOMLDecodeError) as error:
        raise NativeStageError(f"invalid source-plugin manifest: {error}") from error
    executable = _safe_relative(plugin.get("executable", ""))
    executable_path = plugin_path.parent / executable
    source_component = _safe_relative(components["source_git"]["path"])
    if (
        plugin.get("manifest_version") != 1
        or plugin.get("name") != "git"
        or plugin.get("version") != native["octacity_agent"]["version"]
        or plugin.get("platforms") != [inputs.EXPECTED_SOURCE_PLUGIN_PLATFORM]
        or (plugin_path.parent.relative_to(root) / executable) != source_component
        or plugin.get("sha256") != sha256(executable_path)
    ):
        raise NativeStageError("installed source-plugin identity is invalid")


def verify_octa_bundle(root: Path, document: dict[str, Any]) -> None:
    """Verify the Octa checksum inventory, contract, and runner identity."""

    inventory = verify_checksum_inventory(root)
    contract_path = root / "octa-release-contract.json"
    release_contract.validate(contract_path)
    octa = document["native"]["octa"]
    release_contract.validate_codex_compatibility(
        root / "codex-compatibility.json", octa["version"]
    )
    required = {
        Path("Octa.lock"),
        Path("codex-compatibility.json"),
        Path("plugins/codex.plugin.yml"),
        Path("plugins/octa_plugin_codex"),
    }
    if not required.issubset(inventory):
        raise NativeStageError("installed Octa release omits the pinned Codex plugin")
    capabilities = _json_object(root / "octa-runner-capabilities.json")
    expected = {
        "type": "capabilities",
        "octa_version": octa["version"],
        "build_commit": octa["source_revision"],
        "platform": inputs.EXPECTED_OCTA_RUNTIME_PLATFORM,
    }
    if {key: capabilities.get(key) for key in expected} != expected:
        raise NativeStageError("installed Octa runner identity differs from immutable inputs")


def _install_microsandbox(raw: Path, destination: Path) -> None:
    files = {
        path.relative_to(raw).as_posix()
        for path in raw.rglob("*")
        if path.is_file()
    }
    if files != {"msb", "libkrunfw.5.dylib"}:
        raise NativeStageError(f"Microsandbox archive has an unexpected file set: {sorted(files)}")
    (destination / "bin").mkdir(parents=True)
    (destination / "lib").mkdir()
    shutil.copyfile(raw / "msb", destination / "bin/msb")
    shutil.copyfile(raw / "libkrunfw.5.dylib", destination / "lib/libkrunfw.5.dylib")
    (destination / "bin/msb").chmod(0o755)
    (destination / "lib/libkrunfw.5.dylib").chmod(0o644)


def _write_validation_fixture(
    root: Path,
    installation: Path,
    template: Path,
    microsandbox_version: str,
) -> Path:
    for name in ("work", "state", "cache"):
        path = root / name
        path.mkdir(mode=0o700)
    credential = root / "credential"
    credential.write_text("offline-validation-placeholder\n", encoding="utf-8")
    credential.chmod(0o600)
    values = {
        "credential_file": json.dumps(str(credential)),
        "work_root": json.dumps(str(root / "work")),
        "state_root": json.dumps(str(root / "state")),
        "cache_root": json.dumps(str(root / "cache")),
        "octa_release_root": json.dumps(str(installation / "octa")),
        "source_plugins_dir": json.dumps(str(installation / "agent/source-plugins")),
        "msb": json.dumps(str(installation / "microsandbox/bin/msb")),
        "libkrunfw": json.dumps(str(installation / "microsandbox/lib/libkrunfw.5.dylib")),
        "microsandbox_environment_identity": json.dumps(
            f"microsandbox-{microsandbox_version}-linux-arm64-v1"
        ),
    }
    config = root / "agent.toml"
    config.write_text(
        Template(template.read_text(encoding="utf-8")).substitute(values),
        encoding="utf-8",
    )
    config.chmod(0o600)
    return config


def validate_native_installation(
    installation: Path, document: dict[str, Any], revision: str, fixture: Path
) -> None:
    """Run offline binary and Agent configuration checks against staged files."""

    verify_agent_bundle(installation / "agent", document, revision)
    verify_octa_bundle(installation / "octa", document)
    agent = installation / "agent/bin/octacity-agent"
    msb = installation / "microsandbox/bin/msb"
    version = subprocess.run(
        [str(agent), "--version"], check=True, capture_output=True, text=True
    )
    expected_agent = f"octacity-agent {document['native']['octacity_agent']['version']}"
    if version.stdout.strip() != expected_agent:
        raise NativeStageError(f"staged Agent version differs: {version.stdout.strip()!r}")
    version = subprocess.run(
        [str(msb), "--version"], check=True, capture_output=True, text=True
    )
    expected_msb = f"msb {document['native']['microsandbox']['version']}"
    if version.stdout.strip() != expected_msb:
        raise NativeStageError(f"staged Microsandbox version differs: {version.stdout.strip()!r}")
    with tempfile.TemporaryDirectory(prefix="octacity-agent-validation-") as value:
        validation_root = Path(value)
        config = _write_validation_fixture(
            validation_root,
            installation,
            fixture,
            document["native"]["microsandbox"]["version"],
        )
        environment = {
            **os.environ,
            "HTTP_PROXY": "http://127.0.0.1:9",
            "HTTPS_PROXY": "http://127.0.0.1:9",
            "ALL_PROXY": "http://127.0.0.1:9",
            "NO_PROXY": "",
        }
        result = subprocess.run(
            [str(agent), "validate", str(config)],
            check=True,
            capture_output=True,
            text=True,
            env=environment,
        )
        if "configuration is valid" not in result.stdout:
            raise NativeStageError("staged Agent did not validate the offline fixture")


def derive_job_spec_policy(
    installation: Path,
    document: dict[str, Any],
    policy_helper: Path,
) -> dict[str, Any]:
    """Derive a bounded signing policy from the exact installed executor assets."""

    result = subprocess.run(
        [
            str(policy_helper),
            "job-spec-policy",
            "--agent-root",
            str(installation / "agent"),
            "--octa-root",
            str(installation / "octa"),
            "--agent-platform",
            document["native"]["octacity_agent"]["platform"],
            "--octa-platform",
            inputs.EXPECTED_OCTA_RUNTIME_PLATFORM,
            "--validity-seconds",
            str(DEFAULT_JOB_SPEC_VALIDITY_SECONDS),
        ],
        check=True,
        capture_output=True,
        text=True,
    )
    if len(result.stdout.encode("utf-8")) > MAX_JOB_SPEC_POLICY_BYTES:
        raise NativeStageError("generated JobSpec policy exceeds its size limit")
    try:
        policy = json.loads(result.stdout)
    except json.JSONDecodeError as error:
        raise NativeStageError(f"generated JobSpec policy is invalid JSON: {error}") from error
    if not isinstance(policy, dict):
        raise NativeStageError("generated JobSpec policy must be an object")
    return policy


def validate_policy_helper(path: Path, document: dict[str, Any]) -> None:
    """Require the staged policy helper to be a bounded executable of this release."""

    metadata = path.lstat()
    if path.is_symlink() or not stat.S_ISREG(metadata.st_mode):
        raise NativeStageError("JobSpec policy helper must be a regular file")
    if os.name == "posix" and metadata.st_mode & 0o111 == 0:
        raise NativeStageError("JobSpec policy helper must be executable")
    result = subprocess.run(
        [str(path), "--version"],
        check=True,
        capture_output=True,
        text=True,
    )
    expected = (
        "octacity-release-harness "
        f"{document['native']['octacity_agent']['version']}"
    )
    if result.stdout.strip() != expected:
        raise NativeStageError("JobSpec policy helper version differs from the Agent release")


def _write_job_spec_policy(
    installation: Path,
    document: dict[str, Any],
    policy_helper: Path,
) -> None:
    policy = derive_job_spec_policy(installation, document, policy_helper)
    destination = installation / "job-spec-policy.json"
    destination.write_text(
        json.dumps(policy, sort_keys=True, separators=(",", ":")) + "\n",
        encoding="utf-8",
    )
    destination.chmod(0o600)


def _verify_job_spec_policy(
    installation: Path,
    document: dict[str, Any],
    policy_helper: Path,
) -> None:
    configured = _json_object(installation / "job-spec-policy.json")
    expected = derive_job_spec_policy(installation, document, policy_helper)
    if configured != expected:
        raise NativeStageError(
            "installed JobSpec policy differs from the verified Agent toolchain"
        )


def _installed_file_digests(root: Path) -> dict[str, str]:
    result: dict[str, str] = {}
    for path in sorted(root.rglob("*")):
        if path.is_symlink() or (not path.is_dir() and not path.is_file()):
            raise NativeStageError(f"native installation contains an unsafe entry: {path}")
        if path.is_file() and path != root / "installation-manifest.json":
            result[path.relative_to(root).as_posix()] = sha256(path)
    return result


def _verify_installation_manifest(
    root: Path,
    document: dict[str, Any],
    entries: dict[str, dict[str, Any]],
    inputs_root: Path,
    revision: str,
) -> None:
    manifest = _json_object(root / "installation-manifest.json")
    expected_inputs = {
        role: sha256(inputs_root / entry["filename"]) for role, entry in entries.items()
    }
    expected_identity = {
        "schema_version": 1,
        "agent_revision": revision,
        "agent_version": document["native"]["octacity_agent"]["version"],
        "octa_revision": document["native"]["octa"]["source_revision"],
        "microsandbox_version": document["native"]["microsandbox"]["version"],
        "inputs": expected_inputs,
    }
    if {key: manifest.get(key) for key in expected_identity} != expected_identity:
        raise NativeStageError("native installation manifest differs from immutable inputs")
    if manifest.get("files") != _installed_file_digests(root):
        raise NativeStageError("native installation file inventory differs from its manifest")


def install_native_bundle(
    inputs_root: Path,
    destination: Path,
    document: dict[str, Any],
    entries: dict[str, dict[str, Any]],
    revision: str,
    fixture: Path,
) -> Path:
    """Install only verified local inputs, validate them, and atomically activate."""

    context.validate_staging_directory(inputs_root, entries, document, revision)
    policy_helper = inputs_root / entries[POLICY_HELPER_ROLE]["filename"]
    validate_policy_helper(policy_helper, document)
    if destination.is_dir() and not destination.is_symlink():
        _verify_installation_manifest(destination, document, entries, inputs_root, revision)
        validate_native_installation(destination, document, revision, fixture)
        _verify_job_spec_policy(destination, document, policy_helper)
        return destination.resolve()

    def populate(root: Path) -> None:
        for role, target in (("octacity_agent", "agent"), ("octa", "octa")):
            entry = entries[role]
            extract_tar_archive(
                inputs_root / entry["filename"],
                root / target,
                max_expanded_bytes=entry["max_expanded_bytes"],
                max_file_bytes=entry["max_expanded_bytes"],
                max_members=entry["max_members"],
            )
        entry = entries["microsandbox"]
        raw = root / ".microsandbox-archive"
        extract_tar_archive(
            inputs_root / entry["filename"],
            raw,
            max_expanded_bytes=entry["max_expanded_bytes"],
            max_file_bytes=entry["max_expanded_bytes"],
            max_members=entry["max_members"],
        )
        _install_microsandbox(raw, root / "microsandbox")
        shutil.rmtree(raw)
        validate_native_installation(root, document, revision, fixture)
        _write_job_spec_policy(root, document, policy_helper)
        manifest = {
            "schema_version": 1,
            "agent_revision": revision,
            "agent_version": document["native"]["octacity_agent"]["version"],
            "octa_revision": document["native"]["octa"]["source_revision"],
            "microsandbox_version": document["native"]["microsandbox"]["version"],
            "inputs": {
                role: sha256(inputs_root / value["filename"])
                for role, value in entries.items()
            },
            "files": _installed_file_digests(root),
        }
        (root / "installation-manifest.json").write_text(
            json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8"
        )

    return _atomic_directory(destination, populate)


def require_native_host() -> None:
    if platform.system() != "Darwin" or platform.machine().lower() not in {
        "arm64",
        "aarch64",
    }:
        raise NativeStageError("local native staging requires Apple Silicon macOS")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("prepare", "install", "all"))
    parser.add_argument("--cache-root", required=True, type=Path)
    parser.add_argument("--install-root", type=Path)
    parser.add_argument("--manifest", type=Path, default=DEFAULT_MANIFEST)
    parser.add_argument("--allowlist", type=Path, default=DEFAULT_ALLOWLIST)
    parser.add_argument("--fixture", type=Path, default=DEFAULT_AGENT_FIXTURE)
    parser.add_argument("--repository", type=Path, default=REPOSITORY)
    parser.add_argument("--agent-revision")
    arguments = parser.parse_args()
    try:
        require_native_host()
        repository = arguments.repository.resolve()
        revision = arguments.agent_revision or git_revision(repository)
        document, entries = load_policy(
            arguments.manifest.resolve(), arguments.allowlist.resolve(), repository, revision
        )
        input_root = arguments.cache_root.resolve() / input_set_name(document, revision)
        if arguments.command in {"prepare", "all"}:
            prepare_inputs(input_root, document, entries, revision, repository)
        if arguments.command in {"install", "all"}:
            if arguments.install_root is None:
                raise NativeStageError("--install-root is required for install and all")
            destination = arguments.install_root.resolve() / installation_name(
                document, revision
            )
            path = install_native_bundle(
                input_root,
                destination,
                document,
                entries,
                revision,
                arguments.fixture.resolve(),
            )
            print(path)
        else:
            print(input_root)
    except (
        OSError,
        KeyError,
        TypeError,
        ValueError,
        NativeStageError,
        SourceArchiveError,
        subprocess.CalledProcessError,
    ) as error:
        parser.error(str(error))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
