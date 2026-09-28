#!/usr/bin/env python3
"""Run release lifecycle contracts for service managers and execution providers."""

from __future__ import annotations

import argparse
from enum import StrEnum
from pathlib import Path
from typing import Callable, NamedTuple, Sequence

from contract_matrix import execute, run_contracts as execute_contracts


class ServiceManager(StrEnum):
    """Service manager shipped for one release-qualified host platform."""

    SYSTEMD = "systemd"
    LAUNCHD = "launchd"
    WINDOWS_SCM = "windows-scm"


class ExecutionProvider(StrEnum):
    """Execution provider represented in release lifecycle evidence."""

    HOST = "host"
    LEGACY_NATIVE = "legacy-native"
    CONTAINERD = "containerd"
    APPLE_VF = "apple-vf"
    MICROSANDBOX = "microsandbox"


class ReleaseTarget(StrEnum):
    """One release-qualified service-manager and execution-provider pair."""

    LINUX_HOST = "linux-systemd-host"
    LINUX_LEGACY_NATIVE = "linux-systemd-legacy-native"
    LINUX_CONTAINERD = "linux-systemd-containerd"
    LINUX_MICROSANDBOX = "linux-systemd-microsandbox"
    MACOS_HOST = "macos-launchd-host"
    MACOS_APPLE_VF = "macos-launchd-apple-vf"
    MACOS_MICROSANDBOX = "macos-launchd-microsandbox"
    WINDOWS_HOST = "windows-scm-host"


class LifecyclePhase(StrEnum):
    """Operator-visible lifecycle transition required by task 9.10."""

    INSTALL = "install"
    VALIDATE = "validate"
    START = "start"
    REBOOT = "reboot"
    GRACEFUL_DRAIN = "graceful-drain"
    FORCED_DRAIN = "forced-drain"
    UPGRADE = "upgrade"
    ROLLBACK_WINDOW = "rollback-window"
    UNINSTALL = "uninstall"


class PipelineState(StrEnum):
    """Whether service maintenance occurs with executable work present."""

    ABSENT = "absent"
    ACTIVE = "active"


class TargetDetails(NamedTuple):
    """Platform facts and required real job for one qualified target."""

    service_manager: ServiceManager
    execution_provider: ExecutionProvider
    release_job: str


TARGET_DETAILS: dict[ReleaseTarget, TargetDetails] = {
    ReleaseTarget.LINUX_HOST: TargetDetails(
        ServiceManager.SYSTEMD, ExecutionProvider.HOST, "released-host"
    ),
    ReleaseTarget.LINUX_LEGACY_NATIVE: TargetDetails(
        ServiceManager.SYSTEMD, ExecutionProvider.LEGACY_NATIVE, "linux-native"
    ),
    ReleaseTarget.LINUX_CONTAINERD: TargetDetails(
        ServiceManager.SYSTEMD, ExecutionProvider.CONTAINERD, "linux-containerd"
    ),
    ReleaseTarget.LINUX_MICROSANDBOX: TargetDetails(
        ServiceManager.SYSTEMD, ExecutionProvider.MICROSANDBOX, "linux-microsandbox"
    ),
    ReleaseTarget.MACOS_HOST: TargetDetails(
        ServiceManager.LAUNCHD, ExecutionProvider.HOST, "released-host"
    ),
    ReleaseTarget.MACOS_APPLE_VF: TargetDetails(
        ServiceManager.LAUNCHD, ExecutionProvider.APPLE_VF, "macos-apple-vf"
    ),
    ReleaseTarget.MACOS_MICROSANDBOX: TargetDetails(
        ServiceManager.LAUNCHD, ExecutionProvider.MICROSANDBOX, "macos-microsandbox"
    ),
    ReleaseTarget.WINDOWS_HOST: TargetDetails(
        ServiceManager.WINDOWS_SCM, ExecutionProvider.HOST, "released-host"
    ),
}
REQUIRED_RELEASE_JOBS = frozenset(details.release_job for details in TARGET_DETAILS.values())


class LifecycleContract(NamedTuple):
    """One executable lifecycle contract and its retained coverage claims."""

    name: str
    targets: tuple[ReleaseTarget, ...]
    phases: tuple[LifecyclePhase, ...]
    pipeline_states: tuple[PipelineState, ...]
    command: tuple[str, ...]


ALL_TARGETS = tuple(ReleaseTarget)
HOST_TARGETS = tuple(
    target
    for target, details in TARGET_DETAILS.items()
    if details.execution_provider is ExecutionProvider.HOST
)
ALL_PIPELINE_STATES = tuple(PipelineState)

CONTRACTS = (
    LifecycleContract(
        name="packaged-service-managers",
        targets=HOST_TARGETS,
        phases=(
            LifecyclePhase.INSTALL,
            LifecyclePhase.VALIDATE,
            LifecyclePhase.START,
            LifecyclePhase.UPGRADE,
            LifecyclePhase.ROLLBACK_WINDOW,
            LifecyclePhase.UNINSTALL,
        ),
        pipeline_states=(PipelineState.ABSENT,),
        command=("python3", "-m", "unittest", "tools.tests.test_package_release", "-v"),
    ),
    LifecycleContract(
        name="agent-restart-and-drain",
        targets=HOST_TARGETS,
        phases=(
            LifecyclePhase.START,
            LifecyclePhase.REBOOT,
            LifecyclePhase.GRACEFUL_DRAIN,
            LifecyclePhase.FORCED_DRAIN,
        ),
        pipeline_states=ALL_PIPELINE_STATES,
        command=(
            "cargo",
            "test",
            "--locked",
            "--all-features",
            "-p",
            "octacity-agent",
            "-p",
            "octacity-coordinator",
            "-p",
            "octacity-lifecycle",
        ),
    ),
    LifecycleContract(
        name="execution-provider-recovery",
        targets=ALL_TARGETS,
        phases=(
            LifecyclePhase.VALIDATE,
            LifecyclePhase.START,
            LifecyclePhase.REBOOT,
            LifecyclePhase.GRACEFUL_DRAIN,
            LifecyclePhase.FORCED_DRAIN,
            LifecyclePhase.UNINSTALL,
        ),
        pipeline_states=ALL_PIPELINE_STATES,
        command=(
            "cargo",
            "test",
            "--locked",
            "--all-features",
            "-p",
            "octacity-job",
            "-p",
            "octacity-execution-host",
            "-p",
            "octacity-execution-native",
            "-p",
            "octacity-execution-containerd",
            "-p",
            "octacity-execution-apple-vf",
            "-p",
            "octacity-execution-microsandbox",
        ),
    ),
    LifecycleContract(
        name="server-directed-pipeline-drain",
        targets=ALL_TARGETS,
        phases=(
            LifecyclePhase.REBOOT,
            LifecyclePhase.GRACEFUL_DRAIN,
            LifecyclePhase.FORCED_DRAIN,
        ),
        pipeline_states=(PipelineState.ACTIVE,),
        command=(
            "cargo",
            "test",
            "--locked",
            "--all-features",
            "-p",
            "octacity-server-application",
            "-p",
            "octacity-server-scheduler",
            "-p",
            "octacity-server-store",
        ),
    ),
    LifecycleContract(
        name="immutable-release-switching",
        targets=ALL_TARGETS,
        phases=(
            LifecyclePhase.INSTALL,
            LifecyclePhase.VALIDATE,
            LifecyclePhase.UPGRADE,
            LifecyclePhase.ROLLBACK_WINDOW,
            LifecyclePhase.UNINSTALL,
        ),
        pipeline_states=ALL_PIPELINE_STATES,
        command=(
            "cargo",
            "test",
            "--locked",
            "-p",
            "octacity-release-harness",
            "--test",
            "harness",
        ),
    ),
)


def validate_contracts(contracts: Sequence[LifecycleContract] = CONTRACTS) -> None:
    """Reject lifecycle catalogs that could report partial release evidence."""
    names = [contract.name for contract in contracts]
    if len(names) != len(set(names)):
        raise ValueError("lifecycle contract names must be unique")

    def covered(attribute: str) -> set[object]:
        return {
            value
            for contract in contracts
            for value in getattr(contract, attribute)
        }

    expected = {
        "targets": set(ReleaseTarget),
        "phases": set(LifecyclePhase),
        "pipeline_states": set(PipelineState),
    }
    for attribute, required in expected.items():
        missing = required - covered(attribute)
        if missing:
            raise ValueError(f"lifecycle {attribute} are incomplete: {missing}")

    phase_states = {
        (phase, state)
        for contract in contracts
        for phase in contract.phases
        for state in contract.pipeline_states
    }
    required_phase_states = {
        (phase, state) for phase in LifecyclePhase for state in PipelineState
    }
    if phase_states != required_phase_states:
        raise ValueError(
            f"lifecycle phase/Pipeline coverage is incomplete: "
            f"{required_phase_states - phase_states}"
        )

    selected_targets = {
        target for contract in contracts for target in contract.targets
    }
    managers = {TARGET_DETAILS[target].service_manager for target in selected_targets}
    providers = {
        TARGET_DETAILS[target].execution_provider for target in selected_targets
    }
    if managers != set(ServiceManager) or providers != set(ExecutionProvider):
        raise ValueError("lifecycle targets do not cover every service manager and provider")

    release_jobs = {TARGET_DETAILS[target].release_job for target in selected_targets}
    if release_jobs != set(REQUIRED_RELEASE_JOBS):
        raise ValueError(
            f"lifecycle release jobs are incomplete: "
            f"{set(REQUIRED_RELEASE_JOBS) - release_jobs}"
        )
    if any(
        not contract.command or contract.command[0] not in {"cargo", "python3"}
        for contract in contracts
    ):
        raise ValueError("every lifecycle contract must execute an explicit test command")


def run_contracts(
    repository: Path,
    evidence_dir: Path,
    *,
    runner: Callable[[Sequence[str], Path, Path], int] = execute,
) -> int:
    """Run every lifecycle contract and retain machine-readable evidence."""
    validate_contracts()
    return execute_contracts(
        CONTRACTS,
        repository,
        evidence_dir,
        group="release lifecycle contract",
        report_name="lifecycle-matrix.json",
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
