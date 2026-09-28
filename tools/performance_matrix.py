#!/usr/bin/env python3
"""Validate raw release measurements against versioned performance budgets."""

from __future__ import annotations

import argparse
from datetime import datetime, timezone
import json
import math
from pathlib import Path
from typing import Any


DEFAULT_BUDGETS = Path(__file__).with_name("performance_budgets.json")
SUPPORTED_STATISTICS = frozenset({"maximum", "minimum", "p95"})


def load_object(path: Path) -> dict[str, Any]:
    """Load one JSON object while retaining a useful path in failures."""
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise ValueError(f"cannot read {path}: {error}") from error
    if not isinstance(value, dict):
        raise ValueError(f"{path} must contain a JSON object")
    return value


def validate_budgets(document: dict[str, Any]) -> dict[str, dict[str, Any]]:
    """Return a complete, strictly typed metric-budget catalog."""
    if document.get("schema_version") != 1:
        raise ValueError("performance budget schema_version must equal 1")
    budgets = document.get("budgets")
    if not isinstance(budgets, dict) or not budgets:
        raise ValueError("performance budgets must be a non-empty object")
    for name, budget in budgets.items():
        if not isinstance(name, str) or not name or not isinstance(budget, dict):
            raise ValueError(
                "every performance budget must have a non-empty name and object value"
            )
        if set(budget) != {"unit", "statistic", "limit", "minimum_samples"}:
            raise ValueError(f"performance budget {name!r} has an invalid shape")
        if not isinstance(budget["unit"], str) or not budget["unit"]:
            raise ValueError(f"performance budget {name!r} has no unit")
        if budget["statistic"] not in SUPPORTED_STATISTICS:
            raise ValueError(f"performance budget {name!r} has an unsupported statistic")
        if not finite_number(budget["limit"]) or budget["limit"] < 0:
            raise ValueError(f"performance budget {name!r} has an invalid limit")
        if (
            not isinstance(budget["minimum_samples"], int)
            or budget["minimum_samples"] <= 0
        ):
            raise ValueError(f"performance budget {name!r} has an invalid sample count")
    return budgets


def finite_number(value: object) -> bool:
    return isinstance(value, (int, float)) and not isinstance(value, bool) and math.isfinite(value)


def validate_measurements(
    document: dict[str, Any], budgets: dict[str, dict[str, Any]]
) -> dict[str, tuple[str, list[float]]]:
    """Validate raw samples and require exact budget coverage."""
    if document.get("schema_version") != 1:
        raise ValueError("performance measurement schema_version must equal 1")
    if not isinstance(document.get("producer"), str) or not document["producer"]:
        raise ValueError("performance measurements must identify their producer")
    entries = document.get("measurements")
    if not isinstance(entries, list):
        raise ValueError("performance measurements must be a list")
    measurements: dict[str, tuple[str, list[float]]] = {}
    for entry in entries:
        if not isinstance(entry, dict) or set(entry) != {"name", "unit", "samples"}:
            raise ValueError("every performance measurement has an invalid shape")
        name = entry["name"]
        unit = entry["unit"]
        samples = entry["samples"]
        if not isinstance(name, str) or not name or name in measurements:
            raise ValueError("performance measurement names must be non-empty and unique")
        if not isinstance(unit, str) or not isinstance(samples, list):
            raise ValueError(
                f"performance measurement {name!r} has invalid unit or samples"
            )
        if not samples or not all(
            finite_number(sample) and sample >= 0 for sample in samples
        ):
            raise ValueError(f"performance measurement {name!r} has invalid raw samples")
        measurements[name] = (unit, [float(sample) for sample in samples])
    missing = set(budgets) - set(measurements)
    extra = set(measurements) - set(budgets)
    if missing or extra:
        raise ValueError(
            "performance measurement coverage differs: "
            f"missing={sorted(missing)}, extra={sorted(extra)}"
        )
    for name, budget in budgets.items():
        unit, samples = measurements[name]
        if unit != budget["unit"]:
            raise ValueError(
                f"performance measurement {name!r} uses {unit!r}, "
                f"expected {budget['unit']!r}"
            )
        if len(samples) < budget["minimum_samples"]:
            raise ValueError(
                f"performance measurement {name!r} has {len(samples)} samples, "
                f"expected at least {budget['minimum_samples']}"
            )
    return measurements


def statistic(kind: str, samples: list[float]) -> float:
    """Calculate the exact statistic named by a versioned budget."""
    if kind == "maximum":
        return max(samples)
    if kind == "minimum":
        return min(samples)
    ordered = sorted(samples)
    return ordered[max(0, math.ceil(0.95 * len(ordered)) - 1)]


def evaluate(
    budget_document: dict[str, Any], measurement_document: dict[str, Any]
) -> dict[str, Any]:
    """Evaluate every raw metric and return a machine-readable gate report."""
    budgets = validate_budgets(budget_document)
    measurements = validate_measurements(measurement_document, budgets)
    results = []
    failed = False
    for name, budget in sorted(budgets.items()):
        unit, samples = measurements[name]
        observed = statistic(budget["statistic"], samples)
        passed = (
            observed >= budget["limit"]
            if budget["statistic"] == "minimum"
            else observed <= budget["limit"]
        )
        failed |= not passed
        results.append(
            {
                "name": name,
                "unit": unit,
                "statistic": budget["statistic"],
                "observed": observed,
                "limit": budget["limit"],
                "sample_count": len(samples),
                "passed": passed,
            }
        )
    return {
        "schema_version": 1,
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "producer": measurement_document["producer"],
        "result": "failed" if failed else "passed",
        "metrics": results,
    }


def parser() -> argparse.ArgumentParser:
    root = argparse.ArgumentParser(description=__doc__)
    commands = root.add_subparsers(dest="command", required=True)
    validate = commands.add_parser("validate")
    validate.add_argument("--budgets", type=Path, default=DEFAULT_BUDGETS)
    evaluate_parser = commands.add_parser("evaluate")
    evaluate_parser.add_argument("--budgets", type=Path, default=DEFAULT_BUDGETS)
    evaluate_parser.add_argument("--measurements", type=Path, required=True)
    evaluate_parser.add_argument("--report", type=Path, required=True)
    return root


def main() -> int:
    arguments = parser().parse_args()
    budgets = load_object(arguments.budgets)
    validate_budgets(budgets)
    if arguments.command == "validate":
        return 0
    measurements = load_object(arguments.measurements)
    report = evaluate(budgets, measurements)
    arguments.report.parent.mkdir(parents=True, exist_ok=True)
    arguments.report.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    for metric in report["metrics"]:
        print(
            f"{metric['name']}: {metric['observed']:.3f} {metric['unit']} "
            f"({metric['statistic']} budget {metric['limit']})"
        )
    return 0 if report["result"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())
