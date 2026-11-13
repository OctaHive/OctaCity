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
        IssueOutcomeCard: {
          properties: { conclusion: { maxLength: 92, 'x-max-utf8-bytes': 73 } },
        },
        IssuePlanning: {
          properties: {
            labels: { maxItems: 3 },
            size: {
              anyOf: [{ type: 'string', enum: ['small', 'medium', 'large'] }, { type: 'null' }],
            },
          },
        },
        IssuePropertyDefinitionSet: {
          properties: {
            priorities: { default: [{ id: 'urgent', name: 'Urgent', retired: false }] },
            types: { default: [{ id: 'request', name: 'Request', retired: false }] },
          },
        },
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

test('derives reason and outcome conclusion limits in UTF-8 bytes', () => {
  assert.deepEqual(issueConstraints(contract()), {
    commentMaximumBytes: 7,
    labelsMaximumCount: 3,
    defaultPriorities: [{ id: 'urgent', name: 'Urgent', retired: false }],
    defaultTypes: [{ id: 'request', name: 'Request', retired: false }],
    sizes: ['small', 'medium', 'large'],
    titleMaximumBytes: 37,
    descriptionMaximumBytes: 1024,
    acceptanceMaximumBytes: 2048,
    priorityMinimum: 0,
    priorityMaximum: 9,
    defaultPageSize: 3,
    reasonMaximumBytes: 7,
    outcomeConclusionMaximumBytes: 73,
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
  document.components.schemas.IssuePlanning.properties.labels.maxItems = 2;
  assert.equal(issueConstraints(document).labelsMaximumCount, 2);
});

test('retains legacy numeric bounds when Issue priority also accepts Project choice IDs', () => {
  const document = contract();
  document.components.schemas.IssueContent.properties.priority = {
    anyOf: [
      {
        oneOf: [
          { type: 'string', minLength: 1 },
          { type: 'integer', minimum: 0, maximum: 255 },
        ],
      },
      { type: 'null' },
    ],
  };
  assert.equal(issueConstraints(document).priorityMaximum, 255);
  assert.equal(issueConstraints(document).priorityMinimum, 0);
});

test('generated Issue catalogs keep default choice lists optional on the wire', async () => {
  const { readFile } = await import('node:fs/promises');
  const ts = await import('typescript');
  const text = await readFile(new URL('../.generated/api/schema.d.ts', import.meta.url), 'utf8');
  const source = ts.createSourceFile('schema.d.ts', text, ts.ScriptTarget.Latest, true);
  const components = source.statements.find(
    (node) => ts.isInterfaceDeclaration(node) && node.name.text === 'components',
  );
  const property = (type, name) =>
    type.members.find((member) => member.name?.getText(source) === name);
  const schemas = property(components, 'schemas').type;
  const catalog = property(schemas, 'IssuePropertyDefinitionSet').type;
  for (const field of ['priorities', 'types']) {
    assert.ok(
      property(catalog, field).questionToken,
      `${field} may be omitted to use Project defaults`,
    );
  }
  for (const field of ['labels', 'properties']) {
    assert.equal(property(catalog, field).questionToken, undefined, `${field} remains required`);
  }
});
