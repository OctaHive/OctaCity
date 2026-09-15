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


EXPECTED_CONTRACT = {
    "format_version": 1,
    "runner_capabilities": "octa-runner-capabilities.json",
    "checksums": "SHA256SUMS",
    "provenance": "github-build-provenance",
}


def validate(path: Path) -> None:
    if not path.is_file() or path.is_symlink():
        raise ValueError(f"Octa release contract must be a regular file: {path}")
    try:
        contract = json.loads(path.read_text(encoding="utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise ValueError(f"Octa release contract is invalid JSON: {error}") from error
    if contract != EXPECTED_CONTRACT:
        raise ValueError("Octa release contract does not match the supported artifact contract")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("contract", type=Path)
    args = parser.parse_args()
    validate(args.contract)


if __name__ == "__main__":
    main()
