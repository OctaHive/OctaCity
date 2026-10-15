"""Unix pilot process contracts must not execute on unsupported hosts."""
import importlib.util
import os
from pathlib import Path
import sys
from types import SimpleNamespace
import unittest
from unittest.mock import patch


class FactoryFixturePlatformTests(unittest.TestCase):
    def test_windows_discovery_skips_unix_process_contracts_before_starting_tools(self):
        for name, selected in (
            ("test_factory_codex_author_fixture", None),
            ("test_factory_codex_fixture", None),
            ("test_factory_pilot_requirements_author", None),
            ("test_factory_pilot_cli", "FactoryPilotCliTests.test_tool_read_uses_the_injected_hook_then_the_real_mcp_contract"),
            ("test_factory_pilot_cli", "FactoryPilotCliTests.test_denied_hook_never_dispatches_the_tool_or_writes_requested_files"),
            ("test_factory_pilot_cli", "FactoryPilotCliTests.test_tool_client_allows_cold_reader_startup_within_the_hook_deadline"),
            ("test_local_stand_launcher", "LocalStandLauncherTests.test_all_compose_operations_reuse_only_an_installed_private_factory_bundle"),
            ("test_stage_local_stand_native", "NativeStandStagingTests.test_public_install_cli_stages_a_separate_pinned_guest_reader"),
        ):
            with self.subTest(module=name, selected=selected):
                spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(name + ".py"))
                module = importlib.util.module_from_spec(spec)
                # Load host-specific dependencies before simulating Windows discovery.
                spec.loader.exec_module(module)
                windows_os = SimpleNamespace(**{**vars(os), "name": "nt"})
                with patch.dict(sys.modules, {"os": windows_os}):
                    spec.loader.exec_module(module)
                suite = (unittest.defaultTestLoader.loadTestsFromName(selected, module) if selected
                         else unittest.defaultTestLoader.loadTestsFromModule(module))
                result = unittest.TestResult()
                with patch("subprocess.run", side_effect=AssertionError("unsupported Unix process was started")):
                    suite.run(result)
                self.assertGreater(result.testsRun, 0)
                self.assertEqual(result.errors, [])
                self.assertEqual(result.failures, [])
                self.assertEqual(len(result.skipped), result.testsRun)


if __name__ == "__main__":
    unittest.main()
