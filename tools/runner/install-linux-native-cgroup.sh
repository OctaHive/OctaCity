#!/usr/bin/env bash
set -euo pipefail

INSTALL_ROOT=/usr/local/libexec/octacity
SERVICE_PATH=/etc/systemd/system/octacity-native-cgroup.service
WRAPPER_PATH=/usr/local/sbin/octacity-in-cgroup

if [[ $EUID -ne 0 ]]; then
  echo "run this installer through sudo" >&2
  exit 1
fi
if (( $# != 1 )); then
  echo "usage: sudo $0 RUNNER_USER" >&2
  exit 2
fi

runner_user=$1
runner_group=$(id -gn "$runner_user")
script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)

install -d -m 0755 "$INSTALL_ROOT"
install -o root -g root -m 0755 "$script_dir/octacity-in-cgroup" "$WRAPPER_PATH"

setup_script="$INSTALL_ROOT/setup-native-cgroup"
install -o root -g root -m 0755 /dev/null "$setup_script"
{
  printf '%s\n' '#!/usr/bin/env bash' 'set -euo pipefail'
  printf 'runner_user=%q\nrunner_group=%q\n' "$runner_user" "$runner_group"
  cat <<'SCRIPT'
root=/sys/fs/cgroup/octacity
runner=$root/runner
jobs=$root/jobs

mkdir -p "$root"
chown "$runner_user:$runner_group" "$root" "$root/cgroup.procs" "$root/cgroup.subtree_control"
echo '+cpuset +cpu +io +memory +pids' > "$root/cgroup.subtree_control"
mkdir -p "$runner" "$jobs"
chown "$runner_user:$runner_group" "$runner" "$jobs" "$jobs/cgroup.procs" "$jobs/cgroup.subtree_control"
echo '+cpuset +cpu +io +memory +pids' > "$jobs/cgroup.subtree_control"
SCRIPT
} > "$setup_script"
chmod 0755 "$setup_script"

cat > "$SERVICE_PATH" <<EOF
[Unit]
Description=Delegate the OctaCity Native runner cgroup tree
After=local-fs.target
Before=multi-user.target

[Service]
Type=oneshot
ExecStart=$setup_script
RemainAfterExit=yes

[Install]
WantedBy=multi-user.target
EOF

systemctl daemon-reload
systemctl enable octacity-native-cgroup.service
systemctl restart octacity-native-cgroup.service
echo "installed persistent Native cgroup delegation for $runner_user"
