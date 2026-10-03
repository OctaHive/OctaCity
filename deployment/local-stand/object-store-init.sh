#!/bin/sh
set -eu

readonly MAX_SECRET_BYTES=4096
readonly PROBE_SOURCE="local-stand/readiness/source"
readonly PROBE_COPY="local-stand/readiness/copy"

fail() {
  printf '%s\n' "object-store initialization failed: $1" >&2
  exit 1
}

read_secret() {
  path="$1"
  label="$2"
  [ -f "$path" ] || fail "$label file is absent or not regular"
  [ ! -L "$path" ] || fail "$label file must not be symbolic"
  size="$(wc -c < "$path")"
  [ "$size" -gt 0 ] && [ "$size" -le "$MAX_SECRET_BYTES" ] \
    || fail "$label file has an invalid size"
  cat "$path"
}

endpoint="${OCTACITY_OBJECT_ENDPOINT:-}"
bucket="${OCTACITY_OBJECT_BUCKET:-}"
operation_timeout="${OCTACITY_OBJECT_OPERATION_TIMEOUT_SECONDS:-}"
[ "$endpoint" = "http://minio:9000" ] || fail "object endpoint is not the private MinIO service"
case "$bucket" in
  ''|*[!a-z0-9.-]*|.*|*.) fail "object bucket has an invalid DNS-compatible name" ;;
  *..*) fail "object bucket must not contain adjacent periods" ;;
esac
[ "${#bucket}" -ge 3 ] && [ "${#bucket}" -le 63 ] \
  || fail "object bucket length is outside 3..63 characters"
case "$operation_timeout" in
  ''|*[!0-9]*) fail "operation timeout is not an integer" ;;
esac
[ "$operation_timeout" -ge 1 ] && [ "$operation_timeout" -le 60 ] \
  || fail "operation timeout is outside 1..60 seconds"

access_key="$(read_secret /run/secrets/object-access-key "object access key")"
secret_key="$(read_secret /run/secrets/object-secret-key "object secret key")"
printf '%s' "$access_key" | grep -Eq '^OCTA[0-9A-F]{24}$' \
  || fail "object access key has an invalid format"
printf '%s' "$secret_key" | grep -Eq '^[A-Za-z0-9_-]{43}$' \
  || fail "object secret key has an invalid format"

export MC_CONFIG_DIR=/tmp/.mc
export MC_HOST_octacity="http://${access_key}:${secret_key}@minio:9000"

run_mc() {
  timeout -s TERM "$operation_timeout" mc --quiet --no-color "$@"
}

cleanup() {
  run_mc rm "octacity/${bucket}/${PROBE_COPY}" >/dev/null 2>&1 || true
  run_mc rm "octacity/${bucket}/${PROBE_SOURCE}" >/dev/null 2>&1 || true
}
trap cleanup EXIT HUP INT TERM

run_mc mb --ignore-existing "octacity/${bucket}" >/dev/null
printf '%s\n' 'octacity-object-lifecycle-probe' > /tmp/probe-source
run_mc pipe "octacity/${bucket}/${PROBE_SOURCE}" < /tmp/probe-source >/dev/null
run_mc cat "octacity/${bucket}/${PROBE_SOURCE}" > /tmp/probe-read
cmp /tmp/probe-source /tmp/probe-read \
  || fail "PUT/GET lifecycle probe returned different bytes"
run_mc cp \
  "octacity/${bucket}/${PROBE_SOURCE}" \
  "octacity/${bucket}/${PROBE_COPY}" >/dev/null
run_mc cat "octacity/${bucket}/${PROBE_COPY}" > /tmp/probe-read
cmp /tmp/probe-source /tmp/probe-read \
  || fail "COPY/GET lifecycle probe returned different bytes"
run_mc rm "octacity/${bucket}/${PROBE_COPY}" >/dev/null
run_mc rm "octacity/${bucket}/${PROBE_SOURCE}" >/dev/null
if run_mc stat "octacity/${bucket}/${PROBE_COPY}" >/dev/null 2>&1 \
  || run_mc stat "octacity/${bucket}/${PROBE_SOURCE}" >/dev/null 2>&1; then
  fail "DELETE lifecycle probe left an object readable"
fi

printf '%s\n' 'object store bucket and lifecycle are ready'
