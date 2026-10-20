import { URL } from 'node:url';
import assert from 'node:assert/strict';
import test from 'node:test';
import { issueConstraints } from './issue-constraints.mjs';

function contract() {
  return {
    paths: {
      '/api/v1/projects/{project_id}/issues': {
        get: { parameters: [{ name: 'limit', schema: { default: 3 } }] },
      },
    },
    components: {
      schemas: {
        IssueCommentBody: { type: 'string', maxLength: 31, 'x-max-utf8-bytes': 7 },
        IssuePlanning: { properties: { labels: { maxItems: 3 } } },
        IssueContent: {
          properties: {
            title: { 'x-max-utf8-bytes': 37 },
            description: { 'x-max-utf8-bytes': 1024 },
            acceptance: { anyOf: [{ type: 'string', 'x-max-utf8-bytes': 2048 }, { type: 'null' }] },
            priority: { anyOf: [{ type: 'integer', minimum: 0, maximum: 9 }, { type: 'null' }] },
          },
        },
        TransitionIssueRequest: {
          properties: {
            reason: {
              anyOf: [{ type: 'string', maxLength: 80, 'x-max-utf8-bytes': 7 }, { type: 'null' }],
            },
          },
        },
      },
    },
  };
}

test('derives reason UTF-8 bytes independently of its character bound', () => {
  assert.deepEqual(issueConstraints(contract()), {
    commentMaximumBytes: 7,
    labelsMaximumCount: 3,
    titleMaximumBytes: 37,
    descriptionMaximumBytes: 1024,
    acceptanceMaximumBytes: 2048,
    priorityMinimum: 0,
    priorityMaximum: 9,
    defaultPageSize: 3,
    reasonMaximumBytes: 7,
  });
});

test('runs task constraint regression tests in both standard UI suites', async () => {
  const { readFile } = await import('node:fs/promises');
  const manifest = JSON.parse(await readFile(new URL('../package.json', import.meta.url), 'utf8'));
  assert.match(manifest.scripts['test:unit'], /issue-constraints\.test\.mjs/u);
  assert.match(manifest.scripts.test, /issue-constraints\.test\.mjs/u);
});

test('derives the maximum selected labels from the planning schema', () => {
  const document = contract();
  document.components.schemas.IssuePlanning = { properties: { labels: { maxItems: 2 } } };
  assert.equal(issueConstraints(document).labelsMaximumCount, 2);
});
