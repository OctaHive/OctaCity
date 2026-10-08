#!/usr/bin/env python3
"""Validate immutable inputs used by the hybrid local stand.

The checked-in manifest owns external pins. The repository-owned Agent cannot
pin the commit that contains the manifest itself, so its exact Git revision is
required at staging time and validated by this tool.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import re
import tomllib
from typing import Any


EXPECTED_IMAGE_ROLES = frozenset(
    {"alpine", "go", "nginx", "node", "postgres", "rust"}
)
EXPECTED_CI_IMAGE_ROLES = frozenset({"alpine", "go"})
EXPECTED_SOURCE_REPOSITORIES = {
    "minio": "https://github.com/minio/minio",
    "minio_client": "https://github.com/minio/mc",
}
SHA256 = re.compile(r"[0-9a-f]{64}")
GIT_SHA1 = re.compile(r"[0-9a-f]{40}")
SEMVER = re.compile(r"[0-9]+\.[0-9]+\.[0-9]+(?:[-+][0-9A-Za-z.-]+)?")
MINIO_RELEASE_TAG = re.compile(r"RELEASE\.[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}-[0-9]{2}-[0-9]{2}Z")
MAX_PINNED_INPUT_BYTES = 2 * 1024 * 1024 * 1024
EXPECTED_AGENT_RELEASE_PLATFORM = "macos-arm64"
EXPECTED_SOURCE_PLUGIN_PLATFORM = "macos-aarch64"
EXPECTED_OCTA_RELEASE_PLATFORM = "linux-arm64"
EXPECTED_OCTA_RUNTIME_PLATFORM = "linux-aarch64"


class InputError(ValueError):
    """The local-stand input manifest is incomplete, mutable, or inconsistent."""


def require_object(value: object, label: str) -> dict[str, Any]:
    """Return a JSON object or reject the manifest with a scoped message."""

    if not isinstance(value, dict):
        raise InputError(f"{label} must be an object")
    return value


def require_exact_keys(value: dict[str, Any], expected: set[str], label: str) -> None:
    """Reject missing and unknown fields so metadata cannot be silently ignored."""

    actual = set(value)
    if actual != expected:
        missing = sorted(expected - actual)
        unknown = sorted(actual - expected)
        raise InputError(f"{label} fields differ: missing={missing}, unknown={unknown}")


def require_string(value: object, label: str) -> str:
    """Return a non-empty string without surrounding whitespace."""

    if not isinstance(value, str) or not value or value != value.strip():
        raise InputError(f"{label} must be a non-empty trimmed string")
    return value


def require_match(value: object, pattern: re.Pattern[str], label: str) -> str:
    """Return a string only when it fully matches the requested identity format."""

    text = require_string(value, label)
    if pattern.fullmatch(text) is None:
        raise InputError(f"{label} has an invalid format")
    return text


def require_size_limit(value: object, label: str) -> int:
    """Return a positive bounded byte or member limit."""

    if (
        not isinstance(value, int)
        or isinstance(value, bool)
        or not 0 < value <= MAX_PINNED_INPUT_BYTES
    ):
        raise InputError(f"{label} must be a positive bounded integer")
    return value


def image_tag(reference: object, label: str) -> str:
    """Validate a tag-plus-platform-digest OCI reference and return its tag."""

    text = require_string(reference, label)
    marker = "@sha256:"
    if text.count(marker) != 1:
        raise InputError(f"{label} must contain exactly one sha256 digest")
    tagged_name, digest = text.rsplit(marker, 1)
    require_match(digest, SHA256, f"{label} digest")
    final_component = tagged_name.rsplit("/", 1)[-1]
    if ":" not in final_component:
        raise InputError(f"{label} must retain an explicit human-readable tag")
    tag = final_component.rsplit(":", 1)[1]
    if not tag or tag == "latest":
        raise InputError(f"{label} must not use a mutable or empty tag")
    return tag


def immutable_image(reference: object, label: str) -> str:
    """Validate a digest-only OCI identity used by an execution backend."""

    text = require_string(reference, label)
    marker = "@sha256:"
    if text.count(marker) != 1:
        raise InputError(f"{label} must contain exactly one sha256 digest")
    repository, digest = text.rsplit(marker, 1)
    if repository != "quay.io/fedora/fedora":
        raise InputError(f"{label} must use the approved Fedora repository")
    require_match(digest, SHA256, f"{label} digest")
    return text


def validate_images(
    value: object, label: str, expected_roles: frozenset[str], platform: str
) -> dict[str, str]:
    """Validate one complete set of platform-specific OCI image pins."""

    images = require_object(value, label)
    if set(images) != expected_roles:
        raise InputError(
            f"{label} roles differ: "
            f"missing={sorted(expected_roles - set(images))}, "
            f"unknown={sorted(set(images) - expected_roles)}"
        )
    tags: dict[str, str] = {}
    for role in sorted(expected_roles):
        image_label = f"{label}.{role}"
        image = require_object(images[role], image_label)
        require_exact_keys(image, {"platform", "reference"}, image_label)
        if image["platform"] != platform:
            raise InputError(f"{image_label}.platform must equal {platform}")
        tags[role] = image_tag(image["reference"], f"{image_label}.reference")
    return tags


def validate_manifest(document: object, agent_revision: str) -> dict[str, str]:
    """Validate manifest structure and return parsed image tags by role."""

    root = require_object(document, "manifest")
    require_exact_keys(
        root,
        {
            "schema_version",
            "container_platform",
            "images",
            "ci_images",
            "sources",
            "native",
        },
        "manifest",
    )
    if root["schema_version"] != 1:
        raise InputError("schema_version must equal 1")
    if root["container_platform"] != "linux/arm64":
        raise InputError("container_platform must equal linux/arm64")

    tags = validate_images(root["images"], "images", EXPECTED_IMAGE_ROLES, "linux/arm64")
    validate_images(
        root["ci_images"], "CI image", EXPECTED_CI_IMAGE_ROLES, "linux/amd64"
    )

    validate_sources(require_object(root["sources"], "sources"))

    native = require_object(root["native"], "native")
    require_exact_keys(native, {"microsandbox", "octa", "octacity_agent"}, "native")
    validate_microsandbox(require_object(native["microsandbox"], "native.microsandbox"))
    validate_octa(require_object(native["octa"], "native.octa"))
    validate_agent(
        require_object(native["octacity_agent"], "native.octacity_agent"),
        agent_revision,
    )
    return tags


def validate_sources(value: dict[str, Any]) -> None:
    """Validate source-only MinIO releases used for repository-owned images."""

    if set(value) != set(EXPECTED_SOURCE_REPOSITORIES):
        raise InputError(
            "source roles differ: "
            f"missing={sorted(set(EXPECTED_SOURCE_REPOSITORIES) - set(value))}, "
            f"unknown={sorted(set(value) - set(EXPECTED_SOURCE_REPOSITORIES))}"
        )
    for role, repository in EXPECTED_SOURCE_REPOSITORIES.items():
        source = require_object(value[role], f"sources.{role}")
        require_exact_keys(
            source,
            {
                "archive_max_bytes",
                "archive_url",
                "expanded_max_bytes",
                "file_max_bytes",
                "license",
                "member_max_count",
                "repository",
                "sha256",
                "source_revision",
                "tag",
            },
            f"sources.{role}",
        )
        if source["repository"] != repository:
            raise InputError(f"sources.{role}.repository is unexpected")
        if source["license"] != "AGPL-3.0-only":
            raise InputError(f"sources.{role}.license is unexpected")
        require_match(source["tag"], MINIO_RELEASE_TAG, f"sources.{role}.tag")
        revision = require_match(
            source["source_revision"], GIT_SHA1, f"sources.{role}.source_revision"
        )
        require_match(source["sha256"], SHA256, f"sources.{role}.sha256")
        archive_limit = require_size_limit(
            source["archive_max_bytes"], f"sources.{role}.archive_max_bytes"
        )
        file_limit = require_size_limit(
            source["file_max_bytes"], f"sources.{role}.file_max_bytes"
        )
        expanded_limit = require_size_limit(
            source["expanded_max_bytes"], f"sources.{role}.expanded_max_bytes"
        )
        require_size_limit(
            source["member_max_count"], f"sources.{role}.member_max_count"
        )
        if file_limit > expanded_limit or archive_limit > expanded_limit:
            raise InputError(f"sources.{role} archive limits are inconsistent")
        repository_path = repository.removeprefix("https://github.com/")
        expected_archive = (
            f"https://codeload.github.com/{repository_path}/tar.gz/{revision}"
        )
        if source["archive_url"] != expected_archive:
            raise InputError(
                f"sources.{role}.archive_url does not match its repository and revision"
            )


def validate_microsandbox(value: dict[str, Any]) -> None:
    """Validate the native Microsandbox release and asset identity."""

    require_exact_keys(
        value,
        {
            "asset",
            "asset_id",
            "firmware",
            "guest_image",
            "platform",
            "release_url",
            "repository",
            "sha256",
            "source_revision",
            "tag",
            "version",
        },
        "native.microsandbox",
    )
    version = require_match(value["version"], SEMVER, "native.microsandbox.version")
    tag = require_string(value["tag"], "native.microsandbox.tag")
    if tag != f"v{version}":
        raise InputError("native.microsandbox.tag must match its version")
    if value["platform"] != "darwin-arm64":
        raise InputError("native.microsandbox.platform must equal darwin-arm64")
    asset = require_string(value["asset"], "native.microsandbox.asset")
    if asset != "microsandbox-darwin-aarch64.tar.gz":
        raise InputError("native.microsandbox.asset must be the macOS ARM64 bundle")
    if not isinstance(value["asset_id"], int) or isinstance(value["asset_id"], bool) or value["asset_id"] <= 0:
        raise InputError("native.microsandbox.asset_id must be a positive integer")
    if value["firmware"] != "libkrunfw.5.dylib":
        raise InputError("native.microsandbox.firmware is unexpected")
    if value["repository"] != "https://github.com/superradcompany/microsandbox":
        raise InputError("native.microsandbox.repository is unexpected")
    immutable_image(value["guest_image"], "native.microsandbox.guest_image")
    require_match(value["sha256"], SHA256, "native.microsandbox.sha256")
    require_match(value["source_revision"], GIT_SHA1, "native.microsandbox.source_revision")
    expected_url = (
        f"{value['repository']}/releases/download/"
        f"{tag}/{asset}"
    )
    if value["release_url"] != expected_url:
        raise InputError("native.microsandbox.release_url does not match tag and asset")


def validate_octa(value: dict[str, Any]) -> None:
    """Validate the separately released Linux ARM64 Octa input."""

    require_exact_keys(
        value,
        {
            "archive_max_bytes",
            "asset",
            "codex",
            "license",
            "platform",
            "provenance",
            "release_name",
            "release_url",
            "repository",
            "sha256",
            "source_archive_expanded_max_bytes",
            "source_archive_file_max_bytes",
            "source_archive_max_bytes",
            "source_archive_member_max_count",
            "source_archive_sha256",
            "source_archive_url",
            "source_revision",
            "version",
        },
        "native.octa",
    )
    version = require_match(value["version"], SEMVER, "native.octa.version")
    require_match(value["source_revision"], GIT_SHA1, "native.octa.source_revision")
    codex = require_object(value["codex"], "native.octa.codex")
    require_exact_keys(
        codex,
        {
            "executable_product",
            "manifest",
            "plugin_name",
            "plugin_protocol",
            "required_capabilities",
            "selection_environment",
            "supported_cli_versions",
        },
        "native.octa.codex",
    )
    if codex != {
        "executable_product": "codex-cli",
        "manifest": "plugins/codex.plugin.yml",
        "plugin_name": "codex",
        "plugin_protocol": 2,
        "required_capabilities": ["codex.blocking-pre-tool-authorization.v1"],
        "selection_environment": "OCTA_CODEX_EXECUTABLE",
        "supported_cli_versions": ["0.161.0"],
    }:
        raise InputError("native.octa.codex identity is unexpected")
    if value["license"] != "MIT":
        raise InputError("native.octa.license is unexpected")
    if value["provenance"] != "github-build-provenance":
        raise InputError("native.octa.provenance is unexpected")
    if value["platform"] != EXPECTED_OCTA_RELEASE_PLATFORM:
        raise InputError(
            f"native.octa.platform must equal {EXPECTED_OCTA_RELEASE_PLATFORM}"
        )
    if value["release_name"] != f"octa-linux-arm64-v{version}":
        raise InputError("native.octa.release_name does not match its version and platform")
    if value["repository"] != "https://github.com/OctaHive/octa":
        raise InputError("native.octa.repository is unexpected")
    asset = require_string(value["asset"], "native.octa.asset")
    if asset != "octa-Linux-arm64.tar.gz":
        raise InputError("native.octa.asset is unexpected")
    expected_release_url = f"{value['repository']}/releases/download/v{version}/{asset}"
    if value["release_url"] != expected_release_url:
        raise InputError("native.octa.release_url does not match its version and asset")
    require_match(value["sha256"], SHA256, "native.octa.sha256")
    require_size_limit(value["archive_max_bytes"], "native.octa.archive_max_bytes")
    source_revision = value["source_revision"]
    expected_source_url = (
        "https://codeload.github.com/OctaHive/octa/tar.gz/" f"{source_revision}"
    )
    if value["source_archive_url"] != expected_source_url:
        raise InputError("native.octa.source_archive_url does not match its revision")
    require_match(
        value["source_archive_sha256"], SHA256, "native.octa.source_archive_sha256"
    )
    archive_limit = require_size_limit(
        value["source_archive_max_bytes"], "native.octa.source_archive_max_bytes"
    )
    file_limit = require_size_limit(
        value["source_archive_file_max_bytes"],
        "native.octa.source_archive_file_max_bytes",
    )
    expanded_limit = require_size_limit(
        value["source_archive_expanded_max_bytes"],
        "native.octa.source_archive_expanded_max_bytes",
    )
    require_size_limit(
        value["source_archive_member_max_count"],
        "native.octa.source_archive_member_max_count",
    )
    if archive_limit > expanded_limit or file_limit > expanded_limit:
        raise InputError("native.octa source archive limits are inconsistent")


def validate_agent(value: dict[str, Any], agent_revision: str) -> None:
    """Validate native Agent metadata and its staging-time source revision."""

    require_exact_keys(value, {"platform", "source_revision", "version"}, "native.octacity_agent")
    require_match(value["version"], SEMVER, "native.octacity_agent.version")
    if value["platform"] != EXPECTED_AGENT_RELEASE_PLATFORM:
        raise InputError(
            "native.octacity_agent.platform must equal "
            f"{EXPECTED_AGENT_RELEASE_PLATFORM}"
        )
    policy = require_object(value["source_revision"], "native.octacity_agent.source_revision")
    require_exact_keys(policy, {"format", "mode"}, "native.octacity_agent.source_revision")
    if policy != {"format": "lowercase-git-sha1", "mode": "required-at-stage"}:
        raise InputError("native.octacity_agent.source_revision policy is unexpected")
    require_match(agent_revision, GIT_SHA1, "Agent staging revision")


def validate_repository_pins(
    document: dict[str, Any], tags: dict[str, str], repository: Path
) -> None:
    """Ensure the central manifest agrees with repository-owned version pins."""

    toolchain = tomllib.loads((repository / "rust-toolchain.toml").read_text(encoding="utf-8"))
    rust_version = toolchain["toolchain"]["channel"]
    if tags["rust"] != f"{rust_version}-bookworm":
        raise InputError("Rust image tag differs from rust-toolchain.toml")

    node_version = (repository / "ui/.node-version").read_text(encoding="ascii").strip()
    if tags["node"] != f"{node_version}-bookworm-slim":
        raise InputError("Node image tag differs from ui/.node-version")

    workspace = tomllib.loads((repository / "Cargo.toml").read_text(encoding="utf-8"))
    native = document["native"]
    agent_version = workspace["workspace"]["package"]["version"]
    if native["octacity_agent"]["version"] != agent_version:
        raise InputError("native Agent version differs from Cargo.toml")
    microsandbox_dependency = workspace["workspace"]["dependencies"]["microsandbox"]
    if microsandbox_dependency["version"] != f'={native["microsandbox"]["version"]}':
        raise InputError("Microsandbox version differs from Cargo.toml")
    octa_dependency = workspace["workspace"]["dependencies"]["octa-runner-protocol"]
    if octa_dependency["version"] != native["octa"]["version"]:
        raise InputError("Octa version differs from Cargo.toml")

    octa_revision = (repository / ".github/octa-source-revision").read_text(encoding="ascii").strip()
    if native["octa"]["source_revision"] != octa_revision:
        raise InputError("Octa revision differs from .github/octa-source-revision")


def load_manifest(path: Path) -> dict[str, Any]:
    """Load a UTF-8 JSON manifest and normalize parser errors."""

    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
        raise InputError(f"cannot load {path}: {error}") from error
    return require_object(value, "manifest")


def main() -> int:
    """Validate one manifest and its repository relationships."""

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("manifest", type=Path)
    parser.add_argument("--agent-revision", required=True)
    parser.add_argument("--repository", type=Path, default=Path(__file__).resolve().parents[1])
    args = parser.parse_args()

    try:
        document = load_manifest(args.manifest)
        tags = validate_manifest(document, args.agent_revision)
        validate_repository_pins(document, tags, args.repository.resolve())
    except InputError as error:
        parser.error(str(error))
    print(f"validated immutable local-stand inputs: {args.manifest}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
