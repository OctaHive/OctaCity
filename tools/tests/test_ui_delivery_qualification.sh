#!/bin/sh
set -eu
TOOLS_DIR=$(CDPATH= cd -P "$(dirname "$0")/.." && pwd)
node - "$TOOLS_DIR/ui-delivery-qualification-20261115.txt" <<'NODE'
const fs = require('fs');
const assert = require('assert');
assert.deepStrictEqual(fs.readFileSync(process.argv[2]), Buffer.from('octacity-ui-delivery\n', 'utf8'));
console.log('PASS: exact UTF-8 bytes');
NODE
