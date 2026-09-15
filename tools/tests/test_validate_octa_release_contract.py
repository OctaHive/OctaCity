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


if __name__ == "__main__":
    unittest.main()
