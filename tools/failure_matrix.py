#!/usr/bin/env python3
"""Run the deterministic failure-injection contracts required for release."""

from __future__ import annotations

import argparse
from datetime import datetime, timezone
from enum import StrEnum
import json
from pathlib import Path
import subprocess
import sys
import time
from typing import Callable, NamedTuple, Sequence


class Boundary(StrEnum):
    """External or process boundary at which the release injects failure."""

    AGENT = "agent"
    SERVER = "server"
    POSTGRESQL = "postgresql"
    S3 = "s3"
    LOG_INDEXER = "build-log-indexer"
    VCS = "vcs"
    WEBHOOK = "webhook"
    SECRET_PROVIDER = "secret-provider"
    TELEMETRY_EXPORTER = "telemetry-exporter"
    BACKEND = "backend"
    GUEST = "guest"
    NETWORK = "network"


class Phase(StrEnum):
    """Job lifecycle phase exercised by an injected failure."""

    ADMISSION = "admission"
    PREPARATION = "preparation"
    EXECUTION = "execution"
    OUTPUT = "output"
    COMPLETION = "completion"
    RECOVERY = "recovery"
    CLEANUP = "cleanup"


class Invariant(StrEnum):
    """Correctness property retained while a boundary is unavailable."""

    FENCING = "fencing"
    REPLAY = "replay"
    INDEX_REBUILD = "search-index-rebuild"
    OUTBOX_RECOVERY = "outbox-recovery"
    BOUNDED_RETRY = "bounded-retry"
    TEARDOWN = "teardown"
    NO_FALSE_SUCCESS = "no-false-success"


class FailureContract(NamedTuple):
    """One executable group of production failure-injection tests."""

    name: str
    boundaries: tuple[Boundary, ...]
    phases: tuple[Phase, ...]
    invariants: tuple[Invariant, ...]
    command: tuple[str, ...]


CONTRACTS = (
    FailureContract(
        name="agent-coordination",
        boundaries=(Boundary.AGENT, Boundary.NETWORK),
        phases=(
            Phase.PREPARATION,
            Phase.EXECUTION,
            Phase.COMPLETION,
            Phase.RECOVERY,
            Phase.CLEANUP,
        ),
        invariants=(
            Invariant.FENCING,
            Invariant.REPLAY,
            Invariant.BOUNDED_RETRY,
            Invariant.TEARDOWN,
            Invariant.NO_FALSE_SUCCESS,
        ),
        command=(
            "cargo", "test", "--locked", "--all-features",
            "-p", "octacity-lifecycle", "-p", "octacity-coordinator",
        ),
    ),
    FailureContract(
        name="server-workers-and-adapters",
        boundaries=(
            Boundary.SERVER,
            Boundary.LOG_INDEXER,
            Boundary.VCS,
            Boundary.WEBHOOK,
            Boundary.SECRET_PROVIDER,
            Boundary.TELEMETRY_EXPORTER,
        ),
        phases=(Phase.ADMISSION, Phase.PREPARATION, Phase.COMPLETION, Phase.RECOVERY),
        invariants=(
            Invariant.REPLAY,
            Invariant.INDEX_REBUILD,
            Invariant.OUTBOX_RECOVERY,
            Invariant.BOUNDED_RETRY,
            Invariant.NO_FALSE_SUCCESS,
        ),
        command=(
            "cargo", "test", "--locked", "--all-features",
            "-p", "octacity-server",
            "-p", "octacity-server-application",
            "-p", "octacity-server-secrets",
            "-p", "octacity-server-vcs",
            "-p", "octacity-server-webhook",
            "-p", "octacity-server-api-agent",
        ),
    ),
    FailureContract(
        name="execution-backend-and-guest",
        boundaries=(Boundary.BACKEND, Boundary.GUEST),
        phases=(Phase.PREPARATION, Phase.EXECUTION, Phase.CLEANUP),
        invariants=(
            Invariant.FENCING,
            Invariant.BOUNDED_RETRY,
            Invariant.TEARDOWN,
            Invariant.NO_FALSE_SUCCESS,
        ),
        command=(
            "cargo", "test", "--locked", "--all-features",
            "-p", "octacity-runner",
            "-p", "octacity-job",
            "-p", "octacity-execution-oci",
            "-p", "octacity-execution-microsandbox",
        ),
    ),
    FailureContract(
        name="postgresql-recovery",
        boundaries=(Boundary.POSTGRESQL, Boundary.SERVER, Boundary.LOG_INDEXER, Boundary.WEBHOOK),
        phases=(Phase.ADMISSION, Phase.COMPLETION, Phase.RECOVERY),
        invariants=(
            Invariant.FENCING,
            Invariant.REPLAY,
            Invariant.INDEX_REBUILD,
            Invariant.OUTBOX_RECOVERY,
            Invariant.BOUNDED_RETRY,
            Invariant.NO_FALSE_SUCCESS,
        ),
        command=(
            "cargo", "test", "--locked", "--all-features",
            "-p", "octacity-server-store-postgres", "--tests", "--", "--ignored", "--nocapture",
        ),
    ),
    FailureContract(
        name="s3-outage-and-integrity",
        boundaries=(Boundary.S3, Boundary.NETWORK),
        phases=(Phase.OUTPUT, Phase.RECOVERY, Phase.CLEANUP),
        invariants=(
            Invariant.REPLAY,
            Invariant.BOUNDED_RETRY,
            Invariant.TEARDOWN,
            Invariant.NO_FALSE_SUCCESS,
        ),
        command=(
            "cargo", "test", "--locked", "--all-features",
            "-p", "octacity-artifact-s3", "--test", "minio", "--",
            "s3_store_satisfies_the_minio_contract", "--ignored", "--exact", "--nocapture",
        ),
    ),
)


def validate_contracts(contracts: Sequence[FailureContract] = CONTRACTS) -> None:
    """Reject incomplete or ambiguous release evidence before running Cargo."""
    names = [contract.name for contract in contracts]
    if len(names) != len(set(names)):
        raise ValueError("failure contract names must be unique")
    covered_boundaries = {boundary for contract in contracts for boundary in contract.boundaries}
    covered_phases = {phase for contract in contracts for phase in contract.phases}
    covered_invariants = {invariant for contract in contracts for invariant in contract.invariants}
    if covered_boundaries != set(Boundary):
        raise ValueError(f"failure boundaries are incomplete: {set(Boundary) - covered_boundaries}")
    if covered_phases != set(Phase):
        raise ValueError(f"lifecycle phases are incomplete: {set(Phase) - covered_phases}")
    if covered_invariants != set(Invariant):
        missing = set(Invariant) - covered_invariants
        raise ValueError(f"recovery invariants are incomplete: {missing}")
    if any(Invariant.NO_FALSE_SUCCESS not in contract.invariants for contract in contracts):
        raise ValueError("every failure contract must reject false success")
    if any(not contract.command or contract.command[0] != "cargo" for contract in contracts):
        raise ValueError("every failure contract must execute an explicit Cargo test command")


def execute(command: Sequence[str], repository: Path, log_path: Path) -> int:
    """Stream one test command to CI and a retained evidence log."""
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
    repository: Path,
    evidence_dir: Path,
    *,
    runner: Callable[[Sequence[str], Path, Path], int] = execute,
) -> int:
    """Run every contract, retain all outcomes, and fail after the full matrix."""
    validate_contracts()
    evidence_dir.mkdir(parents=True, exist_ok=True)
    outcomes = []
    failed = False
    for contract in CONTRACTS:
        print(f"\n::group::failure contract: {contract.name}", flush=True)
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
    (evidence_dir / "failure-matrix.json").write_text(
        json.dumps(report, indent=2) + "\n", encoding="utf-8"
    )
    return 1 if failed else 0


def parser() -> argparse.ArgumentParser:
    root = argparse.ArgumentParser(description=__doc__)
    commands = root.add_subparsers(dest="command", required=True)
    commands.add_parser("validate")
    run = commands.add_parser("run")
    run.add_argument("--repository", type=Path, default=Path(__file__).resolve().parents[1])
    run.add_argument("--evidence-dir", type=Path, required=True)
    return root


def main() -> int:
    arguments = parser().parse_args()
    if arguments.command == "validate":
        validate_contracts()
        return 0
    return run_contracts(arguments.repository.resolve(), arguments.evidence_dir.resolve())


if __name__ == "__main__":
    raise SystemExit(main())
