// Operator-installed finite author. Args: frozen intent, output root, declared relative output.
import fs from 'node:fs';
import path from 'node:path';
const portable = value => typeof value === 'string' && value.length > 0 && value.length <= 128
  && !path.isAbsolute(value) && !/[\\\x00-\x1f\x7f]/.test(value)
  && !value.split('/').some(part => !part || part === '.' || part === '..');
const noLinks = file => {
  if (!path.isAbsolute(file) || path.resolve(file) !== file) throw new Error();
  let current = path.parse(file).root;
  for (const part of file.slice(current.length).split('/')) {
    current = path.join(current, part);
    if (fs.lstatSync(current).isSymbolicLink()) throw new Error();
  }
  return file;
};
try {
  const args = process.argv.slice(2);
  if (args.length !== 3) throw new Error();
  const [input, root, output] = args;
  if (!fs.lstatSync(noLinks(input)).isFile()) throw new Error();
  const fd = fs.openSync(input, fs.constants.O_RDONLY | fs.constants.O_NOFOLLOW | fs.constants.O_NONBLOCK);
  let accepted;
  try {
    if (!fs.fstatSync(fd).isFile()) throw new Error();
    const bytes = Buffer.alloc(65537);
    let size = 0;
    while (size < bytes.length) {
      const count = fs.readSync(fd, bytes, size, bytes.length - size, size);
      if (!count) break;
      size += count;
    }
    if (size > 65536) throw new Error();
    accepted = JSON.parse(bytes.subarray(0, size)).input.payload.value;
  } finally { fs.closeSync(fd); }
  const fields = ['acceptance','criteria_artifact','eligible','expected_text','path','risk','size','task','work_kind'];
  if (!accepted || typeof accepted !== 'object' || Object.keys(accepted).sort().join(',') !== fields.join(',')
      || accepted.eligible !== true || accepted.risk !== 'low'
      || !['defect','feature_request'].includes(accepted.work_kind) || !['small','medium'].includes(accepted.size)
      || typeof accepted.expected_text !== 'string' || !Buffer.byteLength(accepted.expected_text)
      || Buffer.byteLength(accepted.expected_text) > 1024 || accepted.expected_text.includes('\0')) throw new Error();
  for (const name of ['task','acceptance','criteria_artifact']) {
    const reference = accepted[name];
    if (!reference || Object.keys(reference).sort().join(',') !== 'artifact_id,content_digest,encoded_size'
        || typeof reference.artifact_id !== 'string' || !/^[0-9a-f]{8}(-[0-9a-f]{4}){3}-[0-9a-f]{12}$/.test(reference.artifact_id)
        || typeof reference.content_digest !== 'string' || !/^[0-9a-f]{64}$/.test(reference.content_digest)
        || !Number.isSafeInteger(reference.encoded_size) || reference.encoded_size < 1 || reference.encoded_size > 64*1024*1024) throw new Error();
  }
  if (!portable(accepted.path) || !portable(output) || !fs.lstatSync(noLinks(root)).isDirectory()) throw new Error();
  const destination = path.join(root, output);
  noLinks(path.dirname(destination));
  fs.writeFileSync(destination, JSON.stringify({path: accepted.path, expected: accepted.expected_text}), {flag:'wx', mode:0o600});
} catch {
  process.stderr.write('requirements author rejected its input\n');
  process.exitCode = 64;
}
