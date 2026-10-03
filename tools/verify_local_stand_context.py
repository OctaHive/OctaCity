#!/usr/bin/env python3
"""Validate the bounded Docker context and native staging allowlist."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path, PurePosixPath
import re
import stat
import tarfile
from typing import Any


MANDATORY_FINAL_DENIES = frozenset(
    {
        ".git/**",
        "**/.env.*",
        "**/*.key",
        "**/*.pem",
        "**/node_modules/**",
        "**/target/**",
        ".local-stand/**",
        "deployment/local-stand/generated/**",
        "deployment/local-stand/state/**",
    }
)

REQUIRED_CONTEXT_PATHS = (
    "Cargo.lock",
    "Cargo.toml",
    "rust-toolchain.toml",
    "agent/octacity-agent/Cargo.toml",
    "agent/octacity-agent/src/main.rs",
    "agent/octacity-source-plugin/README.md",
    "server/app/Cargo.toml",
    "server/app/src/main.rs",
    "server/api/octacity-server-api-rest/examples/export_openapi.rs",
    "server/infrastructure/octacity-server-store-postgres/migrations/0001_authoritative_store.sql",
    "server/tests/octacity-service-contract-tests/Cargo.toml",
    "server/tests/octacity-release-harness/Cargo.toml",
    "shared/octacity-protocol/Cargo.toml",
    "shared/octacity-protocol/src/lib.rs",
    "ui/package.json",
    "ui/pnpm-lock.yaml",
    "ui/.node-version",
    "ui/scripts/generate-api.mjs",
    "ui/src/main.tsx",
    "deployment/local-stand/inputs.json",
    "deployment/local-stand/minio.Dockerfile",
    "deployment/local-stand/nginx.conf",
    "deployment/local-stand/server.Dockerfile",
    "deployment/local-stand/server-entrypoint.sh",
    "deployment/local-stand/staging-allowlist.json",
)

FORBIDDEN_CONTEXT_PATHS = (
    ".git/config",
    ".github/workflows/ci.yml",
    ".DS_Store",
    ".idea/workspace.xml",
    ".local-stand/state/agent.json",
    ".pnpm-store/v3/index.json",
    "README.md",
    "developer-notes.txt",
    "agent/developer-notes.txt",
    "docs/README.md",
    "fuzz/Cargo.toml",
    "openspec/config.yaml",
    "server/api/developer-notes.txt",
    "shared/README.md",
    "target/debug/octacity-server",
    "agent/octacity-agent/target/debug/octacity-agent",
    "ui/dist/index.html",
    "ui/.generated/api/schema.d.ts",
    "ui/node_modules/react/index.js",
    "ui/.env.local",
    "deployment/local-stand/generated/server.toml",
    "deployment/local-stand/state/secrets/signing.key",
    "server/app/src/private.pem",
    "tools/__pycache__/helper.pyc",
)

STAGING_FIELDS = {
    "role",
    "filename",
    "manifest_entry",
    "integrity",
    "max_bytes",
    "max_expanded_bytes",
    "max_members",
}
STAGING_POLICY = {
    "octacity_agent": ("native.octacity_agent", "release-manifest-and-sha256-sidecar"),
    "octacity_release_harness": (
        "native.octacity_agent",
        "built-revision-and-sha256-sidecar",
    ),
    "octa": ("native.octa", "release-manifest-and-sha256-sidecar"),
    "microsandbox": ("native.microsandbox", "manifest-sha256"),
}
MAX_STAGING_INPUT_BYTES = 1024 * 1024 * 1024
MAX_IDENTITY_BYTES = 1024 * 1024
MAX_CHECKSUM_BYTES = 1024
GIT_REVISION = re.compile(r"[0-9a-f]{40}")
SHA256 = re.compile(r"[0-9a-f]{64}")


class ContextError(ValueError):
    """A build-context or staging policy is unsafe or inconsistent."""


def active_docker_rules(text: str) -> tuple[str, ...]:
    """Return meaningful Docker ignore rules without comments or whitespace."""

    return tuple(line.strip() for line in text.splitlines() if line.strip() and not line.lstrip().startswith("#"))


def validate_dockerignore(path: Path) -> tuple[str, ...]:
    """Require a default-deny policy with immutable exclusions after all allows."""

    rules = active_docker_rules(path.read_text(encoding="utf-8"))
    if not rules or rules[0] != "**":
        raise ContextError(".dockerignore must begin with the default-deny '**' rule")
    missing = MANDATORY_FINAL_DENIES - set(rules)
    if missing:
        raise ContextError(
            f".dockerignore misses mandatory exclusions: {sorted(missing)}"
        )
    last_allow = max(
        (index for index, rule in enumerate(rules) if rule.startswith("!")),
        default=0,
    )
    if any(rules.index(rule) <= last_allow for rule in MANDATORY_FINAL_DENIES):
        raise ContextError("mandatory .dockerignore exclusions must follow every allow rule")
    return rules


def _glob_regex(pattern: str) -> re.Pattern[str]:
    pieces: list[str] = ["^"]
    index = 0
    while index < len(pattern):
        character = pattern[index]
        if character == "*":
            if index + 1 < len(pattern) and pattern[index + 1] == "*":
                index += 2
                if index < len(pattern) and pattern[index] == "/":
                    pieces.append("(?:.*/)?")
                    index += 1
                else:
                    pieces.append(".*")
                continue
            pieces.append("[^/]*")
        elif character == "?":
            pieces.append("[^/]")
        else:
            pieces.append(re.escape(character))
        index += 1
    pieces.append("$")
    return re.compile("".join(pieces))


def _rule_matches(rule: str, path: str, is_directory: bool) -> bool:
    pattern = rule.removeprefix("!")
    directory_only = pattern.endswith("/")
    if directory_only and not is_directory:
        return False
    normalized = pattern.strip("/")
    return bool(_glob_regex(normalized).fullmatch(path))


def _path_state(path: str, is_directory: bool, rules: tuple[str, ...]) -> bool:
    included = True
    for rule in rules:
        if _rule_matches(rule, path, is_directory):
            included = rule.startswith("!")
    return included


def docker_path_included(path: str, rules: tuple[str, ...]) -> bool:
    """Evaluate the constrained rule grammar used by this repository."""

    candidate = PurePosixPath(path)
    if candidate.is_absolute() or not candidate.parts or any(part in {"", ".", ".."} for part in candidate.parts):
        raise ContextError(f"invalid repository-relative path: {path}")
    for depth in range(1, len(candidate.parts)):
        parent = "/".join(candidate.parts[:depth])
        if not _path_state(parent, True, rules):
            return False
    return _path_state(candidate.as_posix(), False, rules)


def _require_object(value: object, label: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise ContextError(f"{label} must be an object")
    return value


def _load_json(path: Path, label: str) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
        raise ContextError(f"cannot load {label} {path}: {error}") from error
    return _require_object(value, label)


def load_staging_allowlist(path: Path, inputs_path: Path) -> dict[str, dict[str, Any]]:
    """Validate the staging allowlist against immutable input metadata."""

    document = _load_json(path, "staging allowlist")
    if set(document) != {"schema_version", "inputs"} or document["schema_version"] != 1:
        raise ContextError("staging allowlist root is invalid")
    entries = document["inputs"]
    if not isinstance(entries, list) or len(entries) != len(STAGING_POLICY):
        raise ContextError("staging allowlist must contain every expected input exactly once")

    immutable = _load_json(inputs_path, "immutable inputs")
    native = _require_object(immutable.get("native"), "immutable inputs.native")
    expected_filenames = {
        "octacity_agent": f"octacity-agent-{native['octacity_agent']['platform']}.tar.gz",
        "octacity_release_harness": (
            f"octacity-release-harness-{native['octacity_agent']['platform']}"
        ),
        "octa": f"{native['octa']['release_name']}.tar.gz",
        "microsandbox": native["microsandbox"]["asset"],
    }

    by_role: dict[str, dict[str, Any]] = {}
    for index, raw_entry in enumerate(entries):
        entry = _require_object(raw_entry, f"staging allowlist input {index}")
        if set(entry) != STAGING_FIELDS:
            raise ContextError(f"staging allowlist input {index} fields are invalid")
        role = entry.get("role")
        if role not in STAGING_POLICY or role in by_role:
            raise ContextError(f"staging allowlist role is unknown or duplicated: {role}")
        manifest_entry, integrity = STAGING_POLICY[role]
        if entry["manifest_entry"] != manifest_entry or entry["integrity"] != integrity:
            raise ContextError(f"staging policy for {role} is inconsistent")
        filename = entry["filename"]
        if filename != expected_filenames[role] or Path(filename).name != filename:
            raise ContextError(f"staging filename for {role} is inconsistent")
        max_bytes = entry["max_bytes"]
        if not isinstance(max_bytes, int) or isinstance(max_bytes, bool) or not 0 < max_bytes <= MAX_STAGING_INPUT_BYTES:
            raise ContextError(f"staging size limit for {role} is invalid")
        expanded = entry["max_expanded_bytes"]
        members = entry["max_members"]
        if (
            not isinstance(expanded, int)
            or isinstance(expanded, bool)
            or not max_bytes <= expanded <= MAX_STAGING_INPUT_BYTES
        ):
            raise ContextError(f"staging expansion limit for {role} is invalid")
        if (
            not isinstance(members, int)
            or isinstance(members, bool)
            or not 0 < members <= 65536
        ):
            raise ContextError(f"staging member limit for {role} is invalid")
        by_role[role] = entry
    return by_role


def _file_sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        while chunk := source.read(1024 * 1024):
            digest.update(chunk)
    return digest.hexdigest()


def _sidecar_digest(path: Path, filename: str) -> str:
    try:
        fields = path.read_text(encoding="utf-8").strip().split()
    except (OSError, UnicodeDecodeError) as error:
        raise ContextError(f"cannot read staging checksum {path}: {error}") from error
    if (
        len(fields) != 2
        or fields[1].lstrip("*") != filename
        or SHA256.fullmatch(fields[0]) is None
    ):
        raise ContextError(f"staging checksum does not identify {filename}")
    return fields[0]


def _tar_identity(path: Path, filename: str, entry: dict[str, Any]) -> dict[str, Any]:
    """Read one bounded JSON identity while validating the surrounding tar shape."""

    identity: bytes | None = None
    count = 0
    expanded = 0
    try:
        with tarfile.open(path, mode="r:gz") as archive:
            for member in archive:
                count += 1
                if count > entry["max_members"]:
                    raise ContextError(f"staging archive has too many members: {path.name}")
                if member.name.rstrip("/") == "." and member.isdir():
                    continue
                candidate = PurePosixPath(member.name)
                if (
                    candidate.is_absolute()
                    or not candidate.parts
                    or any(part in {"", ".", ".."} for part in candidate.parts)
                ):
                    raise ContextError(f"unsafe staging archive member: {member.name}")
                if member.isdir():
                    continue
                if not member.isfile():
                    raise ContextError(f"unsupported staging archive member: {member.name}")
                expanded += member.size
                if expanded > entry["max_expanded_bytes"]:
                    raise ContextError(f"staging archive expands beyond its limit: {path.name}")
                if candidate.as_posix() != filename:
                    continue
                if identity is not None or member.size > MAX_IDENTITY_BYTES:
                    raise ContextError(f"staging archive identity is duplicated or oversized: {filename}")
                source = archive.extractfile(member)
                if source is None:
                    raise ContextError(f"cannot read staging archive identity: {filename}")
                with source:
                    identity = source.read(MAX_IDENTITY_BYTES + 1)
    except (OSError, tarfile.TarError) as error:
        raise ContextError(f"cannot inspect staging archive {path.name}: {error}") from error
    if identity is None:
        raise ContextError(f"staging archive omits identity: {filename}")
    try:
        document = json.loads(identity)
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise ContextError(f"staging archive identity is invalid: {filename}") from error
    return _require_object(document, f"staging archive identity {filename}")


def _validate_agent_identity(
    archive: Path, entry: dict[str, Any], native: dict[str, Any], agent_revision: str
) -> None:
    if GIT_REVISION.fullmatch(agent_revision) is None:
        raise ContextError("staging Agent revision must be a lowercase Git SHA-1")
    document = _tar_identity(archive, "release-manifest.json", entry)
    build_inputs = _require_object(document.get("build_inputs"), "Agent build_inputs")
    expected = {
        "format_version": 1,
        "product": "octacity-agent",
        "version": native["octacity_agent"]["version"],
        "platform": native["octacity_agent"]["platform"],
    }
    actual = {field: document.get(field) for field in expected}
    if actual != expected or build_inputs != {
        "octacity_revision": agent_revision,
        "octa_revision": native["octa"]["source_revision"],
    }:
        raise ContextError("staged Agent release identity does not match immutable inputs")


def _validate_octa_identity(
    archive: Path, entry: dict[str, Any], native: dict[str, Any]
) -> None:
    document = _tar_identity(archive, "octa-runner-capabilities.json", entry)
    expected = {
        "octa_version": native["octa"]["version"],
        "build_commit": native["octa"]["source_revision"],
        "platform": "linux-aarch64",
    }
    if {field: document.get(field) for field in expected} != expected:
        raise ContextError("staged Octa release identity does not match immutable inputs")


def validate_staging_directory(
    directory: Path,
    entries: dict[str, dict[str, Any]],
    immutable: dict[str, Any],
    agent_revision: str,
) -> None:
    """Verify the exact staged file set, archive bytes, and release identities."""

    if directory.is_symlink() or not directory.is_dir():
        raise ContextError("staging input root must be a real directory")
    expected = {entry["filename"]: entry for entry in entries.values()}
    sidecars = {
        f"{entry['filename']}.sha256": entry
        for entry in entries.values()
        if entry["integrity"].endswith("sha256-sidecar")
    }
    actual = {path.name: path for path in directory.iterdir()}
    expected_names = set(expected) | set(sidecars)
    if set(actual) != expected_names:
        raise ContextError(
            "staging files differ: "
            f"missing={sorted(expected_names - set(actual))}, "
            f"unknown={sorted(set(actual) - expected_names)}"
        )
    for filename, path in actual.items():
        metadata = path.lstat()
        if not stat.S_ISREG(metadata.st_mode):
            raise ContextError(f"staging input must be a regular file: {filename}")
        if filename in expected and metadata.st_size > expected[filename]["max_bytes"]:
            raise ContextError(f"staging input exceeds its size limit: {filename}")
        if filename in sidecars and metadata.st_size > MAX_CHECKSUM_BYTES:
            raise ContextError(f"staging checksum exceeds its size limit: {filename}")

    native = _require_object(immutable.get("native"), "immutable inputs.native")
    for role, entry in entries.items():
        filename = entry["filename"]
        archive = actual[filename]
        digest = _file_sha256(archive)
        if entry["integrity"] == "manifest-sha256":
            pinned = native[role]["sha256"]
        else:
            pinned = _sidecar_digest(actual[f"{filename}.sha256"], filename)
        if digest != pinned:
            raise ContextError(f"staging checksum mismatch for {filename}")
        if role == "octacity_agent":
            _validate_agent_identity(archive, entry, native, agent_revision)
        elif role == "octa":
            if digest != native["octa"]["sha256"]:
                raise ContextError(f"staging checksum mismatch for {filename}")
            _validate_octa_identity(archive, entry, native)


def validate_materialized_context(directory: Path, rules: tuple[str, ...]) -> None:
    """Verify a context exported by Docker against the repository policy."""

    if directory.is_symlink() or not directory.is_dir():
        raise ContextError("materialized Docker context must be a real directory")
    for required in REQUIRED_CONTEXT_PATHS:
        if not (directory / required).is_file():
            raise ContextError(f"materialized Docker context omitted required input: {required}")
    for forbidden in FORBIDDEN_CONTEXT_PATHS:
        if (directory / forbidden).exists() or (directory / forbidden).is_symlink():
            raise ContextError(f"materialized Docker context contains forbidden input: {forbidden}")
    for path in directory.rglob("*"):
        if path.is_symlink() or (not path.is_dir() and not path.is_file()):
            raise ContextError(f"materialized Docker context contains an unsafe entry: {path}")
        if path.is_file():
            relative = path.relative_to(directory).as_posix()
            if not docker_path_included(relative, rules):
                raise ContextError(f"materialized Docker context contains an excluded file: {relative}")


def validate_repository(
    repository: Path,
    staging_directory: Path | None = None,
    materialized_context: Path | None = None,
    agent_revision: str | None = None,
) -> None:
    """Validate every local-stand context boundary owned by the repository."""

    rules = validate_dockerignore(repository / ".dockerignore")
    for required in REQUIRED_CONTEXT_PATHS:
        if not (repository / required).is_file() or not docker_path_included(required, rules):
            raise ContextError(f"required build input is absent or excluded: {required}")
    for forbidden in FORBIDDEN_CONTEXT_PATHS:
        if docker_path_included(forbidden, rules):
            raise ContextError(f"forbidden build input is included: {forbidden}")

    entries = load_staging_allowlist(
        repository / "deployment/local-stand/staging-allowlist.json",
        repository / "deployment/local-stand/inputs.json",
    )
    if staging_directory is not None:
        if agent_revision is None:
            raise ContextError("--agent-revision is required with --staging-directory")
        immutable = _load_json(
            repository / "deployment/local-stand/inputs.json", "immutable inputs"
        )
        validate_staging_directory(
            staging_directory, entries, immutable, agent_revision
        )
    if materialized_context is not None:
        validate_materialized_context(materialized_context, rules)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repository", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--staging-directory", type=Path)
    parser.add_argument("--materialized-context", type=Path)
    parser.add_argument("--agent-revision")
    return parser.parse_args()


def main() -> int:
    arguments = parse_args()
    try:
        validate_repository(
            arguments.repository.resolve(),
            arguments.staging_directory,
            arguments.materialized_context,
            arguments.agent_revision,
        )
    except ContextError as error:
        raise SystemExit(f"local-stand context validation failed: {error}") from error
    print("validated bounded local-stand build and staging contexts")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
