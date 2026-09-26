#!/usr/bin/env bash
set -euo pipefail

fail() {
  echo "github-hosted Native setup: $*" >&2
  exit 1
}

require_command() {
  command -v "$1" >/dev/null 2>&1 || fail "required command is missing: $1"
}

hosted_root() {
  : "${RUNNER_TEMP:?RUNNER_TEMP is required}"
  printf '%s/octacity-hosted-native\n' "$RUNNER_TEMP"
}

remove_cgroup_tree() {
  local cgroup_parent=$1 child leaf
  [[ -d $cgroup_parent ]] || return 0
  for leaf in jobs runner; do
    [[ -d $cgroup_parent/$leaf ]] || continue
    while IFS= read -r -d '' child; do
      [[ ! -f $child/cgroup.kill ]] || printf '1\n' | sudo tee "$child/cgroup.kill" >/dev/null
      sudo rmdir "$child"
    done < <(find "$cgroup_parent/$leaf" -mindepth 1 -maxdepth 1 -type d -print0)
    [[ ! -f $cgroup_parent/$leaf/cgroup.kill ]] \
      || printf '1\n' | sudo tee "$cgroup_parent/$leaf/cgroup.kill" >/dev/null
    sudo rmdir "$cgroup_parent/$leaf"
  done
  sudo rmdir "$cgroup_parent"
}

cleanup() {
  local root cgroup_parent mount
  root=$(hosted_root)
  cgroup_parent=/sys/fs/cgroup/octacity-hosted-native
  case "$root" in
    "$RUNNER_TEMP"/octacity-hosted-native) ;;
    *) fail "refusing to clean unexpected path: $root" ;;
  esac

  for mount in "$root/cache" "$root/work"; do
    if mountpoint --quiet "$mount"; then
      sudo umount "$mount"
    fi
  done
  remove_cgroup_tree "$cgroup_parent"
  if [[ -e $root ]]; then
    sudo rm -rf -- "$root"
  fi
}

enable_controllers() {
  local cgroup=$1 controller available
  available=$(<"$cgroup/cgroup.controllers")
  for controller in cpu memory io pids; do
    grep -qw "$controller" <<<"$available" \
      || fail "cgroup controller '$controller' is unavailable below $cgroup"
  done
  printf '+cpu +memory +io +pids\n' | sudo tee "$cgroup/cgroup.subtree_control" >/dev/null
}

install_bubblewrap_profile() {
  local source=/usr/share/apparmor/extra-profiles/bwrap-userns-restrict
  local destination=/etc/apparmor.d/bwrap-userns-restrict
  [[ -r /sys/module/apparmor/parameters/enabled ]] || return 0
  grep -q '^Y' /sys/module/apparmor/parameters/enabled || return 0
  [[ -f $source ]] || fail "Ubuntu Bubblewrap AppArmor profile is missing: $source"
  require_command apparmor_parser
  sudo install -o root -g root -m 0644 "$source" "$destination"
  sudo apparmor_parser --replace "$destination"
}

platform_metadata() {
  case "$(uname -m)" in
    x86_64) printf 'amd64 linux-x86_64\n' ;;
    aarch64|arm64) printf 'arm64 linux-aarch64\n' ;;
    *) fail "unsupported Linux architecture: $(uname -m)" ;;
  esac
}

verify_release_metadata() {
  local capabilities=$1 version=$2 revision=$3 platform=$4
  python3 - "$capabilities" "$version" "$revision" "$platform" <<'PY'
import json
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
document = json.loads(path.read_text(encoding="utf-8"))
expected = {
    "octa_version": sys.argv[2],
    "build_commit": sys.argv[3],
    "platform": sys.argv[4],
}
actual = {key: document.get(key) for key in expected}
if actual != expected:
    raise SystemExit(f"unexpected Octa release metadata in {path}: expected {expected}, got {actual}")
PY
}

stage_octa_release() {
  local root=$1 version=$2 revision=$3 asset_arch=$4 runtime_platform=$5
  local download_root=$root/download release_root=$root/octa-release asset checksum base_url
  download_root=$root/download
  release_root=$root/octa-release
  asset="octa-Linux-${asset_arch}.tar.gz"
  checksum="${asset}.sha256"
  base_url="https://github.com/OctaHive/octa/releases/download/v${version}"

  mkdir -p "$download_root" "$release_root"
  curl --fail --location --retry 5 --retry-all-errors --connect-timeout 30 \
    --output "$download_root/$asset" "$base_url/$asset"
  curl --fail --location --retry 5 --retry-all-errors --connect-timeout 30 \
    --output "$download_root/$checksum" "$base_url/$checksum"
  (cd "$download_root" && sha256sum --check --strict "$checksum")
  tar --extract --gzip --file "$download_root/$asset" --directory "$release_root" \
    --no-same-owner --no-same-permissions
  python3 "$repo_root/tools/validate_octa_release_contract.py" \
    "$release_root/octa-release-contract.json"
  (cd "$release_root" && sha256sum --check --strict SHA256SUMS)
  verify_release_metadata \
    "$release_root/octa-runner-capabilities.json" "$version" "$revision" "$runtime_platform"
  [[ -x $release_root/octa-runner ]] || fail "released octa-runner is not executable"
}

provision_filesystem() {
  local image=$1 mount=$2 bytes=$3 label=$4
  truncate --size "$bytes" "$image"
  mkfs.ext4 -F -q -L "$label" "$image"
  mkdir -p "$mount"
  sudo mount --options loop,nosuid,nodev "$image" "$mount"
  sudo chown "$(id -u):$(id -g)" "$mount"
  chmod 0700 "$mount"
}

validate_filesystems() {
  local root=$1 limit=$2 work_device cache_device root_device work_bytes cache_bytes
  work_device=$(stat -c %d "$root/work")
  cache_device=$(stat -c %d "$root/cache")
  root_device=$(stat -c %d /)
  [[ $work_device != "$root_device" && $cache_device != "$root_device" ]] \
    || fail "Native work and cache roots must use dedicated filesystems"
  [[ $work_device != "$cache_device" ]] \
    || fail "Native work and cache roots must use separate filesystems"
  work_bytes=$(df -B1 --output=size "$root/work" | tail -n1 | tr -d ' ')
  cache_bytes=$(df -B1 --output=size "$root/cache" | tail -n1 | tr -d ' ')
  (( work_bytes <= limit && cache_bytes <= limit )) \
    || fail "Native work and cache filesystems exceed the signed workspace limit"
}

write_environment() {
  local root=$1 workspace_bytes=$2 cgroup_root=$3
  : "${GITHUB_ENV:?GITHUB_ENV is required}"
  cat >>"$GITHUB_ENV" <<EOF
OCTACITY_HOSTED_NATIVE_ROOT=$root
OCTACITY_CONTRACT_OCTA_RELEASE_ROOT=$root/octa-release
OCTACITY_CONTRACT_WORKSPACE_BYTES=$workspace_bytes
OCTACITY_CONTRACT_NATIVE_CGROUP_ROOT=$cgroup_root
OCTACITY_CONTRACT_NATIVE_WORK_ROOT=$root/work
OCTACITY_RELEASE_NATIVE_CACHE_ROOT=$root/cache
OCTACITY_CONTRACT_NATIVE_BWRAP=/usr/bin/bwrap
OCTACITY_CONTRACT_NATIVE_PATH=/usr/local/bin:/usr/bin:/bin
EOF
}

run_in_cgroup() {
  local command_path runner_user runner_group runner_home
  (( $# > 0 )) || fail "run requires a command"
  if (( EUID != 0 )); then
    if [[ $1 == */* ]]; then
      command_path=$1
    else
      command_path=$(command -v "$1") || fail "command is missing: $1"
    fi
    shift
    exec sudo --preserve-env "$0" run "$command_path" "$@"
  fi

  runner_user=${SUDO_USER:-}
  [[ -n $runner_user && $runner_user != root ]] || fail "run must be entered through sudo by the runner user"
  runner_group=$(id -gn "$runner_user")
  runner_home=$(getent passwd "$runner_user" | cut -d: -f6)
  [[ -n $runner_home ]] || fail "cannot resolve the runner user's home directory"
  [[ -w /sys/fs/cgroup/octacity-hosted-native/runner/cgroup.procs ]] \
    || fail "GitHub-hosted runner cgroup is unavailable"

  printf '%s\n' "$$" > /sys/fs/cgroup/octacity-hosted-native/runner/cgroup.procs
  exec setpriv --reuid="$runner_user" --regid="$runner_group" --init-groups -- \
    env HOME="$runner_home" USER="$runner_user" LOGNAME="$runner_user" "$@"
}

probe_bubblewrap() {
  "$0" run /usr/bin/bwrap \
    --die-with-parent \
    --new-session \
    --unshare-all \
    --share-net \
    --disable-userns \
    --ro-bind / / \
    -- /bin/true \
    || fail "Bubblewrap cannot create the namespaces required by Native execution"
}

setup() {
  local version=${1:-} revision=${2:-} root workspace_bytes cgroup_parent cgroup_root runner_cgroup
  local asset_arch runtime_platform
  [[ $version =~ ^[0-9]+\.[0-9]+\.[0-9]+([-+][0-9A-Za-z.-]+)?$ ]] \
    || fail "a valid Octa release version is required"
  [[ $revision =~ ^[0-9a-f]{40}$ ]] || fail "a valid Octa revision is required"
  [[ $(uname -s) == Linux ]] || fail "Native setup requires Linux"
  for command in curl find getent mkfs.ext4 mountpoint python3 setpriv sha256sum sudo tar truncate; do
    require_command "$command"
  done
  [[ -x /usr/bin/bwrap ]] || fail "Bubblewrap is not installed at /usr/bin/bwrap"
  install_bubblewrap_profile

  repo_root=$(git rev-parse --show-toplevel 2>/dev/null) \
    || fail "run from the OctaCity checkout"
  # shellcheck source=backend-runner.defaults
  source "$repo_root/tools/runner/backend-runner.defaults"
  workspace_bytes=$OCTACITY_RUNNER_WORKSPACE_BYTES
  [[ $workspace_bytes =~ ^[1-9][0-9]*$ ]] \
    || fail "OCTACITY_RUNNER_WORKSPACE_BYTES must be a positive integer"
  (( workspace_bytes % 1048576 == 0 )) \
    || fail "OCTACITY_RUNNER_WORKSPACE_BYTES must be a whole number of MiB"

  root=$(hosted_root)
  [[ ! -e $root ]] || fail "staging root already exists: $root"
  read -r asset_arch runtime_platform < <(platform_metadata)
  mkdir -p "$root"
  stage_octa_release "$root" "$version" "$revision" "$asset_arch" "$runtime_platform"
  provision_filesystem "$root/work.ext4" "$root/work" "$workspace_bytes" octacity-work
  provision_filesystem "$root/cache.ext4" "$root/cache" "$workspace_bytes" octacity-cache
  validate_filesystems "$root" "$workspace_bytes"

  cgroup_parent=/sys/fs/cgroup/octacity-hosted-native
  cgroup_root=$cgroup_parent/jobs
  runner_cgroup=$cgroup_parent/runner
  [[ ! -e $cgroup_parent ]] || fail "cgroup root already exists: $cgroup_parent"
  enable_controllers /sys/fs/cgroup
  sudo mkdir "$cgroup_parent"
  enable_controllers "$cgroup_parent"
  sudo chown "$(id -u):$(id -g)" \
    "$cgroup_parent" "$cgroup_parent/cgroup.procs" "$cgroup_parent/cgroup.subtree_control"
  sudo mkdir "$runner_cgroup" "$cgroup_root"
  enable_controllers "$cgroup_root"
  sudo chown "$(id -u):$(id -g)" \
    "$runner_cgroup" "$runner_cgroup/cgroup.procs" \
    "$cgroup_root" "$cgroup_root/cgroup.procs" "$cgroup_root/cgroup.subtree_control"
  [[ -w $cgroup_root/cgroup.procs && -w $cgroup_root/cgroup.subtree_control ]] \
    || fail "Native cgroup root is not delegated to the runner user"
  probe_bubblewrap

  write_environment "$root" "$workspace_bytes" "$cgroup_root"
  echo "GitHub-hosted Linux Native environment is ready"
}

case "${1:-}" in
  setup)
    shift
    setup "$@"
    ;;
  cleanup)
    cleanup
    ;;
  run)
    shift
    run_in_cgroup "$@"
    ;;
  *)
    echo "usage: $0 setup <octa-version> <octa-revision> | cleanup | run COMMAND [ARG ...]" >&2
    exit 2
    ;;
esac
