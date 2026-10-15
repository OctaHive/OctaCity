"""Build the test-owned JSON runtime image from two immutable stand inputs."""
from __future__ import annotations

import argparse
import json
import re
from pathlib import Path
import subprocess
from collections.abc import Callable


def _image_identity(payload: str) -> str:
    metadata = json.loads(payload)
    if not isinstance(metadata, dict) or metadata.get("os") != "linux" or metadata.get("architecture") != "arm64" or not re.fullmatch(r"sha256:[a-f0-9]{64}", metadata.get("digest", "")):
        raise ValueError("imported pilot image identity or platform is not qualified")
    return metadata["digest"]


def prepare_image(inputs: dict, msb: Path, work: Path, *, execute: Callable = subprocess.run) -> str:
    """Import a JSON-capable image; return its content-addressed MSB reference."""
    node = inputs["images"]["node"]["reference"]
    base = inputs["native"]["microsandbox"]["guest_image"]
    if inputs["images"]["node"]["platform"] != "linux/arm64" or any(
        not re.fullmatch(r"[a-z0-9][a-z0-9./:_-]*@sha256:[a-f0-9]{64}", reference)
        for reference in (node, base)
    ):
        raise ValueError("pilot image requires immutable Linux ARM64 source inputs")
    work.mkdir(parents=True, exist_ok=True)
    (work / "node-wrapper").write_text(
        '#!/bin/sh\nLD_LIBRARY_PATH=/opt/octacity-json/lib exec /opt/octacity-json/node "$@"\n'
    )
    (work / "Dockerfile").write_text(
        f"FROM {node} AS node\nFROM {base}\n"
        "COPY --from=node /usr/local/bin/node /opt/octacity-json/node\n"
        "COPY --from=node /usr/lib/aarch64-linux-gnu/libstdc++.so.6 /opt/octacity-json/lib/libstdc++.so.6\n"
        "COPY --from=node /lib/aarch64-linux-gnu/libgcc_s.so.1 /opt/octacity-json/lib/libgcc_s.so.1\n"
        "COPY --chmod=0555 node-wrapper /usr/local/bin/node\n"
    )
    archive = work / "image.docker.tar"
    staging = "local.octacity/factory-pilot:preparing"
    execute(["docker", "build", "--platform=linux/arm64", "--tag", staging,
             "--file", str(work / "Dockerfile"), str(work)], check=True, capture_output=True, text=True)
    execute(["docker", "image", "save", "--output", str(archive), staging], check=True, capture_output=True, text=True)
    execute([str(msb), "load", "--input", str(archive), "--tag", staging], check=True, capture_output=True, text=True)
    inspected = execute([str(msb), "image", "inspect", staging, "--format", "json"], check=True, capture_output=True, text=True)
    digest = _image_identity(inspected.stdout)
    reference = "local.octacity/factory-pilot@" + digest
    execute([str(msb), "load", "--input", str(archive), "--tag", reference], check=True, capture_output=True, text=True)
    imported = execute([str(msb), "image", "inspect", reference, "--format", "json"], check=True, capture_output=True, text=True)
    if _image_identity(imported.stdout) != digest:
        raise ValueError("imported pilot image identity or platform is not qualified")
    execute([str(msb), "save", reference, "--format", "oci", "--output", str(work / "qualified-image.oci.tar")],
            check=True, capture_output=True, text=True)
    return reference


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--inputs", type=Path, default=Path(__file__).resolve().parents[1] / "deployment/local-stand/inputs.json")
    parser.add_argument("--msb", type=Path, required=True)
    parser.add_argument("--work", type=Path, required=True)
    args = parser.parse_args()
    print(prepare_image(json.loads(args.inputs.read_text()), args.msb, args.work.resolve()))


if __name__ == "__main__":
    main()
