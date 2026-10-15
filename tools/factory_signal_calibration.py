#!/usr/bin/env python3
"""Compare labeled Signal observations; never authorize runtime promotion."""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import re
import sys
from pathlib import Path


SCALE = 1_000_000
MAX_DOCUMENT_BYTES = 1_048_576
MAX_CASES = 512
IDENTITY = re.compile(r"[A-Za-z0-9][A-Za-z0-9._/-]{0,127}\Z")
DIGEST = re.compile(r"[0-9a-f]{64}\Z")
PURPOSES = ("routing", "tool_risk")
BINDINGS = {"provider", "adapter", "model", "probability_semantics", "question_set", "policy"}
STATUSES = {
    "completed", "timeout", "cancelled", "invalid_result", "unavailable",
    "unknown_outcome", "unsupported", "budget_exhausted",
}
PUBLIC_STATE = {
    "contract_ready": bool, "context_complete": bool, "within_permissions": bool,
    "destructive": bool, "evidence_available": bool, "requires_human": bool,
    "kind": {"defect", "feature", "unknown"},
    "risk": {"low", "medium", "high", "unknown"},
    "action": {"read", "write", "execute", "network", "none"},
    "source": {"repository", "protected_input", "scratch", "output", "none"},
}


def require(condition):
    if not condition:
        raise ValueError("invalid calibration input")


def closed(value, keys):
    require(isinstance(value, dict) and set(value) == set(keys))


def identifier(value):
    return isinstance(value, str) and IDENTITY.fullmatch(value) is not None


def integer(value, maximum):
    return type(value) is int and 0 <= value <= maximum


def object_pairs(pairs):
    result = {}
    for key, value in pairs:
        require(key not in result)
        result[key] = value
    return result


def reject_constant(_value):
    raise ValueError("nonfinite JSON number")


def load_document(path):
    with path.open("rb") as source:
        data = source.read(MAX_DOCUMENT_BYTES + 1)
    require(len(data) <= MAX_DOCUMENT_BYTES)
    document = json.loads(
        data, object_pairs_hook=object_pairs, parse_constant=reject_constant,
    )
    return document, hashlib.sha256(data).hexdigest()


def validate_corpus(corpus):
    closed(corpus, {"schema_version", "label_version", "cases"})
    require(type(corpus["schema_version"]) is int and corpus["schema_version"] == 1)
    require(identifier(corpus["label_version"]))
    cases = corpus["cases"]
    require(isinstance(cases, list) and 2 <= len(cases) <= MAX_CASES)
    by_id = {}
    for case in cases:
        closed(case, {"id", "purpose", "choices", "expected", "baseline", "public_state"})
        require(identifier(case["id"]) and case["id"] not in by_id)
        require(isinstance(case["purpose"], str) and case["purpose"] in PURPOSES)
        choices = case["choices"]
        require(
            isinstance(choices, list) and 2 <= len(choices) <= 24
            and all(identifier(x) for x in choices)
        )
        require(len(set(choices)) == len(choices))
        require(case["expected"] in choices and case["baseline"] in choices)
        state = case["public_state"]
        require(
            isinstance(state, dict) and 0 < len(state) <= len(PUBLIC_STATE)
            and set(state) <= set(PUBLIC_STATE)
        )
        require(state.get("within_permissions") is True)
        for name, value in state.items():
            domain = PUBLIC_STATE[name]
            require(type(value) is bool if domain is bool else isinstance(value, str) and value in domain)
        by_id[case["id"]] = case
    require({case["purpose"] for case in cases} == set(PURPOSES))
    return by_id


def validate(corpus, document, corpus_digest):
    by_id = validate_corpus(corpus)
    cases = corpus["cases"]
    closed(document, {"schema_version", "mode", "source_kind", "bindings", "corpus_sha256", "observations"})
    require(type(document["schema_version"]) is int and document["schema_version"] == 1)
    require(isinstance(document["mode"], str) and document["mode"] in {"shadow", "advisory"})
    require(isinstance(document["source_kind"], str) and document["source_kind"] in {"fixture", "live"})
    require(document["corpus_sha256"] == corpus_digest)
    closed(document["bindings"], PURPOSES)
    for binding in document["bindings"].values():
        closed(binding, BINDINGS)
        for reference in binding.values():
            closed(reference, {"id", "version", "sha256"})
            require(identifier(reference["id"]) and identifier(reference["version"]))
            require(reference["version"].lower() not in {"latest", "default", "auto", "current"})
            require(isinstance(reference["sha256"], str) and DIGEST.fullmatch(reference["sha256"]) is not None)
    entries = document["observations"]
    require(isinstance(entries, list) and len(entries) == len(cases))
    observed, receipts = set(), set()
    for entry in entries:
        closed(entry, {
            "case_id", "status", "selected", "confidence_ppm", "latency_ms",
            "final_disposition", "receipt_sha256",
        })
        require(
            identifier(entry["case_id"]) and entry["case_id"] in by_id
            and entry["case_id"] not in observed
        )
        observed.add(entry["case_id"])
        require(isinstance(entry["receipt_sha256"], str) and DIGEST.fullmatch(entry["receipt_sha256"]) is not None)
        require(entry["receipt_sha256"] not in receipts)
        receipts.add(entry["receipt_sha256"])
        require(isinstance(entry["status"], str) and entry["status"] in STATUSES)
        require(
            type(entry["latency_ms"]) in (int, float)
            and math.isfinite(entry["latency_ms"])
            and 0 <= entry["latency_ms"] <= 3_600_000
        )
        if entry["status"] == "completed":
            require(entry["selected"] in by_id[entry["case_id"]]["choices"])
            require(integer(entry["confidence_ppm"], SCALE))
        else:
            require(entry["selected"] is None and entry["confidence_ppm"] is None)
        require(entry["final_disposition"] == by_id[entry["case_id"]]["baseline"])


def summarize(cases, observations):
    """Keep deterministic baseline and model observations separately visible."""
    total = len(cases)
    completed = [case for case in cases if observations[case["id"]]["status"] == "completed"]
    count = len(completed)
    correct = sum(observations[case["id"]]["selected"] == case["expected"] for case in completed)
    confidence = sum(observations[case["id"]]["confidence_ppm"] for case in completed)
    accuracy = correct * SCALE // count if count else None
    mean_confidence = confidence // count if count else None
    squared_error = sum(
        (observations[case["id"]]["confidence_ppm"]
         - SCALE * (observations[case["id"]]["selected"] == case["expected"])) ** 2
        for case in completed
    )
    bins = {}
    for case in completed:
        entry = observations[case["id"]]
        bucket = min(9, entry["confidence_ppm"] // 100_000)
        difference = entry["confidence_ppm"] - SCALE * (entry["selected"] == case["expected"])
        bins[bucket] = bins.get(bucket, 0) + difference
    latencies = sorted(observations[case["id"]]["latency_ms"] for case in cases)
    return {
        "cases": total,
        "completed": count,
        "baseline_accuracy_ppm": sum(case["baseline"] == case["expected"] for case in cases) * SCALE // total,
        "model_accuracy_ppm": accuracy,
        "disagreement_ppm": sum(
            observations[case["id"]]["selected"] != case["baseline"] for case in completed
        ) * SCALE // count if count else None,
        "mean_confidence_ppm": mean_confidence,
        "brier_score_ppm": squared_error // (count * SCALE) if count else None,
        "calibration_gap_ppm": abs(mean_confidence - accuracy) if count else None,
        "expected_calibration_error_ppm": (
            sum(abs(difference) for difference in bins.values()) // count
            if count else None
        ),
        "failure_rate_ppm": (total - count) * SCALE // total,
        "p95_latency_ms": latencies[math.ceil(0.95 * total) - 1],
    }


def evaluate(corpus, document):
    """Produce an offline comparison, not a provider or policy attestation."""
    observations = {entry["case_id"]: entry for entry in document["observations"]}
    if any(
        observations[case["id"]]["final_disposition"] != case["baseline"]
        for case in corpus["cases"]
    ):
        raise ValueError("execution changed")
    return {
        "schema_version": 1,
        "comparison_only": True,
        "corpus_sha256": document["corpus_sha256"],
        "label_version": corpus["label_version"],
        "mode": document["mode"],
        "source_kind": document["source_kind"],
        "bindings": document["bindings"],
        "purposes": {
            purpose: summarize([case for case in corpus["cases"] if case["purpose"] == purpose], observations)
            for purpose in ["routing", "tool_risk"]
        },
    }


def compare_drift(current, previous, reference_digest):
    require(isinstance(previous, dict))
    required = set(current)
    require(set(previous) in (required, required | {"drift"}))
    require(type(previous["schema_version"]) is int and previous["schema_version"] == 1)
    require(previous["comparison_only"] is True)
    require(isinstance(previous["observations_sha256"], str) and DIGEST.fullmatch(previous["observations_sha256"]) is not None)
    for name in ["corpus_sha256", "label_version", "bindings", "source_kind"]:
        require(previous[name] == current[name])
    require(isinstance(previous["mode"], str) and previous["mode"] in {"shadow", "advisory"})
    require(previous["mode"] == "shadow" or current["mode"] == "advisory")
    require(previous["mode"] != "advisory" or "drift" in previous)
    closed(previous["purposes"], PURPOSES)
    for purpose in PURPOSES:
        old = previous["purposes"][purpose]
        now = current["purposes"][purpose]
        closed(old, now)
        require(old["cases"] == now["cases"] and old["baseline_accuracy_ppm"] == now["baseline_accuracy_ppm"])
        require(integer(old["completed"], old["cases"]))
        for name in set(old) - {"cases", "completed", "baseline_accuracy_ppm", "failure_rate_ppm", "p95_latency_ms"}:
            require((old[name] is None) == (old["completed"] == 0))
        for name, value in old.items():
            if name in {"cases", "completed"}:
                require(integer(value, MAX_CASES))
            elif name == "p95_latency_ms":
                require(type(value) in (int, float) and math.isfinite(value) and 0 <= value <= 3_600_000)
            elif value is None:
                require(old["completed"] == 0 and name not in {"failure_rate_ppm", "baseline_accuracy_ppm"})
            else:
                require(integer(value, SCALE))
        require(old["failure_rate_ppm"] == (old["cases"] - old["completed"]) * SCALE // old["cases"])
    if "drift" in previous:
        drift = previous["drift"]
        closed(drift, {"reference_sha256", "purposes"})
        require(isinstance(drift["reference_sha256"], str) and DIGEST.fullmatch(drift["reference_sha256"]) is not None)
        closed(drift["purposes"], PURPOSES)
        for purpose in PURPOSES:
            closed(drift["purposes"][purpose], current["purposes"][purpose])
            require(all(
                value is None or (
                    type(value) in (int, float) and math.isfinite(value)
                    and abs(value) <= 3_600_000
                )
                for value in drift["purposes"][purpose].values()
            ))
    return {
        "reference_sha256": reference_digest,
        "purposes": {
            purpose: {
                name: value - previous["purposes"][purpose][name]
                if value is not None and previous["purposes"][purpose][name] is not None else None
                for name, value in current["purposes"][purpose].items()
            }
            for purpose in PURPOSES
        },
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    validation = commands.add_parser("validate")
    validation.add_argument("--corpus", type=Path, required=True)
    command = commands.add_parser("evaluate")
    for name in ["corpus", "observations", "report"]:
        command.add_argument(f"--{name}", type=Path, required=True)
    command.add_argument("--previous", type=Path)
    arguments = parser.parse_args()
    try:
        corpus, corpus_digest = load_document(arguments.corpus)
        if arguments.command == "validate":
            validate_corpus(corpus)
            print(json.dumps({
                "schema_version": 1, "label_version": corpus["label_version"],
                "corpus_sha256": corpus_digest,
                "cases": {purpose: sum(case["purpose"] == purpose for case in corpus["cases"]) for purpose in PURPOSES},
            }, sort_keys=True))
            return 0
        inputs = [arguments.corpus, arguments.observations]
        if arguments.previous is not None:
            inputs.append(arguments.previous)
        require(all(arguments.report.resolve() != path.resolve() for path in inputs))
        if arguments.report.exists():
            require(all(not arguments.report.samefile(path) for path in inputs))
        observations, observations_digest = load_document(arguments.observations)
        validate(corpus, observations, corpus_digest)
        require(observations["mode"] != "advisory" or arguments.previous is not None)
        report = evaluate(corpus, observations)
        report["observations_sha256"] = observations_digest
        if arguments.previous is not None:
            previous, previous_digest = load_document(arguments.previous)
            report["drift"] = compare_drift(report, previous, previous_digest)
        arguments.report.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    except (OSError, UnicodeError, ValueError, KeyError, TypeError, OverflowError, RecursionError):
        print("invalid calibration input", file=sys.stderr)
        return 2
    print("Recorded comparison; bounded-control promotion is not authorized.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
