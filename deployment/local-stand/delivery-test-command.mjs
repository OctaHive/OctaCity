// Installed command for an operator-reviewed test pack. Task text never becomes code.
import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { constants, fstatSync, openSync, readSync, closeSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const sha256 = (bytes) => createHash('sha256').update(bytes).digest('hex');

function read(path, limit) {
  const fd = openSync(path, constants.O_RDONLY | constants.O_NOFOLLOW | constants.O_NONBLOCK);
  try {
    if (!fstatSync(fd).isFile()) throw new Error();
    const bytes = Buffer.alloc(limit + 1);
    let size = 0;
    while (size < bytes.length) {
      const count = readSync(fd, bytes, size, bytes.length - size, size);
      if (!count) break;
      size += count;
    }
    if (size > limit) throw new Error();
    return bytes.subarray(0, size);
  } finally {
    closeSync(fd);
  }
}

try {
  const [mode, declaration, protectedRoot, source, output] = process.argv.slice(2);
  const pack = JSON.parse(declaration);
  const input = (name, limit = 1024 * 1024) => read(join(protectedRoot, name), limit);
  const write = (name, bytes) => writeFileSync(join(output, name), bytes, { flag: 'wx' });

  const installedProgram = mode === 'author' || mode === 'reproduction';
  if (installedProgram && (sha256(input('work-task')) !== pack.task || sha256(input('work-acceptance')) !== pack.acceptance))
      throw new Error('test pack does not match the retained task and acceptance');
  const program = installedProgram ? Buffer.from(pack.program, 'base64') : input('tests/checks.sh', 16384);
  if (!program.length || program.length > 16384 || sha256(program) !== pack.digest) throw new Error();
  if (mode === 'author') {
    write('checks.sh', program);
    write('author-transcript.txt', 'Matched the installed pack to the exact admitted task and acceptance.\n');
  } else {
    if (mode !== 'baseline' && mode !== 'candidate' && mode !== 'reproduction') throw new Error();
    const frozen = JSON.parse(input('command-input.json'));
    const validation = mode === 'candidate' ? JSON.parse(input('validation-input.json')) : null;

    const result = spawnSync('/bin/sh', ['-s', '--', source], {
      cwd: source, input: program, stdio: ['pipe', 'ignore', 'ignore'],
      env: { PATH: '/usr/bin:/bin', LANG: 'C', HOME: '/workspace/scratch' },
      timeout: pack.timeout ?? 60000,
    });
    const status = result.error || result.signal ? null : result.status;
    const outcome = status === 0 ? 'passed' : status === 1 ? 'failed' : 'indeterminate';
    const testClasses = [{ class: pack.class, outcome }];
    if (mode !== 'candidate') {
      const conclusive = outcome !== 'indeterminate';
      write('baseline.json', JSON.stringify({
        input_digest: frozen.node.input_digest,
        subject: frozen.input.subject,
        command: frozen.profile.model_or_tool,
        environment: pack.environment,
        criterion: pack.criterion,
        status: conclusive ? 'completed' : 'environment_unavailable',
        finding: outcome === 'passed' ? 'not_matched' : outcome === 'failed' ? 'matched' : 'indeterminate',
        ...(conclusive ? { observations: { failed: outcome === 'failed' } } : {}),
        ...(conclusive && mode === 'baseline' ? { test_classes: testClasses } : {}),
      }));
    } else {
      if (validation.classes.length !== 1 || validation.classes[0] !== pack.class ||
          validation.input_digest !== frozen.node.input_digest) throw new Error();
      const report = JSON.stringify({ ...validation, classes: testClasses });
      write('validation.json', report);
      write('validation-facts.json', JSON.stringify({
        accepted: true, input_digest: frozen.node.input_digest, report_digest: sha256(report),
        candidate_revision: validation.candidate.candidate_revision,
        candidate_digest: validation.candidate.changeset_digest, protected_inventory: validation.tests,
        covered_classes: validation.classes, fresh_until: frozen.node.deadline,
      }));
    }
  }
} catch (error) {
  const mismatch = error.message === 'test pack does not match the retained task and acceptance';
  process.stderr.write(mismatch ? `${error.message}\n` : 'Delivery check rejected its installed inputs or outputs\n');
  process.exitCode = 64;
}
