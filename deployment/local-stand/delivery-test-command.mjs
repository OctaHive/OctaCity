// Installed command for running protected checks against the exact selected source.
import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { constants, fstatSync, openSync, readSync, closeSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { performance } from 'node:perf_hooks';

const sha256 = (bytes) => createHash('sha256').update(bytes).digest('hex');
const portablePath = (path) => typeof path === 'string' && path.length <= 512 &&
  path.split('/').every((part) => /^[A-Za-z0-9_][A-Za-z0-9_.-]*$/.test(part) && part.toLowerCase() !== '.git');

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

function checkedProgram(program, digest) {
  if (!program.length || program.length > 16384 || sha256(program) !== digest) throw new Error();
  return program;
}

// The server selects this inventory from the accepted authoring attempt and
// signs its read-only file mounts. Check every entry before starting any script.
function retainedPrograms(input, files, entrypoints) {
  if (!Array.isArray(files) || files.length === 0 || files.length > 64 ||
      !Array.isArray(entrypoints) || entrypoints.length === 0 || entrypoints.length > files.length ||
      new Set(entrypoints).size !== entrypoints.length)
    throw new Error();
  const retained = new Map();
  for (const { path, class: testClass, artifact } of files) {
    if (typeof testClass !== 'string' || !testClass ||
        !portablePath(path) || retained.has(path))
      throw new Error();
    const program = checkedProgram(input(`tests/${path}`, 16384), artifact.content_digest);
    if (program.length !== artifact.encoded_size) throw new Error();
    retained.set(path, { testClass, program });
  }
  return entrypoints.map((path) => {
    if (typeof path !== 'string' || !path.endsWith('.sh') || !retained.has(path)) throw new Error();
    return retained.get(path);
  });
}

function supplementPrograms(supplement, testClass) {
  const { files, entries } = supplement;
  if (!Array.isArray(files) || !files.length || files.length > 64 ||
      !Array.isArray(entries) || !entries.length || entries.length > 64) throw new Error();
  const selected = new Map();
  let total = 0;
  for (const { path, content_base64, sha256: digest } of files) {
    if (!portablePath(path) || selected.has(path.toLowerCase()) ||
        typeof content_base64 !== 'string' || content_base64.length > 349528) throw new Error();
    const bytes = Buffer.from(content_base64, 'base64');
    total += bytes.length;
    if (total > 262144 || bytes.toString('base64') !== content_base64 || sha256(bytes) !== digest)
      throw new Error();
    selected.set(path.toLowerCase(), { path, bytes });
  }
  const used = new Set();
  const checks = entries.map(({ kind, path, task }) => {
    const file = portablePath(path) && selected.get(path.toLowerCase());
    const identity = JSON.stringify([kind, path, task]);
    if (!file || file.path !== path || !file.bytes.length || file.bytes.includes(0) || used.has(identity))
      throw new Error();
    used.add(identity);
    if (kind === 'shell' && path.endsWith('.sh') && task === undefined)
      return { testClass, script: path, files: selected };
    if (kind === 'octafile' && typeof task === 'string' && Buffer.byteLength(task) <= 128 &&
        !/[*\p{Cc}]/u.test(task) && task.split(':').every(Boolean))
      return { testClass, octafile: path, task, files: selected };
    throw new Error();
  });
  return checks;
}

function stageFiles(selected, root) {
  for (const { path, bytes } of selected.values()) {
    const destination = join(root, path);
    mkdirSync(dirname(destination), { recursive: true, mode: 0o700 });
    writeFileSync(destination, bytes, { flag: 'wx', mode: 0o400 });
  }
}

function octaStatus(result, requestId, task) {
  if (!result || result.error || result.signal || ![0, 1].includes(result.status)) return null;
  try {
    const terminal = JSON.parse(result.stdout.trimEnd().split('\n').at(-1));
    if (terminal.type !== 'finished' || terminal.request_id !== requestId ||
        terminal.results?.length !== 1 || terminal.results[0].command !== task) return null;
    const { conclusion } = terminal.results[0];
    if (result.status === 0 && terminal.status === 'succeeded' && conclusion.status === 'succeeded') return 0;
    if (result.status === 1 && terminal.status === 'failed' && conclusion.status === 'failed' &&
        conclusion.failure?.kind === 'command' && conclusion.failure.exit_code === 1) return 1;
  } catch {
    // Discard malformed/private runner output. It is not an assertion result.
  }
  return null;
}

// The namespace init owns the check and all descendants. Give it only its own
// scratch and temporary files, and hide the report directory. Mount these
// in the supervisor's user namespace, then enter a child user namespace so
// the check cannot remove them. The supervisor needs no guest-root grants.
const CHECK_NAMESPACE = `
output=$1
scratch=$2
private=$3
workdir=$4
shift 4
mount --bind "$private" "$scratch" || exit 2
mount -t tmpfs -o ro,nodev,nosuid,noexec,mode=000,size=4k tmpfs "$output" || exit 2
for directory in /tmp /var/tmp /dev/shm; do
  mount -t tmpfs -o nodev,nosuid,mode=1777 tmpfs "$directory" || exit 2
done
cd "$workdir" || exit 2
exec /usr/bin/unshare --map-root-user -- /bin/sh -c '
  "$@"
  case $? in
    0) exit 0 ;;
    1) exit 10 ;;
    *) exit 2 ;;
  esac
' check "$@"
`;

function runChecks(checks, source, deadline, testsRoot, output, octaRunner, policy) {
  const outcomes = new Map();
  const env = { PATH: '/usr/bin:/bin', LANG: 'C', HOME: '/workspace/scratch' };
  const scratchRoot = policy.isolate_checks ? join(dirname(output), 'scratch') : output;
  const execute = (program, args, options, scratch) => {
    if (!policy.isolate_checks) return spawnSync(program, args, options);
    const result = spawnSync('/usr/bin/unshare', [
      '--map-root-user', '--pid', '--mount-proc', '--kill-child',
      '--', '/bin/sh', '-c', CHECK_NAMESPACE, 'check-namespace', output,
      scratchRoot, scratch, options.cwd, program, ...args,
    ], { ...options, cwd: source, killSignal: 'SIGKILL' });
    // A namespace or mount failure cannot be mistaken for an assertion.
    result.status = result.status === 10 ? 1 : result.status === 0 ? 0 : null;
    return result;
  };
  for (const { testClass, program, script, octafile, task, files } of checks) {
    const scratch = policy.isolate_checks || files ? mkdtempSync(join(scratchRoot, '.verification-')) : null;
    const fixtureRoot = files ? (policy.isolate_checks ? scratchRoot : scratch) : null;
    let status = null;
    try {
      if (files) stageFiles(files, scratch);
      const remaining = Math.floor(deadline - performance.now());
      if (remaining > 0) {
        const options = { cwd: fixtureRoot ?? source, stdio: ['pipe', 'ignore', 'ignore'], env, timeout: remaining };
        if (octafile) {
          const release = dirname(octaRunner);
          const request = { workspace: fixtureRoot, octafile, data_dir: join(fixtureRoot, '.octa'),
            plugins_dir: join(release, 'plugins'), plugin_lock: join(release, 'Octa.lock'),
            commands: [task], force: true, failfast: true, quiet: true, silence: true,
            variables: { OCTACITY_TEST_SOURCE: source, OCTACITY_TESTS_ROOT: fixtureRoot } };
          const requestId = 'protected-verification';
          const result = execute(octaRunner, [], {
            ...options, stdio: ['pipe', 'pipe', 'ignore'], encoding: 'utf8', maxBuffer: 512 * 1024,
            input: JSON.stringify({ type: 'start', protocol_version: 3, request_id: requestId, request }) + '\n',
          }, scratch);
          status = octaStatus(result, requestId, task);
        } else {
          const root = fixtureRoot ?? testsRoot;
          const requireSignal = policy.explicit_assertion_failure === true && program;
          const result = execute('/bin/sh', script ? [script, source] : ['-s', '--', source], {
            ...options, input: program, env: root ? { ...env, OCTACITY_TESTS_ROOT: root } : env,
            ...(requireSignal ? { stdio: ['pipe', 'pipe', 'ignore'], encoding: 'utf8', maxBuffer: 64 * 1024 } : {}),
          }, scratch);
          if (!result.error && !result.signal &&
              (!requireSignal || result.status !== 1 ||
                result.stdout.trimEnd().split('\n').at(-1) === 'octacity:assertion:failed')) {
            status = result.status;
          }
        }
      }
    } finally {
      if (scratch) rmSync(scratch, { recursive: true, force: true });
    }
    const previous = outcomes.get(testClass);
    const outcome = status !== 0 && status !== 1 || previous === 'indeterminate' ? 'indeterminate'
      : status === 1 || previous === 'failed' ? 'failed' : 'passed';
    outcomes.set(testClass, outcome);
  }
  return [...outcomes.keys()].sort().map((testClass) => ({ class: testClass, outcome: outcomes.get(testClass) }));
}

try {
  const [mode, declaration, protectedRoot, source, output, octaRunner = '/opt/octacity/octa/octa-runner'] = process.argv.slice(2);
  const pack = JSON.parse(declaration);
  const input = (name, limit = 1024 * 1024) => read(join(protectedRoot, name), limit);
  const write = (name, bytes) => writeFileSync(join(output, name), bytes, { flag: 'wx' });

  const installedProgram = mode === 'author' || mode === 'reproduction';
  if (installedProgram && (sha256(input('work-task')) !== pack.task || sha256(input('work-acceptance')) !== pack.acceptance))
      throw new Error('test pack does not match the retained task and acceptance');
  const installed = installedProgram ? checkedProgram(Buffer.from(pack.program, 'base64'), pack.digest) : null;
  if (mode === 'author') {
    write('checks.sh', installed);
    write('author-transcript.txt', 'Matched the installed pack to the exact admitted task and acceptance.\n');
  } else {
    if (mode !== 'baseline' && mode !== 'candidate' && mode !== 'reproduction') throw new Error();
    const frozen = JSON.parse(input('command-input.json'));
    const validation = mode === 'candidate' ? JSON.parse(input('validation-input.json')) : null;
    const inventory = installed ? null : JSON.parse(input('test-inventory.json'));
    const checks = installed
      ? [{ testClass: pack.class, program: installed }]
      : retainedPrograms(input, inventory.files, pack.entrypoints);
    const classes = [...new Set(checks.map(({ testClass }) => testClass))].sort();
    // The validation descriptor binds the candidate and all test bytes. The
    // command descriptor binds the Flow invocation recorded in the facts.
    if (validation && (!Array.isArray(validation.classes) || validation.classes.length !== classes.length ||
        !classes.every((testClass, index) => testClass === validation.classes[index]))) throw new Error();
    const supplements = inventory?.supplement ? supplementPrograms(inventory.supplement, classes[0]) : [];
    const deadline = performance.now() + (pack.timeout ?? 60000);
    const run = (programs) => runChecks(programs, source, deadline,
      installed ? null : join(protectedRoot, 'tests'), output, octaRunner, pack);
    if (mode !== 'candidate') {
      // Only the Tester-authored assertion can demonstrate the expected failure.
      const testClasses = run(checks);
      const supplementalClasses = run(supplements);
      const failed = testClasses.some(({ outcome }) => outcome === 'failed');
      const unavailable = [...testClasses, ...supplementalClasses].some(({ outcome }) => outcome === 'indeterminate');
      const status = unavailable ? 'environment_unavailable'
        : !failed && supplementalClasses.some(({ outcome }) => outcome === 'failed') ? 'needs_input' : 'completed';
      const conclusive = status === 'completed';
      write('baseline.json', JSON.stringify({
        input_digest: frozen.node.input_digest,
        subject: frozen.input.subject,
        command: frozen.profile.model_or_tool,
        environment: pack.environment,
        criterion: pack.criterion,
        status,
        finding: conclusive ? (failed ? 'matched' : 'not_matched') : 'indeterminate',
        ...(conclusive ? { observations: { failed } } : {}),
        ...(conclusive && mode === 'baseline' ? { test_classes: testClasses } : {}),
      }));
    } else {
      const report = JSON.stringify({ ...validation, classes: run([...checks, ...supplements]) });
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
