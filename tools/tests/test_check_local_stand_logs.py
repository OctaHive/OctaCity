"""Tests for local-stand emitted-log secret scanning."""

from __future__ import annotations

import os
from pathlib import Path
import sys
import tempfile
import unittest


REPOSITORY = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPOSITORY / "tools"))

import check_local_stand_logs as checker  # noqa: E402
import init_local_stand as initializer  # noqa: E402


@unittest.skipUnless(
    os.name == "posix", "local stand log ownership requires POSIX permissions"
)
class LocalStandLogSafetyTests(unittest.TestCase):
    def setUp(self) -> None:
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.base = Path(temporary.name).resolve()
        self.root = self.base / "stand"
        initializer.initialize(self.root, repository=REPOSITORY, home=self.base)
        self.log = self.base / "command.log"

    def test_safe_log_is_accepted(self):
        self.log.write_text("configuration validation passed\n", encoding="utf-8")

        checker.check_logs(self.root, [self.log])

    def test_every_generated_credential_is_rejected_without_echoing_it(self):
        for secret in sorted((self.root / "secrets").iterdir()):
            with self.subTest(secret=secret.name):
                self.log.write_bytes(b"prefix\n" + secret.read_bytes() + b"suffix\n")
                with self.assertRaisesRegex(
                    checker.LogSafetyError, "generated secret material appears"
                ) as failure:
                    checker.check_logs(self.root, [self.log])
                self.assertNotIn(secret.read_text(encoding="ascii").strip(), str(failure.exception))

    def test_private_key_fragment_is_rejected(self):
        private_key = self.root / "pki/gateway-key.pem"
        fragment = private_key.read_bytes().splitlines()[1]
        self.log.write_bytes(b"diagnostic=" + fragment + b"\n")

        with self.assertRaisesRegex(checker.LogSafetyError, "generated secret material"):
            checker.check_logs(self.root, [self.log])

    def test_credential_shape_is_rejected_without_local_value_match(self):
        self.log.write_text(
            "registration.123e4567-e89b-42d3-a456-426614174000."
            "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\n",
            encoding="ascii",
        )

        with self.assertRaisesRegex(checker.LogSafetyError, "credential-shaped material"):
            checker.check_logs(self.root, [self.log])

    def test_symbolic_log_is_rejected(self):
        target = self.base / "target.log"
        target.write_text("safe\n", encoding="utf-8")
        self.log.symlink_to(target)

        with self.assertRaisesRegex(checker.LogSafetyError, "not a bounded regular file"):
            checker.check_logs(self.root, [self.log])


if __name__ == "__main__":
    unittest.main()
