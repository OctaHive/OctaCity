#!/bin/sh
# Finite model peer for the manually authored acceptance test, never a resolver.
set -eu
if [ "$#" = 1 ] && [ "$1" = --version ]; then
  printf '%s\n' 'codex-cli 0.161.0'
  exit 0
fi
fail() { printf '%s\n' 'manual model fixture rejected its input contract' >&2; exit 64; }
if [ "$#" = 9 ]; then
  [ "$7" = --ignore-user-config ] && [ "$8" = '--config=mcp_servers={}' ] && [ "$9" = - ] || fail
elif [ "$#" = 7 ]; then
  [ "$7" = - ] || fail
else
  fail
fi
[ "$5" = --model=finite-model ] || fail
if [ "$1" = --ask-for-approval=never ]; then
  [ "$2" = exec ] && [ "$3" = --json ] && [ "$4" = --sandbox=workspace-write ] || fail
else
  [ "$1" = exec ] && [ "$2" = --json ] && [ "$3" = --sandbox=workspace-write ] \
    && [ "$4" = --ask-for-approval=never ] || fail
fi
case "$6" in --output-schema=*) schema=${6#--output-schema=} ;; *) fail ;; esac
[ -f "$schema" ] && [ ! -L "$schema" ] || fail
prompt=$(head -c 1048577)
[ "$(printf %s "$prompt" | wc -c)" -le 1048576 ] || fail
contains() { printf %s "$prompt" | grep -Fq -- "$1"; }
contains 'ticket-request-canary' && contains 'process-criteria-canary' || fail
! contains 'first-private-investigation-canary' || fail
# Source access is explicit per assignment. The first worker has no checkout;
# the second can read exactly the selected, immutable code guide and no Git metadata.
if contains 'Read the selected code guide.'; then
  grep -Fq '"component"' "$schema" || fail
  [ -f /workspace/source/README.md ] && [ ! -L /workspace/source/README.md ] || fail
  grep -Fq 'code-guide-canary: console owns the operator interface' /workspace/source/README.md || fail
  [ ! -w /workspace/source/README.md ] || fail
  for path in /workspace/source/* /workspace/source/.[!.]* /workspace/source/..?*; do
    [ "$path" = /workspace/source/README.md ] || { [ ! -e "$path" ] && [ ! -L "$path" ]; } || fail
  done
else
  for path in /workspace/source/* /workspace/source/.[!.]* /workspace/source/..?*; do
    [ ! -e "$path" ] && [ ! -L "$path" ] || fail
  done
fi
printf '%s\n' '{"type":"turn.started"}'
if grep -Fq '"eligible"' "$schema"; then
  contains 'Assess Project fit.' && contains 'project-goals-canary' || fail
  ! contains 'component-guide-canary' && ! contains 'priority-guide-canary' || fail
  result='{"eligible":true,"summary":"eligible-public-summary","duplicate_search":"not_requested"}'
  printf '%s\n' '{"type":"item.completed","item":{"id":"item_0","type":"agent_message","text":"first-private-investigation-canary"}}'
elif grep -Fq '"component"' "$schema"; then
  contains 'Classify only the accepted eligible ticket' && contains 'eligible-public-summary' || fail
  contains 'component-guide-canary' && contains 'priority-guide-canary' || fail
  ! contains 'project-goals-canary' && ! contains 'duplicate_search' || fail
  result='{"component":"console","priority":"normal"}'
else
  fail
fi
printf '{"type":"turn.completed","result":%s,"thread_id":"manual-thread","turn_id":"manual-turn","usage":{"input_tokens":3,"cached_input_tokens":0,"output_tokens":5}}\n' "$result"
