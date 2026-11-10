"""Exercise CI profile settings with real Cargo artifacts, not a workspace rebuild."""

import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import unittest


REPOSITORY = Path(__file__).resolve().parents[2]
WORKFLOW = REPOSITORY / ".github/workflows/ci.yml"


def job_environment(name):
    """Read the static job-level environment from the repository's workflow layout."""
    job = WORKFLOW.read_text(encoding="utf-8").split(f"\n  {name}:\n", 1)[1]
    header = job.split("    steps:\n", 1)[0]
    block = re.search(r"^    env:\n((?:      [A-Z_]+: [^\n]+\n)+)", header, re.MULTILINE)
    if block is None:
        return {}
    return {
        key: value.strip().strip("\"'")
        for key, value in re.findall(r"^      ([A-Z_]+): (.+)$", block[1], re.MULTILINE)
    }


def isolated_environment():
    return {
        key: value for key, value in os.environ.items()
        if not key.startswith(("CARGO_PROFILE_", "CARGO_LLVM_COV_"))
        and key not in {
            "CARGO_INCREMENTAL", "CARGO_TARGET_DIR", "CARGO_BUILD_TARGET",
            "RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS", "RUSTC_WRAPPER",
        }
    }


def linked_artifact_bytes(executable):
    """Include debugger sidecars used by Windows and macOS, not just the executable."""
    files = [executable]
    symbols = executable.with_suffix(".pdb")
    if symbols.is_file():
        files.append(symbols)
    bundle = Path(str(executable) + ".dSYM")
    if bundle.is_dir():
        files.extend(path for path in bundle.rglob("*") if path.is_file())
    return sum(path.stat().st_size for path in files)


@unittest.skipUnless(shutil.which("cargo"), "requires the Rust toolchain")
class CiRustProfileTests(unittest.TestCase):
    def fixture(self, root):
        (root / "src").mkdir()
        (root / "Cargo.toml").write_text(
            '[package]\nname="ci-disk-fixture"\nversion="0.0.0"\nedition="2024"\n'
            '[workspace]\n', encoding="utf-8",
        )
        (root / "src/lib.rs").write_text(
            'pub fn total(values: &[u64]) -> u64 { values.iter().sum() }\n'
            '#[test]\nfn computes_total() { assert_eq!(total(&[2, 3, 5]), 10); }\n',
            encoding="utf-8",
        )

    def cargo(self, root, target, environment, *arguments):
        result = subprocess.run(
            [shutil.which("cargo"), *arguments, "--offline", "--manifest-path", str(root / "Cargo.toml"),
             "--target-dir", str(target)],
            env={**isolated_environment(), **environment},
            cwd=root, capture_output=True, text=True, timeout=120,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        return result.stdout

    def test_quality_builds_smaller_test_binaries_without_incremental_cache(self):
        with tempfile.TemporaryDirectory(prefix="octacity-ci-disk-") as directory:
            root = Path(directory)
            self.fixture(root)
            executables = []
            for name, environment in (
                ("baseline", {"CARGO_INCREMENTAL": "1", "CARGO_PROFILE_DEV_DEBUG": "2",
                              "CARGO_PROFILE_TEST_DEBUG": "2"}),
                ("quality", job_environment("quality")),
            ):
                target = root / name
                output = self.cargo(root, target, environment, "test", "--message-format=json")
                self.assertIn("1 passed", output)
                artifacts = [json.loads(line) for line in output.splitlines() if line.startswith("{")]
                executables.append(Path(next(
                    entry["executable"] for entry in artifacts
                    if entry.get("executable") and entry.get("profile", {}).get("test")
                )))
            incremental = root / "quality/debug/incremental"
            self.assertFalse(
                incremental.exists() and any(incremental.iterdir()),
                "CI must not retain incremental compilation sessions alongside every test binary",
            )
            baseline, configured = (linked_artifact_bytes(path) for path in executables)
            self.assertLess(configured, baseline, "CI should not link full debugger information into every test")
            print(f"quality test binary: {configured} bytes; full-debug baseline: {baseline} bytes")

    @unittest.skipUnless(shutil.which("cargo-llvm-cov"), "requires cargo-llvm-cov and llvm-tools-preview")
    def test_coverage_keeps_line_reports_without_full_symbols_or_incremental_cache(self):
        rustlib = subprocess.run(
            ["rustc", "--print", "target-libdir"], capture_output=True, text=True, check=True,
        ).stdout.strip()
        llvm_cov = Path(rustlib).parent / "bin" / ("llvm-cov.exe" if os.name == "nt" else "llvm-cov")
        if not llvm_cov.is_file():
            self.skipTest("requires llvm-tools-preview for the active Rust toolchain")
        with tempfile.TemporaryDirectory(prefix="octacity-ci-coverage-") as directory:
            root = Path(directory)
            self.fixture(root)
            sizes = []
            for name, environment in (
                ("baseline", {"CARGO_INCREMENTAL": "1", "CARGO_PROFILE_DEV_DEBUG": "2",
                              "CARGO_PROFILE_TEST_DEBUG": "2"}),
                ("coverage", job_environment("coverage")),
            ):
                target = root / name
                report = root / f"{name}.lcov"
                result = subprocess.run(
                    [shutil.which("cargo"), "llvm-cov", "--offline", "--manifest-path", str(root / "Cargo.toml"),
                     "--lcov", "--output-path", str(report)],
                    env={**isolated_environment(), **environment, "CARGO_LLVM_COV_TARGET_DIR": str(target)},
                    cwd=root, capture_output=True, text=True, timeout=120,
                )
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                self.assertIn("1 passed", result.stdout)
                source = next(block for block in report.read_text(encoding="utf-8").split("end_of_record")
                              if re.search(r"^SF:.*src[/\\]lib.rs$", block, re.MULTILINE))
                counts = re.findall(r"^DA:\d+,(\d+)", source, re.MULTILINE)
                self.assertTrue(counts and any(int(count) > 0 for count in counts), source)
                executable = next(path for path in (target / "debug/deps").glob("ci_disk_fixture-*")
                                  if path.is_file() and path.suffix in ("", ".exe"))
                sizes.append(linked_artifact_bytes(executable))
            incremental = root / "coverage/debug/incremental"
            self.assertFalse(incremental.exists() and any(incremental.iterdir()),
                             "coverage must not accumulate incremental sessions")
            self.assertLess(sizes[1], sizes[0], "line coverage must not require full debugger symbols")
            print(f"coverage test binary: {sizes[1]} bytes; full-debug baseline: {sizes[0]} bytes")


if __name__ == "__main__":
    unittest.main()
