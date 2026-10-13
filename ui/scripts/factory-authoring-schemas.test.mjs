import { URL } from 'node:url';
import assert from 'node:assert/strict';
import test from 'node:test';
import { factoryAuthoringSchemas } from './factory-authoring-schemas.mjs';
test('exports only reachable authoring schemas and retains exact server-owned bounds', () => {
  const schemas = {
    FactoryConfigurationDefinition: { $ref: '#/components/schemas/FactoryBudget' },
    FactoryFlowDefinitionDraft: {
      type: 'array',
      maxItems: 42,
      items: { $ref: '#/components/schemas/FactoryNode' },
    },
    FactoryNode: { type: 'string', maxLength: 17 },
    FactoryBudget: { type: 'number', maximum: 99 },
    HiddenRuntimeFact: { type: 'string' },
  };
  assert.deepEqual(factoryAuthoringSchemas({ components: { schemas } }), {
    FactoryConfigurationDefinition: schemas.FactoryConfigurationDefinition,
    FactoryFlowDefinitionDraft: schemas.FactoryFlowDefinitionDraft,
    FactoryNode: schemas.FactoryNode,
    FactoryBudget: schemas.FactoryBudget,
  });
});

test('runs the generated authoring contract checks in the standard UI unit and full test suites', async () => {
  const { readFile } = await import('node:fs/promises');
  const manifest = JSON.parse(await readFile(new URL('../package.json', import.meta.url), 'utf8'));
  assert.match(manifest.scripts['test:unit'], /factory-authoring-schemas\.test\.mjs/u);
  assert.match(manifest.scripts.test, /factory-authoring-schemas\.test\.mjs/u);
});
