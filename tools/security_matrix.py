#!/usr/bin/env python3
"""Run the complete security-boundary contracts required for release."""

from __future__ import annotations

import argparse
from enum import StrEnum
from pathlib import Path
from typing import Callable, NamedTuple, Sequence

from contract_matrix import execute, run_contracts as execute_contracts


class Boundary(StrEnum):
    """Security boundary that must retain explicit negative evidence."""

    AGENT_ENROLLMENT = "agent-enrollment"
    POOL_ADMISSION = "pool-admission"
    PROJECT_POLICY = "project-policy"
    SIGNED_JOB_SPEC = "signed-job-spec"
    EXECUTION_MODE_ADMISSION = "execution-mode-admission"
    PROVIDER_CONFIGURATION = "provider-configuration"
    WEBHOOK_AUTHENTICATION = "webhook-authentication"
    SECRET_REFERENCES = "secret-references"
    MALICIOUS_REPOSITORIES = "malicious-repositories"
    PLUGIN_DIGESTS = "plugin-digests"
    ARTIFACT_PATHS = "artifact-paths"
    LOG_ARCHIVE_REDACTION = "build-log-archive-redaction"
    LOG_SEARCH_REDACTION = "build-log-search-redaction"
    PRESIGNED_URL_REDACTION = "presigned-url-redaction"
    CACHE_NAMESPACES = "cache-namespaces"


class SecurityProperty(StrEnum):
    """Observable property proved by one or more negative contracts."""

    AUTHENTICATION = "authentication"
    AUTHORIZATION = "authorization"
    INTEGRITY = "integrity"
    ISOLATION = "isolation"
    NON_EXECUTION = "non-execution"
    REDACTION = "redaction"
    FAIL_CLOSED = "fail-closed"


class SecurityContract(NamedTuple):
    """One executable group of security-boundary tests."""

    name: str
    boundaries: tuple[Boundary, ...]
    properties: tuple[SecurityProperty, ...]
    command: tuple[str, ...]


CONTRACTS = (
    SecurityContract(
        name="identity-policy-and-jobspec",
        boundaries=(
            Boundary.AGENT_ENROLLMENT,
            Boundary.POOL_ADMISSION,
            Boundary.PROJECT_POLICY,
            Boundary.SIGNED_JOB_SPEC,
            Boundary.EXECUTION_MODE_ADMISSION,
            Boundary.SECRET_REFERENCES,
        ),
        properties=(
            SecurityProperty.AUTHENTICATION,
            SecurityProperty.AUTHORIZATION,
            SecurityProperty.INTEGRITY,
            SecurityProperty.ISOLATION,
            SecurityProperty.FAIL_CLOSED,
        ),
        command=(
            "cargo", "test", "--locked", "--all-features",
            "-p", "octacity-protocol",
            "-p", "octacity-server-application",
            "-p", "octacity-server-scheduler",
            "-p", "octacity-server-job",
            "-p", "octacity-server-secrets",
            "-p", "octacity-server-api-agent",
        ),
    ),
    SecurityContract(
        name="provider-webhook-and-repository",
        boundaries=(
            Boundary.PROVIDER_CONFIGURATION,
            Boundary.WEBHOOK_AUTHENTICATION,
            Boundary.MALICIOUS_REPOSITORIES,
        ),
        properties=(
            SecurityProperty.AUTHENTICATION,
            SecurityProperty.INTEGRITY,
            SecurityProperty.ISOLATION,
            SecurityProperty.NON_EXECUTION,
            SecurityProperty.REDACTION,
            SecurityProperty.FAIL_CLOSED,
        ),
        command=(
            "cargo", "test", "--locked", "--all-features",
            "-p", "octacity-server",
            "-p", "octacity-server-api-webhook",
            "-p", "octacity-server-adapter-host",
            "-p", "octacity-server-vcs",
            "-p", "octacity-server-webhook",
            "-p", "octacity-webhook-provider-protocol",
            "-p", "octacity-vcs-protocol",
            "-p", "octacity-vcs-git",
        ),
    ),
    SecurityContract(
        name="agent-filesystem-and-supply-chain",
        boundaries=(
            Boundary.EXECUTION_MODE_ADMISSION,
            Boundary.PLUGIN_DIGESTS,
            Boundary.ARTIFACT_PATHS,
            Boundary.LOG_ARCHIVE_REDACTION,
        ),
        properties=(
            SecurityProperty.AUTHORIZATION,
            SecurityProperty.INTEGRITY,
            SecurityProperty.ISOLATION,
            SecurityProperty.REDACTION,
            SecurityProperty.FAIL_CLOSED,
        ),
        command=(
            "cargo", "test", "--locked", "--all-features",
            "-p", "octacity-runner",
            "-p", "octacity-source",
            "-p", "octacity-job",
            "-p", "octacity-output",
            "-p", "octacity-private-fs",
        ),
    ),
    SecurityContract(
        name="output-search-and-cache",
        boundaries=(
            Boundary.SECRET_REFERENCES,
            Boundary.ARTIFACT_PATHS,
            Boundary.LOG_ARCHIVE_REDACTION,
            Boundary.LOG_SEARCH_REDACTION,
            Boundary.PRESIGNED_URL_REDACTION,
            Boundary.CACHE_NAMESPACES,
        ),
        properties=(
            SecurityProperty.AUTHORIZATION,
            SecurityProperty.INTEGRITY,
            SecurityProperty.ISOLATION,
            SecurityProperty.REDACTION,
            SecurityProperty.FAIL_CLOSED,
        ),
        command=(
            "cargo", "test", "--locked", "--all-features",
            "-p", "octacity-artifact-store",
            "-p", "octacity-server-artifacts",
            "-p", "octacity-server-cache",
            "-p", "octacity-server-store",
            "-p", "octacity-server-application",
            "-p", "octacity-server-api-rest",
        ),
    ),
    SecurityContract(
        name="postgresql-security-state",
        boundaries=(
            Boundary.AGENT_ENROLLMENT,
            Boundary.POOL_ADMISSION,
            Boundary.LOG_ARCHIVE_REDACTION,
            Boundary.LOG_SEARCH_REDACTION,
            Boundary.CACHE_NAMESPACES,
        ),
        properties=(
            SecurityProperty.AUTHENTICATION,
            SecurityProperty.AUTHORIZATION,
            SecurityProperty.INTEGRITY,
            SecurityProperty.ISOLATION,
            SecurityProperty.FAIL_CLOSED,
        ),
        command=(
            "cargo", "test", "--locked", "--all-features",
            "-p", "octacity-server-store-postgres",
            "--test", "store_contract",
            "--test", "cache_sessions",
            "--test", "log_archive",
            "--test", "log_search_index",
            "--", "--ignored", "--nocapture",
        ),
    ),
)


def validate_contracts(contracts: Sequence[SecurityContract] = CONTRACTS) -> None:
    """Reject incomplete or ambiguous security evidence before running Cargo."""
    names = [contract.name for contract in contracts]
    if len(names) != len(set(names)):
        raise ValueError("security contract names must be unique")
    covered_boundaries = {boundary for contract in contracts for boundary in contract.boundaries}
    covered_properties = {prop for contract in contracts for prop in contract.properties}
    if covered_boundaries != set(Boundary):
        missing = set(Boundary) - covered_boundaries
        raise ValueError(f"security boundaries are incomplete: {missing}")
    if covered_properties != set(SecurityProperty):
        missing = set(SecurityProperty) - covered_properties
        raise ValueError(f"security properties are incomplete: {missing}")
    if any(SecurityProperty.FAIL_CLOSED not in contract.properties for contract in contracts):
        raise ValueError("every security contract must fail closed")
    if any(not contract.command or contract.command[0] != "cargo" for contract in contracts):
        raise ValueError("every security contract must execute an explicit Cargo test command")


def run_contracts(
    repository: Path,
    evidence_dir: Path,
    *,
    runner: Callable[[Sequence[str], Path, Path], int] = execute,
) -> int:
    """Run every security contract and retain machine-readable evidence."""
    validate_contracts()
    return execute_contracts(
        CONTRACTS,
        repository,
        evidence_dir,
        group="security contract",
        report_name="security-matrix.json",
        runner=runner,
    )


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
