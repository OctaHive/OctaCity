#!/usr/bin/env python3
"""Build and verify every pinned Linux ARM64 local-stand image."""

from __future__ import annotations

import argparse
from pathlib import Path
import subprocess
import tempfile

import build_local_stand_gateway as gateway_builder
import build_local_stand_server as server_builder
import build_pinned_minio as minio_builder
from local_stand_build_inputs import (
    LocalStandBuildError,
    git_revision,
    load_build_inputs,
    stage_octacity_source,
    stage_octa_source,
)
from pinned_source_archive import SourceArchiveError


REPOSITORY = Path(__file__).resolve().parents[1]
DEFAULT_MANIFEST = REPOSITORY / "deployment/local-stand/inputs.json"
DEFAULT_SERVER_FIXTURE = REPOSITORY / "deployment/local-stand/fixtures/server.toml"
SERVER_IMAGE = "octacity/server:local"
GATEWAY_IMAGE = "octacity/gateway:local"


def build_all(
    manifest: Path,
    server_fixture: Path,
    repository: Path,
) -> dict[str, str]:
    """Stage each source family once, then build and verify all four images."""

    document = load_build_inputs(manifest, repository)
    revision = git_revision(repository)
    with tempfile.TemporaryDirectory(prefix="octacity-build-sources-") as temporary:
        sources = Path(temporary)
        octa_source = sources / "octa"
        octacity_source = sources / "octacity"
        stage_octa_source(document, octa_source)
        stage_octacity_source(repository, octacity_source, revision)
        server_builder.build_staged(
            SERVER_IMAGE,
            document,
            server_fixture,
            octa_source,
            octacity_source,
            revision,
        )
        gateway_builder.build_staged(
            GATEWAY_IMAGE,
            document,
            octa_source,
            octacity_source,
            revision,
        )

    source_images = {
        target: minio_builder.local_image_name(document, target)
        for target in ("minio", "mc")
    }
    minio_builder.build_targets(manifest, source_images, "arm64")
    return {"server": SERVER_IMAGE, "gateway": GATEWAY_IMAGE, **source_images}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", type=Path, default=DEFAULT_MANIFEST)
    parser.add_argument("--server-fixture", type=Path, default=DEFAULT_SERVER_FIXTURE)
    parser.add_argument("--repository", type=Path, default=REPOSITORY)
    arguments = parser.parse_args()
    try:
        images = build_all(
            arguments.manifest.resolve(),
            arguments.server_fixture.resolve(),
            arguments.repository.resolve(),
        )
    except (
        KeyError,
        LocalStandBuildError,
        OSError,
        SourceArchiveError,
        subprocess.CalledProcessError,
        TypeError,
        ValueError,
    ) as error:
        parser.error(str(error))
    print("built and verified local-stand images: " + ", ".join(images.values()))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
