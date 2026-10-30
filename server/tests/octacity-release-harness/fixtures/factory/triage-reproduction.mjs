// Trusted installed probe; the symptom is observed from the exact checked-out source.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { get } from 'node:http';
import { pathToFileURL } from 'node:url';
const [inputPath, sourcePath, outputPath, policyJson] = process.argv.slice(2);
assert.equal(process.versions.node, '24.21.0', 'the selected probe environment must be available');
const frozen = JSON.parse(fs.readFileSync(inputPath, 'utf8'));
const policy = JSON.parse(policyJson);
const { server } = await import(pathToFileURL(`${sourcePath}/health.mjs`));
let requests = 0;
server.on('request', () => { requests += 1; });
await new Promise((resolve, reject) => {
  server.once('error', reject);
  server.listen(0, '127.0.0.1', resolve);
});
let status;
try {
  status = await new Promise((resolve, reject) => {
    const request = get(`http://127.0.0.1:${server.address().port}/health`, response => {
      response.resume();
      response.once('end', () => resolve(response.statusCode));
    });
    request.once('error', reject);
    request.setTimeout(5000, () => request.destroy(new Error('health request timed out')));
  });
} finally {
  await new Promise((resolve, reject) => server.close(error => error ? reject(error) : resolve()));
}
assert.ok(Number.isInteger(status));
fs.writeFileSync(outputPath, JSON.stringify({
  input_digest: frozen.node.input_digest,
  subject: frozen.input.subject,
  command: frozen.profile.model_or_tool,
  environment: policy.environment,
  criterion: policy.criterion,
  status: 'completed',
  observations: { requests, status },
  finding: status === 500 ? 'matched' : 'not_matched',
}));
