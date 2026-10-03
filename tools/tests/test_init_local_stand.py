"""Tests for private local-stand host-state initialization."""

from __future__ import annotations

import importlib.util
import json
import os
from pathlib import Path
import shutil
import stat
import subprocess
import sys
import tempfile
import unittest
from unittest import mock


REPOSITORY = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPOSITORY / "tools"))
SPEC = importlib.util.spec_from_file_location(
    "init_local_stand", REPOSITORY / "tools/init_local_stand.py"
)
assert SPEC and SPEC.loader
INITIALIZER = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = INITIALIZER
SPEC.loader.exec_module(INITIALIZER)


@unittest.skipUnless(os.name == "posix", "local stand host state requires POSIX permissions")
class LocalStandInitializerTests(unittest.TestCase):
    def setUp(self):
        openssl = shutil.which("openssl")
        if openssl is None:
            self.skipTest("OpenSSL is required for local PKI tests")
        self.openssl = openssl
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name) / "stand"

    def initialize(self):
        return INITIALIZER.initialize(
            self.root,
            repository=REPOSITORY,
            home=Path.home(),
            openssl=self.openssl,
        )

    def test_creates_private_separated_credentials_and_valid_local_pki(self):
        receipt = self.initialize()

        self.assertEqual(receipt["root"], str(self.root.resolve()))
        for directory in (self.root, self.root / "secrets", self.root / "pki"):
            self.assertEqual(stat.S_IMODE(directory.stat().st_mode), 0o700)
            self.assertEqual(directory.stat().st_uid, os.getuid())

        credential_paths = {
            name: Path(path) for name, path in receipt["credentials"].items()
        }
        self.assertEqual(set(credential_paths), set(INITIALIZER.CREDENTIAL_FILES))
        credential_values = []
        for path in credential_paths.values():
            metadata = path.lstat()
            self.assertTrue(stat.S_ISREG(metadata.st_mode))
            self.assertFalse(path.is_symlink())
            self.assertEqual(stat.S_IMODE(metadata.st_mode), 0o600)
            self.assertEqual(metadata.st_uid, os.getuid())
            credential_values.append(path.read_bytes())
        self.assertEqual(len(set(credential_values)), len(credential_values))

        pki = {name: Path(path) for name, path in receipt["pki"].items()}
        for name in INITIALIZER.PRIVATE_PKI_FILES:
            self.assertEqual(stat.S_IMODE(pki[name].stat().st_mode), 0o600)
        for name in INITIALIZER.PUBLIC_PKI_FILES:
            self.assertEqual(stat.S_IMODE(pki[name].stat().st_mode), 0o644)
        self.assertEqual(
            INITIALIZER.certificate_dns_names(pki["gateway_certificate"], self.openssl),
            set(INITIALIZER.GATEWAY_HOSTNAMES),
        )
        INITIALIZER.validate_pki(self.root / "pki", self.root / "secrets", self.openssl)

    def test_rerun_preserves_every_credential_and_key(self):
        first = self.initialize()
        paths = [
            Path(path)
            for section in ("credentials", "pki")
            for path in first[section].values()
        ]
        snapshot = {path: (path.read_bytes(), path.stat().st_mtime_ns) for path in paths}

        second = self.initialize()

        self.assertEqual(first, second)
        self.assertEqual(
            snapshot,
            {path: (path.read_bytes(), path.stat().st_mtime_ns) for path in paths},
        )

    def test_cli_emits_only_paths_and_never_credential_values(self):
        result = subprocess.run(
            [
                sys.executable,
                str(REPOSITORY / "tools/init_local_stand.py"),
                "--root",
                str(self.root),
                "--openssl",
                self.openssl,
            ],
            check=True,
            capture_output=True,
            text=True,
        )
        receipt = json.loads(result.stdout)

        self.assertEqual(receipt["root"], str(self.root.resolve()))
        output = result.stdout + result.stderr
        for path in receipt["credentials"].values():
            self.assertNotIn(Path(path).read_text(encoding="ascii").strip(), output)

    def test_rejects_broad_relative_repository_and_symbolic_roots(self):
        candidates = [Path("relative"), Path("/"), Path.home(), REPOSITORY, REPOSITORY / "state"]
        for candidate in candidates:
            with self.subTest(candidate=candidate):
                with self.assertRaises(INITIALIZER.InitializationError):
                    INITIALIZER.initialize(
                        candidate,
                        repository=REPOSITORY,
                        home=Path.home(),
                        openssl=self.openssl,
                    )

        target = Path(self.temporary.name) / "target"
        target.mkdir(mode=0o700)
        symbolic = Path(self.temporary.name) / "symbolic"
        symbolic.symlink_to(target, target_is_directory=True)
        with self.assertRaisesRegex(INITIALIZER.InitializationError, "symbolic"):
            INITIALIZER.initialize(
                symbolic,
                repository=REPOSITORY,
                home=Path.home(),
                openssl=self.openssl,
            )

    def test_rejects_non_regular_symbolic_and_over_permissive_credentials(self):
        receipt = self.initialize()
        credentials = {name: Path(path) for name, path in receipt["credentials"].items()}

        credentials["database_password"].unlink()
        credentials["database_password"].mkdir()
        with self.assertRaisesRegex(INITIALIZER.InitializationError, "regular file"):
            self.initialize()

        credentials["database_password"].rmdir()
        credentials["database_password"].symlink_to(credentials["object_secret_key"])
        with self.assertRaisesRegex(INITIALIZER.InitializationError, "symbolic"):
            self.initialize()

        credentials["database_password"].unlink()
        credentials["database_password"].write_text("replacement", encoding="utf-8")
        credentials["database_password"].chmod(0o644)
        with self.assertRaisesRegex(INITIALIZER.InitializationError, "mode 0600"):
            self.initialize()

    def test_rejects_wrong_ownership_incomplete_pki_and_reused_credentials(self):
        receipt = self.initialize()
        credentials = {name: Path(path) for name, path in receipt["credentials"].items()}

        with mock.patch.object(INITIALIZER.state, "current_uid", return_value=os.getuid() + 1):
            with self.assertRaisesRegex(INITIALIZER.InitializationError, "owned"):
                self.initialize()

        gateway_certificate = Path(receipt["pki"]["gateway_certificate"])
        gateway_certificate.unlink()
        with self.assertRaisesRegex(INITIALIZER.InitializationError, "incomplete"):
            self.initialize()

        gateway_certificate.write_bytes(b"not a certificate")
        gateway_certificate.chmod(0o644)
        credentials["cache_credential_key"].write_bytes(
            credentials["agent_enrollment_key"].read_bytes()
        )
        with self.assertRaisesRegex(INITIALIZER.InitializationError, "reused"):
            self.initialize()

    def test_rejects_a_hard_link_to_private_state(self):
        receipt = self.initialize()
        credential = Path(receipt["credentials"]["database_password"])
        alias = self.root / "credential-alias"
        os.link(credential, alias)

        with self.assertRaisesRegex(INITIALIZER.InitializationError, "hard links"):
            self.initialize()

    def test_bounded_reader_rejects_in_place_changes(self):
        receipt = self.initialize()
        credential = Path(receipt["credentials"]["database_password"])
        before = (1, 2, 3, 4, 5)
        after = (1, 2, 3, 6, 7)

        with (
            mock.patch.object(
                INITIALIZER.state,
                "file_state",
                side_effect=[before, after],
            ),
            self.assertRaisesRegex(INITIALIZER.state.StateError, "changed while it was read"),
        ):
            INITIALIZER.state.read_regular_file(
                credential,
                "fixture credential",
                max_bytes=INITIALIZER.MAX_CREDENTIAL_BYTES,
                modes=frozenset({0o600}),
            )


if __name__ == "__main__":
    unittest.main()
