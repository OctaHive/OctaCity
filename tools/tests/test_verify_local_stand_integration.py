"""Unit contracts for the real local-stand vertical-slice verifier."""

from __future__ import annotations

import hashlib
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock


REPOSITORY = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPOSITORY / "tools"))

import bootstrap_local_stand as bootstrap  # noqa: E402
import configure_local_stand_agent as agent_config  # noqa: E402
import verify_local_stand_integration as integration  # noqa: E402


class FakeClient:
    """Small stateful management/object boundary for verifier unit tests."""

    def __init__(self) -> None:
        self.ui_verified = 0
        self.posts: list[tuple[str, str, dict[str, object]]] = []
        self.artifact = bytes(range(256)) * 4096
        self.pool_id = "66666666-6666-4666-8666-666666666666"
        self.agent_id = "77777777-7777-4777-8777-777777777777"
        self.build_id = "55555555-5555-4555-8555-555555555555"
        self.cancelled_build_id = "99999999-9999-4999-8999-999999999999"
        self.artifact_id = "44444444-4444-4444-8444-444444444444"
        self.attempt_id = "33333333-3333-4333-8333-333333333333"
        self.cancelled_attempt_id = "88888888-8888-4888-8888-888888888888"
        self.job_id = "22222222-2222-4222-8222-222222222222"
        self.cancelled_job_id = "12121212-1212-4212-8212-121212121212"
        self.cancelled_state = "cancelled"

    def verify_ui_and_readiness(self) -> None:
        self.ui_verified += 1

    def get(self, path: str) -> dict[str, object]:
        if path == integration.POOLS_PATH:
            return {
                "items": [
                    {
                        "id": self.pool_id,
                        "name": bootstrap.LOCAL_POOL_REQUEST["name"],
                        "definition": bootstrap.LOCAL_POOL_REQUEST["definition"],
                    }
                ],
                "next_cursor": None,
            }
        if path == integration.AGENTS_PATH:
            return {
                "items": [
                    {
                        "id": self.agent_id,
                        "name": agent_config.LOCAL_AGENT_NAME,
                        "pool_id": self.pool_id,
                        "inventory": {
                            "host_platform": agent_config.LOCAL_HOST_PLATFORM,
                            "labels": agent_config.LOCAL_AGENT_LABELS,
                        },
                        "capacity": {"virtualization_available": True},
                        "status": "online",
                        "current_execution": None,
                    }
                ],
                "next_cursor": None,
            }
        if path == f"/api/v1/builds/{self.build_id}":
            return {
                "id": self.build_id,
                "state": "succeeded",
                "current_attempt": {"id": self.attempt_id},
            }
        if path == f"/api/v1/builds/{self.cancelled_build_id}":
            return {
                "id": self.cancelled_build_id,
                "state": self.cancelled_state,
                "current_attempt": {"id": self.cancelled_attempt_id},
            }
        if path == f"/api/v1/attempts/{self.attempt_id}":
            return {
                "jobs": [
                    {"id": self.job_id, "state": "succeeded", "event_cursor": 7}
                ]
            }
        if path == f"/api/v1/attempts/{self.cancelled_attempt_id}":
            return {
                "jobs": [
                    {
                        "id": self.cancelled_job_id,
                        "state": "running" if self.cancelled_state == "running" else "cancelled",
                        "event_cursor": 3,
                    }
                ]
            }
        if path == (
            f"/api/v1/jobs/{self.cancelled_job_id}/events"
            "?after=0&limit=100&wait_ms=0"
        ):
            return {
                "items": [
                    {
                        "payload": {
                            "source": "runner",
                            "event": {"data": {"type": "output"}},
                        }
                    }
                ]
            }
        if path == f"/api/v1/builds/{self.build_id}/artifacts?limit=10":
            return {
                "items": [
                    {"id": self.artifact_id, "name": integration.EXPECTED_ARTIFACT_NAME},
                    {
                        "id": "11111111-1111-4111-8111-111111111111",
                        "name": integration.EXPECTED_REPORT_NAME,
                    },
                ]
            }
        if path == f"/api/v1/builds/{self.cancelled_build_id}/artifacts?limit=10":
            return {"items": []}
        raise AssertionError(f"unexpected GET {path}")

    def post(
        self, path: str, key: str, document: dict[str, object]
    ) -> dict[str, object]:
        self.posts.append((path, key, document))
        resources = {
            "/api/v1/projects": "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
            "/api/v1/repositories": "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb",
            "/api/v1/pipelines": "cccccccc-cccc-4ccc-8ccc-cccccccccccc",
            "/api/v1/build-configurations": "dddddddd-dddd-4ddd-8ddd-dddddddddddd",
            "/api/v1/trigger-definitions/manual": "eeeeeeee-eeee-4eee-8eee-eeeeeeeeeeee",
        }
        if path in resources:
            return {"resource": {"id": resources[path]}}
        if path == "/api/v1/triggers/manual":
            if key.endswith("cancellation-trigger"):
                self.cancelled_state = "running"
                return {
                    "outcome": "accepted",
                    "build_id": self.cancelled_build_id,
                    "attempt_id": self.cancelled_attempt_id,
                }
            return {
                "outcome": "accepted",
                "build_id": self.build_id,
                "attempt_id": self.attempt_id,
            }
        if path == f"/api/v1/builds/{self.cancelled_build_id}/cancel":
            self.cancelled_state = "cancelled"
            return {"disposition": "applied"}
        if path.startswith(
            "/api/v1/projects/aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa/policy-versions"
        ):
            return {"resource": {"id": "ffffffff-ffff-4fff-8fff-ffffffffffff"}}
        raise AssertionError(f"unexpected POST {path}")

    def post_without_body(self, path: str) -> dict[str, object]:
        if path != f"/api/v1/artifacts/{self.artifact_id}/download":
            raise AssertionError(f"unexpected empty POST {path}")
        return {
            "artifact": {"sha256": hashlib.sha256(self.artifact).hexdigest()},
            "get_url": "https://objects.localhost:8443/octacity-artifacts/artifact?signature=x",
        }

    def download(self, url: str) -> bytes:
        self.download_url = url
        return self.artifact


class LocalStandIntegrationTests(unittest.TestCase):
    def test_terminal_codex_cleanup_requires_an_empty_private_work_root(self):
        with tempfile.TemporaryDirectory() as directory:
            work_root = Path(directory) / "agent/work"
            work_root.mkdir(parents=True, mode=0o700)
            work_root.chmod(0o700)
            client = integration.LocalStandClient.__new__(integration.LocalStandClient)
            client._agent_work_root = work_root

            client.verify_agent_work_root_is_empty()
            (work_root / "orphaned-job").mkdir()

            with self.assertRaisesRegex(
                integration.IntegrationError, "workspace behind"
            ):
                client.verify_agent_work_root_is_empty()

    def test_terminal_codex_cleanup_does_not_treat_windows_acl_as_posix_mode(self):
        with tempfile.TemporaryDirectory() as directory:
            work_root = Path(directory) / "agent/work"
            work_root.mkdir(parents=True)
            work_root.chmod(0o777)
            client = integration.LocalStandClient.__new__(integration.LocalStandClient)
            client._agent_work_root = work_root

            with mock.patch.object(integration.os, "name", "nt"):
                client.verify_agent_work_root_is_empty()

    def test_terminal_codex_cleanup_does_not_follow_a_work_root_symlink(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            target = root / "target"
            target.mkdir(mode=0o700)
            (root / "agent").mkdir()
            work_root = root / "agent/work"
            work_root.symlink_to(target, target_is_directory=True)
            client = integration.LocalStandClient.__new__(integration.LocalStandClient)
            client._agent_work_root = work_root

            with self.assertRaisesRegex(
                integration.IntegrationError, "not a real directory"
            ):
                client.verify_agent_work_root_is_empty()

    def test_vertical_slice_uses_exact_virtualization_policy_and_records_evidence(self):
        client = FakeClient()

        codex_evidence = {"state": "verified"}
        with mock.patch.object(
            integration, "_verify_codex_contract", return_value=codex_evidence
        ) as verify_codex:
            receipt = integration.run_vertical_slice(
                client,  # type: ignore[arg-type]
                source_repository="https://github.com/OctaHive/OctaCity",
                source_revision="a" * 40,
                guest_image="quay.io/fedora/fedora@sha256:" + "b" * 64,
                build_timeout_seconds=1,
            )
        verify_codex.assert_called_once()
        self.assertEqual(receipt["codex"], codex_evidence)

        self.assertEqual(client.ui_verified, 2)
        self.assertEqual(receipt["pool_id"], client.pool_id)
        self.assertEqual(receipt["agent_id"], client.agent_id)
        self.assertEqual(receipt["build_id"], client.build_id)
        self.assertEqual(receipt["cancelled_build_id"], client.cancelled_build_id)
        cancellation = next(
            body
            for path, key, body in client.posts
            if path == "/api/v1/build-configurations"
            and key.endswith("cancellation-configuration")
        )
        self.assertIsNone(cancellation["definition"]["cache"]["namespace"])
        self.assertEqual(cancellation["definition"]["artifacts"]["artifact_count"], 0)
        codex = next(
            body
            for path, key, body in client.posts
            if path == "/api/v1/build-configurations"
            and key.endswith("-codex-configuration")
        )
        self.assertEqual(codex["definition"]["artifacts"]["artifact_count"], 2)
        self.assertEqual(codex["definition"]["artifacts"]["report_count"], 1)
        codex_pipeline = next(
            body
            for path, key, body in client.posts
            if path == "/api/v1/pipelines" and key.endswith("-codex-pipeline")
        )
        self.assertEqual(
            codex_pipeline["dag"]["nodes"][0]["execution"]["commands"],
            ["codex-fixture"],
        )
        cancel_index = next(
            index
            for index, (path, _, _) in enumerate(client.posts)
            if path.endswith("/cancel")
        )
        successful_trigger_index = next(
            index
            for index, (path, key, _) in enumerate(client.posts)
            if path == "/api/v1/triggers/manual"
            and not key.endswith("cancellation-trigger")
        )
        self.assertLess(cancel_index, successful_trigger_index)
        self.assertEqual(receipt["artifact_id"], client.artifact_id)
        configuration = next(
            body
            for path, _, body in client.posts
            if path == "/api/v1/build-configurations"
        )
        definition = configuration["definition"]
        self.assertEqual(definition["runtime"]["class"], "virtualization")
        self.assertEqual(
            definition["runtime"]["host_platform"],
            {"os": "macos", "architecture": "arm64"},
        )
        self.assertEqual(definition["runtime"]["architecture"], "arm64")
        self.assertEqual(
            definition["agent_requirements"]["labels"],
            agent_config.LOCAL_AGENT_LABELS,
        )
        self.assertEqual(
            definition["cache"],
            {
                "namespace": integration.CACHE_NAMESPACE,
                "read": True,
                "write": True,
            },
        )
        policy = next(
            body
            for path, _, body in client.posts
            if path.endswith("/policy-versions")
        )
        self.assertEqual(
            policy["policy"]["execution_targets"]["value"],
            bootstrap.LOCAL_POOL_REQUEST["definition"]["admission_policy"][
                "execution_targets"
            ],
        )
        self.assertEqual(
            policy["policy"]["cache"]["value"],
            {
                "namespaces": [integration.CACHE_NAMESPACE],
                "read": True,
                "write": True,
                "max_bytes": integration.MAX_DOWNLOAD_BYTES,
            },
        )

    def test_observation_proves_postgres_and_minio_state_survived_restart(self):
        client = FakeClient()
        digest = hashlib.sha256(client.artifact).hexdigest()
        receipt = {
            "schema_version": integration.RECEIPT_SCHEMA_VERSION,
            "pool_id": client.pool_id,
            "agent_id": client.agent_id,
            "build_id": client.build_id,
            "cancelled_build_id": client.cancelled_build_id,
            "cancelled_job_id": client.cancelled_job_id,
            "artifact_id": client.artifact_id,
            "artifact_sha256": digest,
            "artifact_bytes": len(client.artifact),
        }

        observed = integration.observe_persisted_slice(  # type: ignore[arg-type]
            client, receipt
        )

        self.assertEqual(observed["state"], "verified")
        self.assertEqual(observed["artifact_sha256"], digest)
        self.assertEqual(client.ui_verified, 1)

    def test_duplicate_agent_is_rejected(self):
        client = FakeClient()
        original = client.get

        def duplicated(path: str) -> dict[str, object]:
            page = original(path)
            if path == integration.AGENTS_PATH:
                items = list(page["items"])
                page["items"] = [*items, dict(items[0])]
            return page

        client.get = duplicated  # type: ignore[method-assign]

        with self.assertRaisesRegex(integration.IntegrationError, "exactly one"):
            integration.verify_registered_topology(client)  # type: ignore[arg-type]

    def test_untrusted_source_repository_is_rejected_before_mutation(self):
        client = FakeClient()

        with self.assertRaisesRegex(integration.IntegrationError, "HTTPS GitHub"):
            integration.run_vertical_slice(
                client,  # type: ignore[arg-type]
                source_repository="https://example.invalid/repository.git",
                source_revision="a" * 40,
                guest_image="quay.io/fedora/fedora@sha256:" + "b" * 64,
                build_timeout_seconds=1,
            )

        self.assertEqual(client.posts, [])


if __name__ == "__main__":
    unittest.main()
