#!/bin/sh
set -eu
script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
node - "$script_dir/../ui-delivery-qualification-20261116-1010.txt" <<'NODE'
const fs = require('fs');
const expected = Buffer.from('octacity-ui-delivery\n', 'utf8');
try {
  const actual = fs.readFileSync(process.argv[2]);
  if (!actual.equals(expected)) throw new Error('file bytes differ');
} catch (error) {
  console.error('FAIL: ' + error.message);
  process.exit(1);
}
console.log('PASS: exact UTF-8 bytes');
NODE
