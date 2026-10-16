#!/bin/sh
set -eu

# The symlink entrypoint sits beside fixture data, even when Git changes cwd.
exec /bin/sh "${0%/*}/fixture-body"
