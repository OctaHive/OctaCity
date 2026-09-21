#!/usr/bin/env bash
set -euo pipefail

mode=${1:-check}
[[ $mode == check || $mode == --write-only ]] || {
  echo "usage: $0 [--write-only]" >&2
  exit 2
}

fail() {
  echo "preflight: $*" >&2
  exit 1
}

require_command() {
  command -v "$1" >/dev/null 2>&1 || fail "required command is missing: $1"
}

require_file() {
  [[ -f $1 ]] || fail "required file is missing: $1"
}

require_directory() {
  [[ -d $1 ]] || fail "required directory is missing: $1"
}

verify_release() {
  local release_root=$1 checker
  require_file "$release_root/octa-release-contract.json"
  require_file "$release_root/SHA256SUMS"
  python3 "$repo_root/tools/validate_octa_release_contract.py" \
    "$release_root/octa-release-contract.json"
  if command -v sha256sum >/dev/null 2>&1; then
    checker=(sha256sum -c SHA256SUMS)
  else
    checker=(shasum -a 256 -c SHA256SUMS)
  fi
  (cd "$release_root" && "${checker[@]}")
}

write_runner_files() {
  local env_file=$runner_dir/.env path_file=$runner_dir/.path
  umask 077
  printf '%s\n' "${runner_environment[@]}" > "$env_file"
  printf '%s\n' "$runner_path" > "$path_file"
  echo "wrote non-secret runner environment: $env_file"
  echo "wrote runner PATH entries: $path_file"
}

repo_root=$(git rev-parse --show-toplevel 2>/dev/null) || fail "run from the OctaCity checkout"
# shellcheck source=backend-runner.defaults
source "$repo_root/tools/runner/backend-runner.defaults"
[[ $OCTACITY_RUNNER_WORKSPACE_BYTES =~ ^[1-9][0-9]*$ ]] \
  || fail "OCTACITY_RUNNER_WORKSPACE_BYTES must be a positive integer"
kernel=$(uname -s)
architecture=$(uname -m)
runner_environment=()
runner_path=

case "$kernel/$architecture" in
  Darwin/arm64)
    runner_root=${OCTACITY_RUNNER_ROOT:-$HOME/.octacity-runner}
    release_root=${OCTACITY_OCTA_RELEASE_ROOT:-$runner_root/$OCTACITY_RUNNER_OCTA_RELEASE_NAME}
    msb=${OCTACITY_MICROSANDBOX_EXECUTABLE:-$HOME/.microsandbox/bin/msb}
    libkrunfw=${OCTACITY_MICROSANDBOX_LIBKRUNFW:-$HOME/.microsandbox/lib/libkrunfw.5.dylib}
    work_root=${OCTACITY_MICROSANDBOX_WORK_ROOT:-$runner_root/w}
    state_root=${OCTACITY_MICROSANDBOX_STATE_ROOT:-$runner_root/s}
    test_name=microsandbox_backend_satisfies_the_real_runner_contract

    require_command cargo
    require_command docker
    require_command python3
    require_file "$runner_root/actions-runner/config.sh"
    require_file "$msb"
    require_file "$libkrunfw"
    require_directory "$work_root"
    require_directory "$state_root"
    docker info >/dev/null
    [[ $($msb --version) == "msb $OCTACITY_RUNNER_MICROSANDBOX_VERSION" ]] \
      || fail "Microsandbox $OCTACITY_RUNNER_MICROSANDBOX_VERSION is required"
    "$msb" doctor
    verify_release "$release_root"

    runner_environment=(
      "OCTACITY_CONTRACT_OCTA_RELEASE_ROOT=$release_root"
      "OCTACITY_CONTRACT_WORKSPACE_BYTES=$OCTACITY_RUNNER_WORKSPACE_BYTES"
      "OCTACITY_CONTRACT_MICROSANDBOX_WORK_ROOT=$work_root"
      "OCTACITY_CONTRACT_MICROSANDBOX_STATE_ROOT=$state_root"
      "OCTACITY_CONTRACT_MICROSANDBOX_EXECUTABLE=$msb"
      "OCTACITY_CONTRACT_MICROSANDBOX_LIBKRUNFW=$libkrunfw"
      "OCTACITY_CONTRACT_MICROSANDBOX_IMAGE=$OCTACITY_RUNNER_MICROSANDBOX_IMAGE"
      "OCTACITY_CONTRACT_MICROSANDBOX_ALLOWED_HOST=$OCTACITY_RUNNER_MICROSANDBOX_ALLOWED_HOST"
      "OCTACITY_CONTRACT_MICROSANDBOX_DENIED_HOST=$OCTACITY_RUNNER_MICROSANDBOX_DENIED_HOST"
    )
    runner_path="$HOME/.cargo/bin:$HOME/.microsandbox/bin:/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin"
    export "${runner_environment[@]}"
    export CARGO_TARGET_DIR="$runner_root/preflight-target"
    ;;
  Linux/aarch64|Linux/arm64)
    runner_root=${OCTACITY_RUNNER_ROOT:-/opt/octacity-runner}
    release_root=${OCTACITY_OCTA_RELEASE_ROOT:-$runner_root/$OCTACITY_RUNNER_OCTA_RELEASE_NAME}
    cgroup_root=$OCTACITY_RUNNER_NATIVE_CGROUP_ROOT
    work_root=$OCTACITY_RUNNER_NATIVE_WORK_ROOT
    cache_root=$OCTACITY_RUNNER_NATIVE_CACHE_ROOT
    test_name=native_backend_satisfies_the_real_runner_contract

    if ! grep -Fxq '0::/octacity/runner' /proc/self/cgroup; then
      require_file /usr/local/sbin/octacity-in-cgroup
      echo "restarting preflight inside the delegated runner cgroup"
      exec sudo /usr/local/sbin/octacity-in-cgroup "$USER" "$0" "$@"
    fi
    require_command bwrap
    require_command cargo
    require_command docker
    require_command python3
    require_file "$runner_root/actions-runner/config.sh"
    require_directory "$cgroup_root"
    require_directory "$work_root"
    require_directory "$cache_root"
    docker info >/dev/null
    work_device=$(stat -c %d "$work_root")
    cache_device=$(stat -c %d "$cache_root")
    root_device=$(stat -c %d /)
    [[ $work_device != "$root_device" && $cache_device != "$root_device" ]] \
      || fail "Native work and cache roots must use dedicated filesystems"
    [[ $work_device != "$cache_device" ]] \
      || fail "Native work and cache roots must use separate filesystems"
    work_bytes=$(df -B1 --output=size "$work_root" | tail -n1)
    cache_bytes=$(df -B1 --output=size "$cache_root" | tail -n1)
    (( work_bytes <= OCTACITY_RUNNER_WORKSPACE_BYTES && cache_bytes <= OCTACITY_RUNNER_WORKSPACE_BYTES )) \
      || fail "Native work and cache filesystems exceed the signed workspace limit"
    grep -qw cpu "$cgroup_root/cgroup.subtree_control" || fail "cpu controller is not delegated"
    grep -qw memory "$cgroup_root/cgroup.subtree_control" || fail "memory controller is not delegated"
    grep -qw io "$cgroup_root/cgroup.subtree_control" || fail "io controller is not delegated"
    grep -qw pids "$cgroup_root/cgroup.subtree_control" || fail "pids controller is not delegated"
    verify_release "$release_root"

    runner_environment=(
      "OCTACITY_CONTRACT_OCTA_RELEASE_ROOT=$release_root"
      "OCTACITY_CONTRACT_WORKSPACE_BYTES=$OCTACITY_RUNNER_WORKSPACE_BYTES"
      "OCTACITY_CONTRACT_NATIVE_CGROUP_ROOT=$cgroup_root"
      "OCTACITY_CONTRACT_NATIVE_WORK_ROOT=$work_root"
      "OCTACITY_RELEASE_NATIVE_CACHE_ROOT=$cache_root"
      "OCTACITY_CONTRACT_NATIVE_BWRAP=$OCTACITY_RUNNER_NATIVE_BWRAP"
      "OCTACITY_CONTRACT_NATIVE_PATH=$OCTACITY_RUNNER_NATIVE_PATH"
    )
    runner_path="$HOME/.cargo/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin"
    export "${runner_environment[@]}"
    export CARGO_TARGET_DIR="$runner_root/preflight-target"
    ;;
  *)
    fail "supported hosts are Apple Silicon macOS and ARM64 Linux; got $kernel/$architecture"
    ;;
esac

runner_dir=$runner_root/actions-runner
write_runner_files
if [[ $mode == check ]]; then
  cargo test -p octacity-job --test backend_contract "$test_name" \
    --manifest-path "$repo_root/Cargo.toml" -- --ignored --exact --nocapture
  echo "preflight passed for $kernel/$architecture"
fi
