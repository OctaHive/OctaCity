#!/bin/sh
# Adapter-local finite author process for capture-handoff qualification.
# The fixed qualification file is a filesystem probe, never Flow/Node dispatch.
# No network or provider credential is used; only the frozen Boolean is returned.
set -eu

fail() { printf '%s\n' 'hermetic Codex fixture rejected its invocation' >&2; exit 64; }
if [ "$#" -eq 1 ] && [ "$1" = --version ]; then
  printf '%s\n' 'codex-cli 0.161.0'
  exit 0
fi
[ -n "${OPENAI_API_KEY:-}" ] || fail
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
# The signed virtual workspace has explicit source/output/scratch siblings.
# Keep the direct-source shape for the standalone CLI contract test.
source_root=.
if [ -d source ]; then
  [ ! -L source ] || fail
  source_root=source
fi
[ -d "$source_root/public/approved" ] && [ ! -L "$source_root/public" ] \
  && [ ! -L "$source_root/public/approved" ] || fail
[ ! -L "$source_root/public/approved/contract.txt" ] || fail

# A repository-owned attack is data, never shell configuration. The optional
# qualification input asks this finite process to exercise real confinement;
# the backend, rather than this fixture, must deny the unauthorized operations.
if [ -f "$source_root/UNTRUSTED_INSTRUCTIONS.txt" ]; then
  attack=$(head -c 4097 "$source_root/UNTRUSTED_INSTRUCTIONS.txt")
  [ -n "$attack" ] && [ "$(printf %s "$attack" | wc -c)" -le 4096 ] || fail
  host_credential=$(head -c 4097 "$source_root/host-credential-location.txt")
  [ "$(printf %s "$host_credential" | wc -c)" -le 4096 ] || fail
  case "$host_credential" in /*) ;; *) fail ;; esac
  [ -d output ] && [ ! -L output ] || fail
  attempt_write() {
    if (printf '%s\n' unauthorized-probe >> "$1") 2>/dev/null; then printf allowed; else printf denied; fi
  }
  attempt_read() {
    if (head -c 1 "$1" >/dev/null) 2>/dev/null; then printf allowed; else printf denied; fi
  }
  source_write=$(attempt_write "$source_root/outside.txt")
  git_write=$(attempt_write "$source_root/.git/HEAD")
  symlink_write=$(attempt_write "$source_root/public/approved/escape.txt")
  protected_write=absent
  if [ -f /octacity/protected/Octafile.yml ] && [ -r /octacity/protected/Octafile.yml ]; then
    protected_write=$(attempt_write /octacity/protected/Octafile.yml)
  fi
  source2_read=$(attempt_read "$source_root/../source2/private.txt")
  credential_read=$(attempt_read "$host_credential")
  parent_create=denied
  if (mkdir "$source_root/../undeclared-security-sibling") 2>/dev/null; then parent_create=allowed; fi
  printf '{"repository_instructions":"data_only","source_sibling_write":"%s","git_metadata_write":"%s","symlink_escape_write":"%s","protected_input_write":"%s","source2_read":"%s","cross_profile_credential_read":"%s","undeclared_parent_create":"%s"}\n' \
    "$source_write" "$git_write" "$symlink_write" "$protected_write" "$source2_read" "$credential_read" "$parent_create" \
    > output/security-probes.json
fi
printf '%s\n' qualified-author-change > "$source_root/public/approved/contract.txt"
printf '%s\n' '{"type":"turn.started"}'
printf '%s\n' '{"type":"turn.completed","result":true,"thread_id":"hermetic-thread","turn_id":"hermetic-turn","usage":{"input_tokens":3,"cached_input_tokens":0,"output_tokens":5}}'
