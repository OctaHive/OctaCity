#!/bin/sh
set -eu
tools_dir=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
node - "$tools_dir/ui-delivery-qualification-20261115.txt" <<'NODE'
const fs = require('fs');
const assert = require('assert');
assert.deepStrictEqual(fs.readFileSync(process.argv[2]), Buffer.from('octacity-ui-delivery\n', 'utf8'));
console.log('PASS: qualification file has the exact requested bytes');
NODE
