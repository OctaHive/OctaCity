#!/usr/bin/env python3
"""Hermetic installed provider: read checks plus the shared Source protocol peer."""
import json
import pathlib
import runpy
import sys


class RequestInput:
    """Replay the inspected frame while preserving the host pipe for cancellation or EOF."""
    def __init__(self, first, remaining):
        self.first = first
        self.remaining = remaining

    def readline(self):
        if self.first is not None:
            first, self.first = self.first, None
            return first
        return self.remaining.readline()

raw = sys.stdin.readline()
request = json.loads(raw)
message = request["message"]
if message["command"]["operation"] != "check":
    sys.stdin = RequestInput(raw, sys.stdin)
    runpy.run_path("source-provider.py", run_name="__main__")
    sys.exit(0)

pathlib.Path("captured.json").write_text(raw, encoding="utf-8")
with open("calls", "a", encoding="utf-8") as calls:
    calls.write("called\n")
intent = message["command"]["payload"]
outcome = pathlib.Path("outcome.json").read_text(encoding="utf-8")
outcome = outcome.replace("CHECK_ID", intent["check_id"]).replace("PROFILE", intent["configuration_digest"])
print(json.dumps({"family": "source", "message": {
    "protocol_version": 1, "request_id": message["request_id"], "outcome": json.loads(outcome)
}}))
