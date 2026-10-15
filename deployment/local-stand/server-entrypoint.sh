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
copy_delivery_directory() {
  [ "$3" -le 4 ] || { echo "Factory Delivery installation is invalid" >&2; exit 1; }
  mkdir -p "$2"
  chmod 0700 "$2"
  chown octacity:octacity "$2"
  for source in "$1"/* "$1"/.[!.]* "$1"/..?*; do
    [ -e "$source" ] || [ -L "$source" ] || continue
    name=${source##*/}
    case "$name" in
      .*|-*|*[!A-Za-z0-9_.-]*) echo "Factory Delivery installation is invalid" >&2; exit 1 ;;
    esac
    delivery_count=$((delivery_count + 1))
    if [ "$delivery_count" -gt 1024 ] || [ "${#name}" -gt 128 ] || [ -L "$source" ]; then
      echo "Factory Delivery installation is invalid" >&2; exit 1
    fi
    if [ -d "$source" ]; then
      copy_delivery_directory "$source" "$2/$name" "$(( $3 + 1 ))"
    elif [ -f "$source" ]; then
      mode=0600
      limit=4194304
      if [ -x "$source" ]; then mode=0700; limit=268435456; fi
      destination=$2/$name
      head -c "$((limit + 1))" "$source" > "$destination"
      bytes=$(wc -c < "$destination")
      delivery_bytes=$((delivery_bytes + bytes))
      if [ "$bytes" -gt "$limit" ] || [ "$delivery_bytes" -gt 536870912 ]; then
        echo "Factory Delivery installation exceeds its bound" >&2; exit 1
      fi
      chmod "$mode" "$destination"
      chown octacity:octacity "$destination"
    else
      echo "Factory Delivery installation is invalid" >&2; exit 1
    fi
  done
}
if [ -e "$bundle/factory-delivery" ] || [ -L "$bundle/factory-delivery" ]; then
  if [ ! -d "$bundle/factory-delivery" ] || [ -L "$bundle/factory-delivery" ]; then
    echo "Factory Delivery installation is invalid" >&2; exit 1
  fi
  delivery_count=0
  delivery_bytes=0
  copy_delivery_directory "$bundle/factory-delivery" /run/octacity-delivery 0
fi
chown octacity:octacity "$target"

exec su -s /bin/sh -- octacity -c 'exec "$0" "$@"' "$server" "$@"
