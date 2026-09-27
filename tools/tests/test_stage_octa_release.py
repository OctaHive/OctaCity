"""Safety tests for cross-platform Octa release staging."""

from __future__ import annotations

import importlib.util
import io
import sys
import tarfile
import tempfile
import unittest
import zipfile
from pathlib import Path


REPOSITORY = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("stage_octa_release", REPOSITORY / "tools/stage_octa_release.py")
assert SPEC and SPEC.loader
STAGE = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = STAGE
SPEC.loader.exec_module(STAGE)


class StageOctaReleaseTests(unittest.TestCase):
    def test_checksum_must_name_the_exact_asset(self):
        digest = "a" * 64
        self.assertEqual(STAGE.expected_digest(f"{digest}  octa.zip\n".encode(), "octa.zip"), digest)
        with self.assertRaisesRegex(ValueError, "requested asset"):
            STAGE.expected_digest(f"{digest}  another.zip\n".encode(), "octa.zip")

    def test_tar_extraction_rejects_links_and_parent_traversal(self):
        for name, member_type in [("../escape", tarfile.REGTYPE), ("runner-link", tarfile.SYMTYPE)]:
            payload = io.BytesIO()
            with tarfile.open(fileobj=payload, mode="w:gz") as archive:
                member = tarfile.TarInfo(name)
                member.type = member_type
                if member_type == tarfile.SYMTYPE:
                    member.linkname = "runner"
                else:
                    member.size = 1
                    archive.addfile(member, io.BytesIO(b"x"))
                    continue
                archive.addfile(member)
            with tempfile.TemporaryDirectory() as temporary:
                with self.assertRaises(ValueError):
                    STAGE.extract_tar(payload.getvalue(), Path(temporary))

    def test_zip_extraction_rejects_parent_traversal(self):
        payload = io.BytesIO()
        with zipfile.ZipFile(payload, mode="w") as archive:
            archive.writestr("../escape", b"x")
        with tempfile.TemporaryDirectory() as temporary:
            with self.assertRaisesRegex(ValueError, "unsafe"):
                STAGE.extract_zip(payload.getvalue(), Path(temporary))

    def test_extraction_rejects_a_payload_over_the_declared_budget(self):
        original = STAGE.MAX_EXTRACTED_BYTES
        STAGE.MAX_EXTRACTED_BYTES = 1
        try:
            tar_payload = io.BytesIO()
            with tarfile.open(fileobj=tar_payload, mode="w:gz") as archive:
                member = tarfile.TarInfo("runner")
                member.size = 2
                archive.addfile(member, io.BytesIO(b"xx"))
            zip_payload = io.BytesIO()
            with zipfile.ZipFile(zip_payload, mode="w") as archive:
                archive.writestr("runner.exe", b"xx")
            for extractor, payload in (
                (STAGE.extract_tar, tar_payload.getvalue()),
                (STAGE.extract_zip, zip_payload.getvalue()),
            ):
                with tempfile.TemporaryDirectory() as temporary:
                    with self.assertRaisesRegex(ValueError, "expands beyond"):
                        extractor(payload, Path(temporary))
        finally:
            STAGE.MAX_EXTRACTED_BYTES = original


if __name__ == "__main__":
    unittest.main()
