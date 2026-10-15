# Hermetic Codex CLI fixture

`codex-cli.sh` is an operator-selected test executable for the minimal read-only
Factory profile. Its Codex wire format belongs to this adapter fixture. The
Factory coordinator continues to use generic typed results and usage.

It accepts the pinned plugin's normal `exec --json --sandbox=workspace-write
--ask-for-approval=never --model=… --output-schema=… -` invocation and reports
CLI version `0.161.0`. It reads the frozen Boolean output schema, consumes the
prompt from stdin, and returns `true` with deterministic reported token counts:
3 input, 0 cached input, and 5 output. Other schemas, extra arguments, absent
prompts, and prompts over 1 MiB fail before any event. Diagnostics omit prompts,
schema paths, and credential values. The fixture makes no network calls or
repository writes.

Select an explicit `TokenRates` policy for the exact installed model; this
fixture does not invent a monetary cost counter. Pin its actual SHA-256 in the
independent Agent executable inventory and the installed server profile.

Run its process contracts from the repository root:

```shell
python3 -m unittest discover -s tools/tests -p test_factory_codex_fixture.py
```

The Docker test runs an actual Linux process with a read-only filesystem, no
network, no capabilities, and an unprivileged UID. It proves the CLI boundary.
The current qualified Factory Agent routes are Containerd isolation and
Microsandbox virtualization; ordinary Native and Apple VF release scenarios
do not substitute for that qualification. The full released-product pilot
also requires current checksummed server, Agent, Octa, and provider installations,
the independent local Agent permission ceiling, protected input transfer,
and the remaining installed execution roles.
