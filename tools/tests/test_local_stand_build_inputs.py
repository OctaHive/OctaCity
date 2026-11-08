"""Released Octa source staging keeps the verified upstream bytes unchanged."""
from __future__ import annotations

import hashlib
import io
import json
from pathlib import Path
import shutil
import sys
import tarfile
import tempfile
import unittest
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import local_stand_build_inputs as sources


class ReleasedOctaSourceTests(unittest.TestCase):
    def test_stages_verified_release_source_without_a_local_adapter_patch(self):
        document = json.loads((Path(__file__).resolve().parents[2] / "deployment/local-stand/inputs.json").read_text())
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive = root / "source.tar.gz"
            files = {
                "Cargo.toml": b'[workspace.package]\nversion = "0.9.0"\n',
                "Cargo.lock": b"version = 4\n",
                "LICENSE": b"MIT\n",
                "crates/octa-runner-protocol/Cargo.toml": b'[package]\nname = "octa-runner-protocol"\n',
                "crates/octa_plugin_codex/src/config.rs": b"// published execution_workspace contract\n",
            }
            with tarfile.open(archive, "w:gz") as bundle:
                for name, data in files.items():
                    member = tarfile.TarInfo("octa-release/" + name)
                    member.size = len(data)
                    bundle.addfile(member, io.BytesIO(data))
            document["native"]["octa"]["source_archive_sha256"] = hashlib.sha256(archive.read_bytes()).hexdigest()
            destination = root / "staged"
            with mock.patch.object(sources, "download", side_effect=lambda policy, path: shutil.copyfile(archive, path)):
                sources.stage_octa_source(document, destination)
            for name, expected in files.items():
                self.assertEqual((destination / name).read_bytes(), expected)
