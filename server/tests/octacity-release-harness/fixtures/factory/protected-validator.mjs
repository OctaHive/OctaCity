// Operator-installed finite assertion checker; private values never become diagnostics.
// Args: invocation, frozen intent, inventory, private root, candidate root, report, facts.
// Assertions are closed {path, expected} JSON; each regular input is bounded to 64 KiB.
import fs from 'node:fs';
import path from 'node:path';
import { createHash } from 'node:crypto';
const sha = bytes => createHash('sha256').update(bytes).digest('hex');
const read = file => {
  if (!fs.lstatSync(file).isFile()) throw new Error();
  const fd = fs.openSync(file, fs.constants.O_RDONLY | fs.constants.O_NOFOLLOW);
  try {
    if (!fs.fstatSync(fd).isFile()) throw new Error();
    const buffer = Buffer.alloc(65537);
    let size = 0;
    while (size < buffer.length) {
      const count = fs.readSync(fd, buffer, size, buffer.length - size, size);
      if (!count) break;
      size += count;
    }
    if (size > 65536) throw new Error();
    return buffer.subarray(0, size);
  } finally { fs.closeSync(fd); }
};
const json = file => JSON.parse(read(file));
const verified = (file, reference) => {
  const bytes = read(file);
  if (bytes.length !== reference.encoded_size || sha(bytes) !== reference.content_digest) throw new Error();
  return JSON.parse(bytes);
};
const below = (root, relative) => {
  if (typeof relative !== 'string' || path.isAbsolute(relative) || relative.includes('\\') ||
      relative.split('/').some(part => !part || part === '.' || part === '..')) throw new Error();
  const canonicalRoot = fs.realpathSync(root), file = path.resolve(canonicalRoot, relative);
  const actual = fs.realpathSync(file);
  if (!actual.startsWith(canonicalRoot + path.sep) || !fs.lstatSync(file).isFile()) throw new Error();
  return actual;
};
try {
  const args = process.argv.slice(2);
  if (args.length !== 7) throw new Error();
  const [invocationFile, frozenFile, inventoryFile, privateRoot, candidateRoot, primaryFile, factsFile] = args;
  const invocation = json(invocationFile), frozen = json(frozenFile);
  const inventory = verified(inventoryFile, invocation.tests);
  if (!Array.isArray(invocation.classes) || !invocation.classes.length || invocation.classes.length > 64 ||
      invocation.classes.some((item, index, all) => typeof item !== 'string' || !item || (index > 0 && all[index - 1] >= item)) ||
      !Array.isArray(inventory.files) || !inventory.files.length || inventory.files.length > 64 ||
      inventory.files.some(file => !invocation.classes.includes(file.class))) throw new Error();
  const classes = invocation.classes.map(className => {
    const files = inventory.files.filter(file => file.class === className);
    if (!files.length) throw new Error();
    let passed = true;
    for (const file of files) {
      const check = verified(below(privateRoot, file.path), file.artifact);
      if (Object.keys(check).sort().join(',') !== 'expected,path' || typeof check.expected !== 'string') throw new Error();
      const actual = read(below(candidateRoot, check.path));
      if (!actual.equals(Buffer.from(check.expected, 'utf8'))) passed = false;
    }
    return { class: className, outcome: passed ? 'passed' : 'failed' };
  });
  const report = Buffer.from(JSON.stringify({ ...invocation, classes }));
  fs.writeFileSync(primaryFile, report);
  fs.writeFileSync(factsFile, JSON.stringify({
    accepted: true, input_digest: frozen.node.input_digest, report_digest: sha(report),
    candidate_revision: invocation.candidate.candidate_revision,
    candidate_digest: invocation.candidate.changeset_digest, protected_inventory: invocation.tests,
    covered_classes: invocation.classes, fresh_until: frozen.node.deadline,
  }));
} catch {
  process.stderr.write('protected validation rejected its invocation\n');
  process.exitCode = 64;
}
