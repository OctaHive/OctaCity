#!/usr/bin/env python3
"""Package and verify the platform-independent OctaCity console release."""

from __future__ import annotations

import argparse
import hashlib
from html.parser import HTMLParser
import json
from pathlib import Path, PurePosixPath
import re
import tarfile
import tempfile

from release_packaging import (
    archive_tar,
    checksum_lines,
    copy_file,
    load_release_contract,
    require_file,
    sha256,
    validate_revision,
    validate_version,
    write_text,
)


PRODUCT = "octacity-console"
ENTRYPOINT = "index.html"
MANIFEST = "release-manifest.json"
CHECKSUMS = "SHA256SUMS"
CONTRACT = "release-contract.json"
LICENSE = "LICENSE"
DEPLOYMENT_GUIDE = "share/operator-console.md"
NGINX_DIRECTORY = "share/nginx"
NGINX_FILES = (
    "cache-map.conf",
    "nginx.conf",
    "routes.conf",
    "security-headers.conf",
)
SUPPORT_FILES = {f"{NGINX_DIRECTORY}/{name}" for name in NGINX_FILES}
MAX_ARCHIVE_BYTES = 64 * 1024 * 1024
MAX_EXPANDED_BYTES = 128 * 1024 * 1024
MAX_FILE_BYTES = 32 * 1024 * 1024
MAX_MEMBERS = 1_024
MAX_CHECKSUM_BYTES = 1_024
ASSET_NAME = re.compile(
    r"[^/]+-[A-Za-z0-9_-]{8,}\.(?:css|js|svg|png|jpe?g|webp|avif|ico|woff2)\Z"
)
SHA256 = re.compile(r"[0-9a-f]{64}\Z")
ENVIRONMENT_ORIGIN = re.compile(
    rb"(?i)(?:"
    rb"(?:https?:)?//[^\s\"'`<>]+/(?:api/v1|health)(?:[/\s\"'`<>?]|$)|"
    rb"(?:VITE_|OCTACITY_)(?:API_)?ORIGIN)"
)
SOURCE_MAP_MARKER = re.compile(rb"sourceMappingURL=")
EXECUTABLE_MAGICS = (
    b"\x7fELF",
    b"MZ",
    b"\xca\xfe\xba\xbe",
    b"\xce\xfa\xed\xfe",
    b"\xcf\xfa\xed\xfe",
    b"\xfe\xed\xfa\xce",
    b"\xfe\xed\xfa\xcf",
)
FORBIDDEN_NAMES = {
    "node",
    "node.exe",
    "node_modules",
    "npm",
    "npm.cmd",
    "package.json",
    "pnpm",
    "pnpm-lock.yaml",
    "yarn.lock",
}


class ConsoleReleaseError(ValueError):
    """The console release cannot be packaged or trusted."""


class _AssetReferences(HTMLParser):
    def __init__(self) -> None:
        super().__init__()
        self.references: list[str] = []

    def handle_starttag(
        self, tag: str, attrs: list[tuple[str, str | None]]
    ) -> None:
        for name, value in attrs:
            if name in {"href", "src"} and value is not None:
                self.references.append(value)


def _digest_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def _safe_path(value: str) -> str:
    candidate = value.removesuffix("/")
    path = PurePosixPath(candidate)
    if (
        not candidate
        or value.startswith("/")
        or "\\" in value
        or path.is_absolute()
        or any(part in {"", ".", ".."} for part in path.parts)
        or path.as_posix() != candidate
    ):
        raise ConsoleReleaseError(f"unsafe console release path: {value!r}")
    return path.as_posix()


def _validate_payload_name(relative: str) -> None:
    path = PurePosixPath(relative)
    lowered = {part.lower() for part in path.parts}
    if lowered & FORBIDDEN_NAMES:
        raise ConsoleReleaseError(f"console release contains development tooling: {relative}")
    if relative.endswith(".map") or any(part in {"src", "source", "sources"} for part in lowered):
        raise ConsoleReleaseError(f"console release contains source-bearing output: {relative}")
    if any(part.startswith(".") for part in path.parts):
        raise ConsoleReleaseError(f"console release contains a hidden file: {relative}")


def _validate_application(files: dict[str, bytes]) -> list[dict[str, object]]:
    application = {
        name: contents
        for name, contents in files.items()
        if name == ENTRYPOINT or name.startswith("assets/")
    }
    if ENTRYPOINT not in application:
        raise ConsoleReleaseError("console release is missing index.html")
    unexpected = set(files) - application.keys() - {
        MANIFEST,
        CHECKSUMS,
        CONTRACT,
        LICENSE,
        DEPLOYMENT_GUIDE,
    } - SUPPORT_FILES
    if unexpected:
        raise ConsoleReleaseError(
            f"console release contains an unexpected file: {min(unexpected)}"
        )
    assets = sorted(name for name in application if name.startswith("assets/"))
    if not assets:
        raise ConsoleReleaseError("console release contains no static assets")
    for name, contents in application.items():
        _validate_payload_name(name)
        if len(contents) > MAX_FILE_BYTES:
            raise ConsoleReleaseError(f"console release file is too large: {name}")
        if ENVIRONMENT_ORIGIN.search(contents):
            raise ConsoleReleaseError(
                f"console release contains an environment-specific origin: {name}"
            )
        if contents.startswith((b"#!", *EXECUTABLE_MAGICS)):
            raise ConsoleReleaseError(
                f"console release contains an executable runtime: {name}"
            )
        if name.endswith((".css", ".js")) and SOURCE_MAP_MARKER.search(contents):
            raise ConsoleReleaseError(f"console release contains a source-map reference: {name}")
    for name in assets:
        relative = name.removeprefix("assets/")
        if "/" in relative or not ASSET_NAME.fullmatch(relative):
            raise ConsoleReleaseError(
                f"console static asset is not content-hashed: {name}"
            )
    try:
        parser = _AssetReferences()
        parser.feed(application[ENTRYPOINT].decode("utf-8"))
    except (UnicodeDecodeError, ValueError) as error:
        raise ConsoleReleaseError(f"console index.html is invalid: {error}") from error
    for reference in parser.references:
        if reference.startswith(("http://", "https://", "//")):
            raise ConsoleReleaseError(
                f"console index.html contains an external origin: {reference}"
            )
        if not reference.startswith("/assets/"):
            raise ConsoleReleaseError(
                f"console index.html references an unhashed resource: {reference}"
            )
        target = reference.removeprefix("/").split("?", 1)[0].split("#", 1)[0]
        if target not in application:
            raise ConsoleReleaseError(
                f"console index.html references a missing asset: {target}"
            )
    return [
        {"path": name, "sha256": _digest_bytes(application[name]), "size": len(application[name])}
        for name in assets
    ]


def _read_distribution(
    distribution: Path,
) -> tuple[dict[str, bytes], list[dict[str, object]]]:
    if not distribution.is_dir() or distribution.is_symlink():
        raise ConsoleReleaseError(
            f"console distribution must be a regular directory: {distribution}"
        )
    files: dict[str, bytes] = {}
    for path in sorted(distribution.rglob("*")):
        relative = path.relative_to(distribution).as_posix()
        if path.is_symlink() or (not path.is_file() and not path.is_dir()):
            raise ConsoleReleaseError(
                f"console distribution contains an unsupported entry: {relative}"
            )
        if path.is_file():
            if len(files) >= MAX_MEMBERS:
                raise ConsoleReleaseError("console distribution contains too many files")
            contents = path.read_bytes()
            if len(contents) > MAX_FILE_BYTES:
                raise ConsoleReleaseError(f"console distribution file is too large: {relative}")
            files[_safe_path(relative)] = contents
    return files, _validate_application(files)


def _copy_distribution(
    distribution: Path, root: Path
) -> tuple[dict[str, bytes], list[dict[str, object]]]:
    files, assets = _read_distribution(distribution)
    for relative, contents in files.items():
        destination = root / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes(contents)
        destination.chmod(0o644)
    return files, assets


def _manifest(
    version: str,
    revision: str,
    files: dict[str, bytes],
    assets: list[dict[str, object]],
    proxy: bytes,
) -> dict[str, object]:
    return {
        "format_version": 1,
        "product": PRODUCT,
        "version": version,
        "platform": "any",
        "build_inputs": {"octacity_revision": revision},
        "protocols": {},
        "components": {
            "application": {
                "path": ENTRYPOINT,
                "sha256": _digest_bytes(files[ENTRYPOINT]),
            },
            "proxy": {
                "path": f"{NGINX_DIRECTORY}/nginx.conf",
                "sha256": _digest_bytes(proxy),
            },
        },
        "assets": assets,
    }


def package_console(
    repository: Path,
    distribution: Path,
    version: str,
    revision: str,
    output: Path,
) -> Path:
    """Write and verify one deterministic platform-independent console archive."""

    repository = repository.resolve()
    version = validate_version(version)
    revision = validate_revision("OctaCity revision", revision)
    if not output.name.endswith(".tar.gz"):
        raise ConsoleReleaseError("console release output must end with .tar.gz")
    contract_path, product_contract = load_release_contract(repository, PRODUCT)
    if product_contract["protocols"] or product_contract["required_components"] != [
        "application",
        "proxy",
    ]:
        raise ConsoleReleaseError("console release contract has an unsupported shape")
    output = output.resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="octacity-console-release-") as temporary:
        root = Path(temporary) / PRODUCT
        root.mkdir()
        files, assets = _copy_distribution(distribution.resolve(), root)
        copy_file(contract_path, root / CONTRACT)
        copy_file(require_file("license", repository / LICENSE), root / LICENSE)
        copy_file(
            require_file(
                "operator console deployment guide",
                repository / "docs/operations/operator-console.md",
            ),
            root / DEPLOYMENT_GUIDE,
        )
        nginx_root = repository / "deployment/console/nginx"
        for name in NGINX_FILES:
            copy_file(
                require_file(f"operator console nginx {name}", nginx_root / name),
                root / NGINX_DIRECTORY / name,
            )
        proxy = (root / NGINX_DIRECTORY / "nginx.conf").read_bytes()
        manifest = _manifest(version, revision, files, assets, proxy)
        write_text(root / MANIFEST, json.dumps(manifest, indent=2, sort_keys=True) + "\n")
        write_text(root / CHECKSUMS, checksum_lines(root, {CHECKSUMS}))
        archive_tar(root, output)
    write_text(
        output.with_name(output.name + ".sha256"),
        f"{sha256(output)}  {output.name}\n",
    )
    verify_console(
        output,
        expected_version=version,
        expected_revision=revision,
    )
    return output


def _read_archive(archive_path: Path) -> dict[str, bytes]:
    if not archive_path.is_file() or archive_path.is_symlink():
        raise ConsoleReleaseError(f"console archive must be a regular file: {archive_path}")
    if archive_path.stat().st_size > MAX_ARCHIVE_BYTES:
        raise ConsoleReleaseError("console archive exceeds its size limit")
    files: dict[str, bytes] = {}
    members: set[str] = set()
    expanded = 0
    try:
        with tarfile.open(archive_path, mode="r:gz") as archive:
            for index, member in enumerate(archive):
                if index >= MAX_MEMBERS:
                    raise ConsoleReleaseError("console archive contains too many entries")
                relative = _safe_path(member.name)
                if relative in members:
                    raise ConsoleReleaseError(
                        f"console archive contains a duplicate entry: {relative}"
                    )
                members.add(relative)
                if member.isdir():
                    continue
                if not member.isfile():
                    raise ConsoleReleaseError(
                        f"console archive contains an unsupported entry: {relative}"
                    )
                if member.size > MAX_FILE_BYTES:
                    raise ConsoleReleaseError(f"console archive file is too large: {relative}")
                expanded += member.size
                if expanded > MAX_EXPANDED_BYTES:
                    raise ConsoleReleaseError("console archive exceeds its expanded size limit")
                source = archive.extractfile(member)
                if source is None:
                    raise ConsoleReleaseError(
                        f"console archive file cannot be read: {relative}"
                    )
                contents = source.read(MAX_FILE_BYTES + 1)
                if len(contents) != member.size:
                    raise ConsoleReleaseError(
                        f"console archive file is truncated: {relative}"
                    )
                files[relative] = contents
    except (OSError, tarfile.TarError) as error:
        raise ConsoleReleaseError(f"cannot read console archive: {error}") from error
    return files


def _verify_sidecar(archive_path: Path) -> None:
    sidecar = archive_path.with_name(archive_path.name + ".sha256")
    if not sidecar.is_file() or sidecar.is_symlink():
        raise ConsoleReleaseError(
            f"console archive checksum sidecar must be a regular file: {sidecar}"
        )
    try:
        with sidecar.open("rb") as source:
            contents = source.read(MAX_CHECKSUM_BYTES + 1)
    except OSError as error:
        raise ConsoleReleaseError(f"console archive checksum sidecar is invalid: {error}") from error
    if len(contents) > MAX_CHECKSUM_BYTES:
        raise ConsoleReleaseError("console archive checksum sidecar exceeds its size limit")
    try:
        line = contents.decode("ascii")
        expected, name = line.rstrip("\n").split("  ", 1)
    except (UnicodeDecodeError, ValueError) as error:
        raise ConsoleReleaseError(f"console archive checksum sidecar is invalid: {error}") from error
    if line != f"{expected}  {name}\n" or not SHA256.fullmatch(expected):
        raise ConsoleReleaseError("console archive checksum sidecar is malformed")
    if name != archive_path.name or sha256(archive_path) != expected:
        raise ConsoleReleaseError("console archive checksum does not match")


def _load_json(files: dict[str, bytes], name: str) -> dict[str, object]:
    try:
        value = json.loads(files[name])
    except (KeyError, UnicodeDecodeError, json.JSONDecodeError) as error:
        raise ConsoleReleaseError(f"console {name} is invalid: {error}") from error
    if not isinstance(value, dict):
        raise ConsoleReleaseError(f"console {name} must be an object")
    return value


def _verify_checksums(files: dict[str, bytes]) -> None:
    try:
        lines = files[CHECKSUMS].decode("utf-8").splitlines()
    except (KeyError, UnicodeDecodeError) as error:
        raise ConsoleReleaseError(f"console {CHECKSUMS} is invalid: {error}") from error
    checked: dict[str, str] = {}
    for line in lines:
        try:
            digest, relative = line.split("  ", 1)
        except ValueError as error:
            raise ConsoleReleaseError(f"console {CHECKSUMS} contains an invalid line") from error
        relative = _safe_path(relative)
        if not SHA256.fullmatch(digest) or relative == CHECKSUMS or relative in checked:
            raise ConsoleReleaseError(f"console {CHECKSUMS} contains an invalid entry")
        checked[relative] = digest
    expected = set(files) - {CHECKSUMS}
    if set(checked) != expected:
        raise ConsoleReleaseError(f"console {CHECKSUMS} does not cover the exact file set")
    for relative, digest in checked.items():
        if _digest_bytes(files[relative]) != digest:
            raise ConsoleReleaseError(f"console checksum differs for {relative}")


def _verify_manifest(files: dict[str, bytes]) -> dict[str, object]:
    manifest = _load_json(files, MANIFEST)
    expected_keys = {
        "format_version",
        "product",
        "version",
        "platform",
        "build_inputs",
        "protocols",
        "components",
        "assets",
    }
    if set(manifest) != expected_keys:
        raise ConsoleReleaseError("console release manifest has an unexpected shape")
    build_inputs = manifest["build_inputs"]
    components = manifest["components"]
    assets = manifest["assets"]
    if (
        not isinstance(build_inputs, dict)
        or not isinstance(components, dict)
        or not isinstance(assets, list)
        or set(build_inputs) != {"octacity_revision"}
        or set(components) != {"application", "proxy"}
        or not isinstance(components["application"], dict)
        or not isinstance(components["proxy"], dict)
    ):
        raise ConsoleReleaseError("console release manifest identity is invalid")
    application = components["application"]
    proxy = components["proxy"]
    try:
        validate_version(manifest["version"])
        validate_revision("OctaCity revision", build_inputs["octacity_revision"])
    except (TypeError, ValueError) as error:
        raise ConsoleReleaseError(f"console release manifest is invalid: {error}") from error
    if (
        manifest["format_version"] != 1
        or manifest["product"] != PRODUCT
        or manifest["platform"] != "any"
        or manifest["protocols"] != {}
        or set(application) != {"path", "sha256"}
        or application["path"] != ENTRYPOINT
        or application["sha256"] != _digest_bytes(files[ENTRYPOINT])
        or set(proxy) != {"path", "sha256"}
        or proxy["path"] != f"{NGINX_DIRECTORY}/nginx.conf"
        or proxy["sha256"] != _digest_bytes(files[f"{NGINX_DIRECTORY}/nginx.conf"])
    ):
        raise ConsoleReleaseError("console release manifest identity is invalid")
    expected_assets = _validate_application(files)
    if assets != expected_assets:
        raise ConsoleReleaseError("console release manifest asset inventory differs")
    contract = _load_json(files, CONTRACT)
    try:
        product = contract["products"][PRODUCT]
    except (KeyError, TypeError) as error:
        raise ConsoleReleaseError("console release contract omits the console product") from error
    if (
        not isinstance(product, dict)
        or contract.get("format_version") != 1
        or contract.get("manifest") != MANIFEST
        or contract.get("checksums") != CHECKSUMS
        or product.get("protocols") != {}
        or product.get("required_components") != ["application", "proxy"]
    ):
        raise ConsoleReleaseError("console release contract is invalid")
    return manifest


def verify_console(
    archive_path: Path,
    *,
    expected_version: str | None = None,
    expected_revision: str | None = None,
) -> dict[str, object]:
    """Verify the archive sidecar, safe layout, checksums, and release identity."""

    try:
        if expected_version is not None:
            expected_version = validate_version(expected_version)
        if expected_revision is not None:
            expected_revision = validate_revision(
                "expected OctaCity revision", expected_revision
            )
    except (TypeError, ValueError) as error:
        raise ConsoleReleaseError(f"expected console release identity is invalid: {error}") from error
    archive_path = archive_path.resolve()
    _verify_sidecar(archive_path)
    files = _read_archive(archive_path)
    for relative in files:
        _validate_payload_name(relative)
    required = {
        ENTRYPOINT,
        MANIFEST,
        CHECKSUMS,
        CONTRACT,
        LICENSE,
        DEPLOYMENT_GUIDE,
        *SUPPORT_FILES,
    }
    if not required.issubset(files):
        raise ConsoleReleaseError(
            f"console archive omits required files: {sorted(required - files)}"
        )
    empty_support = sorted(
        name
        for name in {LICENSE, DEPLOYMENT_GUIDE, *SUPPORT_FILES}
        if not files[name]
    )
    if empty_support:
        raise ConsoleReleaseError(
            f"console release support file must not be empty: {empty_support[0]}"
        )
    _verify_checksums(files)
    manifest = _verify_manifest(files)
    if expected_version is not None and manifest["version"] != expected_version:
        raise ConsoleReleaseError("console release version does not match the expected release")
    revision = manifest["build_inputs"]["octacity_revision"]
    if expected_revision is not None and revision != expected_revision:
        raise ConsoleReleaseError("console release revision does not match the expected source")
    return manifest


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    package = commands.add_parser("package", help="package a built ui/dist directory")
    package.add_argument(
        "--repository", type=Path, default=Path(__file__).resolve().parents[1]
    )
    package.add_argument("--distribution", type=Path, required=True)
    package.add_argument("--version", required=True)
    package.add_argument("--octacity-revision", required=True)
    package.add_argument("--output", type=Path, required=True)
    verify = commands.add_parser("verify", help="verify a console release archive")
    verify.add_argument("--archive", type=Path, required=True)
    verify.add_argument("--expected-version")
    verify.add_argument("--expected-octacity-revision")
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    if args.command == "package":
        output = package_console(
            args.repository,
            args.distribution,
            args.version,
            args.octacity_revision,
            args.output,
        )
        print(output)
    else:
        manifest = verify_console(
            args.archive,
            expected_version=args.expected_version,
            expected_revision=args.expected_octacity_revision,
        )
        print(json.dumps(manifest, sort_keys=True))


if __name__ == "__main__":
    main()
