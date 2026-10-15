"""Isolated CLI processes retain OS bootstrap state without ambient credentials."""
import json
import os
import subprocess
import sys
import unittest
from unittest.mock import patch


class FixtureEnvironmentTests(unittest.TestCase):
    def test_child_retains_system_root_and_only_explicit_credentials(self):
        from tools.tests.fixture_environment import fixture_environment

        system_root = os.environ.get("SystemRoot", r"C:\system-root-canary")
        with patch.dict(os.environ, {"SystemRoot": system_root,
                                    "OPENAI_API_KEY": "ambient-private-canary",
                                    "NODE_OPTIONS": "ambient-runtime-injection-canary"}):
            result = subprocess.run(
                [sys.executable, "-c", "import json,os; print(json.dumps({key:os.environ.get(key) "
                 "for key in ('SystemRoot','PATH','OPENAI_API_KEY','NODE_OPTIONS')}))"],
                env=fixture_environment(OPENAI_API_KEY="explicit-fixture-credential"),
                text=True, capture_output=True, check=True, timeout=5,
            )
        self.assertEqual(result.stderr, "")
        self.assertEqual(json.loads(result.stdout), {
            "SystemRoot": system_root, "PATH": os.defpath,
            "OPENAI_API_KEY": "explicit-fixture-credential", "NODE_OPTIONS": None,
        })


if __name__ == "__main__":
    unittest.main()
