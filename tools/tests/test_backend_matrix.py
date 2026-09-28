import importlib.util
import io
import unittest
from pathlib import Path


REPOSITORY = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("backend_matrix", REPOSITORY / "tools/backend_matrix.py")
assert SPEC and SPEC.loader
BACKEND_MATRIX = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(BACKEND_MATRIX)


class BackendMatrixTests(unittest.TestCase):
    def test_workflow_consumes_every_planner_output(self):
        workflow = (REPOSITORY / ".github/workflows/backend-contracts.yml").read_text(
            encoding="utf-8"
        )
        for suite in BACKEND_MATRIX.MANUAL_SUITES:
            self.assertIn(f"          - {suite}\n", workflow)
        for job, output in BACKEND_MATRIX.RUN_OUTPUTS.items():
            self.assertIn(f"      {output}: ${{{{ steps.plan.outputs.{output} }}}}", workflow)
            self.assertIn(f"needs.matrix-plan.outputs.{output} == 'true'", workflow)
            self.assertIn(f"      - {job}\n", workflow)
        self.assertIn(
            "run_performance_gate: ${{ steps.plan.outputs.run_performance_gate }}",
            workflow,
        )

    def test_ordinary_push_requires_only_portable_host_evidence(self):
        self.assertEqual(
            BACKEND_MATRIX.expected_jobs("push", "", ""), frozenset({"released-host"})
        )
        self.assertFalse(BACKEND_MATRIX.plan_outputs("push", "", "")["run_performance_gate"])

    def test_nightly_and_release_require_every_qualified_backend(self):
        self.assertEqual(
            BACKEND_MATRIX.expected_jobs("schedule", "", ""), BACKEND_MATRIX.RELEASE_JOBS
        )
        self.assertEqual(
            BACKEND_MATRIX.expected_jobs("workflow_call", "", "release"),
            BACKEND_MATRIX.RELEASE_JOBS,
        )
        self.assertNotIn("windows-microsandbox-preview", BACKEND_MATRIX.RELEASE_JOBS)

        outputs = BACKEND_MATRIX.plan_outputs("schedule", "", "")
        self.assertTrue(outputs["requires_self_hosted"])
        self.assertTrue(outputs["run_performance_gate"])
        self.assertTrue(outputs["run_macos_apple_vf"])
        self.assertFalse(outputs["run_windows_microsandbox_preview"])
        self.assertTrue(
            BACKEND_MATRIX.plan_outputs("workflow_call", "", "release")["run_performance_gate"]
        )

    def test_manual_preview_does_not_expand_into_the_release_matrix(self):
        self.assertEqual(
            BACKEND_MATRIX.expected_jobs(
                "workflow_dispatch", "windows-microsandbox-preview", ""
            ),
            frozenset({"windows-microsandbox-preview"}),
        )

    def test_manual_lifecycle_suite_requires_every_qualified_provider(self):
        self.assertEqual(
            BACKEND_MATRIX.expected_jobs("workflow_dispatch", "lifecycle", ""),
            BACKEND_MATRIX.LIFECYCLE_JOBS | {"lifecycle-matrix"},
        )

    def test_manual_performance_suite_uses_the_hosted_real_backend(self):
        self.assertEqual(
            BACKEND_MATRIX.expected_jobs("workflow_dispatch", "performance", ""),
            frozenset({"linux-native"}),
        )

        workflow = (REPOSITORY / ".github/workflows/backend-contracts.yml").read_text(
            encoding="utf-8"
        )
        self.assertIn("OCTACITY_PERFORMANCE_GATE", workflow)
        self.assertIn("python3 tools/performance_matrix.py evaluate", workflow)
        self.assertIn("performance-measurements.json", workflow)
        self.assertTrue(
            BACKEND_MATRIX.plan_outputs("workflow_dispatch", "performance", "")["run_performance_gate"]
        )
        self.assertTrue(BACKEND_MATRIX.plan_outputs("workflow_dispatch", "all", "")["run_performance_gate"])

    def test_runner_inventory_requires_online_runner_with_every_label(self):
        runners = (
            {
                "status": "online",
                "labels": [
                    {"name": "self-hosted"},
                    {"name": "macOS"},
                    {"name": "ARM64"},
                    {"name": "octacity-microsandbox"},
                ],
            },
            {
                "status": "offline",
                "labels": [
                    {"name": "self-hosted"},
                    {"name": "macOS"},
                    {"name": "ARM64"},
                    {"name": "octacity-apple-vf"},
                ],
            },
        )
        availability = BACKEND_MATRIX.runner_availability(runners)
        self.assertTrue(availability["macos-microsandbox"])
        self.assertFalse(availability["macos-apple-vf"])

    def test_only_repository_visible_unrestricted_release_group_is_eligible(self):
        eligible = {
            "id": 42,
            "name": "octacity-release",
            "allows_public_repositories": True,
            "restricted_to_workflows": False,
        }
        self.assertEqual(
            BACKEND_MATRIX.select_runner_group([eligible], repository_private=False),
            eligible,
        )
        self.assertIsNone(
            BACKEND_MATRIX.select_runner_group(
                [{**eligible, "restricted_to_workflows": True}],
                repository_private=False,
            )
        )
        self.assertIsNone(
            BACKEND_MATRIX.select_runner_group(
                [{**eligible, "allows_public_repositories": False}],
                repository_private=False,
            )
        )

    def test_inventory_reads_only_the_repository_visible_release_group(self):
        calls = []

        def github_items(url, item_key, _token):
            calls.append((url, item_key))
            if item_key == "runner_groups":
                return [
                    {
                        "id": 42,
                        "name": "octacity-release",
                        "allows_public_repositories": True,
                        "restricted_to_workflows": False,
                    }
                ]
            return [{"id": 7, "status": "online", "labels": []}]

        original_github_items = BACKEND_MATRIX.github_items
        BACKEND_MATRIX.github_items = github_items
        try:
            inventory = BACKEND_MATRIX.load_runner_inventory(
                api_url="https://api.github.test",
                organization="OctaHive",
                repository="OctaCity",
                repository_private=False,
                token="token",
            )
        finally:
            BACKEND_MATRIX.github_items = original_github_items

        self.assertTrue(inventory.group_available)
        self.assertIn("visible_to_repository=OctaCity", calls[0][0])
        self.assertIn("/runner-groups/42/runners", calls[1][0])

    def test_github_pagination_follows_only_the_next_link(self):
        header = (
            '<https://api.github.test/items?page=3>; rel="next", '
            '<https://api.github.test/items?page=9>; rel="last"'
        )
        self.assertEqual(
            BACKEND_MATRIX.next_link(header),
            "https://api.github.test/items?page=3",
        )

    def test_github_collection_reads_every_page(self):
        class Response(io.BytesIO):
            def __init__(self, payload, link=None):
                super().__init__(payload.encode())
                self.headers = {"Link": link} if link else {}

        responses = iter(
            (
                Response(
                    '{"runners":[{"id":1}]}',
                    '<https://api.github.test/runners?page=2>; rel="next"',
                ),
                Response('{"runners":[{"id":2}]}'),
            )
        )
        original_urlopen = BACKEND_MATRIX.urlopen
        BACKEND_MATRIX.urlopen = lambda _request, timeout: next(responses)
        try:
            self.assertEqual(
                BACKEND_MATRIX.github_items(
                    "https://api.github.test/runners?page=1", "runners", "token"
                ),
                [{"id": 1}, {"id": 2}],
            )
        finally:
            BACKEND_MATRIX.urlopen = original_urlopen

    def test_unavailable_runner_is_missing_evidence_instead_of_success(self):
        missing = BACKEND_MATRIX.missing_evidence(
            frozenset({"macos-microsandbox", "linux-microsandbox"}),
            {
                "macos-microsandbox": {"result": "skipped"},
                "linux-microsandbox": {"result": "success"},
            },
            {"macos-microsandbox": False},
        )
        self.assertEqual(missing, ["macos-microsandbox: runner unavailable"])

    def test_unavailable_inventory_is_not_misreported_as_an_absent_runner(self):
        missing = BACKEND_MATRIX.missing_evidence(
            frozenset({"macos-apple-vf"}),
            {"macos-apple-vf": {"result": "skipped"}},
            {"macos-apple-vf": False},
            inventory_available=False,
        )
        self.assertEqual(missing, ["macos-apple-vf: runner inventory unavailable"])

    def test_ineligible_runner_group_is_reported_separately(self):
        missing = BACKEND_MATRIX.missing_evidence(
            frozenset({"macos-apple-vf"}),
            {"macos-apple-vf": {"result": "skipped"}},
            {"macos-apple-vf": False},
            group_available=False,
        )
        self.assertEqual(
            missing,
            ["macos-apple-vf: runner group 'octacity-release' unavailable"],
        )


if __name__ == "__main__":
    unittest.main()
