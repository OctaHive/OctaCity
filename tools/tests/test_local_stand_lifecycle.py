"""Tests for private local-stand lifecycle ownership primitives."""

from __future__ import annotations

import importlib.util
import json
import os
from pathlib import Path
import stat
import sys
import tempfile
import time
import unittest
from unittest import mock


REPOSITORY = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPOSITORY / "tools"))


def load_module(name: str, path: Path):
    spec = importlib.util.spec_from_file_location(name, path)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


LIFECYCLE = load_module(
    "local_stand_lifecycle", REPOSITORY / "tools/local_stand_lifecycle.py"
)


@unittest.skipUnless(os.name == "posix", "lifecycle ownership requires POSIX locks")
class LocalStandLifecycleTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name) / "stand"
        self.root.mkdir(mode=0o700)
        self.identity = LIFECYCLE.ProcessIdentity(
            pid=4242,
            process_group_id=4242,
            uid=os.getuid(),
            start_identity="fixture-start-1",
        )

    def write_record(self, lifecycle_state, identity=None):
        identity = self.identity if identity is None else identity
        with mock.patch.object(LIFECYCLE, "_process_identity", return_value=identity):
            return lifecycle_state._write_process_record(
                "agent",
                identity.pid,
                self.root.resolve() / "lifecycle/agent.sock",
                "a" * LIFECYCLE.CONTROL_TOKEN_LENGTH,
            )

    def test_exclusive_lock_rejects_concurrent_invocations_and_can_be_reacquired(self):
        first = LIFECYCLE.LifecycleState(self.root)
        second = LIFECYCLE.LifecycleState(self.root)
        with first:
            with self.assertRaises(LIFECYCLE.LockUnavailable):
                second.__enter__()
        with second:
            self.assertEqual(
                second.reconcile_before_start("agent"),
                LIFECYCLE.StartDisposition.STARTABLE,
            )

    def test_atomic_record_makes_duplicate_up_idempotent(self):
        with mock.patch.object(LIFECYCLE, "_process_identity", return_value=self.identity):
            with LIFECYCLE.LifecycleState(self.root) as state:
                self.write_record(state)
                self.assertEqual(
                    state.reconcile_before_start("agent"),
                    LIFECYCLE.StartDisposition.ALREADY_RUNNING,
                )
                with self.assertRaisesRegex(
                    LIFECYCLE.LifecycleError, "already exists"
                ):
                    self.write_record(state)

        path = self.root / "lifecycle/agent.json"
        metadata = path.lstat()
        self.assertTrue(stat.S_ISREG(metadata.st_mode))
        self.assertEqual(stat.S_IMODE(metadata.st_mode), 0o600)
        self.assertEqual(metadata.st_nlink, 1)
        self.assertEqual(
            json.loads(path.read_text(encoding="utf-8"))["start_identity"],
            "fixture-start-1",
        )

    def test_stale_and_reused_pid_records_are_repaired_without_false_running_state(self):
        for observed in (
            None,
            LIFECYCLE.ProcessIdentity(4242, 4242, os.getuid(), "reused-start"),
        ):
            with self.subTest(observed=observed):
                record = self.root / "lifecycle/agent.json"
                with LIFECYCLE.LifecycleState(self.root) as state:
                    self.write_record(state)
                    with mock.patch.object(
                        LIFECYCLE, "_process_identity", return_value=observed
                    ):
                        self.assertEqual(
                            state.reconcile_before_start("agent"),
                            LIFECYCLE.StartDisposition.STALE_REPAIRED,
                        )
                self.assertFalse(record.exists())

    def test_log_paths_are_bounded_private_non_links(self):
        state = LIFECYCLE.LifecycleState(self.root)
        with state.open_log("agent") as output:
            output.write(b"agent started\n")
        path = state.log_path("agent")
        self.assertEqual(path, self.root.resolve() / "logs/agent.log")
        self.assertEqual(path.read_bytes(), b"agent started\n")
        self.assertEqual(stat.S_IMODE(path.lstat().st_mode), 0o600)
        with self.assertRaises(LIFECYCLE.LifecycleError):
            state.log_path("../escape")

    def test_malformed_or_replaced_records_fail_closed(self):
        with LIFECYCLE.LifecycleState(self.root) as state:
            with mock.patch.object(
                LIFECYCLE, "_process_identity", return_value=self.identity
            ):
                self.write_record(state)
        path = self.root / "lifecycle/agent.json"
        path.write_text("{}\n", encoding="utf-8")
        path.chmod(0o600)
        with LIFECYCLE.LifecycleState(self.root) as state:
            with self.assertRaisesRegex(LIFECYCLE.LifecycleError, "invalid shape"):
                state.reconcile_before_start("agent")

    def test_readiness_is_bounded_and_repairs_a_process_that_exits(self):
        probes = iter((False, True))
        with LIFECYCLE.LifecycleState(self.root) as state:
            with mock.patch.object(
                LIFECYCLE, "_process_identity", return_value=self.identity
            ):
                self.write_record(state)
            with mock.patch.object(
                LIFECYCLE, "_process_identity", return_value=self.identity
            ):
                record = state.wait_until_ready(
                    "agent",
                    lambda _remaining: next(probes),
                    timeout_seconds=1,
                    poll_interval_seconds=0.001,
                )
            self.assertEqual(record.identity, self.identity)

        with LIFECYCLE.LifecycleState(self.root) as state:
            with mock.patch.object(
                LIFECYCLE, "_process_identity", return_value=None
            ):
                with self.assertRaisesRegex(LIFECYCLE.LifecycleError, "before readiness"):
                    state.wait_until_ready(
                        "agent",
                        lambda _remaining: False,
                        timeout_seconds=1,
                        poll_interval_seconds=0.001,
                    )
        self.assertFalse((self.root / "lifecycle/agent.json").exists())

    def test_readiness_timeout_does_not_claim_success_or_remove_live_process(self):
        with LIFECYCLE.LifecycleState(self.root) as state:
            with mock.patch.object(
                LIFECYCLE, "_process_identity", return_value=self.identity
            ):
                self.write_record(state)
            with mock.patch.object(
                LIFECYCLE, "_process_identity", return_value=self.identity
            ):
                with self.assertRaisesRegex(LIFECYCLE.LifecycleError, "did not become ready"):
                    state.wait_until_ready(
                        "agent",
                        lambda _remaining: False,
                        timeout_seconds=0,
                        poll_interval_seconds=0.001,
                    )
        self.assertTrue((self.root / "lifecycle/agent.json").exists())

    def test_graceful_shutdown_and_repeated_down_are_idempotent(self):
        with LIFECYCLE.LifecycleState(self.root) as state:
            self.write_record(state)
            with (
                mock.patch.object(
                    LIFECYCLE, "_process_identity", side_effect=[self.identity, None]
                ),
                mock.patch.object(
                    LIFECYCLE.supervisor, "request", return_value="graceful"
                ) as request,
            ):
                self.assertEqual(
                    state.shutdown(
                        "agent",
                        graceful_timeout_seconds=1,
                        forced_timeout_seconds=1,
                    ),
                    LIFECYCLE.ShutdownDisposition.GRACEFUL,
                )
                self.assertEqual(
                    state.shutdown(
                        "agent",
                        graceful_timeout_seconds=1,
                        forced_timeout_seconds=1,
                    ),
                    LIFECYCLE.ShutdownDisposition.ALREADY_STOPPED,
                )
        request.assert_called_once()

    def test_forced_shutdown_is_owned_by_the_stable_supervisor(self):
        with LIFECYCLE.LifecycleState(self.root) as state:
            self.write_record(state)
            with (
                mock.patch.object(
                    LIFECYCLE, "_process_identity", side_effect=[self.identity, None]
                ),
                mock.patch.object(
                    LIFECYCLE.supervisor, "request", return_value="forced"
                ) as request,
            ):
                result = state.shutdown(
                    "agent",
                    graceful_timeout_seconds=0,
                    forced_timeout_seconds=1,
                )
        self.assertEqual(result, LIFECYCLE.ShutdownDisposition.FORCED)
        request.assert_called_once()

    def test_pid_reuse_during_shutdown_never_signals_the_replacement(self):
        replacement = LIFECYCLE.ProcessIdentity(
            4242, 4242, os.getuid(), "replacement-start"
        )
        with LIFECYCLE.LifecycleState(self.root) as state:
            self.write_record(state)
            with (
                mock.patch.object(
                    LIFECYCLE,
                    "_process_identity",
                    side_effect=[self.identity, replacement],
                ),
                mock.patch.object(
                    LIFECYCLE.supervisor,
                    "request",
                    side_effect=LIFECYCLE.supervisor.SupervisorError("closed"),
                ),
                mock.patch.object(LIFECYCLE.os, "killpg") as kill_group,
            ):
                result = state.shutdown(
                    "agent",
                    graceful_timeout_seconds=0,
                    forced_timeout_seconds=1,
                )
        self.assertEqual(result, LIFECYCLE.ShutdownDisposition.STALE_REPAIRED)
        kill_group.assert_not_called()

    def test_stale_record_shutdown_repairs_state_without_any_signal(self):
        with LIFECYCLE.LifecycleState(self.root) as state:
            self.write_record(state)
            with (
                mock.patch.object(LIFECYCLE, "_process_identity", return_value=None),
                mock.patch.object(LIFECYCLE.supervisor, "request") as request,
            ):
                result = state.shutdown(
                    "agent",
                    graceful_timeout_seconds=1,
                    forced_timeout_seconds=1,
                )
        self.assertEqual(result, LIFECYCLE.ShutdownDisposition.STALE_REPAIRED)
        request.assert_not_called()

    def test_real_process_identity_and_graceful_shutdown(self):
        with LIFECYCLE.LifecycleState(self.root) as state:
            record = state.launch(
                "agent",
                [sys.executable, "-c", "import time; time.sleep(30)"],
            )
            self.assertEqual(record.identity.pid, record.identity.process_group_id)
            with self.assertRaisesRegex(
                LIFECYCLE.supervisor.SupervisorError,
                "authenticated",
            ):
                LIFECYCLE.supervisor.request(
                    record.control_socket,
                    "b" * LIFECYCLE.CONTROL_TOKEN_LENGTH,
                    "status",
                )
            self.assertEqual(
                LIFECYCLE._process_identity(record.identity.pid),
                record.identity,
            )
            result = state.shutdown(
                "agent",
                graceful_timeout_seconds=2,
                forced_timeout_seconds=2,
                poll_interval_seconds=0.01,
            )
        self.assertEqual(result, LIFECYCLE.ShutdownDisposition.GRACEFUL)
        self.assertFalse((self.root / "lifecycle/agent.json").exists())

    def test_real_forced_shutdown_removes_a_stubborn_descendant(self):
        marker = self.root / "descendant.pid"
        program = (
            "import pathlib, signal, subprocess, sys, time; "
            "signal.signal(signal.SIGTERM, signal.SIG_IGN); "
            "child=subprocess.Popen([sys.executable, '-c', "
            "'import signal,time; signal.signal(signal.SIGTERM, signal.SIG_IGN); time.sleep(30)']); "
            f"pathlib.Path({str(marker)!r}).write_text(str(child.pid)); "
            "time.sleep(30)"
        )
        with LIFECYCLE.LifecycleState(self.root) as state:
            state.launch("agent", [sys.executable, "-c", program])
            deadline = time.monotonic() + 2
            while not marker.exists() and time.monotonic() < deadline:
                time.sleep(0.01)
            self.assertTrue(marker.exists())
            descendant_pid = int(marker.read_text())
            result = state.shutdown(
                "agent",
                graceful_timeout_seconds=0.1,
                forced_timeout_seconds=2,
                poll_interval_seconds=0.01,
            )
        self.assertEqual(result, LIFECYCLE.ShutdownDisposition.FORCED)
        self.assertIsNone(LIFECYCLE._process_identity(descendant_pid))


if __name__ == "__main__":
    unittest.main()
