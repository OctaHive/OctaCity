#!/usr/bin/env python3
"""Create and validate private host state for the hybrid local stand."""

from __future__ import annotations

import argparse
import base64
import json
import os
from pathlib import Path
import re
import secrets
import shutil
import subprocess
import tempfile
from typing import Any

import local_stand_state as state


REPOSITORY = Path(__file__).resolve().parents[1]
DEFAULT_ROOT = state.DEFAULT_ROOT
DIRECTORY_MODE = state.DIRECTORY_MODE
PRIVATE_FILE_MODE = state.PRIVATE_FILE_MODE
PUBLIC_FILE_MODE = state.PUBLIC_FILE_MODE
MAX_CREDENTIAL_BYTES = 4096
GATEWAY_HOSTNAMES = (
    "octacity.localhost",
    "agent.localhost",
    "cache.localhost",
    "objects.localhost",
)
CREDENTIAL_FILES = {
    "database_password": "postgres-password",
    "object_access_key": "object-access-key",
    "object_secret_key": "object-secret-key",
    "signing_key": "signing-key",
    "management_bootstrap_key": "management-bootstrap-key",
    "agent_enrollment_key": "agent-enrollment-key",
    "cache_credential_key": "cache-credential-key",
}
PKI_FILES = {
    "ca_private_key": "local-ca-key.pem",
    "ca_certificate": "local-ca.pem",
    "gateway_private_key": "gateway-key.pem",
    "gateway_certificate": "gateway.pem",
    "signing_public_key": "server-signing-public-key",
}
PRIVATE_PKI_FILES = frozenset({"ca_private_key", "gateway_private_key"})
PUBLIC_PKI_FILES = frozenset(set(PKI_FILES) - PRIVATE_PKI_FILES)
BASE64_CREDENTIALS = frozenset(
    {
        "signing_key",
        "management_bootstrap_key",
        "agent_enrollment_key",
        "cache_credential_key",
    }
)
ED25519_PRIVATE_DER_PREFIX = bytes.fromhex("302e020100300506032b657004220420")
ED25519_PUBLIC_DER_PREFIX = bytes.fromhex("302a300506032b6570032100")


InitializationError = state.StateError
current_uid = state.current_uid
private_umask = state.private_umask
resolve_safe_root = state.resolve_safe_root
validate_private_directory = state.validate_private_directory
ensure_private_directory = state.ensure_private_directory


def _credential_value(name: str) -> bytes:
    if name == "object_access_key":
        value = "OCTA" + secrets.token_hex(12).upper()
    elif name in BASE64_CREDENTIALS:
        value = base64.b64encode(secrets.token_bytes(32)).decode("ascii")
    else:
        value = secrets.token_urlsafe(32)
    return f"{value}\n".encode("ascii")


def read_credential(name: str, path: Path) -> bytes:
    """Read and validate one role-specific private credential from one handle."""

    raw = state.read_regular_file(
        path,
        f"{name} credential",
        max_bytes=MAX_CREDENTIAL_BYTES,
        modes=frozenset({PRIVATE_FILE_MODE}),
    )
    if raw.count(b"\n") > 1 or (b"\n" in raw and not raw.endswith(b"\n")):
        raise InitializationError(f"{name} credential has an invalid format")
    value = raw.strip()
    try:
        text = value.decode("ascii")
    except UnicodeDecodeError as error:
        raise InitializationError(f"{name} credential must be ASCII") from error
    if name in BASE64_CREDENTIALS:
        try:
            decoded = base64.b64decode(text, validate=True)
        except ValueError as error:
            raise InitializationError(f"{name} credential is not valid base64") from error
        if len(decoded) != 32:
            raise InitializationError(f"{name} credential must encode exactly 32 bytes")
    elif name == "object_access_key":
        if re.fullmatch(r"OCTA[0-9A-F]{24}", text) is None:
            raise InitializationError("object_access_key credential has an invalid format")
    elif re.fullmatch(r"[A-Za-z0-9_-]{43}", text) is None:
        raise InitializationError(f"{name} credential has an invalid format")
    return value


def _ensure_credentials(directory: Path) -> dict[str, Path]:
    paths = {name: directory / filename for name, filename in CREDENTIAL_FILES.items()}
    for name, path in paths.items():
        if not path.exists() and not path.is_symlink():
            state.publish_exclusive_private_file(path, _credential_value(name))
    return _validate_credentials(directory)


def _validate_credentials(directory: Path) -> dict[str, Path]:
    paths = {name: directory / filename for name, filename in CREDENTIAL_FILES.items()}
    values = [read_credential(name, path) for name, path in paths.items()]
    if len(set(values)) != len(values):
        raise InitializationError("local stand credentials must not be reused between roles")
    return paths


def _run_openssl(openssl: str, arguments: list[str], purpose: str) -> bytes:
    try:
        result = subprocess.run(
            [openssl, *arguments],
            check=False,
            capture_output=True,
        )
    except OSError as error:
        raise InitializationError(f"cannot run OpenSSL for {purpose}: {error}") from error
    if result.returncode != 0:
        diagnostic = result.stderr.decode("utf-8", errors="replace").strip()[-1024:]
        raise InitializationError(f"OpenSSL failed to {purpose}: {diagnostic}")
    return result.stdout


def _signing_public_key(signing_key: Path, openssl: str) -> bytes:
    encoded = read_credential("signing_key", signing_key)
    seed = base64.b64decode(encoded, validate=True)
    with tempfile.TemporaryDirectory(prefix="octacity-signing-public-") as value:
        root = Path(value)
        root.chmod(DIRECTORY_MODE)
        private_der = root / "private.der"
        private_der.write_bytes(ED25519_PRIVATE_DER_PREFIX + seed)
        private_der.chmod(PRIVATE_FILE_MODE)
        public_der = _run_openssl(
            openssl,
            ["pkey", "-inform", "DER", "-in", str(private_der), "-pubout", "-outform", "DER"],
            "derive the server signing public key",
        )
    if not public_der.startswith(ED25519_PUBLIC_DER_PREFIX) or len(public_der) != len(ED25519_PUBLIC_DER_PREFIX) + 32:
        raise InitializationError("OpenSSL returned an invalid Ed25519 public key")
    return base64.b64encode(public_der[len(ED25519_PUBLIC_DER_PREFIX) :]) + b"\n"


def _pki_configuration() -> str:
    names = "\n".join(f"DNS.{index} = {name}" for index, name in enumerate(GATEWAY_HOSTNAMES, 1))
    return f"""
[req]
prompt = no
distinguished_name = subject

[subject]
CN = octacity.localhost

[ca_extensions]
basicConstraints = critical, CA:TRUE
keyUsage = critical, keyCertSign, cRLSign
subjectKeyIdentifier = hash
authorityKeyIdentifier = keyid:always

[gateway_extensions]
basicConstraints = critical, CA:FALSE
keyUsage = critical, digitalSignature, keyEncipherment
extendedKeyUsage = serverAuth
subjectAltName = @gateway_names
subjectKeyIdentifier = hash
authorityKeyIdentifier = keyid

[gateway_names]
{names}
""".lstrip()


def _generate_pki(directory: Path, secrets_directory: Path, openssl: str) -> None:
    paths = {name: directory / filename for name, filename in PKI_FILES.items()}
    configuration = directory / ".openssl.cnf"
    request = directory / ".gateway.csr"
    configuration.write_text(_pki_configuration(), encoding="utf-8")
    configuration.chmod(PRIVATE_FILE_MODE)
    _run_openssl(
        openssl,
        [
            "req", "-x509", "-newkey", "rsa:3072", "-nodes", "-sha256", "-days", "3650",
            "-keyout", str(paths["ca_private_key"]), "-out", str(paths["ca_certificate"]),
            "-subj", "/CN=OctaCity Local Stand CA", "-config", str(configuration),
            "-extensions", "ca_extensions",
        ],
        "create the local certificate authority",
    )
    _run_openssl(
        openssl,
        [
            "req", "-new", "-newkey", "rsa:2048", "-nodes", "-sha256",
            "-keyout", str(paths["gateway_private_key"]), "-out", str(request),
            "-subj", "/CN=octacity.localhost", "-config", str(configuration),
        ],
        "create the gateway certificate request",
    )
    _run_openssl(
        openssl,
        [
            "x509", "-req", "-sha256", "-days", "825", "-in", str(request),
            "-CA", str(paths["ca_certificate"]), "-CAkey", str(paths["ca_private_key"]),
            "-set_serial", f"0x{secrets.token_hex(16)}", "-out", str(paths["gateway_certificate"]),
            "-extfile", str(configuration), "-extensions", "gateway_extensions",
        ],
        "sign the gateway certificate",
    )
    paths["signing_public_key"].write_bytes(
        _signing_public_key(secrets_directory / CREDENTIAL_FILES["signing_key"], openssl)
    )
    for name in PRIVATE_PKI_FILES:
        paths[name].chmod(PRIVATE_FILE_MODE)
    for name in PUBLIC_PKI_FILES:
        paths[name].chmod(PUBLIC_FILE_MODE)
    request.unlink()
    configuration.unlink()


def _certificate_text(certificate: Path, openssl: str) -> str:
    return _run_openssl(
        openssl,
        ["x509", "-in", str(certificate), "-noout", "-text"],
        "inspect a local stand certificate",
    ).decode("utf-8", errors="replace")


def certificate_dns_names(certificate: Path, openssl: str) -> set[str]:
    """Return the DNS subject-alternative names in one certificate."""

    return set(re.findall(r"DNS:([A-Za-z0-9.-]+)", _certificate_text(certificate, openssl)))


def _certificate_public_key(certificate: Path, openssl: str) -> bytes:
    return _run_openssl(
        openssl,
        ["x509", "-in", str(certificate), "-pubkey", "-noout"],
        "read a certificate public key",
    )


def _private_public_key(private_key: Path, openssl: str) -> bytes:
    return _run_openssl(
        openssl,
        ["pkey", "-in", str(private_key), "-pubout"],
        "read a private key public component",
    )


def validate_pki(directory: Path, secrets_directory: Path, openssl: str) -> None:
    """Validate the complete local CA, gateway certificate, and signing public key."""

    validate_private_directory(directory)
    paths = {name: directory / filename for name, filename in PKI_FILES.items()}
    if any(not path.exists() or path.is_symlink() for path in paths.values()):
        raise InitializationError("local stand PKI is incomplete or contains symbolic files")
    validated_contents: dict[str, bytes] = {}
    for name in PRIVATE_PKI_FILES:
        validated_contents[name] = state.read_regular_file(
            paths[name], name, max_bytes=MAX_CREDENTIAL_BYTES, modes=frozenset({PRIVATE_FILE_MODE})
        )
    for name in PUBLIC_PKI_FILES:
        validated_contents[name] = state.read_regular_file(
            paths[name], name, max_bytes=MAX_CREDENTIAL_BYTES, modes=frozenset({PUBLIC_FILE_MODE})
        )

    _run_openssl(
        openssl,
        ["verify", "-CAfile", str(paths["ca_certificate"]), str(paths["gateway_certificate"])],
        "verify the gateway certificate chain",
    )
    for name in ("ca_certificate", "gateway_certificate"):
        _run_openssl(
            openssl,
            ["x509", "-in", str(paths[name]), "-checkend", "0", "-noout"],
            f"check {name} validity",
        )
    if "CA:TRUE" not in _certificate_text(paths["ca_certificate"], openssl):
        raise InitializationError("local CA certificate lacks CA authority")
    if "CA:FALSE" not in _certificate_text(paths["gateway_certificate"], openssl):
        raise InitializationError("gateway certificate is not constrained to an end entity")
    if certificate_dns_names(paths["gateway_certificate"], openssl) != set(GATEWAY_HOSTNAMES):
        raise InitializationError("gateway certificate SANs differ from the local stand hosts")
    ca_public = _certificate_public_key(paths["ca_certificate"], openssl)
    gateway_public = _certificate_public_key(paths["gateway_certificate"], openssl)
    if ca_public != _private_public_key(paths["ca_private_key"], openssl):
        raise InitializationError("local CA certificate and private key do not match")
    if gateway_public != _private_public_key(paths["gateway_private_key"], openssl):
        raise InitializationError("gateway certificate and private key do not match")
    if ca_public == gateway_public:
        raise InitializationError("local CA and gateway must use distinct private keys")
    expected_signing_public = _signing_public_key(
        secrets_directory / CREDENTIAL_FILES["signing_key"], openssl
    )
    if validated_contents["signing_public_key"] != expected_signing_public:
        raise InitializationError("server signing public key differs from its private key")


def _ensure_pki(root: Path, secrets_directory: Path, openssl: str) -> Path:
    destination = root / "pki"
    if destination.exists() or destination.is_symlink():
        validate_pki(destination, secrets_directory, openssl)
        return destination
    staging = Path(tempfile.mkdtemp(prefix=".pki.staging-", dir=root))
    staging.chmod(DIRECTORY_MODE)
    try:
        _generate_pki(staging, secrets_directory, openssl)
        validate_pki(staging, secrets_directory, openssl)
        try:
            staging.rename(destination)
        except OSError:
            if not destination.exists() and not destination.is_symlink():
                raise
            validate_pki(destination, secrets_directory, openssl)
    finally:
        if staging.exists():
            shutil.rmtree(staging)
    validate_pki(destination, secrets_directory, openssl)
    return destination


def initialize(
    root: Path,
    *,
    repository: Path = REPOSITORY,
    home: Path = Path.home(),
    openssl: str = "openssl",
) -> dict[str, Any]:
    """Create missing host state, validate all existing state, and return public paths."""

    root = resolve_safe_root(root, repository, home)
    with private_umask():
        ensure_private_directory(root)
        secrets_directory = root / "secrets"
        ensure_private_directory(secrets_directory)
        credentials = _ensure_credentials(secrets_directory)
        pki_directory = _ensure_pki(root, secrets_directory, openssl)
    return {
        "schema_version": 1,
        "root": str(root),
        "credentials": {name: str(path) for name, path in credentials.items()},
        "pki": {name: str(pki_directory / filename) for name, filename in PKI_FILES.items()},
    }


def validate_initialized_state(
    root: Path,
    *,
    repository: Path = REPOSITORY,
    home: Path = Path.home(),
    openssl: str = "openssl",
) -> dict[str, Any]:
    """Validate existing host state without creating or repairing any path."""

    root = resolve_safe_root(root, repository, home)
    validate_private_directory(root)
    secrets_directory = root / "secrets"
    validate_private_directory(secrets_directory)
    credentials = _validate_credentials(secrets_directory)
    pki_directory = root / "pki"
    validate_pki(pki_directory, secrets_directory, openssl)
    return {
        "schema_version": 1,
        "root": str(root),
        "credentials": {name: str(path) for name, path in credentials.items()},
        "pki": {name: str(pki_directory / filename) for name, filename in PKI_FILES.items()},
    }


def parse_arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--root",
        type=Path,
        default=Path(os.environ.get("OCTACITY_LOCAL_STAND_ROOT", DEFAULT_ROOT)),
        help="private host state root (default: %(default)s)",
    )
    parser.add_argument("--openssl", default="openssl", help="OpenSSL executable")
    return parser.parse_args()


def main() -> int:
    arguments = parse_arguments()
    try:
        receipt = initialize(arguments.root, openssl=arguments.openssl)
    except (InitializationError, OSError, ValueError) as error:
        raise SystemExit(f"local stand initialization failed: {error}") from error
    print(json.dumps(receipt, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
