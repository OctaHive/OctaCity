#!/bin/sh
set -eu
script_dir=$(CDPATH= cd -P "$(dirname "$0")" && pwd)
target="$script_dir/../ui-delivery-qualification-20261120-125.txt"
if ! printf 'octacity-ui-delivery\n' | cmp - "$target"; then
    printf '%s\n' 'FAIL: qualification file must contain the exact requested bytes.' >&2
    exit 1
fi
printf '%s\n' 'PASS: qualification file contains the exact requested bytes.'
