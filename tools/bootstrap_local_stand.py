#!/usr/bin/env python3
"""Converge the local Agent Pool and install its enrollment credential."""

from __future__ import annotations

import argparse
import base64
from dataclasses import dataclass
import hashlib
from http import client as http_client
import json
import os
from pathlib import Path
import ssl
import time
from typing import Callable, Mapping, Protocol
from urllib import parse
import uuid

import init_local_stand as initializer
import local_stand_lifecycle as lifecycle
import local_stand_state as state


DEFAULT_ORIGIN = initializer.gateway_origin("octacity.localhost")
MAX_RESPONSE_BYTES = 128 * 1024
MAX_CREDENTIAL_BYTES = 4096
RETRYABLE_STATUS = frozenset({408, 425, 429, 500, 502, 503, 504})
POOL_PATH = "/api/v1/agent-pools"
ENROLLMENT_PATH = "/api/v1/agent-enrollments"
READY_PATH = "/health/ready"
LOCAL_POOL_REQUEST = {
    "name": "local-macos-microsandbox",
    "definition": {
        "enabled": True,
        "drain_state": "accepting",
        "admission_policy": {
            "mode": "execution_allowlist",
            "platforms": [
                {"operating_system": "macos", "architecture": "arm64"}
            ],
            "execution_targets": [
                {
                    "mode": "virtualization",
                    "host_platform": {"os": "macos", "architecture": "arm64"},
                    "target_platform": {"os": "linux", "architecture": "arm64"},
                    "required_guarantees": [
                        "filesystem_isolation",
                        "process_isolation",
                        "network_isolation",
                        "resource_isolation",
                        "hardware_virtualization",
                    ],
                }
            ],
        },
        "concurrency_limit": 1,
        "fairness_policy": "priority_fifo",
        "static_capacity_limit": 1,
    },
}


class BootstrapError(RuntimeError):
    """Bootstrap cannot safely converge the local stand."""


class TransportError(BootstrapError):
    """One HTTP attempt failed before a trusted response was available."""


@dataclass(frozen=True)
class HttpResponse:
    """Bounded transport response used by the retry layer."""

    status: int
    headers: Mapping[str, str]
    body: bytes


class Transport(Protocol):
    """Single-attempt HTTP boundary used by production and fixture transports."""

    def request(
        self,
        method: str,
        path: str,
        headers: dict[str, str],
        body: bytes | None,
        timeout_seconds: float,
    ) -> HttpResponse: ...


@dataclass(frozen=True)
class RetryPolicy:
    """Bounded retry policy for readiness and replay-safe mutations."""

    attempts: int = 5
    timeout_seconds: float = 5
    initial_delay_seconds: float = 0.2
    maximum_delay_seconds: float = 2

    def __post_init__(self) -> None:
        if isinstance(self.attempts, bool) or not 1 <= self.attempts <= 10:
            raise ValueError("retry attempts must be between 1 and 10")
        if not 0 < self.timeout_seconds <= 30:
            raise ValueError(
                "request timeout must be greater than 0 and no greater than 30 seconds"
            )
        if not 0 <= self.initial_delay_seconds <= self.maximum_delay_seconds <= 5:
            raise ValueError("retry delays must be ordered and no greater than 5 seconds")


@dataclass(frozen=True)
class BootstrapResult:
    """Secret-free bootstrap outcome suitable for operator output."""

    state: str
    credential_file: Path
    pool_id: str | None = None
    pool_version: int | None = None

    def receipt(self) -> dict[str, object]:
        receipt: dict[str, object] = {
            "schema_version": 1,
            "state": self.state,
            "credential_file": str(self.credential_file),
        }
        if self.pool_id is not None:
            receipt["pool_id"] = self.pool_id
        if self.pool_version is not None:
            receipt["pool_version"] = self.pool_version
        return receipt


class HttpsTransport:
    """One-attempt, proxy-free HTTPS transport for the fixed local gateway."""

    def __init__(self, origin: str, ca_certificate: bytes):
        parsed = parse.urlsplit(origin)
        if (
            parsed.scheme != "https"
            or parsed.hostname != "octacity.localhost"
            or parsed.username is not None
            or parsed.password is not None
            or parsed.path not in {"", "/"}
            or parsed.query
            or parsed.fragment
            or parsed.port is None
        ):
            raise BootstrapError(
                "management origin must be https://octacity.localhost:<port>"
            )
        try:
            context = ssl.create_default_context(cadata=ca_certificate.decode("ascii"))
        except (UnicodeDecodeError, ssl.SSLError) as cause:
            raise BootstrapError("local stand CA certificate is invalid") from cause
        self._port = parsed.port
        self._context = context

    def request(
        self,
        method: str,
        path: str,
        headers: dict[str, str],
        body: bytes | None,
        timeout_seconds: float,
    ) -> HttpResponse:
        if not path.startswith("/") or path.startswith("//"):
            raise BootstrapError("management request path is invalid")
        connection = http_client.HTTPSConnection(
            "octacity.localhost",
            self._port,
            timeout=timeout_seconds,
            context=self._context,
        )
        try:
            connection.request(method, path, body=body, headers=headers)
            response = connection.getresponse()
            status = response.status
            response_headers = {
                name.lower(): value for name, value in response.getheaders()
            }
            contents = response.read(MAX_RESPONSE_BYTES + 1)
        except (http_client.HTTPException, TimeoutError, OSError) as cause:
            raise TransportError("management gateway request failed") from cause
        finally:
            connection.close()
        if len(contents) > MAX_RESPONSE_BYTES:
            raise TransportError("management gateway response exceeded its size limit")
        return HttpResponse(status, response_headers, contents)


class BootstrapClient:
    """Strict management client with bounded retries and stable request bytes."""

    def __init__(
        self,
        transport: Transport,
        retry: RetryPolicy,
        *,
        sleep: Callable[[float], None] = time.sleep,
    ):
        self._transport = transport
        self._retry = retry
        self._sleep = sleep

    def ready(self) -> None:
        response = self._request("GET", READY_PATH, {}, None)
        document = _json_object(response.body, "readiness response")
        _exact_keys(document, {"status"}, "readiness response")
        if response.status != 200 or document["status"] != "ready":
            raise BootstrapError("management gateway did not report readiness")

    def get(self, path: str, *, timeout_seconds: float | None = None) -> dict[str, object]:
        """Read one bounded JSON object from the management API."""

        headers = {
            "Accept": "application/json",
            "User-Agent": "octacity-local-stand-bootstrap/1",
        }
        response = self._request(
            "GET", path, headers, None, timeout_seconds=timeout_seconds
        )
        if response.status != 200:
            raise BootstrapError(f"management query returned HTTP {response.status}")
        return _json_object(response.body, "management query response")

    def post(
        self, path: str, idempotency_key: str, document: dict[str, object]
    ) -> dict[str, object]:
        body = json.dumps(document, sort_keys=True, separators=(",", ":")).encode("utf-8")
        headers = {
            "Accept": "application/json",
            "Content-Type": "application/json",
            "Idempotency-Key": idempotency_key,
            "User-Agent": "octacity-local-stand-bootstrap/1",
        }
        response = self._request("POST", path, headers, body)
        if response.status not in {200, 201}:
            raise BootstrapError(f"management mutation returned HTTP {response.status}")
        return _json_object(response.body, "management mutation response")

    def _request(
        self,
        method: str,
        path: str,
        headers: dict[str, str],
        body: bytes | None,
        *,
        timeout_seconds: float | None = None,
    ) -> HttpResponse:
        request_timeout = (
            self._retry.timeout_seconds
            if timeout_seconds is None
            else min(self._retry.timeout_seconds, timeout_seconds)
        )
        if not 0 < request_timeout <= 30:
            raise BootstrapError("management request timeout is invalid")
        last_status: int | None = None
        for attempt in range(self._retry.attempts):
            try:
                response = self._transport.request(
                    method, path, headers, body, request_timeout
                )
            except TransportError:
                response = None
            if response is not None and response.status not in RETRYABLE_STATUS:
                return response
            if response is not None:
                last_status = response.status
            if attempt + 1 < self._retry.attempts:
                self._sleep(self._delay(attempt, response))
        if last_status is None:
            raise BootstrapError("management gateway remained unreachable after bounded retries")
        raise BootstrapError(
            f"management gateway remained unavailable with HTTP {last_status} after bounded retries"
        )

    def _delay(self, attempt: int, response: HttpResponse | None) -> float:
        if response is not None:
            raw = response.headers.get("retry-after")
            if raw is not None:
                try:
                    return max(
                        0, min(float(int(raw)), self._retry.maximum_delay_seconds)
                    )
                except ValueError:
                    pass
        return min(
            self._retry.initial_delay_seconds * (2**attempt),
            self._retry.maximum_delay_seconds,
        )


def management_client(
    root: Path,
    *,
    origin: str = DEFAULT_ORIGIN,
    transport: Transport | None = None,
    retry: RetryPolicy = RetryPolicy(),
) -> BootstrapClient:
    """Build a trusted-network client using the local stand CA."""

    try:
        state.validate_private_directory(root)
        if transport is None:
            ca_certificate = state.read_regular_file(
                root / "pki/local-ca.pem",
                "local stand CA certificate",
                max_bytes=64 * 1024,
                modes=frozenset({state.PUBLIC_FILE_MODE}),
            )
            transport = HttpsTransport(origin, ca_certificate)
    except (state.StateError, UnicodeDecodeError) as cause:
        raise BootstrapError(str(cause)) from cause
    return BootstrapClient(transport, retry)


def _json_object(contents: bytes, description: str) -> dict[str, object]:
    try:
        return state.load_json_object(contents, description)
    except state.StateError as cause:
        raise BootstrapError(str(cause)) from cause


def _exact_keys(document: dict[str, object], expected: set[str], description: str) -> None:
    if set(document) != expected:
        raise BootstrapError(f"{description} has an unexpected shape")


def _positive_integer(value: object, description: str) -> int:
    if not isinstance(value, int) or isinstance(value, bool) or value <= 0:
        raise BootstrapError(f"{description} must be a positive integer")
    return value


def _canonical_uuid(value: object, description: str) -> str:
    if not isinstance(value, str):
        raise BootstrapError(f"{description} is invalid")
    try:
        parsed = uuid.UUID(value)
    except ValueError as cause:
        raise BootstrapError(f"{description} is invalid") from cause
    if str(parsed) != value:
        raise BootstrapError(f"{description} is invalid")
    return value


def _credential_token(value: bytes | str, description: str) -> tuple[str, str]:
    if isinstance(value, bytes):
        try:
            value = value.decode("ascii")
        except UnicodeDecodeError as cause:
            raise BootstrapError(f"{description} is invalid") from cause
    token = value.removesuffix("\n")
    if not token or any(character.isspace() for character in token):
        raise BootstrapError(f"{description} is invalid")
    parts = token.split(".")
    if len(parts) != 3 or parts[0] not in {"enrollment", "registration"}:
        raise BootstrapError(f"{description} is invalid")
    identity, encoded_secret = parts[1:]
    if not identity or len(identity.encode("utf-8")) > 256 or any(ord(c) < 32 for c in identity):
        raise BootstrapError(f"{description} is invalid")
    try:
        secret = base64.b64decode(encoded_secret + "=", altchars=b"-_", validate=True)
    except ValueError as cause:
        raise BootstrapError(f"{description} is invalid") from cause
    if (
        len(secret) != 32
        or base64.urlsafe_b64encode(secret).decode("ascii").rstrip("=")
        != encoded_secret
    ):
        raise BootstrapError(f"{description} is invalid")
    return parts[0], token


def _existing_agent_credential(path: Path) -> tuple[str, str] | None:
    if not path.exists() and not path.is_symlink():
        return None
    try:
        contents = state.read_regular_file(
            path,
            "Agent credential",
            max_bytes=MAX_CREDENTIAL_BYTES,
            modes=frozenset({state.PRIVATE_FILE_MODE}),
        )
    except state.StateError as cause:
        raise BootstrapError(str(cause)) from cause
    return _credential_token(contents, "Agent credential")


def _stable_key(purpose: bytes) -> str:
    """Return a non-secret identity stable across ordinary stand restarts."""

    digest = hashlib.sha256(b"octacity.local-stand.bootstrap.v1\0" + purpose)
    return f"local-stand-{purpose.decode('ascii')}-{digest.hexdigest()}"


def _pool(document: dict[str, object]) -> tuple[str, int]:
    _exact_keys(document, {"disposition", "resource"}, "Agent Pool response")
    if document["disposition"] not in {"applied", "replayed"}:
        raise BootstrapError("Agent Pool response has an invalid disposition")
    resource = document["resource"]
    if not isinstance(resource, dict):
        raise BootstrapError("Agent Pool response has an invalid resource")
    _exact_keys(
        resource,
        {"id", "name", "version", "definition", "published_at_unix_ms"},
        "Agent Pool resource",
    )
    if (
        resource["name"] != LOCAL_POOL_REQUEST["name"]
        or resource["definition"] != LOCAL_POOL_REQUEST["definition"]
    ):
        raise BootstrapError("Agent Pool response differs from the requested policy")
    _positive_integer(resource["published_at_unix_ms"], "Agent Pool publication time")
    return (
        _canonical_uuid(resource["id"], "Agent Pool identity"),
        _positive_integer(resource["version"], "Agent Pool version"),
    )


def _enrollment(document: dict[str, object], pool_id: str, pool_version: int) -> str:
    _exact_keys(
        document,
        {"disposition", "credential", "pool_id", "pool_version", "expires_at_unix_ms"},
        "Agent enrollment response",
    )
    if document["disposition"] not in {"applied", "replayed"}:
        raise BootstrapError("Agent enrollment response has an invalid disposition")
    if document["pool_id"] != pool_id or document["pool_version"] != pool_version:
        raise BootstrapError("Agent enrollment response is bound to a different Pool version")
    expires = _positive_integer(document["expires_at_unix_ms"], "Agent enrollment expiry")
    if expires <= int(time.time() * 1000):
        raise BootstrapError("Agent enrollment response is already expired")
    kind, credential = _credential_token(document["credential"], "Agent enrollment credential")
    if kind != "enrollment":
        raise BootstrapError("Agent enrollment response returned the wrong credential kind")
    return credential


def bootstrap(
    root: Path,
    *,
    origin: str = DEFAULT_ORIGIN,
    transport: Transport | None = None,
    lifecycle_state: lifecycle.LifecycleState | None = None,
    repository: Path = initializer.REPOSITORY,
    home: Path = Path.home(),
    retry: RetryPolicy = RetryPolicy(),
) -> BootstrapResult:
    """Converge state under a supplied or locally acquired launcher lock."""

    try:
        root = state.resolve_safe_root(root, repository, home)
        state.validate_private_directory(root)
    except state.StateError as cause:
        raise BootstrapError(str(cause)) from cause

    if lifecycle_state is None:
        try:
            with lifecycle.LifecycleState(root) as owner:
                return _converge(root, origin, transport, owner, retry)
        except lifecycle.LifecycleError as cause:
            raise BootstrapError(str(cause)) from cause
    try:
        lifecycle_state.require_exclusive_lock()
    except lifecycle.LifecycleError as cause:
        raise BootstrapError(str(cause)) from cause
    if lifecycle_state.root != root:
        raise BootstrapError("launcher lock belongs to a different local stand root")
    return _converge(
        root,
        origin,
        transport,
        lifecycle_state,
        retry,
    )


def _converge(
    root: Path,
    origin: str,
    transport: Transport | None,
    owner: lifecycle.LifecycleState,
    retry: RetryPolicy,
) -> BootstrapResult:
    owner.require_exclusive_lock()
    agent_root = root / "agent"
    with state.private_umask():
        state.ensure_private_directory(agent_root)
    credential_path = agent_root / "credential"
    existing = _existing_agent_credential(credential_path)
    if existing is not None and existing[0] == "registration":
        return BootstrapResult("registered", credential_path)

    if transport is None:
        try:
            ca_certificate = state.read_regular_file(
                root / "pki/local-ca.pem",
                "local stand CA certificate",
                max_bytes=64 * 1024,
                modes=frozenset({state.PUBLIC_FILE_MODE}),
            )
        except state.StateError as cause:
            raise BootstrapError(str(cause)) from cause
        transport = HttpsTransport(origin, ca_certificate)
    client = BootstrapClient(transport, retry)
    client.ready()
    pool_id, pool_version = _pool(
        client.post(POOL_PATH, _stable_key(b"pool"), LOCAL_POOL_REQUEST)
    )
    enrollment_request = {
        "pool_id": pool_id,
        "pool_version": pool_version,
        "expected_platform": {
            "operating_system": "macos",
            "architecture": "arm64",
        },
    }
    credential = _enrollment(
        client.post(
            ENROLLMENT_PATH,
            _stable_key(b"enrollment"),
            enrollment_request,
        ),
        pool_id,
        pool_version,
    )
    if existing is not None:
        if existing[1] != credential:
            raise BootstrapError(
                "existing Agent enrollment credential does not match the replayed server result"
            )
    elif not state.publish_exclusive_private_file(
        credential_path, f"{credential}\n".encode("ascii")
    ):
        installed = _existing_agent_credential(credential_path)
        if installed is None or installed[1] != credential:
            raise BootstrapError("Agent credential changed during installation")
    _existing_agent_credential(credential_path)
    return BootstrapResult("enrollment_installed", credential_path, pool_id, pool_version)


def parse_arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    port = os.environ.get(
        "OCTACITY_LOCAL_STAND_HTTPS_PORT", str(initializer.GATEWAY_HTTPS_PORT)
    )
    default_origin = os.environ.get(
        "OCTACITY_LOCAL_STAND_ORIGIN", f"https://octacity.localhost:{port}"
    )
    parser.add_argument(
        "--root",
        type=Path,
        default=Path(os.environ.get("OCTACITY_LOCAL_STAND_ROOT", state.DEFAULT_ROOT)),
        help="private host state root (default: %(default)s)",
    )
    parser.add_argument(
        "--origin",
        default=default_origin,
        help="local management gateway origin (default: %(default)s)",
    )
    parser.add_argument("--attempts", type=int, default=5)
    parser.add_argument("--timeout-seconds", type=float, default=5)
    return parser.parse_args()


def main() -> int:
    arguments = parse_arguments()
    try:
        result = bootstrap(
            arguments.root,
            origin=arguments.origin,
            retry=RetryPolicy(
                attempts=arguments.attempts,
                timeout_seconds=arguments.timeout_seconds,
            ),
        )
    except (BootstrapError, OSError, ValueError) as cause:
        raise SystemExit(f"local stand bootstrap failed: {cause}") from cause
    print(json.dumps(result.receipt(), sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
