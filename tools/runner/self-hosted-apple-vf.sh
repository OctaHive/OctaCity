#!/usr/bin/env bash
set -euo pipefail

WORKSPACE_BYTES=1073741824
CACHE_BYTES=67108864
APPLE_CONTAINER_MINIMUM=0.6.0
APPLE_CONTAINER=${OCTACITY_APPLE_CONTAINER_EXECUTABLE:-/usr/local/bin/container}

fail() {
  echo "Apple VF setup: $*" >&2
  exit 1
}

require_command() {
  command -v "$1" >/dev/null 2>&1 || fail "required command is missing: $1"
}

hosted_root() {
  : "${RUNNER_TEMP:?RUNNER_TEMP is required}"
  printf '%s/octacity-self-hosted-apple-vf\n' "$RUNNER_TEMP"
}

validated_root() {
  local root runner_temp_real root_real marker
  root=$(hosted_root)
  [[ -d $root && ! -L $root ]] || fail "staging root is absent or symbolic: $root"
  runner_temp_real=$(realpath "$RUNNER_TEMP")
  root_real=$(realpath "$root")
  [[ $(dirname "$root_real") == "$runner_temp_real" && $(basename "$root_real") == octacity-self-hosted-apple-vf ]] \
    || fail "refusing a staging root outside RUNNER_TEMP: $root_real"
  marker=$root_real/.octacity-apple-vf
  [[ -f $marker && ! -L $marker && $(<"$marker") == apple-vf-isolation ]] \
    || fail "staging root has no valid ownership marker: $root_real"
  printf '%s\n' "$root_real"
}

write_environment() {
  : "${GITHUB_ENV:?GITHUB_ENV is required}"
  printf '%s\n' "$@" >>"$GITHUB_ENV"
}

verify_download() {
  local archive=$1 checksum=$2
  python3 - "$archive" "$checksum" <<'PY'
import hashlib
import pathlib
import sys

archive = pathlib.Path(sys.argv[1])
expected = pathlib.Path(sys.argv[2]).read_text(encoding="utf-8").split()[0].lower()
actual = hashlib.sha256(archive.read_bytes()).hexdigest()
if actual != expected:
    raise SystemExit(f"checksum mismatch for {archive}: expected {expected}, got {actual}")
PY
}

stage_octa_release() {
  local root=$1 version=$2 revision=$3
  local download=$root/download release=$root/octa-release asset checksum base_url
  asset=octa-Linux-arm64.tar.gz
  checksum=${asset}.sha256
  base_url=https://github.com/OctaHive/octa/releases/download/v${version}
  mkdir -p "$download" "$release"
  curl --fail --location --retry 5 --retry-all-errors --connect-timeout 30 \
    --output "$download/$asset" "$base_url/$asset"
  curl --fail --location --retry 5 --retry-all-errors --connect-timeout 30 \
    --output "$download/$checksum" "$base_url/$checksum"
  verify_download "$download/$asset" "$download/$checksum"
  tar --extract --gzip --file "$download/$asset" --directory "$release"
  python3 "$repo_root/tools/validate_octa_release_contract.py" "$release/octa-release-contract.json"
  python3 - "$release" "$version" "$revision" <<'PY'
import hashlib
import json
import pathlib
import sys

root = pathlib.Path(sys.argv[1])
for line in (root / "SHA256SUMS").read_text(encoding="utf-8").splitlines():
    digest, name = line.split(maxsplit=1)
    path = root / name.lstrip("* ")
    if hashlib.sha256(path.read_bytes()).hexdigest() != digest:
        raise SystemExit(f"release checksum mismatch: {path}")
capabilities = json.loads((root / "octa-runner-capabilities.json").read_text(encoding="utf-8"))
expected = {"octa_version": sys.argv[2], "build_commit": sys.argv[3], "platform": "linux-aarch64"}
actual = {key: capabilities.get(key) for key in expected}
if actual != expected:
    raise SystemExit(f"unexpected Octa release metadata: expected {expected}, got {actual}")
PY
  [[ -x $release/octa-runner ]] || fail "released Linux ARM64 runner is not executable"
}

attach_volume() {
  local root=$1 name=$2 bytes=$3 image=$root/$name.sparseimage mount=$root/$name mebibytes
  mebibytes=$((bytes / 1048576))
  mkdir -p "$mount"
  hdiutil create -quiet -size "${mebibytes}m" -fs APFS -volname "octacity-$name" -type SPARSE "$image"
  hdiutil attach -quiet -nobrowse -mountpoint "$mount" "$image"
  chmod 0700 "$mount"
}

setup() {
  local version=$1 revision=$2 root version_json installed_version
  [[ $(uname -s) == Darwin && $(uname -m) == arm64 ]] || fail "Apple VF isolation requires Apple Silicon macOS"
  [[ $version =~ ^[0-9]+\.[0-9]+\.[0-9]+([-+][0-9A-Za-z.-]+)?$ ]] || fail "a valid Octa version is required"
  [[ $revision =~ ^[0-9a-f]{40}$ ]] || fail "a valid Octa revision is required"
  for command in curl hdiutil python3 realpath tar; do
    require_command "$command"
  done
  [[ -x $APPLE_CONTAINER ]] || fail "Apple container executable is unavailable: $APPLE_CONTAINER"
  installed_version=$($APPLE_CONTAINER --version | python3 -c 'import re,sys; match=re.search(r"[0-9]+\.[0-9]+\.[0-9]+", sys.stdin.read()); print(match.group(0) if match else "")')
  [[ -n $installed_version ]] || fail "could not parse Apple container version"
  python3 - "$installed_version" "$APPLE_CONTAINER_MINIMUM" <<'PY'
import sys

def version(value):
    return tuple(int(part) for part in value.split(".")[:3])

if version(sys.argv[1]) < version(sys.argv[2]):
    raise SystemExit(f"Apple container {sys.argv[1]} is older than required {sys.argv[2]}")
PY
  version_json=$($APPLE_CONTAINER system version --format json)
  [[ $(python3 -c 'import json,sys; print(len(json.load(sys.stdin)))' <<<"$version_json") -ge 2 ]] \
    || fail "Apple container API service is not running"

  repo_root=$(git rev-parse --show-toplevel 2>/dev/null) || fail "run from the OctaCity checkout"
  root=$(hosted_root)
  [[ ! -e $root ]] || fail "staging root already exists: $root"
  mkdir -p "$root/state"
  printf '%s\n' apple-vf-isolation >"$root/.octacity-apple-vf"
  chmod 0600 "$root/.octacity-apple-vf"
  stage_octa_release "$root" "$version" "$revision"
  attach_volume "$root" work "$WORKSPACE_BYTES"
  attach_volume "$root" cache "$CACHE_BYTES"
  write_environment \
    "OCTACITY_CONTRACT_OCTA_RELEASE_ROOT=$root/octa-release" \
    "OCTACITY_CONTRACT_WORKSPACE_BYTES=$WORKSPACE_BYTES" \
    "OCTACITY_CONTRACT_APPLE_VF_EXECUTABLE=$APPLE_CONTAINER" \
    "OCTACITY_CONTRACT_APPLE_VF_ENVIRONMENT_IDENTITY=apple-vf-release-v1" \
    "OCTACITY_CONTRACT_APPLE_VF_WORK_ROOT=$root/work" \
    "OCTACITY_CONTRACT_APPLE_VF_CACHE_ROOT=$root/cache" \
    "OCTACITY_CONTRACT_APPLE_VF_STATE_ROOT=$root/state" \
    "OCTACITY_CONTRACT_APPLE_VF_IMAGE=quay.io/fedora/fedora@sha256:83205934094144b56f645f86c42b84f81083f423b0bea9cb233f91c21bab0919"
}

verify_clean() {
  local root
  root=$(validated_root)
  [[ -z $(find "$root/state/apple-vf-isolation" -type f -name '*.owner' -print -quit 2>/dev/null) ]] \
    || fail "Apple VF cleanup markers remain after the contract"
  [[ -z $(find "$root/work" -mindepth 1 -maxdepth 1 -print -quit) ]] \
    || fail "Apple VF workspaces remain after the contract"
}

cleanup() {
  local root
  [[ -e $(hosted_root) ]] || return 0
  root=$(validated_root)
  for mount in "$root/cache" "$root/work"; do
    hdiutil detach -quiet "$mount" 2>/dev/null || hdiutil detach -quiet -force "$mount" 2>/dev/null || true
  done
  rm -rf -- "$root"
}

case "${1:-}" in
  setup) shift; setup "$@" ;;
  verify-clean) verify_clean ;;
  cleanup) cleanup ;;
  *) fail "usage: $0 {setup <octa-version> <octa-revision>|verify-clean|cleanup}" ;;
esac
