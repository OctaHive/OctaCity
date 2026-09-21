#!/bin/sh
set -eu

printf '%s\n' '{"type":"hello","protocol_version":1,"plugin_name":"fixture","plugin_version":"1.0.0"}'
IFS= read -r _
/bin/sh ./fixture-body
