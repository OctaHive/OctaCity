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
# Factory is opt-in. Its installed profile crosses the private input boundary;
# separately installed connector files below use fixed names and size bounds.
profile=/run/secrets/factory-profiles.json
if [ -e "$profile" ] || [ -L "$profile" ]; then
  if [ ! -f "$profile" ] || [ -L "$profile" ]; then
    echo "Factory profile catalog is not a regular file" >&2
    exit 1
  fi
  destination=$target/factory-profiles.json
  head -c 4194305 "$profile" > "$destination"
  if [ "$(wc -c < "$destination")" -gt 4194304 ]; then
    echo "Factory profile catalog exceeds its bound" >&2
    exit 1
  fi
  chmod 0600 "$destination"
  chown octacity:octacity "$destination"
fi
bundle=/run/octacity-factory-connectors
if [ -e "$bundle/factory-signal-connectors.json" ] || [ -L "$bundle/factory-signal-connectors.json" ]; then
  if [ ! -d "$bundle" ] || [ -L "$bundle" ]; then
    echo "Factory connector bundle is invalid" >&2
    exit 1
  fi
  count=0
  for source in "$bundle"/factory-signal-*; do
    name=${source##*/}
    case "$name" in
      factory-signal-connectors.json|factory-signal-[0-9][0-9][0-9][0-9]-configuration) limit=4194304 ;;
      factory-signal-[0-9][0-9][0-9][0-9]-credential) limit=65536 ;;
      *) echo "Factory connector bundle is invalid" >&2; exit 1 ;;
    esac
    count=$((count + 1))
    if [ "$count" -gt 129 ] || [ ! -f "$source" ] || [ -L "$source" ]; then
      echo "Factory connector bundle is invalid" >&2
      exit 1
    fi
    destination=$target/$name
    head -c "$((limit + 1))" "$source" > "$destination"
    if [ "$(wc -c < "$destination")" -gt "$limit" ]; then
      echo "Factory connector input exceeds its bound" >&2
      exit 1
    fi
    chmod 0600 "$destination"
    chown octacity:octacity "$destination"
  done
fi
copy_factory_directory() {
  [ "$3" -le 4 ] || { echo "Factory executable installation is invalid" >&2; exit 1; }
  mkdir -p "$2"
  chmod 0700 "$2"
  chown octacity:octacity "$2"
  for source in "$1"/* "$1"/.[!.]* "$1"/..?*; do
    [ -e "$source" ] || [ -L "$source" ] || continue
    name=${source##*/}
    case "$name" in
      .|..|-*|*[!A-Za-z0-9_.-]*) echo "Factory executable installation is invalid" >&2; exit 1 ;;
    esac
    delivery_count=$((delivery_count + 1))
    if [ "$delivery_count" -gt 1024 ] || [ "${#name}" -gt 128 ] || [ -L "$source" ]; then
      echo "Factory executable installation is invalid" >&2; exit 1
    fi
    if [ -d "$source" ]; then
      copy_factory_directory "$source" "$2/$name" "$(( $3 + 1 ))"
    elif [ -f "$source" ]; then
      mode=0600
      limit=4194304
      if [ -x "$source" ]; then mode=0700; limit=268435456; fi
      destination=$2/$name
      head -c "$((limit + 1))" "$source" > "$destination"
      bytes=$(wc -c < "$destination")
      delivery_bytes=$((delivery_bytes + bytes))
      if [ "$bytes" -gt "$limit" ] || [ "$delivery_bytes" -gt 536870912 ]; then
        echo "Factory executable installation exceeds its bound" >&2; exit 1
      fi
      chmod "$mode" "$destination"
      chown octacity:octacity "$destination"
    else
      echo "Factory executable installation is invalid" >&2; exit 1
    fi
  done
}
for installation in delivery runtime; do
  source=$bundle/factory-$installation
  if [ -e "$source" ] || [ -L "$source" ]; then
    if [ ! -d "$source" ] || [ -L "$source" ]; then
      echo "Factory executable installation is invalid" >&2; exit 1
    fi
    delivery_count=0
    delivery_bytes=0
    copy_factory_directory "$source" "/run/octacity-$installation" 0
  fi
done

integration_bundle=/run/octacity-integration-installation
if [ -e "$integration_bundle" ] || [ -L "$integration_bundle" ]; then
  [ -d "$integration_bundle" ] && [ ! -L "$integration_bundle" ] || { echo "integration installation is invalid" >&2; exit 1; }
  key_file=$integration_bundle/installation-key
  [ -f "$key_file" ] && [ ! -L "$key_file" ] || { echo "integration installation is invalid" >&2; exit 1; }
  key=$(head -c 66 "$key_file")
  case "$key" in ''|*[!a-f0-9]*) echo "integration installation is invalid" >&2; exit 1 ;; esac
  [ "${#key}" -eq 64 ] || { echo "integration installation is invalid" >&2; exit 1; }
  integration_root=/var/lib/octacity-integrations/$key
  provider=$integration_root/adapters/github
  for directory in /var/lib/octacity-integrations "$integration_root" "$integration_root/adapters" "$provider" "$provider/credentials"; do
    [ ! -L "$directory" ] || { echo "integration directory is invalid" >&2; exit 1; }
    mkdir -p "$directory"
    chmod 0700 "$directory"
    chown octacity:octacity "$directory"
  done
  install_integration_file() {
    [ -f "$1" ] && [ ! -L "$1" ] || { echo "integration file is invalid" >&2; exit 1; }
    if [ -e "$2" ] || [ -L "$2" ]; then
      [ -f "$2" ] && [ ! -L "$2" ] && cmp -s "$1" "$2" || { echo "immutable integration installation differs" >&2; exit 1; }
    else
      head -c "$(( $3 + 1 ))" "$1" > "$2"
      [ "$(wc -c < "$2")" -le "$3" ] || { echo "integration file exceeds its bound" >&2; exit 1; }
      chmod "$4" "$2"
      chown octacity:octacity "$2"
    fi
  }
  install_integration_file "$integration_bundle/catalog.json" "$integration_root/catalog.json" 4194304 0600
  install_integration_file "$integration_bundle/adapter.toml" "$provider/adapter.toml" 1048576 0600
  install_integration_file "$integration_bundle/github-work.toml" "$provider/github-work.toml" 65536 0600
  install_integration_file /usr/local/bin/octacity-github "$provider/plugin" 268435456 0700
  install_integration_file "$integration_bundle/server.toml" "$target/integration-server.toml" 65536 0600
  [ ! -L /run/octacity-management ] || { echo "management channel is invalid" >&2; exit 1; }
  mkdir -p /run/octacity-management
  chmod 0711 /run/octacity-management
  chown octacity:octacity /run/octacity-management
fi
chown octacity:octacity "$target"

exec su -s /bin/sh -- octacity -c 'exec "$0" "$@"' "$server" "$@"
