#!/bin/sh
# Adapter-local deterministic executable for the minimal read-only pilot.
# The frozen Boolean schema is the supported contract; Flow and node names
# never select behavior. No network, provider credential, or repository write.
set -eu

fail() { printf '%s\n' 'hermetic Codex fixture rejected its invocation' >&2; exit 64; }
if [ "$#" -eq 1 ] && [ "$1" = --version ]; then
  printf '%s\n' 'codex-cli 0.161.0'
  exit 0
fi
[ "$#" -eq 7 ] || fail
[ "$1" = exec ] && [ "$2" = --json ] && [ "$3" = --sandbox=workspace-write ] \
  && [ "$4" = --ask-for-approval=never ] && [ "$7" = - ] || fail
case "$5" in --model=?*) ;; *) fail ;; esac
case "$6" in --output-schema=?*) schema=${6#--output-schema=} ;; *) fail ;; esac
[ -f "$schema" ] && [ ! -L "$schema" ] || fail
schema_bytes=$(head -c 4097 "$schema")
[ "$(printf %s "$schema_bytes" | wc -c)" -le 4096 ] || fail
[ "$(printf %s "$schema_bytes" | tr -d ' \t\r\n')" = '{"type":"boolean"}' ] || fail
prompt=$(head -c 1048577)
[ -n "$prompt" ] && [ "$(printf %s "$prompt" | wc -c)" -le 1048576 ] || fail
printf '%s\n' '{"type":"turn.started"}'
printf '%s\n' '{"type":"turn.completed","result":true,"thread_id":"hermetic-thread","turn_id":"hermetic-turn","usage":{"input_tokens":3,"cached_input_tokens":0,"output_tokens":5}}'
