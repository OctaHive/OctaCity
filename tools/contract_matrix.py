"""Shared execution and evidence handling for release contract matrices."""

from __future__ import annotations

from datetime import datetime, timezone
import json
from pathlib import Path
import subprocess
import sys
import time
from typing import Callable, Protocol, Sequence


class Contract(Protocol):
    """Minimum shape accepted by the release-matrix executor."""

    name: str
    command: tuple[str, ...]

    def _asdict(self) -> dict[str, object]: ...


def execute(command: Sequence[str], repository: Path, log_path: Path) -> int:
    """Stream one contract command to CI and a retained evidence log."""
    with log_path.open("w", encoding="utf-8") as log:
        process = subprocess.Popen(
            command,
            cwd=repository,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            encoding="utf-8",
            errors="replace",
        )
        assert process.stdout is not None
        for line in process.stdout:
            sys.stdout.write(line)
            log.write(line)
        return process.wait()


def run_contracts(
    contracts: Sequence[Contract],
    repository: Path,
    evidence_dir: Path,
    *,
    group: str,
    report_name: str,
    runner: Callable[[Sequence[str], Path, Path], int] = execute,
) -> int:
    """Run every contract, retain all outcomes, and fail after the full matrix."""
    evidence_dir.mkdir(parents=True, exist_ok=True)
    outcomes = []
    failed = False
    for contract in contracts:
        print(f"\n::group::{group}: {contract.name}", flush=True)
        started = time.monotonic()
        status = runner(contract.command, repository, evidence_dir / f"{contract.name}.log")
        duration = time.monotonic() - started
        print("::endgroup::", flush=True)
        outcomes.append(
            {
                **contract._asdict(),
                "command": list(contract.command),
                "duration_seconds": round(duration, 3),
                "status": "passed" if status == 0 else "failed",
                "exit_code": status,
            }
        )
        failed |= status != 0
    report = {
        "schema_version": 1,
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "result": "failed" if failed else "passed",
        "contracts": outcomes,
    }
    (evidence_dir / report_name).write_text(
        json.dumps(report, indent=2) + "\n", encoding="utf-8"
    )
    return 1 if failed else 0
