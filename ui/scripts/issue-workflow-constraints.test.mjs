import assert from 'node:assert/strict';
import test from 'node:test';
import * as generator from './issue-workflow-constraints.mjs';

test('derives workflow form bounds and finite choices from the authoritative schema', () => {
  const schemas = {
    IssueWorkflowDefinition: {
      properties: {
        name: { 'x-max-utf8-bytes': 37 },
        statuses: { minItems: 1, maxItems: 12 },
        transitions: { maxItems: 19 },
      },
    },
    IssueWorkflowTransition: {
      properties: {
        origins: { items: { enum: ['manual', 'source'] } },
        evidence: { items: { enum: ['accepted_execution'] } },
      },
    },
  };
  assert.deepEqual(generator.issueWorkflowConstraints({ components: { schemas } }), {
    nameMaximumBytes: 37,
    minimumStatuses: 1,
    maximumStatuses: 12,
    maximumTransitions: 19,
    origins: ['manual', 'source'],
    evidence: ['accepted_execution'],
  });
  assert.throws(() => generator.issueWorkflowConstraints({ components: { schemas: {} } }));
});
