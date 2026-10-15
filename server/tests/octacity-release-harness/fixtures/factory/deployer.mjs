// Installed finite deployer: transfers actual candidate bytes to a separate target,
// then brackets the configured observation window with two verified target reads.
// Args: installed target, promotion input, frozen intent, candidate root, file, report, installed strategy.
import fs from 'node:fs';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { isDeepStrictEqual as equal } from 'node:util';
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
const sha = bytes => createHash('sha256').update(bytes).digest('hex');
try {
  if (process.argv.length !== 9) throw new Error();
  const [targetFile, promotionFile, frozenFile, root, relative, reportFile, strategy] = process.argv.slice(2);
  const target = JSON.parse(read(targetFile));
  const { plan, plan_digest } = JSON.parse(read(promotionFile));
  const { node } = JSON.parse(read(frozenFile));
  const policy = plan.policy;
  const environments = Object.entries(policy.environments);
  const endpoint = new URL(target.endpoint);
  const window = policy.observation_window_millis, deadline = node.deadline;
  if (!strategy || policy.strategy !== strategy || !equal(policy.allocation, {}) ||
      !equal(policy.production_environments, []) ||
      environments.length !== 1 || !equal(environments[0][1], target.environment) ||
      plan.input_digest !== node.input_digest || !/^[a-f0-9]{64}$/u.test(plan_digest) ||
      !/^[a-f0-9]{64}$/u.test(node.input_digest) || target.credential_profile !== null ||
      !['http:', 'https:'].includes(endpoint.protocol) || endpoint.username || endpoint.password ||
      endpoint.search || endpoint.hash || endpoint.href !== target.endpoint ||
      !Number.isSafeInteger(window) || window <= 0 || !Number.isSafeInteger(deadline) ||
      deadline - Date.now() <= window) throw new Error();
  const [[environment, contract]] = environments;
  if (path.isAbsolute(relative) || relative.includes('\\') ||
      relative.split('/').some(part => !part || part === '.' || part === '..')) throw new Error();
  const base = fs.realpathSync(root), file = fs.realpathSync(path.resolve(base, relative));
  if (!file.startsWith(base + path.sep)) throw new Error();
  const content = read(file), digest = sha(content);
  const request = async (endpoint, options) => {
    const remaining = deadline - Date.now();
    if (remaining <= 0) throw new Error();
    const response = await fetch(endpoint, { ...options, redirect: 'error', signal: AbortSignal.timeout(remaining) });
    if (!response.ok) throw new Error();
    const chunks = [];
    let size = 0;
    for await (const chunk of response.body) {
      size += chunk.length;
      if (size > 65536) throw new Error();
      chunks.push(chunk);
    }
    return JSON.parse(Buffer.concat(chunks, size));
  };
  const receipt = await request(target.endpoint, { method: 'POST', headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ operation_id: node.input_digest, candidate: plan.subject,
      content_sha256: digest, content: content.toString('base64') }) });
  if (!equal(receipt.candidate, plan.subject) || receipt.content_sha256 !== digest) throw new Error();
  const observe = async () => {
    const observed = await request(`${target.endpoint}/${encodeURIComponent(receipt.deployment.identity)}`);
    if (!equal(observed.deployment, receipt.deployment) || !equal(observed.candidate, plan.subject) ||
        observed.content_sha256 !== digest || !Buffer.from(observed.content, 'base64').equals(content)) throw new Error();
  };
  await observe();
  await new Promise(resolve => setTimeout(resolve, window));
  await observe();
  const finished = Date.now();
  if (finished > deadline) throw new Error();
  fs.writeFileSync(reportFile, JSON.stringify({ plan_digest, input_digest: node.input_digest,
    subject: plan.subject, environment, environment_contract: contract, deployment: receipt.deployment,
    outcome: 'passed', started_at: finished - window, finished_at: finished,
    measurements: { matching_reads: 2 } }));
} catch {
  process.stderr.write('deployment rejected its invocation\n');
  process.exitCode = 64;
}
