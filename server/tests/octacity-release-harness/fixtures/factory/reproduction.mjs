import { readFileSync, writeFileSync } from 'node:fs';

const setup = JSON.parse(process.argv[2]);
const intent = JSON.parse(readFileSync(setup.input, 'utf8'));
// A read failure is a technical stop. Only a completed read can establish
// that the reviewed regression marker was absent from this exact revision.
const markerPresent = readFileSync(setup.file, 'utf8').trim() === setup.marker;
writeFileSync(setup.report, JSON.stringify({
  input_digest: intent.node.input_digest,
  subject: intent.input.subject,
  command: intent.profile.model_or_tool,
  environment: setup.environment,
  criterion: setup.criterion,
  status: 'completed',
  finding: markerPresent ? 'matched' : 'not_matched',
  observations: { checks: 1, symptom_present: markerPresent },
}));
