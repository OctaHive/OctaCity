"""Tests for checksum-pinned MinIO image construction."""

from __future__ import annotations

import importlib.util
import io
import json
from pathlib import Path
import sys
import tarfile
import tempfile
import unittest
from unittest import mock


REPOSITORY = Path(__file__).resolve().parents[2]
MANIFEST = REPOSITORY / "deployment/local-stand/inputs.json"
sys.path.insert(0, str(REPOSITORY / "tools"))
import pinned_source_archive as ARCHIVES

SPEC = importlib.util.spec_from_file_location(
    "build_pinned_minio", REPOSITORY / "tools/build_pinned_minio.py"
)
assert SPEC and SPEC.loader
BUILDER = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = BUILDER
SPEC.loader.exec_module(BUILDER)


class PinnedMinioBuildTests(unittest.TestCase):
    def setUp(self):
        self.document = json.loads(MANIFEST.read_text(encoding="utf-8"))

    def test_release_tag_is_converted_to_the_upstream_version_format(self):
        self.assertEqual(
            BUILDER.release_version("RELEASE.2025-10-15T17-29-55Z"),
            "2025-10-15T17:29:55Z",
        )
        self.assertEqual(
            BUILDER.release_year("RELEASE.2025-10-15T17-29-55Z"), "2025"
        )

    def test_each_supported_architecture_uses_digest_qualified_images(self):
        for architecture in ("arm64", "amd64"):
            with self.subTest(architecture=architecture):
                images = BUILDER.manifest_images(self.document, architecture)
                self.assertEqual(set(images), {"go", "alpine"})
                self.assertTrue(all("@sha256:" in image for image in images.values()))

    def test_docker_arguments_include_exact_source_revisions(self):
        arguments = BUILDER.docker_arguments(
            self.document,
            "minio",
            "octacity/minio:test",
            "arm64",
            Path("/tmp/context"),
        )
        command = "\n".join(arguments)
        for source in self.document["sources"].values():
            self.assertIn(source["source_revision"], command)
            self.assertIn(source["tag"], command)

    def test_mc_image_labels_identify_the_mc_source(self):
        arguments = BUILDER.docker_arguments(
            self.document,
            "mc",
            "octacity/mc:test",
            "amd64",
            Path("/tmp/context"),
        )
        command = "\n".join(arguments)
        source = self.document["sources"]["minio_client"]
        self.assertIn(f"org.opencontainers.image.revision={source['source_revision']}", command)
        self.assertIn(f"org.opencontainers.image.version={source['tag']}", command)

    def test_source_extraction_rejects_path_traversal(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive = root / "source.tar.gz"
            with tarfile.open(archive, "w:gz") as bundle:
                member = tarfile.TarInfo("source/../../escape")
                payload = b"unsafe"
                member.size = len(payload)
                bundle.addfile(member, io.BytesIO(payload))
            with self.assertRaisesRegex(BUILDER.BuildError, "unsafe source archive member"):
                BUILDER.extract_source(
                    archive, root / "output", self.document["sources"]["minio"]
                )
            self.assertFalse((root / "escape").exists())

    def test_download_rejects_a_response_over_its_manifest_limit(self):
        source = dict(self.document["sources"]["minio"])
        source["archive_max_bytes"] = 4
        response = io.BytesIO(b"oversized")
        response.headers = {}
        with tempfile.TemporaryDirectory() as temporary:
            destination = Path(temporary) / "source.tar.gz"
            with mock.patch.object(ARCHIVES, "urlopen", return_value=response):
                with self.assertRaisesRegex(BUILDER.BuildError, "exceeds 4 bytes"):
                    BUILDER.download(source, destination, attempts=1)
            self.assertFalse(destination.exists())

    def test_source_extraction_enforces_expanded_and_member_limits(self):
        source = dict(self.document["sources"]["minio"])
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive = root / "source.tar.gz"
            with tarfile.open(archive, "w:gz") as bundle:
                for name in ("source/first", "source/second"):
                    member = tarfile.TarInfo(name)
                    member.size = 4
                    bundle.addfile(member, io.BytesIO(b"data"))

            source["member_max_count"] = 1
            with self.assertRaisesRegex(BUILDER.BuildError, "too many members"):
                BUILDER.extract_source(archive, root / "member-output", source)

            source["member_max_count"] = 2
            source["expanded_max_bytes"] = 7
            with self.assertRaisesRegex(BUILDER.BuildError, "expands beyond"):
                BUILDER.extract_source(archive, root / "size-output", source)

    def test_release_extraction_preserves_root_files_and_rejects_links(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive = root / "release.tar.gz"
            with tarfile.open(archive, "w:gz") as bundle:
                directory = tarfile.TarInfo(".")
                directory.type = tarfile.DIRTYPE
                bundle.addfile(directory)
                executable = tarfile.TarInfo("bin/tool")
                executable.mode = 0o755
                executable.size = 4
                bundle.addfile(executable, io.BytesIO(b"tool"))
            ARCHIVES.extract_tar_archive(
                archive,
                root / "release",
                max_expanded_bytes=16,
                max_file_bytes=8,
                max_members=2,
            )
            self.assertEqual((root / "release/bin/tool").read_bytes(), b"tool")
            self.assertEqual((root / "release/bin/tool").stat().st_mode & 0o777, 0o755)

            linked = root / "linked.tar.gz"
            with tarfile.open(linked, "w:gz") as bundle:
                member = tarfile.TarInfo("tool-link")
                member.type = tarfile.SYMTYPE
                member.linkname = "/tmp/tool"
                bundle.addfile(member)
            with self.assertRaisesRegex(ARCHIVES.SourceArchiveError, "unsupported"):
                ARCHIVES.extract_tar_archive(
                    linked,
                    root / "linked",
                    max_expanded_bytes=16,
                    max_file_bytes=8,
                    max_members=2,
                )

    def test_release_tag_must_resolve_to_the_pinned_revision(self):
        source = self.document["sources"]["minio_client"]
        tag_ref = f"refs/tags/{source['tag']}"
        output = (
            f"{'0' * 40}\t{tag_ref}\n"
            f"{source['source_revision']}\t{tag_ref}^{{}}\n"
        )
        result = mock.Mock(stdout=output)
        with mock.patch.object(ARCHIVES.subprocess, "run", return_value=result) as run:
            BUILDER.verify_release_ref(source)
        run.assert_called_once_with(
            ["git", "ls-remote", source["repository"], tag_ref, f"{tag_ref}^{{}}"],
            check=True,
            capture_output=True,
            text=True,
            timeout=60,
        )

        result.stdout = f"{'f' * 40}\t{tag_ref}\n"
        with mock.patch.object(ARCHIVES.subprocess, "run", return_value=result):
            with self.assertRaisesRegex(BUILDER.BuildError, "resolves to"):
                BUILDER.verify_release_ref(source)

    def test_release_tag_resolution_retries_a_transient_git_failure(self):
        source = self.document["sources"]["minio"]
        tag_ref = f"refs/tags/{source['tag']}"
        failure = ARCHIVES.subprocess.CalledProcessError(
            128,
            ["git", "ls-remote"],
            stderr="temporary upstream failure",
        )
        result = mock.Mock(stdout=f"{source['source_revision']}\t{tag_ref}\n")
        with (
            mock.patch.object(
                ARCHIVES.subprocess, "run", side_effect=[failure, result]
            ) as run,
            mock.patch.object(ARCHIVES.time, "sleep") as sleep,
        ):
            BUILDER.verify_release_ref(source)
        self.assertEqual(run.call_count, 2)
        sleep.assert_called_once_with(1)

    def test_release_tag_resolution_failure_is_bounded_and_diagnostic(self):
        source = self.document["sources"]["minio"]
        failure = ARCHIVES.subprocess.CalledProcessError(
            128,
            ["git", "ls-remote"],
            stderr="upstream unavailable",
        )
        with (
            mock.patch.object(ARCHIVES.subprocess, "run", side_effect=failure) as run,
            mock.patch.object(ARCHIVES.time, "sleep") as sleep,
        ):
            with self.assertRaisesRegex(
                BUILDER.BuildError,
                "after 2 attempts: upstream unavailable",
            ):
                BUILDER.verify_release_ref(source, attempts=2)
        self.assertEqual(run.call_count, 2)
        sleep.assert_called_once_with(1)

    def test_github_release_asset_identity_is_verified(self):
        source = self.document["native"]["microsandbox"]
        response = mock.MagicMock()
        response.read.return_value = json.dumps(
            {
                "tag_name": source["tag"],
                "assets": [
                    {
                        "id": source["asset_id"],
                        "name": source["asset"],
                        "browser_download_url": source["release_url"],
                    }
                ],
            }
        ).encode("utf-8")
        response.__enter__.return_value = response

        with mock.patch.object(ARCHIVES, "urlopen", return_value=response) as request:
            ARCHIVES.verify_github_release_asset(source)

        request.assert_called_once()

    def test_github_release_asset_identity_mismatch_fails_without_retry(self):
        source = self.document["native"]["microsandbox"]
        response = mock.MagicMock()
        response.read.return_value = json.dumps(
            {"tag_name": source["tag"], "assets": []}
        ).encode("utf-8")
        response.__enter__.return_value = response

        with (
            mock.patch.object(ARCHIVES, "urlopen", return_value=response) as request,
            mock.patch.object(ARCHIVES.time, "sleep") as sleep,
            self.assertRaisesRegex(
                ARCHIVES.SourceArchiveError, "does not identify the pinned asset"
            ),
        ):
            ARCHIVES.verify_github_release_asset(source)

        request.assert_called_once()
        sleep.assert_not_called()


if __name__ == "__main__":
    unittest.main()
