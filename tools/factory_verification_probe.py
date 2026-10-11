"""Bounded HTTP contract assertions for an ordinary, source-free Factory Build.

Node names, acceptance classes, targets and thresholds are supplied in protected
configuration. This tool has no Flow dispatch or repository access.
"""
import time
import json
import http.client
import re
import sys
import urllib.error
import urllib.parse
import urllib.request


def _integer(value, minimum, maximum):
    if type(value) is not int or not minimum <= value <= maximum:
        raise ValueError("invalid probe bound")
    return value


def _validate(config):
    allowed = {"plan_digest", "input_digest", "check", "base_url", "allowed_hosts", "limits", "steps", "repetitions", "interval_ms", "target"}
    if set(config) - allowed or any(not re.fullmatch(r"[a-f0-9]{64}", config[field]) for field in ("plan_digest", "input_digest")):
        raise ValueError("invalid probe identity")
    if not re.fullmatch(r"[a-z0-9][a-z0-9._-]{0,127}", config["check"]):
        raise ValueError("invalid check key")
    url = urllib.parse.urlsplit(config["base_url"])
    if config.get("target", {}).get("endpoint", config["base_url"]) != config["base_url"]:
        raise ValueError("probe endpoint differs from the frozen target")
    if (url.scheme not in {"http", "https"} or not url.hostname or url.username or url.password
            or url.query or url.fragment or url.hostname not in config["allowed_hosts"]):
        raise ValueError("undeclared probe destination")
    limits = config["limits"]
    if set(limits) != {"max_requests", "timeout_ms", "max_response_bytes", "max_elapsed_ms"}:
        raise ValueError("invalid probe limits")
    for key, ceiling in [("max_requests", 128), ("timeout_ms", 60000), ("max_response_bytes", 1048576), ("max_elapsed_ms", 86400000)]:
        _integer(limits[key], 1, ceiling)
    repetitions = _integer(config.get("repetitions", 1), 1, 128)
    interval = _integer(config.get("interval_ms", 0), 0, limits["max_elapsed_ms"])
    if not config["steps"] or len(config["steps"]) * repetitions > limits["max_requests"] or interval * (repetitions - 1) >= limits["max_elapsed_ms"]:
        raise ValueError("probe work exceeds the selected bounds")
    for step in config["steps"]:
        if set(step) - {"path", "status", "contains", "headers", "max_latency_ms", "json_equals", "method"}:
            raise ValueError("undeclared assertion")
        path = step["path"]
        if not path.startswith("/") or path.startswith("//") or "#" in path or any(ord(c) < 32 for c in path):
            raise ValueError("invalid contract path")
        _integer(step["status"], 100, 599)
        if step.get("method", "GET") not in {"GET", "HEAD", "OPTIONS"}:
            raise ValueError("probe method grants mutation authority")
        if "max_latency_ms" in step:
            _integer(step["max_latency_ms"], 1, limits["max_elapsed_ms"])
    return repetitions, interval


class _NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, *_args, **_kwargs):
        return None


def _pointer(value, pointer):
    if pointer == "":
        return value
    if not pointer.startswith("/"):
        raise ValueError("invalid observation pointer")
    for component in pointer[1:].split("/"):
        if re.search(r"~(?![01])", component):
            raise ValueError("invalid observation pointer escape")
        key = component.replace("~1", "/").replace("~0", "~")
        if isinstance(value, list):
            if not re.fullmatch(r"0|[1-9][0-9]*", key):
                raise ValueError("invalid observation array index")
            value = value[int(key)]
        else:
            value = value[key]
    return value


def _expected(value, config):
    if isinstance(value, dict) and value.get("source") == "target":
        if set(value) != {"source", "path"}:
            raise ValueError("invalid target selection")
        return _pointer(config["target"], value["path"])
    return value


def _json_equal(actual, expected):
    """Compare JSON values without Python's boolean/number coercion."""
    if type(actual) in (int, float) and type(expected) in (int, float):
        return actual == expected
    if type(actual) is not type(expected):
        return False
    if isinstance(actual, dict):
        return actual.keys() == expected.keys() and all(_json_equal(value, expected[key]) for key, value in actual.items())
    if isinstance(actual, list):
        return len(actual) == len(expected) and all(_json_equal(a, b) for a, b in zip(actual, expected))
    return actual == expected


def _body(response, limit, deadline):
    parts = []
    count = 0
    while count <= limit:
        if time.monotonic() >= deadline:
            raise ValueError("observation window expired")
        chunk = response.read1(min(4096, limit + 1 - count))
        if not chunk:
            break
        parts.append(chunk)
        count += len(chunk)
    return b"".join(parts)


def run(config):
    """Return observations only; the server qualifies publication and plan scope."""
    repetitions, interval = _validate(config)
    limits = config["limits"]
    started = time.monotonic()
    outcome = "passed"
    requests = 0
    response_bytes = 0
    # Ambient proxy settings cannot silently grant a different network destination.
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}), _NoRedirect())
    deadline = started + limits["max_elapsed_ms"] / 1000
    for repetition in range(repetitions):
        if repetition:
            time.sleep(min(interval / 1000, max(0, deadline - time.monotonic())))
        for step in config["steps"]:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                if outcome != "failed":
                    outcome = "indeterminate"
                break
            requested_at = time.monotonic()
            requests += 1
            try:
                request = urllib.request.Request(config["base_url"].rstrip("/") + step["path"], method=step.get("method", "GET"))
                try:
                    response = opener.open(request, timeout=min(remaining, limits["timeout_ms"] / 1000))
                except urllib.error.HTTPError as response_error:
                    response = response_error
                with response:
                    body = _body(response, limits["max_response_bytes"], deadline)
                    response_bytes += len(body)
                    if len(body) > limits["max_response_bytes"] or time.monotonic() > deadline:
                        raise ValueError("response exceeds observation bounds")
                    passed = response.status == step["status"] and all(text.encode("utf-8") in body for text in step.get("contains", []))
                    passed &= all(response.headers.get(name) == value for name, value in step.get("headers", {}).items())
                    if "max_latency_ms" in step:
                        passed &= (time.monotonic() - requested_at) * 1000 <= step["max_latency_ms"]
                    if "json_equals" in step:
                        try:
                            data = json.loads(body)
                            passed &= all(_json_equal(_pointer(data, pointer), _expected(value, config)) for pointer, value in step["json_equals"].items())
                        except (ValueError, KeyError, IndexError, TypeError):
                            passed = False
                    if not passed:
                        outcome = "failed"
            except (OSError, ValueError, http.client.HTTPException):
                if outcome != "failed":
                    outcome = "indeterminate"
    return {
        "plan_digest": config["plan_digest"],
        "input_digest": config["input_digest"],
        "check": config["check"],
        "outcome": outcome,
        "measurements": [
            {"name": "requests", "value": requests},
            {"name": "response-bytes", "value": response_bytes},
            {"name": "elapsed-ms", "value": int((time.monotonic() - started) * 1000)},
        ],
    }


def main():
    """Read bounded protected JSON on stdin; stdout is the declared report artifact."""
    try:
        source = sys.stdin.buffer.read(1048577)
        if len(source) > 1048576:
            raise ValueError("oversized configuration")
        report = run(json.loads(source))
    except (ValueError, KeyError, TypeError, AttributeError):
        print("invalid verification probe configuration", file=sys.stderr)
        return 2
    print(json.dumps(report, separators=(",", ":"), sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
