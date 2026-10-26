#!/usr/bin/env python3
"""Hermetic installed provider: read checks plus the shared Source protocol peer."""
import json
import pathlib
import subprocess
import sys

raw = sys.stdin.readline()
request = json.loads(raw)
message = request["message"]
if message["command"]["operation"] != "check":
    subprocess.run([sys.executable, "source-provider.py"], input=raw, text=True, check=True)
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
