#!/usr/bin/env bash

# Shared, checksum-first Octa release staging for disposable Linux backend gates.
# The caller owns `fail`, `require_command`, and `repo_root`.

octacity_hosted_platform_metadata() {
  case "$(uname -m)" in
    x86_64) printf 'amd64 linux-x86_64\n' ;;
    aarch64|arm64) printf 'arm64 linux-aarch64\n' ;;
    *) fail "unsupported Linux architecture: $(uname -m)" ;;
  esac
}

octacity_verify_release_metadata() {
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

octacity_stage_octa_release() {
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
  octacity_verify_release_metadata \
    "$release_root/octa-runner-capabilities.json" "$version" "$revision" "$runtime_platform"
  [[ -x $release_root/octa-runner ]] || fail "released octa-runner is not executable"
}
