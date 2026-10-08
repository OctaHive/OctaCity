"""Tests for the pinned Octa release-contract validator."""

from __future__ import annotations

import importlib.util
import json
import sys
import tempfile
import unittest
from pathlib import Path


REPOSITORY = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location(
    "validate_octa_release_contract", REPOSITORY / "tools/validate_octa_release_contract.py"
)
assert SPEC and SPEC.loader
VALIDATOR = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = VALIDATOR
SPEC.loader.exec_module(VALIDATOR)


class OctaReleaseContractTests(unittest.TestCase):
    def test_accepts_the_exact_supported_contract(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "contract.json"
            path.write_text(json.dumps(VALIDATOR.EXPECTED_CONTRACT), encoding="utf-8")
            VALIDATOR.validate(path)

    def test_rejects_missing_extra_and_malformed_contract_fields(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "contract.json"
            for contract in (
                {"format_version": 1},
                {**VALIDATOR.EXPECTED_CONTRACT, "workflow": "build.yml"},
                {**VALIDATOR.EXPECTED_CONTRACT, "format_version": 2},
            ):
                path.write_text(json.dumps(contract), encoding="utf-8")
                with self.assertRaisesRegex(ValueError, "supported artifact contract"):
                    VALIDATOR.validate(path)
            path.write_text("not json", encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "invalid JSON"):
                VALIDATOR.validate(path)

    def test_accepts_only_the_exact_codex_compatibility_identity(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "codex-compatibility.json"
            expected = VALIDATOR.expected_codex_compatibility("0.5.1")
            path.write_text(json.dumps(expected), encoding="utf-8")
            VALIDATOR.validate_codex_compatibility(path, "0.5.1")

            mutations = (
                ("plugin", "name", "another-plugin"),
                ("plugin", "version", "0.5.2"),
                ("plugin", "protocol", 3),
                ("plugin", "capabilities", []),
                ("executable", "product", "another-cli"),
                ("executable", "supported_versions", ["0.131.0"]),
                ("executable", "selection_environment", "PATH"),
                ("tool_authorization", "mode", "post_tool_use"),
                ("tool_authorization", "capability", "another-capability"),
                ("tool_authorization", "selection_environment", "PATH"),
                ("tool_authorization", "hook_event", "PostToolUse"),
            )
            for section, field, value in mutations:
                document = json.loads(json.dumps(expected))
                document[section][field] = value
                path.write_text(json.dumps(document), encoding="utf-8")
                with self.subTest(section=section, field=field), self.assertRaisesRegex(
                    ValueError, "supported identity"
                ):
                    VALIDATOR.validate_codex_compatibility(path, "0.5.1")

    def test_accepts_the_released_blocking_tool_authorization_contract(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "codex-compatibility.json"
            released = {
                "format_version": 2,
                "plugin": {
                    "name": "codex",
                    "version": "0.5.1",
                    "protocol": 2,
                    "manifest": "plugins/codex.plugin.yml",
                    "capabilities": ["codex.blocking-pre-tool-authorization.v1"],
                },
                "executable": {
                    "product": "codex-cli",
                    "supported_versions": ["0.161.0"],
                    "selection_environment": "OCTA_CODEX_EXECUTABLE",
                },
                "tool_authorization": {
                    "mode": "blocking_pre_tool_use",
                    "capability": "codex.blocking-pre-tool-authorization.v1",
                    "selection_environment": "OCTA_CODEX_TOOL_AUTHORIZER",
                    "hook_event": "PreToolUse",
                },
            }
            path.write_text(json.dumps(released), encoding="utf-8")

            VALIDATOR.validate_codex_compatibility(path, "0.5.1")

    def test_rejects_oversized_release_metadata_before_parsing(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "contract.json"
            path.write_bytes(b" " * (VALIDATOR.MAX_RELEASE_METADATA_BYTES + 1))
            with self.assertRaisesRegex(ValueError, "exceeds"):
                VALIDATOR.validate(path)

    def test_rejects_release_metadata_symlinks(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            target = root / "target.json"
            target.write_text(json.dumps(VALIDATOR.EXPECTED_CONTRACT), encoding="utf-8")
            link = root / "contract.json"
            link.symlink_to(target)
            with self.assertRaisesRegex(ValueError, "regular file"):
                VALIDATOR.validate(link)


if __name__ == "__main__":
    unittest.main()
