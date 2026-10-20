import { URL } from 'node:url';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import { issueSourceConstraints } from './issue-constraints.mjs';

function contract() {
  return {
    components: {
      schemas: {
        IssueSourceMappingDefinition: { 'x-max-utf8-bytes': 77 },
        IssueSourceStatusMapping: { properties: { pull: { maxItems: 2 } } },
        WorkSourceStatusMapping: { properties: { statuses: { maxItems: 3 } } },
        WorkSourceStatusEffects: { properties: { add: { maxItems: 5 } } },
        WorkSourcePredicate: {
          properties: {
            state: {
              anyOf: [{ type: 'string', maxLength: 99, 'x-max-utf8-bytes': 11 }, { type: 'null' }],
            },
            all_of: { maxItems: 4, items: { 'x-max-utf8-bytes': 13 } },
          },
        },
        IssueSourceItem: { properties: { item: { 'x-max-utf8-bytes': 17 } } },
      },
    },
  };
}
test('derives task Source form bytes and row ceilings from the shared published contract', () => {
  assert.deepEqual(issueSourceConstraints(contract()), {
    mappingMaximumBytes: 77,
    pullMaximumRows: 2,
    pushMaximumRows: 3,
    predicateMaximumLabels: 4,
    effectMaximumLabels: 5,
    stateMaximumBytes: 11,
    labelMaximumBytes: 13,
    nativeIdentityMaximumBytes: 17,
  });
  const incomplete = contract();
  delete incomplete.components.schemas.WorkSourcePredicate.properties.state.anyOf[0][
    'x-max-utf8-bytes'
  ];
  assert.throws(() => issueSourceConstraints(incomplete), /Missing Issue Source form bound/u);
});
test('keeps the Source constraint regression in both standard UI suites', async () => {
  const manifest = JSON.parse(await readFile(new URL('../package.json', import.meta.url), 'utf8'));
  assert.match(manifest.scripts['test:unit'], /issue-source-constraints\.test\.mjs/u);
  assert.match(manifest.scripts.test, /issue-source-constraints\.test\.mjs/u);
});
