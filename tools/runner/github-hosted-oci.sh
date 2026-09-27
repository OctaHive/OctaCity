#!/usr/bin/env bash
set -euo pipefail

MICROSANDBOX_VERSION=0.6.18
CONTAINERD_VERSION=2.3.6
WORKSPACE_BYTES=1073741824

fail() {
  echo "github-hosted OCI setup: $*" >&2
  exit 1
}

require_command() {
  command -v "$1" >/dev/null 2>&1 || fail "required command is missing: $1"
}

hosted_root() {
  local backend=$1
  : "${RUNNER_TEMP:?RUNNER_TEMP is required}"
  case "$backend" in
    containerd|microsandbox) printf '%s/octacity-hosted-%s\n' "$RUNNER_TEMP" "$backend" ;;
    *) fail "unsupported OCI backend: $backend" ;;
  esac
}

mark_root() {
  local backend=$1 root=$2
  printf '%s\n' "$backend" >"$root/.octacity-hosted-oci"
  chmod 0600 "$root/.octacity-hosted-oci"
}

validated_root() {
  local backend=$1 root expected_name runner_temp_real root_real marker
  root=$(hosted_root "$backend")
  [[ -d $root && ! -L $root ]] || fail "staging root is absent or symbolic: $root"
  runner_temp_real=$(realpath -e "$RUNNER_TEMP")
  root_real=$(realpath -e "$root")
  expected_name="octacity-hosted-$backend"
  [[ $(dirname "$root_real") == "$runner_temp_real" && $(basename "$root_real") == "$expected_name" ]] \
    || fail "refusing an OCI root outside the runner temporary directory: $root_real"
  marker=$root_real/.octacity-hosted-oci
  [[ -f $marker && ! -L $marker && $(<"$marker") == "$backend" ]] \
    || fail "staging root has no valid ownership marker: $root_real"
  printf '%s\n' "$root_real"
}

write_environment() {
  : "${GITHUB_ENV:?GITHUB_ENV is required}"
  printf '%s\n' "$@" >>"$GITHUB_ENV"
}

provision_workspace() {
  local root=$1 mount=$root/work
  mkdir -p "$mount"
  sudo mount --types tmpfs --options "size=$WORKSPACE_BYTES,nosuid,nodev" octacity-oci-work "$mount"
  sudo chown "$(id -u):$(id -g)" "$mount"
  chmod 0700 "$mount"
}

stage_release() {
  local root=$1 version=$2 revision=$3 asset_arch runtime_platform
  repo_root=$(git rev-parse --show-toplevel 2>/dev/null) || fail "run from the OctaCity checkout"
  # shellcheck source=github-hosted-release.sh
  source "$repo_root/tools/runner/github-hosted-release.sh"
  read -r asset_arch runtime_platform < <(octacity_hosted_platform_metadata)
  octacity_stage_octa_release "$root" "$version" "$revision" "$asset_arch" "$runtime_platform"
}

wait_for_containerd() {
  local ctr=$1 socket=$2 log=$3
  for _ in {1..60}; do
    if sudo "$ctr" --address "$socket" version >/dev/null 2>&1; then
      sudo chown "$(id -u):$(id -g)" "$socket"
      "$ctr" --address "$socket" version >/dev/null 2>&1 && return 0
    fi
    sleep 1
  done
  [[ ! -f $log ]] || cat "$log" >&2
  fail "containerd did not become ready"
}

containerd_asset() {
  case "$(uname -m)" in
    x86_64) printf '%s %s\n' containerd-2.3.6-linux-amd64.tar.gz df806bb9b86653ebc94822a7076642ff3c3614324dbc1b511bdf75ac1669b762 ;;
    aarch64|arm64) printf '%s %s\n' containerd-2.3.6-linux-arm64.tar.gz 6b3d383cc051f0bd7b2303f9034fdc9029f04c237cadb23b5e890a5b68534e05 ;;
    *) fail "unsupported Linux architecture: $(uname -m)" ;;
  esac
}

oci_image() {
  case "$(uname -m)" in
    x86_64) printf '%s\n' quay.io/fedora/fedora@sha256:63773f454664cd77e239f8e0b13ae7f18effe9e3d6612a325b5646eb3bda11f1 ;;
    aarch64|arm64) printf '%s\n' quay.io/fedora/fedora@sha256:83205934094144b56f645f86c42b84f81083f423b0bea9cb233f91c21bab0919 ;;
    *) fail "unsupported Linux architecture: $(uname -m)" ;;
  esac
}

setup_containerd() {
  local root=$1 socket=$root/containerd.sock namespace log=$root/containerd.log
  local asset digest archive=$root/containerd.tar.gz runtime=$root/runtime containerd ctr image
  for command in curl mountpoint python3 runc sha256sum sudo tar; do
    require_command "$command"
  done
  read -r asset digest < <(containerd_asset)
  curl --fail --location --retry 5 --retry-all-errors --connect-timeout 30 \
    --output "$archive" \
    "https://github.com/containerd/containerd/releases/download/v${CONTAINERD_VERSION}/${asset}"
  printf '%s  %s\n' "$digest" "$archive" | sha256sum --check --strict
  mkdir -p "$runtime"
  tar --extract --gzip --file "$archive" --directory "$runtime" --no-same-owner --no-same-permissions
  containerd=$runtime/bin/containerd
  ctr=$runtime/bin/ctr
  [[ -x $containerd && -x $ctr ]] || fail "containerd release is incomplete"
  "$containerd" --version | grep -F " v${CONTAINERD_VERSION} " >/dev/null \
    || fail "unexpected containerd version"
  provision_workspace "$root"
  mkdir -p "$root/containerd-root" "$root/containerd-state" "$root/agent-state"
  namespace="octacity-${GITHUB_RUN_ID:-local}-${GITHUB_RUN_ATTEMPT:-1}"
  [[ $namespace =~ ^[A-Za-z0-9_.-]+$ ]] || fail "derived containerd namespace is invalid"
  sudo sh -c 'nohup "$1" --address "$2" --root "$3" --state "$4" >"$5" 2>&1 & echo $! >"$6"' sh \
    "$containerd" "$socket" "$root/containerd-root" "$root/containerd-state" "$log" "$root/containerd.pid"
  wait_for_containerd "$ctr" "$socket" "$log"
  "$ctr" --address "$socket" plugins list \
    | awk '$1 == "io.containerd.grpc.v1" && $2 == "transfer" && $4 == "ok" { found = 1 } END { exit !found }' \
    || fail "containerd Transfer plugin is unavailable"
  image=$(oci_image)
  write_environment \
    "OCTACITY_CONTRACT_OCTA_RELEASE_ROOT=$root/octa-release" \
    "OCTACITY_CONTRACT_WORKSPACE_BYTES=$WORKSPACE_BYTES" \
    "OCTACITY_CONTRACT_CONTAINERD_ENDPOINT=$socket" \
    "OCTACITY_CONTRACT_CONTAINERD_NAMESPACE=$namespace" \
    "OCTACITY_CONTRACT_CONTAINERD_SNAPSHOTTER=overlayfs" \
    "OCTACITY_CONTRACT_CONTAINERD_RUNTIME=io.containerd.runc.v2" \
    "OCTACITY_CONTRACT_CONTAINERD_WORK_ROOT=$root/work" \
    "OCTACITY_CONTRACT_CONTAINERD_STATE_ROOT=$root/agent-state" \
    "OCTACITY_CONTRACT_CONTAINERD_IMAGE=$image"
}

microsandbox_asset() {
  case "$(uname -m)" in
    x86_64) printf '%s %s\n' microsandbox-linux-x86_64.tar.gz b001b3c6b980ab1ffcceb817496648c1520dba36b9e0caac37ea8d2f4acd9bdd ;;
    aarch64|arm64) printf '%s %s\n' microsandbox-linux-aarch64.tar.gz e53098e7601fddd85af7e943d4af3d4370ace276d9e863a89456338f2d076d1b ;;
    *) fail "unsupported Linux architecture: $(uname -m)" ;;
  esac
}

setup_microsandbox() {
  local root=$1 asset digest archive=$root/microsandbox.tar.gz runtime=$root/runtime image
  for command in curl find mountpoint python3 sha256sum sudo tar; do
    require_command "$command"
  done
  [[ -c /dev/kvm ]] || fail "GitHub-hosted runner does not expose /dev/kvm"
  sudo chown "$(id -u):$(id -g)" /dev/kvm
  [[ -r /dev/kvm && -w /dev/kvm ]] || fail "runner user cannot access /dev/kvm"
  read -r asset digest < <(microsandbox_asset)
  curl --fail --location --retry 5 --retry-all-errors --connect-timeout 30 \
    --output "$archive" \
    "https://github.com/superradcompany/microsandbox/releases/download/v${MICROSANDBOX_VERSION}/${asset}"
  printf '%s  %s\n' "$digest" "$archive" | sha256sum --check --strict
  mkdir -p "$runtime" "$root/work" "$root/state"
  tar --extract --gzip --file "$archive" --directory "$runtime" --no-same-owner --no-same-permissions
  chmod 0755 "$runtime/msb"
  "$runtime/msb" --version | grep -Fx "msb $MICROSANDBOX_VERSION" >/dev/null \
    || fail "unexpected Microsandbox version"
  "$runtime/msb" doctor
  local firmware
  firmware=$(find "$runtime" -maxdepth 1 -type f -name 'libkrunfw.so.*' -print -quit)
  [[ -n $firmware ]] || fail "Microsandbox bundle contains no libkrun firmware"
  image=$(oci_image)
  write_environment \
    "OCTACITY_CONTRACT_OCTA_RELEASE_ROOT=$root/octa-release" \
    "OCTACITY_CONTRACT_WORKSPACE_BYTES=$WORKSPACE_BYTES" \
    "OCTACITY_CONTRACT_MICROSANDBOX_WORK_ROOT=$root/work" \
    "OCTACITY_CONTRACT_MICROSANDBOX_STATE_ROOT=$root/state" \
    "OCTACITY_CONTRACT_MICROSANDBOX_EXECUTABLE=$runtime/msb" \
    "OCTACITY_CONTRACT_MICROSANDBOX_LIBKRUNFW=$firmware" \
    "OCTACITY_CONTRACT_MICROSANDBOX_IMAGE=$image" \
    "OCTACITY_CONTRACT_MICROSANDBOX_ALLOWED_HOST=example.com" \
    "OCTACITY_CONTRACT_MICROSANDBOX_DENIED_HOST=www.cloudflare.com"
}

verify_clean() {
  local backend=$1 root
  root=$(validated_root "$backend")
  case "$backend" in
    containerd)
      local socket=$root/containerd.sock namespace=${OCTACITY_CONTRACT_CONTAINERD_NAMESPACE:-} ctr=$root/runtime/bin/ctr
      [[ -n $namespace ]] || fail "containerd namespace is unavailable"
      [[ -x $ctr ]] || fail "staged ctr executable is unavailable"
      [[ -z $("$ctr" --address "$socket" --namespace "$namespace" tasks list --quiet) ]] \
        || fail "containerd tasks remain after the contract"
      [[ -z $("$ctr" --address "$socket" --namespace "$namespace" containers list --quiet) ]] \
        || fail "containerd containers remain after the contract"
      ;;
    microsandbox)
      [[ -d $root/work ]] || fail "Microsandbox work root is missing"
      ;;
  esac
  [[ -d $root/work && ! -L $root/work ]] || fail "$backend work root is absent or symbolic"
  [[ -z $(find "$root/work" -mindepth 1 -maxdepth 1 -print -quit) ]] \
    || fail "$backend workspaces remain after the contract"
}

cleanup() {
  local backend=$1 root pid executable containerd_executable
  [[ -e $(hosted_root "$backend") ]] || return 0
  root=$(validated_root "$backend")
  if [[ -f $root/containerd.pid ]]; then
    [[ ! -L $root/containerd.pid && $(stat -c %u "$root/containerd.pid") == 0 ]] \
      || fail "containerd pid file is not root-owned"
    pid=$(sudo cat "$root/containerd.pid")
    [[ $pid =~ ^[1-9][0-9]*$ ]] || fail "invalid containerd pid file"
    containerd_executable=$(realpath -e "$root/runtime/bin/containerd")
    executable=$(sudo readlink -f "/proc/$pid/exe" 2>/dev/null || true)
    [[ -z $executable || $executable == "$containerd_executable" ]] \
      || fail "pid file does not identify the staged containerd process"
    sudo kill "$pid" 2>/dev/null || true
    for _ in {1..30}; do
      sudo kill -0 "$pid" 2>/dev/null || break
      sleep 1
    done
    sudo kill -KILL "$pid" 2>/dev/null || true
  fi
  if mountpoint --quiet "$root/work"; then
    sudo umount "$root/work"
  fi
  [[ ! -e $root ]] || sudo rm -rf -- "$root"
}

setup() {
  local backend=$1 version=$2 revision=$3 root
  [[ $(uname -s) == Linux ]] || fail "hosted OCI setup requires Linux"
  [[ $version =~ ^[0-9]+\.[0-9]+\.[0-9]+([-+][0-9A-Za-z.-]+)?$ ]] || fail "a valid Octa version is required"
  [[ $revision =~ ^[0-9a-f]{40}$ ]] || fail "a valid Octa revision is required"
  root=$(hosted_root "$backend")
  [[ ! -e $root ]] || fail "staging root already exists: $root"
  mkdir -p "$root"
  mark_root "$backend" "$root"
  stage_release "$root" "$version" "$revision"
  case "$backend" in
    containerd) setup_containerd "$root" ;;
    microsandbox) setup_microsandbox "$root" ;;
  esac
}

case "${1:-}" in
  setup)
    shift
    setup "$@"
    ;;
  verify-clean)
    shift
    verify_clean "$@"
    ;;
  cleanup)
    shift
    cleanup "$@"
    ;;
  *)
    echo "usage: $0 setup BACKEND OCTA_VERSION OCTA_REVISION | verify-clean BACKEND | cleanup BACKEND" >&2
    exit 2
    ;;
esac
