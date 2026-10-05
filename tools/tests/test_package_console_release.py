"""Contracts for deterministic, static-only console release packaging."""

from __future__ import annotations

import importlib.util
import json
from pathlib import Path
import sys
import tarfile
import tempfile
import unittest


REPOSITORY = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPOSITORY / "tools"))
SPEC = importlib.util.spec_from_file_location(
    "package_console_release", REPOSITORY / "tools/package_console_release.py"
)
assert SPEC and SPEC.loader
PACKAGE = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = PACKAGE
SPEC.loader.exec_module(PACKAGE)


class ConsoleReleaseTests(unittest.TestCase):
    def distribution(self, root: Path) -> Path:
        distribution = root / "dist"
        assets = distribution / "assets"
        assets.mkdir(parents=True)
        (distribution / "index.html").write_text(
            """<!doctype html>
<html lang="en">
  <head>
    <script type="module" src="/assets/index-AbCd1234.js"></script>
    <link rel="stylesheet" href="/assets/index-EfGh5678.css">
  </head>
  <body><div id="root"></div></body>
</html>
""",
            encoding="utf-8",
        )
        (assets / "index-AbCd1234.js").write_text(
            "const endpoint = '/api/v1'; export { endpoint };\n", encoding="utf-8"
        )
        (assets / "index-EfGh5678.css").write_text(
            ":root { color: #10251d; }\n", encoding="utf-8"
        )
        return distribution

    def package(self, root: Path, name: str = "octacity-console.tar.gz") -> Path:
        return PACKAGE.package_console(
            REPOSITORY,
            self.distribution(root),
            "1.2.3",
            "a" * 40,
            root / name,
        )

    def extract(self, archive: Path, destination: Path) -> None:
        destination.mkdir()
        with tarfile.open(archive, "r:gz") as source:
            source.extractall(destination, filter="data")

    def archive(self, root: Path, output: Path) -> None:
        PACKAGE.archive_tar(root, output)
        PACKAGE.write_text(
            output.with_name(output.name + ".sha256"),
            f"{PACKAGE.sha256(output)}  {output.name}\n",
        )

    def test_archive_is_reproducible_rooted_at_index_and_self_verifying(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            first_root = root / "first"
            second_root = root / "second"
            first_root.mkdir()
            second_root.mkdir()
            first = self.package(first_root, "first.tar.gz")
            second = self.package(second_root, "second.tar.gz")

            self.assertEqual(PACKAGE.sha256(first), PACKAGE.sha256(second))
            first_manifest = PACKAGE.verify_console(first)
            second_manifest = PACKAGE.verify_console(second)
            self.assertEqual(first_manifest, second_manifest)
            self.assertEqual(first_manifest["product"], "octacity-console")
            self.assertEqual(first_manifest["platform"], "any")
            self.assertEqual(
                first_manifest["components"]["application"]["path"], "index.html"
            )

            with tarfile.open(first, "r:gz") as archive:
                files = {member.name for member in archive if member.isfile()}
            self.assertIn("index.html", files)
            self.assertIn("release-manifest.json", files)
            self.assertIn("SHA256SUMS", files)
            self.assertIn("LICENSE", files)
            self.assertIn("share/operator-console.md", files)
            for name in PACKAGE.NGINX_FILES:
                self.assertIn(f"share/nginx/{name}", files)
            self.assertNotIn("package.json", files)
            self.assertFalse(any(name.endswith(".map") for name in files))
            self.assertFalse(any("node_modules" in name for name in files))

            self.extract(first, root / "first-extracted")
            for name in PACKAGE.NGINX_FILES:
                self.assertEqual(
                    (root / "first-extracted" / "share/nginx" / name).read_bytes(),
                    (REPOSITORY / "deployment/console/nginx" / name).read_bytes(),
                )

    def test_verifier_rejects_tampering_even_with_a_rewritten_archive_sidecar(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source_root = root / "source"
            source_root.mkdir()
            original = self.package(source_root)
            extracted = root / "extracted"
            self.extract(original, extracted)
            (extracted / "assets/index-AbCd1234.js").write_text(
                "tampered\n", encoding="utf-8"
            )
            tampered = root / "tampered.tar.gz"
            self.archive(extracted, tampered)

            with self.assertRaisesRegex(PACKAGE.ConsoleReleaseError, "checksum differs"):
                PACKAGE.verify_console(tampered)

    def test_verifier_rejects_forbidden_files_even_when_checksums_match(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source_root = root / "source"
            source_root.mkdir()
            original = self.package(source_root)
            extracted = root / "extracted"
            self.extract(original, extracted)
            forbidden = extracted / "node_modules/runtime.js"
            forbidden.parent.mkdir()
            forbidden.write_text("runtime\n", encoding="utf-8")
            PACKAGE.write_text(
                extracted / "SHA256SUMS",
                PACKAGE.checksum_lines(extracted, {"SHA256SUMS"}),
            )
            repacked = root / "forbidden.tar.gz"
            self.archive(extracted, repacked)

            with self.assertRaisesRegex(
                PACKAGE.ConsoleReleaseError, "development tooling"
            ):
                PACKAGE.verify_console(repacked)

    def test_packager_rejects_unhashed_source_bearing_and_origin_bound_output(self):
        cases = (
            ("assets/application.js", b"export {};\n", "content-hashed"),
            ("assets/source-AbCd1234.js.map", b"{}\n", "source-bearing"),
            (
                "assets/index-AbCd1234.js",
                b"const api = 'https://console.example.test/api/v1';\n",
                "environment-specific origin",
            ),
            (
                "assets/index-AbCd1234.js",
                b"console.log('x');\n//# sourceMappingURL=index.js.map\n",
                "source-map reference",
            ),
            (
                "assets/index-EfGh5678.css",
                b"body {}\n/*# sourceMappingURL=data:application/json;base64,e30= */\n",
                "source-map reference",
            ),
            (
                "assets/index-AbCd1234.js",
                b"\x7fELF" + b"\0" * 32,
                "executable runtime",
            ),
        )
        for relative, contents, message in cases:
            with self.subTest(relative=relative, message=message):
                with tempfile.TemporaryDirectory() as temporary:
                    root = Path(temporary)
                    distribution = self.distribution(root)
                    original_relative = (
                        "assets/index-EfGh5678.css"
                        if relative.endswith(".css")
                        else "assets/index-AbCd1234.js"
                    )
                    original = distribution / original_relative
                    if relative != original_relative:
                        original.unlink()
                    target = distribution / relative
                    target.parent.mkdir(parents=True, exist_ok=True)
                    target.write_bytes(contents)
                    with self.assertRaisesRegex(PACKAGE.ConsoleReleaseError, message):
                        PACKAGE.package_console(
                            REPOSITORY,
                            distribution,
                            "1.2.3",
                            "a" * 40,
                            root / "console.tar.gz",
                        )

    def test_verifier_bounds_the_external_checksum_sidecar(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive = self.package(root)
            archive.with_name(archive.name + ".sha256").write_bytes(
                b"0" * (PACKAGE.MAX_CHECKSUM_BYTES + 1)
            )

            with self.assertRaisesRegex(PACKAGE.ConsoleReleaseError, "size limit"):
                PACKAGE.verify_console(archive)

    def test_verifier_rejects_an_empty_proxy_fixture(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source_root = root / "source"
            source_root.mkdir()
            original = self.package(source_root)
            extracted = root / "extracted"
            self.extract(original, extracted)
            (extracted / "share/nginx/routes.conf").write_bytes(b"")
            PACKAGE.write_text(
                extracted / "SHA256SUMS",
                PACKAGE.checksum_lines(extracted, {"SHA256SUMS"}),
            )
            repacked = root / "empty-proxy.tar.gz"
            self.archive(extracted, repacked)

            with self.assertRaisesRegex(PACKAGE.ConsoleReleaseError, "must not be empty"):
                PACKAGE.verify_console(repacked)

    def test_verifier_can_bind_the_archive_to_the_expected_release_identity(self):
        with tempfile.TemporaryDirectory() as temporary:
            archive = self.package(Path(temporary))

            PACKAGE.verify_console(
                archive,
                expected_version="1.2.3",
                expected_revision="a" * 40,
            )
            with self.assertRaisesRegex(PACKAGE.ConsoleReleaseError, "version"):
                PACKAGE.verify_console(archive, expected_version="1.2.4")
            with self.assertRaisesRegex(PACKAGE.ConsoleReleaseError, "revision"):
                PACKAGE.verify_console(archive, expected_revision="b" * 40)

    def test_manifest_inventory_matches_every_content_hashed_asset(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive = self.package(root)
            manifest = PACKAGE.verify_console(archive)
            self.assertEqual(
                [asset["path"] for asset in manifest["assets"]],
                ["assets/index-AbCd1234.js", "assets/index-EfGh5678.css"],
            )
            self.assertTrue(
                all(len(asset["sha256"]) == 64 for asset in manifest["assets"])
            )

    def test_local_stand_keeps_building_and_copying_the_same_ui_dist(self):
        dockerfile = (
            REPOSITORY / "deployment/local-stand/server.Dockerfile"
        ).read_text(encoding="utf-8")
        self.assertIn("corepack pnpm build", dockerfile)
        self.assertIn(
            "COPY --from=ui-builder /workspace/ui/dist/ /srv/octacity-ui/",
            dockerfile,
        )
        gateway = dockerfile.split("FROM ${NGINX_IMAGE} AS gateway", 1)[1]
        self.assertNotIn("package_console_release.py", gateway)
        self.assertNotIn("node_modules", gateway)

    def test_release_contract_declares_the_static_application_component(self):
        contract = json.loads(
            (REPOSITORY / "packaging/release-contract.json").read_text(
                encoding="utf-8"
            )
        )
        console = contract["products"]["octacity-console"]
        self.assertEqual(console["protocols"], {})
        self.assertEqual(console["required_components"], ["application"])

    def test_openapi_generation_uses_the_workspace_lockfile(self):
        generator = (REPOSITORY / "ui/scripts/generate-api.mjs").read_text(
            encoding="utf-8"
        )
        self.assertIn("'--locked'", generator)


class ConsoleWorkflowTests(unittest.TestCase):
    """Keep the checked-in CI and release pipelines fail-closed."""

    def setUp(self):
        self.ci = (REPOSITORY / ".github/workflows/ci.yml").read_text(
            encoding="utf-8"
        )
        self.release = (REPOSITORY / ".github/workflows/release.yml").read_text(
            encoding="utf-8"
        )

    def console_job(self, workflow: str, start: str, end: str) -> str:
        try:
            return workflow.split(start, 1)[1].split(end, 1)[0]
        except IndexError as error:
            raise AssertionError(f"console workflow job is missing: {start.strip()}") from error

    def assert_ci_policy(self, workflow: str) -> None:
        job = self.console_job(workflow, "  console:\n", "  quality:\n")
        required = (
            "node-version-file: octacity/ui/.node-version",
            "package_json_file: octacity/ui/package.json",
            "pnpm install --frozen-lockfile --ignore-scripts",
            "run: pnpm api:generate",
            "run: pnpm format",
            "run: pnpm lint",
            "run: pnpm test:unit",
            "run: pnpm test:browser",
            "run: pnpm build",
            "tools/package_console_release.py package",
            "tools/package_console_release.py verify",
            "--expected-version \"${version}\"",
            '--expected-octacity-revision "${{ github.sha }}"',
            "name: octacity-console-ci-${{ github.run_id }}",
            "overwrite: true",
        )
        for marker in required:
            self.assertIn(marker, job)
        self.assertNotIn("continue-on-error:", job)
        self.assertNotIn("github.run_attempt", job)
        self.assertNotIn("version: 12.8.1", job)
        self.assertEqual(job.count("pnpm typecheck"), 0)
        self.assertEqual(job.count("pnpm test:accessibility"), 0)
        self.assertLess(job.index("pnpm build"), job.index("package_console_release.py package"))
        self.assertLess(
            job.index("package_console_release.py package"),
            job.index("package_console_release.py verify"),
        )

    def assert_release_policy(self, workflow: str) -> None:
        job = self.console_job(
            workflow, "  console-package:\n", "  attest-and-publish:\n"
        )
        for gate in (
            "portable-release-gate",
            "security-release-gate",
            "backend-release-gate",
        ):
            self.assertIn(f"      - {gate}\n", job)
        for marker in (
            "name: octacity-console-ci-${{ github.run_id }}",
            "tools/package_console_release.py verify",
            "--expected-version \"${version}\"",
            '--expected-octacity-revision "${{ github.sha }}"',
            'cp "${candidate}" "${candidate}.sha256" dist/',
            "name: octacity-release-console",
        ):
            self.assertIn(marker, job)
        publish = workflow.split("  attest-and-publish:\n", 1)[1]
        self.assertLess(
            job.index("package_console_release.py verify"),
            job.index('cp "${candidate}" "${candidate}.sha256" dist/'),
        )
        self.assertIn("      - console-package\n", publish)
        self.assertIn("pattern: octacity-release-*", publish)
        self.assertIn("subject-path: dist/*", publish)
        self.assertNotIn("continue-on-error:", job)
        self.assertNotIn("github.run_attempt", job)

    def test_console_ci_and_release_are_complete_and_fail_closed(self):
        self.assert_ci_policy(self.ci)
        self.assert_release_policy(self.release)

    def test_ci_policy_rejects_stale_lockfile_generation_test_and_archive_gaps(self):
        failures = (
            (
                "pnpm install --frozen-lockfile --ignore-scripts",
                "pnpm install --ignore-scripts",
            ),
            ("run: pnpm api:generate", "run: echo generation-skipped"),
            ("run: pnpm test:unit", "run: echo tests-skipped"),
            (
                "tools/package_console_release.py verify",
                "tools/package_console_release.py --help",
            ),
        )
        for required, replacement in failures:
            with self.subTest(required=required):
                fixture = self.ci.replace(required, replacement, 1)
                with self.assertRaises(AssertionError):
                    self.assert_ci_policy(fixture)

    def test_release_policy_rejects_an_unattested_console_archive(self):
        fixture = self.release.replace("      - console-package\n", "", 1)
        with self.assertRaises(AssertionError):
            self.assert_release_policy(fixture)


if __name__ == "__main__":
    unittest.main()
