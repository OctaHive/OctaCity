#!/usr/bin/env python3
"""Plan backend-contract cadence and verify that required evidence exists."""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import sys
from typing import Any, NamedTuple
from urllib.parse import quote, urlencode
from urllib.request import Request, urlopen


RUNNER_GROUP = "octacity-release"
RELEASE_JOBS = frozenset(
    {
        "failure-matrix",
        "released-host",
        "linux-native",
        "linux-containerd",
        "linux-microsandbox",
        "macos-apple-vf",
        "macos-microsandbox",
    }
)
MANUAL_SUITES = {
    "all": RELEASE_JOBS | {"phase6-vault-minio"},
    "released-agent": RELEASE_JOBS,
    "failure": frozenset({"failure-matrix"}),
    "host": frozenset({"released-host"}),
    "phase6": frozenset({"phase6-vault-minio"}),
    "linux-native": frozenset({"linux-native"}),
    "linux-containerd": frozenset({"linux-containerd"}),
    "linux-microsandbox": frozenset({"linux-microsandbox"}),
    "macos-apple-vf": frozenset({"macos-apple-vf"}),
    "macos-microsandbox": frozenset({"macos-microsandbox"}),
    "windows-microsandbox-preview": frozenset({"windows-microsandbox-preview"}),
}
ALL_JOBS = frozenset().union(*MANUAL_SUITES.values())
SELF_HOSTED_LABELS = {
    "macos-apple-vf": frozenset({"self-hosted", "macos", "arm64", "octacity-apple-vf"}),
    "macos-microsandbox": frozenset({"self-hosted", "macos", "arm64", "octacity-microsandbox"}),
    "windows-microsandbox-preview": frozenset(
        {"self-hosted", "windows", "x64", "octacity-microsandbox-whp"}
    ),
}
RUN_OUTPUTS = {job: f"run_{job.replace('-', '_')}" for job in ALL_JOBS}
RUNNER_OUTPUTS = {job: f"runner_{job.replace('-', '_')}" for job in SELF_HOSTED_LABELS}


class RunnerInventory(NamedTuple):
    """Availability facts for the one repository-scoped release runner group."""

    inventory_available: bool
    group_available: bool
    runners: tuple[dict[str, Any], ...] = ()


def expected_jobs(event_name: str, suite: str, cadence: str) -> frozenset[str]:
    """Return jobs whose evidence is mandatory for this invocation."""
    if cadence == "release" or event_name == "schedule":
        return RELEASE_JOBS
    if event_name == "push":
        return frozenset({"released-host"})
    if event_name == "workflow_dispatch":
        try:
            return MANUAL_SUITES[suite]
        except KeyError as error:
            raise ValueError(f"unknown backend suite: {suite}") from error
    raise ValueError(f"unsupported backend workflow event: {event_name}")


def plan_outputs(event_name: str, suite: str, cadence: str) -> dict[str, bool]:
    """Translate one invocation into stable GitHub job outputs."""
    expected = expected_jobs(event_name, suite, cadence)
    return {
        "requires_self_hosted": bool(expected & SELF_HOSTED_LABELS.keys()),
        **{RUN_OUTPUTS[job]: job in expected for job in sorted(ALL_JOBS)},
    }


def runner_availability(runners: tuple[dict[str, Any], ...]) -> dict[str, bool]:
    """Map each self-hosted job to an online group runner with all required labels."""
    online_label_sets = []
    for runner in runners:
        if runner.get("status") != "online":
            continue
        online_label_sets.append(
            {str(label.get("name", "")).lower() for label in runner.get("labels", [])}
        )
    return {
        job: any(required <= labels for labels in online_label_sets)
        for job, required in SELF_HOSTED_LABELS.items()
    }


def select_runner_group(
    groups: list[dict[str, Any]], *, repository_private: bool
) -> dict[str, Any] | None:
    """Select the canonical group only when its policy can run this repository."""
    for group in groups:
        if group.get("name") != RUNNER_GROUP:
            continue
        if group.get("restricted_to_workflows", False):
            return None
        if not repository_private and not group.get("allows_public_repositories", False):
            return None
        return group
    return None


def next_link(value: str | None) -> str | None:
    """Extract the next page URL from an RFC 8288-style GitHub Link header."""
    if not value:
        return None
    for entry in value.split(","):
        target, *parameters = (part.strip() for part in entry.split(";"))
        if 'rel="next"' in parameters and target.startswith("<") and target.endswith(">"):
            return target[1:-1]
    return None


def github_items(url: str, item_key: str, token: str) -> list[dict[str, Any]]:
    """Read every page of one trusted GitHub collection endpoint."""
    items = []
    while url:
        request = Request(
            url,
            headers={
                "Accept": "application/vnd.github+json",
                "Authorization": f"Bearer {token}",
                "X-GitHub-Api-Version": "2022-11-28",
            },
        )
        with urlopen(request, timeout=20) as response:
            payload = json.load(response)
            page_items = payload.get(item_key)
            if not isinstance(page_items, list):
                raise ValueError(f"GitHub response does not contain a {item_key!r} list")
            if not all(isinstance(item, dict) for item in page_items):
                raise ValueError(f"GitHub {item_key!r} list contains a non-object item")
            items.extend(page_items)
            url = next_link(response.headers.get("Link"))
    return items


def load_runner_inventory(
    *,
    api_url: str,
    organization: str,
    repository: str,
    repository_private: bool,
    token: str,
) -> RunnerInventory:
    """Load online candidates from a group GitHub says is visible to the repository."""
    if not token:
        return RunnerInventory(inventory_available=False, group_available=False)
    api_root = api_url.rstrip("/")
    organization_path = quote(organization, safe="")
    query = urlencode({"visible_to_repository": repository, "per_page": 100})
    groups = github_items(
        f"{api_root}/orgs/{organization_path}/actions/runner-groups?{query}",
        "runner_groups",
        token,
    )
    group = select_runner_group(groups, repository_private=repository_private)
    if group is None:
        return RunnerInventory(inventory_available=True, group_available=False)
    group_id = group.get("id")
    if not isinstance(group_id, int):
        raise ValueError("GitHub runner group does not have an integer id")
    runners = github_items(
        f"{api_root}/orgs/{organization_path}/actions/runner-groups/{group_id}/runners?per_page=100",
        "runners",
        token,
    )
    return RunnerInventory(
        inventory_available=True,
        group_available=True,
        runners=tuple(runners),
    )


def missing_evidence(
    expected: frozenset[str],
    needs: dict[str, Any],
    availability: dict[str, bool],
    *,
    inventory_available: bool = True,
    group_available: bool = True,
) -> list[str]:
    """Describe every required job that did not produce successful evidence."""
    missing = []
    for job in sorted(expected):
        result = needs.get(job, {}).get("result", "missing")
        if result == "success":
            continue
        if job in availability and not availability[job]:
            if not inventory_available:
                reason = "runner inventory unavailable"
            elif not group_available:
                reason = f"runner group {RUNNER_GROUP!r} unavailable"
            else:
                reason = "runner unavailable"
            missing.append(f"{job}: {reason}")
        else:
            missing.append(f"{job}: job result is {result}")
    return missing


def write_outputs(path: Path, values: dict[str, bool]) -> None:
    """Append boolean values using the GitHub Actions output-file protocol."""
    with path.open("a", encoding="utf-8") as output:
        for name, value in sorted(values.items()):
            output.write(f"{name}={'true' if value else 'false'}\n")


def plan_command(arguments: argparse.Namespace) -> int:
    write_outputs(
        arguments.github_output,
        plan_outputs(arguments.event_name, arguments.suite, arguments.cadence),
    )
    return 0


def inventory_command(arguments: argparse.Namespace) -> int:
    token = os.environ.get("OCTACITY_RUNNER_INVENTORY_TOKEN", "")
    if not token:
        print(
            "::warning::Self-hosted runner inventory token is not configured",
            file=sys.stderr,
        )
    try:
        inventory = load_runner_inventory(
            api_url=arguments.api_url,
            organization=arguments.organization,
            repository=arguments.repository,
            repository_private=arguments.repository_private == "true",
            token=token,
        )
    except (OSError, ValueError) as error:
        print(f"::warning::Self-hosted runner inventory is unavailable: {error}", file=sys.stderr)
        inventory = RunnerInventory(inventory_available=False, group_available=False)
    availability = runner_availability(inventory.runners)
    write_outputs(
        arguments.github_output,
        {
            "inventory_available": inventory.inventory_available,
            "runner_group_available": inventory.group_available,
            **{RUNNER_OUTPUTS[job]: available for job, available in availability.items()},
        },
    )
    return 0


def verify_command(arguments: argparse.Namespace) -> int:
    needs = json.loads(arguments.needs.read_text(encoding="utf-8"))
    inventory_outputs = needs.get("runner-inventory", {}).get("outputs", {})
    availability = {
        job: inventory_outputs.get(RUNNER_OUTPUTS[job], "false") == "true"
        for job in SELF_HOSTED_LABELS
    }
    expected = expected_jobs(arguments.event_name, arguments.suite, arguments.cadence)
    missing = missing_evidence(
        expected,
        needs,
        availability,
        inventory_available=inventory_outputs.get("inventory_available", "false") == "true",
        group_available=inventory_outputs.get("runner_group_available", "false") == "true",
    )
    lines = ["## Backend release evidence", "", f"Required: {', '.join(sorted(expected))}"]
    if missing:
        lines.extend(("", "Missing release evidence:", *(f"- {item}" for item in missing)))
    else:
        lines.extend(("", "Every required backend produced successful evidence."))
    summary = "\n".join(lines) + "\n"
    print(summary, end="")
    summary_path = os.environ.get("GITHUB_STEP_SUMMARY")
    if summary_path:
        with Path(summary_path).open("a", encoding="utf-8") as output:
            output.write(summary)
    return 1 if missing else 0


def parser() -> argparse.ArgumentParser:
    root = argparse.ArgumentParser(description=__doc__)
    commands = root.add_subparsers(required=True)
    plan = commands.add_parser("plan")
    plan.add_argument("--event-name", required=True)
    plan.add_argument("--suite", default="")
    plan.add_argument("--cadence", default="")
    plan.add_argument("--github-output", type=Path, required=True)
    plan.set_defaults(handler=plan_command)
    inventory = commands.add_parser("inventory")
    inventory.add_argument("--api-url", required=True)
    inventory.add_argument("--organization", required=True)
    inventory.add_argument("--repository", required=True)
    inventory.add_argument("--repository-private", choices=("true", "false"), required=True)
    inventory.add_argument("--github-output", type=Path, required=True)
    inventory.set_defaults(handler=inventory_command)
    verify = commands.add_parser("verify")
    verify.add_argument("--event-name", required=True)
    verify.add_argument("--suite", default="")
    verify.add_argument("--cadence", default="")
    verify.add_argument("--needs", type=Path, required=True)
    verify.set_defaults(handler=verify_command)
    return root


def main() -> int:
    arguments = parser().parse_args()
    return arguments.handler(arguments)


if __name__ == "__main__":
    raise SystemExit(main())
