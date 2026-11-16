"""Disposable native qualification must clean up even when evidence cannot be retained."""
from contextlib import ExitStack
import errno
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

from tools.tests import test_factory_stand_native as native


class FactoryStandNativeCleanupTests(unittest.TestCase):
    def test_evidence_failures_preserve_the_journey_error_and_release_owned_resources(self):
        temporary_directory = tempfile.TemporaryDirectory
        for failure in (None, "directory", "copy", "docker", "reset", "tag"):
            with self.subTest(failure=failure), tempfile.TemporaryDirectory() as temporary, ExitStack() as mocks:
                root = Path(temporary)
                evidence = root / "evidence"
                if failure == "directory":
                    evidence.write_text("occupied")
                inputs = {"native_installation": temporary, "source_tree": "revision",
                          "server_image": "new-server", "gateway_image": "new-gateway",
                          "github_token_file": "unused", "source_repository": "owner/repository",
                          "source_revision": "revision", "delivery_write_directory": "tools",
                          "delivery_target_branch": "main", "delivery_token_file": "unused",
                          "evidence_directory": str(evidence)}
                original = RuntimeError("native journey failed")
                def install(arguments):
                    stand_root = Path(arguments[arguments.index("--root") + 1])
                    stand_root.mkdir()
                    (stand_root / "factory-bootstrap.log").write_text("private diagnostic")
                    raise original
                def docker(arguments, **kwargs):
                    if arguments[:3] == ["docker", "compose", "logs"] and failure == "docker":
                        raise subprocess.TimeoutExpired(arguments, 30)
                    if arguments == ["docker", "tag", "retained-image", native.stand.launcher.SERVER_IMAGE] and failure == "tag":
                        raise subprocess.CalledProcessError(1, arguments)
                    return subprocess.CompletedProcess(arguments, 0,
                        stdout="retained-image" if kwargs.get("text") else b"", stderr=b"")
                stand = native.stand
                # The real native journey needs short macOS socket paths. This
                # unit fixture owns no sockets and must also run without /tmp.
                mocks.enter_context(patch.object(native.tempfile, "TemporaryDirectory",
                    side_effect=lambda *args, **kwargs: temporary_directory(*args, **{**kwargs, "dir": root})))
                mocks.enter_context(patch.object(native.state, "DEFAULT_ROOT", root / "absent"))
                mocks.enter_context(patch.object(native.subprocess, "check_output", return_value=b""))
                commands = mocks.enter_context(patch.object(native.subprocess, "run", side_effect=docker))
                mocks.enter_context(patch.object(stand.launcher, "load_build_inputs", return_value={}))
                mocks.enter_context(patch.object(stand.launcher.native_stager, "verify_agent_bundle"))
                mocks.enter_context(patch.object(stand.launcher, "NativeInstallation"))
                mocks.enter_context(patch.object(stand.launcher, "_compose_command", return_value=["docker", "compose", "logs"]))
                mocks.enter_context(patch.object(stand.launcher, "_compose_environment", return_value={}))
                mocks.enter_context(patch.object(stand, "main", side_effect=install))
                reset = mocks.enter_context(patch.object(stand.launcher, "run",
                    side_effect=RuntimeError("reset failed") if failure == "reset" else None))
                if failure == "copy":
                    mocks.enter_context(patch.object(native.shutil, "copyfile", side_effect=OSError(errno.ENOSPC, "No space left on device")))
                with self.assertRaises(Exception) as raised:
                    native.FactoryStandNativeTests()._installed_stand(inputs, ui_authoring=True)
                self.assertEqual([call.args[0].command for call in reset.call_args_list], ["reset"])
                for target in (stand.launcher.SERVER_IMAGE, stand.launcher.GATEWAY_IMAGE):
                    commands.assert_any_call(["docker", "tag", "retained-image", target], check=True)
                self.assertIs(raised.exception, original)
                if failure:
                    self.assertTrue(original.__notes__, "retain evidence failure context without masking the journey error")
