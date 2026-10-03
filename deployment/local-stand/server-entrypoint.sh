#!/bin/sh
set -eu

server=/usr/local/bin/octacity-server
if [ "$(id -u)" -ne 0 ]; then
  exec "$server" "$@"
fi

# Compose file-backed secrets retain host-side ownership. Copy only the fixed
# server allowlist into an ephemeral directory before dropping privileges.
target=/run/octacity-secrets
mkdir -p "$target"
chmod 0700 "$target"
for name in \
  postgres-url \
  object-access-key \
  object-secret-key \
  signing-key \
  agent-enrollment-key \
  cache-credential-key
do
  source=/run/secrets/$name
  if [ ! -f "$source" ] || [ -L "$source" ]; then
    echo "required server secret is not a regular file: $name" >&2
    exit 1
  fi
  destination=$target/$name
  cp "$source" "$destination"
  chmod 0600 "$destination"
  chown octacity:octacity "$destination"
done
chown octacity:octacity "$target"

exec su -s /bin/sh -- octacity -c 'exec "$0" "$@"' "$server" "$@"
