"""Packaged triage policy and recovery with finite native model/MCP peers.

These cases qualify recorded execution and authority, not a paid model's
semantic judgment or a live provider's availability.
"""
import os
import unittest

from tools.tests import test_factory_triage_pipeline as pipeline


@unittest.skipUnless(os.environ.get("OCTACITY_TEST_PACKAGED_TRIAGE") == "1",
                     "requires verified packaged products and native MCP inputs")
class PackagedTriageAdversarialTests(unittest.TestCase):
    def test_malformed_tool_reply_cannot_authorize_classification_or_reporting(self):
        pipeline.PackagedTriagePipelineTests.run_pipeline(self, defect=False, case="malformed_tool")

    def test_injected_tool_and_model_text_cannot_change_handoff_or_authority(self):
        pipeline.PackagedTriagePipelineTests.run_pipeline(self, defect=False, case="injected_text")

    def test_incomplete_lookup_cannot_establish_an_absence_or_dispatch_the_analyst(self):
        pipeline.PackagedTriagePipelineTests.run_pipeline(self, defect=False, case="incomplete_lookup")

    def test_missing_reproduction_dependency_is_unresolved_not_not_reproduced(self):
        pipeline.PackagedTriagePipelineTests.run_pipeline(self, defect=True, case="reproduction_failed")

    def test_removed_mcp_connection_cannot_use_source_or_reporter_authority(self):
        pipeline.PackagedTriagePipelineTests.run_pipeline(self, defect=False, case="mcp_connection_revoked")

    def test_writer_revocation_retains_both_decisions_and_the_independent_reader(self):
        pipeline.PackagedTriagePipelineTests.run_pipeline(self, defect=True, case="writer_revoked")

    def test_revoked_mcp_token_cannot_fall_back_to_another_credential(self):
        pipeline.PackagedTriagePipelineTests.run_pipeline(self, defect=False, case="mcp_token_revoked")

    def test_malformed_model_result_cannot_accept_facts_or_dispatch_the_analyst(self):
        pipeline.PackagedTriagePipelineTests.run_pipeline(self, defect=False, case="malformed_model")

    def test_observed_duplicate_skips_the_analyst_without_unconfigured_closure(self):
        pipeline.PackagedTriagePipelineTests.run_pipeline(self, defect=False, case="duplicate")

    def test_restart_after_provider_mutation_keeps_one_effect_and_truthful_receipt(self):
        pipeline.PackagedTriagePipelineTests.run_pipeline(self, defect=True, case="mutation_restart")

    def test_restart_after_stage_acceptance_never_redispatches_the_project(self):
        pipeline.PackagedTriagePipelineTests.run_pipeline(self, defect=False, case="stage_restart")

    def test_restart_after_admission_keeps_one_original_run_and_each_ai_invocation(self):
        pipeline.PackagedTriagePipelineTests.run_pipeline(self, defect=False, case="admission_restart")

    def test_out_of_scope_skips_the_analyst_and_never_reports(self):
        pipeline.PackagedTriagePipelineTests.run_pipeline(self, defect=False, case="out_of_scope")


if __name__ == "__main__":
    unittest.main()
