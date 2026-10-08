#!/usr/bin/env python3
"""Validate the machine-readable contract of a pinned Octa release.

OctaCity consumes release contents, not Octa's workflow implementation.  This
validator deliberately accepts one exact schema so a renamed or removed
security artifact fails release packaging without brittle workflow-text checks.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import stat


EXPECTED_CONTRACT = {
    "format_version": 1,
    "runner_capabilities": "octa-runner-capabilities.json",
    "codex_compatibility": "codex-compatibility.json",
    "checksums": "SHA256SUMS",
    "provenance": "github-build-provenance",
}
SUPPORTED_CODEX_CLI_VERSIONS = ["0.161.0"]
CODEX_TOOL_AUTHORIZATION_CAPABILITY = "codex.blocking-pre-tool-authorization.v1"
CODEX_TOOL_AUTHORIZER_ENVIRONMENT = "OCTA_CODEX_TOOL_AUTHORIZER"
MAX_RELEASE_METADATA_BYTES = 1024 * 1024


def expected_codex_compatibility(octa_version: str) -> dict[str, object]:
    """Return the exact Codex identity supported by this OctaCity release line."""

    return {
        "format_version": 2,
        "plugin": {
            "name": "codex",
            "version": octa_version,
            "protocol": 2,
            "manifest": "plugins/codex.plugin.yml",
            "capabilities": [CODEX_TOOL_AUTHORIZATION_CAPABILITY],
        },
        "executable": {
            "product": "codex-cli",
            "supported_versions": SUPPORTED_CODEX_CLI_VERSIONS,
            "selection_environment": "OCTA_CODEX_EXECUTABLE",
        },
        "tool_authorization": {
            "mode": "blocking_pre_tool_use",
            "capability": CODEX_TOOL_AUTHORIZATION_CAPABILITY,
            "selection_environment": CODEX_TOOL_AUTHORIZER_ENVIRONMENT,
            "hook_event": "PreToolUse",
        },
    }


def _load_object(path: Path, description: str) -> dict[str, object]:
    try:
        metadata = path.stat(follow_symlinks=False)
    except OSError as error:
        raise ValueError(f"{description} must be a regular file: {path}") from error
    if not stat.S_ISREG(metadata.st_mode):
        raise ValueError(f"{description} must be a regular file: {path}")
    if metadata.st_size > MAX_RELEASE_METADATA_BYTES:
        raise ValueError(
            f"{description} exceeds {MAX_RELEASE_METADATA_BYTES} bytes: {path}"
        )
    try:
        with path.open("rb") as source:
            payload = source.read(MAX_RELEASE_METADATA_BYTES + 1)
        if len(payload) > MAX_RELEASE_METADATA_BYTES:
            raise ValueError(
                f"{description} exceeds {MAX_RELEASE_METADATA_BYTES} bytes: {path}"
            )
        document = json.loads(payload.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise ValueError(f"{description} is invalid JSON: {error}") from error
    if not isinstance(document, dict):
        raise ValueError(f"{description} must be a JSON object")
    return document


def validate(path: Path) -> None:
    contract = _load_object(path, "Octa release contract")
    if contract != EXPECTED_CONTRACT:
        raise ValueError("Octa release contract does not match the supported artifact contract")


def validate_codex_compatibility(path: Path, octa_version: str) -> None:
    """Require exact plugin and Codex CLI compatibility metadata."""

    document = _load_object(path, "Octa Codex compatibility metadata")
    if document != expected_codex_compatibility(octa_version):
        raise ValueError("Octa Codex compatibility metadata does not match the supported identity")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("contract", type=Path)
    args = parser.parse_args()
    validate(args.contract)


if __name__ == "__main__":
    main()
