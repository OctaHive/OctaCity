#!/usr/bin/env python3
"""Build and verify the pinned local-stand UI gateway image."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import subprocess
import tempfile
from typing import Any

from init_local_stand import GATEWAY_HOSTNAMES, GATEWAY_HTTPS_PORT
from local_stand_build_inputs import (
    CONTAINER_PLATFORM,
    LocalStandBuildError,
    git_revision,
    load_build_inputs,
    stage_octacity_source,
    stage_octa_source,
    ui_toolchain,
    workspace_version,
)
from pinned_source_archive import SourceArchiveError
import verify_local_stand_inputs as inputs


REPOSITORY = Path(__file__).resolve().parents[1]
DEFAULT_MANIFEST = REPOSITORY / "deployment/local-stand/inputs.json"
DOCKERFILE = REPOSITORY / "deployment/local-stand/server.Dockerfile"
STATIC_ROOT = "/srv/octacity-ui"


def docker_arguments(
    document: dict[str, Any],
    image: str,
    octa_source: Path,
    octacity_source: Path,
    revision: str,
) -> list[str]:
    """Create the deterministic BuildKit command for the gateway target."""

    node_version, pnpm_version = ui_toolchain(octacity_source)
    node_image = document["images"]["node"]["reference"]
    nginx_image = document["images"]["nginx"]["reference"]
    return [
        "docker",
        "build",
        "--platform",
        CONTAINER_PLATFORM,
        "--file",
        str(octacity_source / "deployment/local-stand/server.Dockerfile"),
        "--target",
        "gateway",
        "--tag",
        image,
        "--build-context",
        f"octa-source={octa_source.resolve()}",
        "--build-arg",
        f"RUST_IMAGE={document['images']['rust']['reference']}",
        "--build-arg",
        f"NODE_IMAGE={node_image}",
        "--build-arg",
        f"NGINX_IMAGE={nginx_image}",
        "--build-arg",
        f"OCTACITY_VERSION={workspace_version(octacity_source)}",
        "--build-arg",
        f"OCTACITY_REVISION={revision}",
        "--build-arg",
        f"NODE_VERSION={node_version}",
        "--build-arg",
        f"PNPM_VERSION={pnpm_version}",
        str(octacity_source),
    ]


def runtime_contract_command(image: str, certificate: Path, private_key: Path) -> list[str]:
    """Return the isolated runtime, routing, and filesystem contract check."""

    contract = """
test -f __STATIC_ROOT__/index.html
test -d __STATIC_ROOT__/assets
test ! -e /workspace
test ! -e /pnpm
test ! -e /root/.cache/node/corepack
test ! -e /root/.local/share/pnpm
test ! -e __STATIC_ROOT__/src
test ! -e __STATIC_ROOT__/package.json
test ! -e __STATIC_ROOT__/pnpm-lock.yaml
! command -v node
! command -v pnpm
! command -v vite

cat >/tmp/mock-nginx.conf <<'NGINX'
pid /tmp/mock-nginx.pid;
error_log /dev/stderr notice;
events {}
http {
  access_log off;
  client_body_temp_path /tmp/mock-client;
  proxy_temp_path /tmp/mock-proxy;
  fastcgi_temp_path /tmp/mock-fastcgi;
  uwsgi_temp_path /tmp/mock-uwsgi;
  scgi_temp_path /tmp/mock-scgi;
  server {
    listen 8080;
    location = /api/v1/management-only { return 200 "management"; }
    location / { return 404; }
  }
  server { listen 8081; location / { return 404; } }
  server { listen 8082; location / { return 404; } }
  server {
    listen 9000;
    location / { return 200 "$http_host|$request_uri"; }
  }
}
NGINX

cleanup() {
  nginx -s quit >/dev/null 2>&1 || true
  nginx -c /tmp/mock-nginx.conf -s quit >/dev/null 2>&1 || true
}
trap cleanup EXIT
nginx -c /tmp/mock-nginx.conf
nginx
attempt=0
until curl --fail --silent --show-error --cacert /run/secrets/gateway.pem \
  --connect-timeout 1 --max-time 2 https://octacity.localhost:__HTTPS_PORT__/ \
  --output /tmp/index.html; do
  attempt=$((attempt + 1))
  test "$attempt" -lt 20
  sleep 0.1
done
cmp /tmp/index.html __STATIC_ROOT__/index.html

curl --fail --silent --show-error --cacert /run/secrets/gateway.pem \
  https://octacity.localhost:__HTTPS_PORT__/api/v1/management-only --output /tmp/management
printf management >/tmp/expected-management
cmp /tmp/management /tmp/expected-management
if curl --fail --silent --cacert /run/secrets/gateway.pem \
  https://agent.localhost:__HTTPS_PORT__/api/v1/management-only --output /tmp/agent-management; then
  exit 1
fi
if curl --fail --silent --cacert /run/secrets/gateway.pem \
  https://cache.localhost:__HTTPS_PORT__/api/v1/management-only --output /tmp/cache-management; then
  exit 1
fi
if curl --fail --silent --cacert /run/secrets/gateway.pem \
  https://octacity.localhost:__HTTPS_PORT__/api/v1/unknown --output /tmp/api-response; then
  exit 1
fi
if curl --fail --silent --cacert /run/secrets/gateway.pem \
  https://octacity.localhost:__HTTPS_PORT__/health/unknown --output /tmp/health-response; then
  exit 1
fi

object_path='/bucket/key%2Fsegment?partNumber=7&X-Amz-SignedHeaders=host'
curl --fail --silent --show-error --path-as-is --cacert /run/secrets/gateway.pem \
  --resolve objects.localhost:__HTTPS_PORT__:127.0.0.1 \
  "https://objects.localhost:__HTTPS_PORT__${object_path}" --output /tmp/object-host
curl --fail --silent --show-error --path-as-is --cacert /run/secrets/gateway.pem \
  "https://objects.localhost:__HTTPS_PORT__${object_path}" --output /tmp/object-compose
printf 'objects.localhost:__HTTPS_PORT__|/bucket/key%%2Fsegment?partNumber=7&X-Amz-SignedHeaders=host' \
  >/tmp/expected-object
cmp /tmp/object-host /tmp/expected-object
cmp /tmp/object-compose /tmp/expected-object
""".replace("__STATIC_ROOT__", STATIC_ROOT).replace(
        "__HTTPS_PORT__", str(GATEWAY_HTTPS_PORT)
    ).strip()
    return [
        "docker",
        "run",
        "--rm",
        "--platform",
        CONTAINER_PLATFORM,
        "--network",
        "none",
        "--read-only",
        "--tmpfs",
        "/tmp:rw,noexec,nosuid,nodev,size=16m",
        "--mount",
        f"type=bind,src={certificate.resolve()},dst=/run/secrets/gateway.pem,readonly",
        "--mount",
        f"type=bind,src={private_key.resolve()},dst=/run/secrets/gateway-key.pem,readonly",
        "--add-host",
        "server:127.0.0.1",
        "--add-host",
        "minio:127.0.0.1",
        "--add-host",
        "octacity.localhost:127.0.0.1",
        "--add-host",
        "agent.localhost:127.0.0.1",
        "--add-host",
        "cache.localhost:127.0.0.1",
        "--add-host",
        "objects.localhost:127.0.0.1",
        "--entrypoint",
        "/bin/sh",
        image,
        "-ec",
        contract,
    ]


def create_contract_certificate(directory: Path) -> tuple[Path, Path]:
    """Create an ephemeral certificate readable by the non-root test container."""

    certificate = directory / "gateway.pem"
    private_key = directory / "gateway-key.pem"
    subject_names = ",".join(f"DNS:{name}" for name in GATEWAY_HOSTNAMES)
    subprocess.run(
        [
            "openssl",
            "req",
            "-x509",
            "-newkey",
            "rsa:2048",
            "-nodes",
            "-sha256",
            "-days",
            "1",
            "-subj",
            "/CN=octacity.localhost",
            "-addext",
            f"subjectAltName={subject_names}",
            "-keyout",
            str(private_key),
            "-out",
            str(certificate),
        ],
        check=True,
        capture_output=True,
    )
    # The containing temporary directory remains owner-private.  Read-only
    # files let the fixed container uid consume the fixture through bind mounts.
    certificate.chmod(0o444)
    private_key.chmod(0o444)
    return certificate, private_key


def verify_image(
    image: str,
    expected_version: str,
    expected_revision: str,
    node_version: str,
    pnpm_version: str,
) -> None:
    """Verify pinned metadata and the minimal static-only runtime contents."""

    inspected = subprocess.run(
        ["docker", "image", "inspect", "--format", "{{json .Config}}", image],
        check=True,
        capture_output=True,
        text=True,
    )
    configuration = json.loads(inspected.stdout)
    if configuration.get("User") != "101:101":
        raise LocalStandBuildError("gateway image must run as nginx uid:gid 101:101")
    labels = configuration.get("Labels") or {}
    expected = {
        "org.opencontainers.image.version": expected_version,
        "org.opencontainers.image.revision": expected_revision,
        "dev.octacity.node.version": node_version,
        "dev.octacity.pnpm.version": pnpm_version,
    }
    if any(labels.get(name) != value for name, value in expected.items()):
        raise LocalStandBuildError("gateway image toolchain labels are invalid")
    with tempfile.TemporaryDirectory(prefix="octacity-gateway-contract-") as temporary:
        certificate, private_key = create_contract_certificate(Path(temporary))
        subprocess.run(
            runtime_contract_command(image, certificate, private_key),
            check=True,
        )


def build(image: str, manifest: Path, repository: Path) -> None:
    """Stage verified source, build the image, and run its contract checks."""

    document = load_build_inputs(manifest, repository)
    revision = git_revision(repository)
    with tempfile.TemporaryDirectory(prefix="octacity-build-sources-") as temporary:
        root = Path(temporary)
        octa_source = root / "octa"
        octacity_source = root / "octacity"
        stage_octa_source(document, octa_source)
        stage_octacity_source(repository, octacity_source, revision)
        build_staged(image, document, octa_source, octacity_source, revision)


def build_staged(
    image: str,
    document: dict[str, Any],
    octa_source: Path,
    octacity_source: Path,
    revision: str,
) -> None:
    """Build and verify the gateway from an already verified shared context."""

    expected_version = workspace_version(octacity_source)
    node_version, pnpm_version = ui_toolchain(octacity_source)
    subprocess.run(
        docker_arguments(document, image, octa_source, octacity_source, revision),
        check=True,
    )
    verify_image(image, expected_version, revision, node_version, pnpm_version)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("image")
    parser.add_argument("--manifest", type=Path, default=DEFAULT_MANIFEST)
    parser.add_argument("--repository", type=Path, default=REPOSITORY)
    arguments = parser.parse_args()
    try:
        build(
            arguments.image,
            arguments.manifest.resolve(),
            arguments.repository.resolve(),
        )
    except (
        OSError,
        KeyError,
        TypeError,
        json.JSONDecodeError,
        inputs.InputError,
        LocalStandBuildError,
        SourceArchiveError,
        subprocess.CalledProcessError,
    ) as error:
        parser.error(str(error))
    print(f"built and verified local-stand gateway image: {arguments.image}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
