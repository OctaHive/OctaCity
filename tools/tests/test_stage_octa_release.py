"""Safety tests for cross-platform Octa release staging."""

from __future__ import annotations

import importlib.util
import io
import json
import sys
import tarfile
import tempfile
import unittest
from unittest import mock
import urllib.error
import zipfile
from pathlib import Path


REPOSITORY = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("stage_octa_release", REPOSITORY / "tools/stage_octa_release.py")
assert SPEC and SPEC.loader
STAGE = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = STAGE
SPEC.loader.exec_module(STAGE)


class _Response(io.BytesIO):
    """Minimal bounded urllib response used by downloader tests."""

    def __init__(self, payload: bytes):
        super().__init__(payload)
        self.headers = {"Content-Length": str(len(payload))}

    def __enter__(self):
        return self

    def __exit__(self, *_args):
        self.close()


def _release_tar(version: str, revision: str, platform: str) -> bytes:
    payload = io.BytesIO()
    manifest = json.dumps(
        {
            "octa_version": version,
            "build_commit": revision,
            "platform": platform,
        }
    ).encode()
    with tarfile.open(fileobj=payload, mode="w:gz") as archive:
        member = tarfile.TarInfo("octa-runner-capabilities.json")
        member.size = len(manifest)
        archive.addfile(member, io.BytesIO(manifest))
    return payload.getvalue()


class StageOctaReleaseTests(unittest.TestCase):
    def test_stage_falls_back_to_exact_api_assets_after_browser_download_500(self):
        version = "1.2.3"
        revision = "a" * 40
        asset = "octa-Linux-amd64.tar.gz"
        checksum_name = f"{asset}.sha256"
        archive = _release_tar(version, revision, "linux-x86_64")
        checksum = f"{STAGE.hashlib.sha256(archive).hexdigest()}  {asset}\n".encode()
        release_api = f"https://api.github.com/repos/OctaHive/octa/releases/tags/v{version}"
        asset_api = "https://api.github.com/repos/OctaHive/octa/releases/assets/101"
        checksum_api = "https://api.github.com/repos/OctaHive/octa/releases/assets/102"
        metadata = json.dumps(
            {
                "tag_name": f"v{version}",
                "assets": [
                    {"name": asset, "state": "uploaded", "url": asset_api},
                    {"name": checksum_name, "state": "uploaded", "url": checksum_api},
                ],
            }
        ).encode()
        calls: list[str] = []

        def open_url(request, *, timeout):
            self.assertEqual(timeout, 60)
            url = request.full_url
            calls.append(url)
            if url.startswith(f"https://github.com/OctaHive/octa/releases/download/v{version}/"):
                raise urllib.error.HTTPError(url, 500, "Internal Server Error", {}, None)
            if url == release_api:
                self.assertEqual(request.get_header("Authorization"), "Bearer test-token")
            if url in {asset_api, checksum_api}:
                self.assertIsNone(request.get_header("Authorization"))
            responses = {
                release_api: metadata,
                asset_api: archive,
                checksum_api: checksum,
            }
            return _Response(responses[url])

        with tempfile.TemporaryDirectory() as temporary, mock.patch.dict(
            STAGE.os.environ, {"GITHUB_TOKEN": "test-token"}
        ), mock.patch.object(STAGE, "DOWNLOAD_ATTEMPTS", 1), mock.patch.object(
            STAGE.urllib.request, "urlopen", side_effect=open_url
        ):
            destination = Path(temporary) / "octa"
            self.assertEqual(
                STAGE.stage(version, "linux-amd64", revision, destination),
                destination.resolve(),
            )

        self.assertEqual(calls.count(release_api), 1)
        self.assertIn(asset_api, calls)
        self.assertIn(checksum_api, calls)

    def test_api_fallback_requires_every_exact_uploaded_asset(self):
        metadata = json.dumps(
            {
                "tag_name": "v1.2.3",
                "assets": [
                    {
                        "name": "octa-Linux-amd64.tar.gz",
                        "state": "uploaded",
                        "url": "https://api.github.com/repos/OctaHive/octa/releases/assets/101",
                    }
                ],
            }
        ).encode()
        with mock.patch.object(STAGE, "download", return_value=metadata), self.assertRaisesRegex(
            ValueError, "missing required assets"
        ):
            STAGE.release_asset_urls(
                "1.2.3",
                ("octa-Linux-amd64.tar.gz", "octa-Linux-amd64.tar.gz.sha256"),
            )

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
