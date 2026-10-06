#!/bin/sh
set -eu

if [ "${1-}" = "--version" ]; then
  printf '%s\n' 'codex-cli 0.130.0'
  exit 0
fi

prompt=$(cat)
[ -n "$prompt" ]
[ "${OCTA_CODEX_FIXTURE_PUBLIC-}" = "release-public-canary" ]
[ "${OCTA_CODEX_FIXTURE_SECRET-}" = "release-secret-canary-must-not-appear" ]
[ -z "${OCTA_CODEX_FIXTURE_UNMAPPED-}" ]

heartbeat() {
  marker=$1
  counter=0
  while :; do
    printf '%s\n' "$counter" > "$marker"
    counter=$((counter + 1))
    sleep 0.01
  done
}

case "${OCTA_CODEX_FIXTURE_MODE-}" in
  redaction)
    printf '%s\n' '{"type":"turn.started"}'
    printf '%s\n' '{"type":"item.completed","item":{"type":"agent_message","text":"credential=release-secret-canary-must-not-appear"}}'
    printf '%s\n' 'fixture diagnostic credential=release-secret-canary-must-not-appear' >&2
    printf '%s\n' '{"type":"turn.completed","result":{"outcome":"completed","files":1},"thread_id":"local-stand-fixture-thread","turn_id":"local-stand-fixture-turn","usage":{"input_tokens":3,"output_tokens":5}}'
    ;;
  overflow)
    mkdir -p .octa
    heartbeat .octa/codex-fixture-overflow-descendant-ready &
    while [ ! -f .octa/codex-fixture-overflow-descendant-ready ]; do sleep 0.01; done
    printf '%s\n' '{"type":"turn.started"}'
    printf '%s' '{"type":"future.additive","padding":"'
    dd if=/dev/zero bs=1048577 count=1 2>/dev/null | tr '\000' x
    printf '%s\n' '"}'
    ;;
  cancel)
    mkdir -p .octa
    heartbeat .octa/codex-fixture-descendant-ready &
    while [ ! -f .octa/codex-fixture-descendant-ready ]; do sleep 0.01; done
    printf '%s\n' '{"type":"turn.started"}'
    heartbeat .octa/codex-fixture-parent-heartbeat
    ;;
  *)
    printf '%s\n' 'fixture mode must be explicitly mapped' >&2
    exit 64
    ;;
esac
