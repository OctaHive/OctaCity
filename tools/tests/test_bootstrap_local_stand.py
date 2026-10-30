"""Contract tests for the replay-safe local-stand REST bootstrap."""

from __future__ import annotations

import base64
import json
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import os
from pathlib import Path
import stat
import shutil
import socket
import ssl
import sys
import tempfile
import threading
import time
import unittest
from unittest.mock import patch


REPOSITORY = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPOSITORY / "tools"))

import bootstrap_local_stand as bootstrap  # noqa: E402
import local_stand_lifecycle as lifecycle  # noqa: E402


ENROLLMENT = (
    "enrollment.88888888-8888-4888-8888-888888888888."
    "BwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwc"
)
REGISTRATION = (
    "registration.99999999-9999-4999-8999-999999999999."
    "BwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwc"
)
POOL_ID = "66666666-6666-4666-8666-666666666666"


def pool_resource(**overrides):
    return {
        "id": POOL_ID,
        "name": bootstrap.LOCAL_POOL_REQUEST["name"],
        "version": 7,
        "definition": json.loads(json.dumps(bootstrap.LOCAL_POOL_REQUEST["definition"])),
        "published_at_unix_ms": 1_790_000_000_000,
        **overrides,
    }


class FixtureTransport:
    def __init__(self, *, lose_pool_response: bool = False, gate: threading.Event | None = None):
        self.calls: list[tuple[str, str, dict[str, str], bytes | None]] = []
        self.lose_pool_response = lose_pool_response
        self.gate = gate

    def request(
        self,
        method: str,
        path: str,
        headers: dict[str, str],
        body: bytes | None,
        timeout_seconds: float,
    ) -> bootstrap.HttpResponse:
        del timeout_seconds
        self.calls.append((method, path, dict(headers), body))
        if self.gate is not None:
            self.gate.wait(timeout=2)
        if path == "/health/ready":
            return response(200, {"status": "ready"})
        if path == "/api/v1/agent-pools":
            if self.lose_pool_response:
                self.lose_pool_response = False
                raise bootstrap.TransportError("connection closed before response")
            request = json.loads(body or b"null")
            return response(
                201,
                {
                    "disposition": "applied",
                    "resource": pool_resource(name=request["name"], definition=request["definition"]),
                },
            )
        if path == "/api/v1/agent-enrollments":
            request = json.loads(body or b"null")
            return response(
                201,
                {
                    "disposition": "applied",
                    "credential": ENROLLMENT,
                    "pool_id": request["pool_id"],
                    "pool_version": request["pool_version"],
                    "expires_at_unix_ms": int(time.time() * 1000) + 900_000,
                },
            )
        raise AssertionError(f"unexpected request: {method} {path}")


class PreparedPoolTransport(FixtureTransport):
    """A pool already exists, but its create receipt belongs to another caller."""

    def __init__(self, *, pages=None, create_status=409):
        super().__init__()
        self.pages = pages if pages is not None else {
            f"{bootstrap.POOL_PATH}?limit=100": {"items": [pool_resource()], "next_cursor": None}
        }
        self.create_status = create_status

    def request(self, method, path, headers, body, timeout_seconds):
        if path == bootstrap.POOL_PATH and method == "POST":
            self.calls.append((method, path, dict(headers), body))
            return response(self.create_status, {"code": "conflict"})
        if path.startswith(bootstrap.POOL_PATH) and method == "GET":
            self.calls.append((method, path, dict(headers), body))
            return response(200, self.pages[path])
        return super().request(method, path, headers, body, timeout_seconds)


def response(status: int, document: dict[str, object]) -> bootstrap.HttpResponse:
    return bootstrap.HttpResponse(status, {}, json.dumps(document).encode("utf-8"))


@unittest.skipUnless(os.name == "posix", "local stand host state requires POSIX permissions")
class LocalStandBootstrapTests(unittest.TestCase):
    @unittest.skipUnless(os.name == "posix" and shutil.which("openssl"), "requires local TLS fixtures")
    def test_management_transport_can_select_a_larger_finite_response_without_widening_bootstrap(self):
        import init_local_stand as initializer
        initializer.initialize(self.root)
        size = {"bytes": bootstrap.MAX_RESPONSE_BYTES + 1}

        class Peer(BaseHTTPRequestHandler):
            def log_message(self, *_):
                pass
            def do_GET(self):
                self.send_response(200)
                self.send_header("Content-Length", str(size["bytes"]))
                self.end_headers()
                self.wfile.write(b"x" * size["bytes"])

        with ThreadingHTTPServer(("127.0.0.1", 0), Peer) as server:
            context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
            context.load_cert_chain(self.root / "pki/gateway.pem", self.root / "pki/gateway-key.pem")
            server.socket = context.wrap_socket(server.socket, server_side=True)
            thread = threading.Thread(target=server.serve_forever, daemon=True)
            thread.start()
            ca = (self.root / "pki/local-ca.pem").read_bytes()
            origin = f"https://octacity.localhost:{server.server_port}"
            resolve = socket.getaddrinfo
            try:
                larger = bootstrap.HttpsTransport(origin, ca, max_response_bytes=bootstrap.MAX_RESPONSE_BYTES + 32)
                with patch.object(socket, "getaddrinfo", side_effect=lambda host, *args, **kwargs:
                        resolve("127.0.0.1" if host == "octacity.localhost" else host, *args, **kwargs)):
                    with self.assertRaises(bootstrap.TransportError):
                        bootstrap.HttpsTransport(origin, ca).request("GET", "/", {}, None, 2)
                    self.assertEqual(len(larger.request("GET", "/", {}, None, 2).body), size["bytes"])
                    size["bytes"] = bootstrap.MAX_RESPONSE_BYTES + 33
                    with self.assertRaises(bootstrap.TransportError):
                        larger.request("GET", "/", {}, None, 2)
            finally:
                server.shutdown()
                thread.join(timeout=2)

    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name) / "stand"
        self.root.mkdir(mode=0o700)

    def run_bootstrap(self, transport: FixtureTransport) -> bootstrap.BootstrapResult:
        return bootstrap.bootstrap(
            self.root,
            transport=transport,
            repository=REPOSITORY,
            home=Path.home(),
            retry=bootstrap.RetryPolicy(attempts=3, initial_delay_seconds=0),
        )

    def test_versioned_post_retries_the_original_precondition_and_mutation_bytes(self):
        calls = []

        class Transport:
            def request(self, method, path, headers, body, timeout_seconds):
                calls.append((method, path, dict(headers), body))
                if len(calls) == 1:
                    raise bootstrap.TransportError("lost response")
                return response(201, {"disposition": "applied"})

        client = bootstrap.BootstrapClient(Transport(),
            bootstrap.RetryPolicy(attempts=2, initial_delay_seconds=0))
        result = client.post("/api/v1/integrations/example/versions", "reviewed-connection",
            {"intent": {"id": "example"}}, expected_version=0)
        self.assertEqual(result, {"disposition": "applied"})
        self.assertEqual(len(calls), 2)
        self.assertEqual(calls[0], calls[1])
        self.assertEqual(calls[0][2]["If-None-Match"], "*")
        self.assertNotIn("If-Match", calls[0][2])
        self.assertEqual(calls[0][3], b'{"intent":{"id":"example"}}')

    def test_clean_start_uses_exact_pool_version_and_installs_private_credential(self):
        transport = FixtureTransport()

        result = self.run_bootstrap(transport)

        self.assertEqual(result.state, "enrollment_installed")
        self.assertEqual(result.pool_id, POOL_ID)
        self.assertEqual(result.pool_version, 7)
        credential = self.root / "agent/credential"
        self.assertEqual(credential.read_text(encoding="ascii").strip(), ENROLLMENT)
        metadata = credential.lstat()
        self.assertTrue(stat.S_ISREG(metadata.st_mode))
        self.assertEqual(stat.S_IMODE(metadata.st_mode), 0o600)
        self.assertEqual(metadata.st_nlink, 1)

        pool = json.loads(transport.calls[1][3] or b"null")
        self.assertEqual(pool, bootstrap.LOCAL_POOL_REQUEST)
        enrollment = json.loads(transport.calls[2][3] or b"null")
        self.assertEqual(
            enrollment,
            {
                "pool_id": POOL_ID,
                "pool_version": 7,
                "expected_platform": {
                    "operating_system": "macos",
                    "architecture": "arm64",
                },
            },
        )
        self.assertTrue(all("Authorization" not in call[2] for call in transport.calls))
        keys = [call[2]["Idempotency-Key"] for call in transport.calls[1:]]
        self.assertEqual(len(set(keys)), 2)

        receipt = json.dumps(result.receipt(), sort_keys=True)
        self.assertNotIn(ENROLLMENT, receipt)

    def test_lost_response_retries_the_exact_pool_mutation(self):
        transport = FixtureTransport(lose_pool_response=True)

        self.run_bootstrap(transport)

        pool_calls = [call for call in transport.calls if call[1] == "/api/v1/agent-pools"]
        self.assertEqual(len(pool_calls), 2)
        self.assertEqual(pool_calls[0][2]["Idempotency-Key"], pool_calls[1][2]["Idempotency-Key"])
        self.assertEqual(pool_calls[0][3], pool_calls[1][3])

    def test_factory_pool_created_before_gateway_start_is_reused_for_enrollment(self):
        transport = PreparedPoolTransport()

        result = self.run_bootstrap(transport)

        self.assertEqual(result.state, "enrollment_installed")
        self.assertEqual((result.pool_id, result.pool_version), (POOL_ID, 7))
        enrollment = json.loads(transport.calls[-1][3])
        self.assertEqual((enrollment["pool_id"], enrollment["pool_version"]), (POOL_ID, 7))
        self.assertEqual((self.root / "agent/credential").read_text().strip(), ENROLLMENT)

    def test_factory_pool_lookup_follows_pages_and_ignores_other_pools(self):
        transport = PreparedPoolTransport(pages={
            f"{bootstrap.POOL_PATH}?limit=100": {
                "items": [pool_resource(name="another-pool")], "next_cursor": "cursor+/="},
            f"{bootstrap.POOL_PATH}?limit=100&after=cursor%2B%2F%3D": {
                "items": [pool_resource()], "next_cursor": None},
        })

        result = self.run_bootstrap(transport)

        self.assertEqual((result.pool_id, result.pool_version), (POOL_ID, 7))
        self.assertEqual(len([call for call in transport.calls if call[0] == "GET"]), 3)

    def test_factory_pool_with_changed_policy_is_never_enrolled_or_overwritten(self):
        resource = pool_resource()
        resource["definition"]["concurrency_limit"] = 2
        transport = PreparedPoolTransport(pages={
            f"{bootstrap.POOL_PATH}?limit=100": {"items": [resource], "next_cursor": None}
        })

        with self.assertRaisesRegex(bootstrap.BootstrapError, "differs from the requested policy"):
            self.run_bootstrap(transport)

        self.assertEqual([call[1] for call in transport.calls if call[0] == "POST"], [bootstrap.POOL_PATH])
        self.assertFalse((self.root / "agent/credential").exists())

    def test_pool_conflict_without_the_expected_pool_fails_closed(self):
        transport = PreparedPoolTransport(pages={
            f"{bootstrap.POOL_PATH}?limit=100": {"items": [pool_resource(name="other")], "next_cursor": None}
        })

        with self.assertRaisesRegex(bootstrap.BootstrapError, "could not be verified"):
            self.run_bootstrap(transport)

        self.assertFalse((self.root / "agent/credential").exists())

    def test_pool_lookup_rejects_malformed_pages_and_repeated_cursors(self):
        for page in (
            {"items": {}, "next_cursor": None},
            {"items": [None], "next_cursor": None},
            {"items": [], "next_cursor": 42},
            {"items": [], "next_cursor": ""},
            {"items": [], "next_cursor": "repeated"},
        ):
            with self.subTest(page=page):
                transport = PreparedPoolTransport(pages={
                    f"{bootstrap.POOL_PATH}?limit=100": page,
                    f"{bootstrap.POOL_PATH}?limit=100&after=repeated": page,
                })
                with self.assertRaises(bootstrap.BootstrapError):
                    self.run_bootstrap(transport)
                self.assertLessEqual(len(transport.calls), 4)
                self.assertFalse((self.root / "agent/credential").exists())

    def test_non_conflict_pool_failures_do_not_reuse_existing_resources(self):
        for status in (400, 403, 500):
            with self.subTest(status=status):
                transport = PreparedPoolTransport(create_status=status)
                with self.assertRaisesRegex(bootstrap.BootstrapError, f"HTTP {status}"):
                    self.run_bootstrap(transport)
                self.assertEqual([call[1] for call in transport.calls if call[0] == "GET"], [bootstrap.READY_PATH])
                self.assertFalse((self.root / "agent/credential").exists())

    def test_rerun_reuses_stable_mutation_identities_and_enrollment(self):
        first = FixtureTransport()
        second = FixtureTransport()

        self.run_bootstrap(first)
        result = self.run_bootstrap(second)

        self.assertEqual(result.state, "enrollment_installed")
        self.assertEqual(
            [call[2]["Idempotency-Key"] for call in first.calls[1:]],
            [call[2]["Idempotency-Key"] for call in second.calls[1:]],
        )
        self.assertEqual(
            (self.root / "agent/credential").read_text(encoding="ascii").strip(),
            ENROLLMENT,
        )

    def test_restart_preserves_registration_without_network_mutation(self):
        agent = self.root / "agent"
        agent.mkdir(mode=0o700)
        credential = agent / "credential"
        credential.write_text(f"{REGISTRATION}\n", encoding="ascii")
        credential.chmod(0o600)
        modified = credential.stat().st_mtime_ns
        transport = FixtureTransport()

        result = self.run_bootstrap(transport)

        self.assertEqual(result.state, "registered")
        self.assertEqual(transport.calls, [])
        self.assertEqual(credential.read_text(encoding="ascii").strip(), REGISTRATION)
        self.assertEqual(credential.stat().st_mtime_ns, modified)

    def test_corrupt_credential_fails_closed_before_network_access(self):
        agent = self.root / "agent"
        agent.mkdir(mode=0o700)
        credential = agent / "credential"
        credential.write_text("not-a-credential\n", encoding="ascii")
        credential.chmod(0o600)
        transport = FixtureTransport()

        with self.assertRaisesRegex(bootstrap.BootstrapError, "credential is invalid"):
            self.run_bootstrap(transport)

        self.assertEqual(transport.calls, [])
        self.assertEqual(credential.read_text(encoding="ascii"), "not-a-credential\n")

    def test_concurrent_invocation_is_rejected_by_the_shared_lifecycle_lock(self):
        entered = threading.Event()
        release = threading.Event()

        class BlockingTransport(FixtureTransport):
            def request(self, *args, **kwargs):  # type: ignore[no-untyped-def]
                entered.set()
                release.wait(timeout=2)
                return super().request(*args, **kwargs)

        transport = BlockingTransport()
        failure: list[BaseException] = []

        def first() -> None:
            try:
                self.run_bootstrap(transport)
            except BaseException as error:  # pragma: no cover - asserted below.
                failure.append(error)

        thread = threading.Thread(target=first)
        thread.start()
        self.assertTrue(entered.wait(timeout=2))
        try:
            with self.assertRaisesRegex(bootstrap.BootstrapError, "lifecycle command is running"):
                self.run_bootstrap(FixtureTransport())
        finally:
            release.set()
            thread.join(timeout=3)

        self.assertFalse(thread.is_alive())
        self.assertEqual(failure, [])

    def test_launcher_can_reuse_its_existing_exclusive_lock(self):
        transport = FixtureTransport()

        with lifecycle.LifecycleState(self.root) as owner:
            result = bootstrap.bootstrap(
                self.root,
                transport=transport,
                lifecycle_state=owner,
                repository=REPOSITORY,
                home=Path.home(),
                retry=bootstrap.RetryPolicy(attempts=1),
            )

        self.assertEqual(result.state, "enrollment_installed")

    def test_unlocked_launcher_state_cannot_bypass_concurrency_control(self):
        owner = lifecycle.LifecycleState(self.root)

        with self.assertRaisesRegex(bootstrap.BootstrapError, "requires the exclusive lock"):
            bootstrap.bootstrap(
                self.root,
                transport=FixtureTransport(),
                lifecycle_state=owner,
                repository=REPOSITORY,
                home=Path.home(),
            )

    def test_existing_enrollment_must_match_the_replayed_server_result(self):
        agent = self.root / "agent"
        agent.mkdir(mode=0o700)
        credential = agent / "credential"
        other = ENROLLMENT.replace("BwcH", "CwsI", 1)
        credential.write_text(f"{other}\n", encoding="ascii")
        credential.chmod(0o600)

        with self.assertRaisesRegex(bootstrap.BootstrapError, "does not match"):
            self.run_bootstrap(FixtureTransport())

        self.assertEqual(credential.read_text(encoding="ascii").strip(), other)

    def test_errors_and_receipts_do_not_expose_credentials_or_response_bodies(self):
        class RejectingTransport(FixtureTransport):
            def request(  # type: ignore[no-untyped-def]
                self, method, path, headers, body, timeout_seconds
            ):
                del method, headers, body, timeout_seconds
                if path == "/health/ready":
                    return response(200, {"status": "ready"})
                return bootstrap.HttpResponse(
                    409,
                    {},
                    json.dumps({"message": ENROLLMENT, "secret": "redacted-fixture"}).encode(),
                )

        with self.assertRaises(bootstrap.BootstrapError) as failure:
            self.run_bootstrap(RejectingTransport())

        diagnostic = str(failure.exception)
        self.assertNotIn(ENROLLMENT, diagnostic)
        self.assertNotIn("redacted-fixture", diagnostic)

    def test_https_transport_rejects_non_local_or_credentialed_origins(self):
        certificate = b"not inspected because origins fail before TLS setup"
        for origin in (
            "http://octacity.localhost:8443",
            "https://example.com:8443",
            "https://user:password@octacity.localhost:8443",
            "https://octacity.localhost:8443/api/v1",
        ):
            with self.subTest(origin=origin):
                with self.assertRaisesRegex(bootstrap.BootstrapError, "management origin"):
                    bootstrap.HttpsTransport(origin, certificate)


if __name__ == "__main__":
    unittest.main()
