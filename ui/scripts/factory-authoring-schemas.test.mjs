import { URL } from 'node:url';
import assert from 'node:assert/strict';
import test from 'node:test';
import { factoryAuthoringSchemas } from './factory-authoring-schemas.mjs';
const commandSchemas = {
  FactoryRuntimeSettingsRequest: {},
  AdmitManualFactoryWorkRequest: {},
  FactoryNodeOperationRequest: {},
  FactoryNodeResponseRequest: {},
  ImportProjectSkillRequest: {
    type: 'object',
    properties: { identity: { pattern: '^[a-z]+$', maxLength: 29 } },
  },
};
test('exports Runtime MCP authoring bounds from the REST contract', () => {
  const result = factoryAuthoringSchemas({
    components: {
      schemas: {
        ...commandSchemas,
        FactoryConfigurationDefinition: {},
        FactoryFlowDefinitionDraft: {},
        FactoryRuntimeSettingsRequest: { $ref: '#/components/schemas/FactoryMcpServer' },
        FactoryMcpServer: { type: 'array', maxItems: 23, items: { type: 'string', maxLength: 71 } },
      },
    },
  });
  assert.equal(result.FactoryMcpServer?.maxItems, 23);
  assert.equal(result.FactoryMcpServer?.items.maxLength, 71);
});
test('exports only reachable authoring schemas and retains exact server-owned bounds', () => {
  const schemas = {
    ...commandSchemas,
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
    ...commandSchemas,
    FactoryConfigurationDefinition: schemas.FactoryConfigurationDefinition,
    FactoryFlowDefinitionDraft: schemas.FactoryFlowDefinitionDraft,
    FactoryNode: schemas.FactoryNode,
    FactoryBudget: schemas.FactoryBudget,
  });
});

test('retains authoritative Work and operator-command bounds through their referenced schemas', () => {
  const schemas = {
    ...commandSchemas,
    FactoryConfigurationDefinition: {},
    FactoryFlowDefinitionDraft: {},
    AdmitManualFactoryWorkRequest: { $ref: '#/components/schemas/FactoryWorkClassification' },
    FactoryWorkClassification: { type: 'object', properties: { priority: { maximum: 37 } } },
    FactoryNodeOperationRequest: { type: 'object', properties: { reason: { maxLength: 19 } } },
  };
  const result = factoryAuthoringSchemas({ components: { schemas } });
  assert.equal(result.FactoryWorkClassification?.properties.priority.maximum, 37);
  assert.equal(result.FactoryNodeOperationRequest?.properties.reason.maxLength, 19);
});

test('runs the generated authoring contract checks in the standard UI unit and full test suites', async () => {
  const { readFile } = await import('node:fs/promises');
  const manifest = JSON.parse(await readFile(new URL('../package.json', import.meta.url), 'utf8'));
  assert.match(manifest.scripts['test:unit'], /factory-authoring-schemas\.test\.mjs/u);
  assert.match(manifest.scripts.test, /factory-authoring-schemas\.test\.mjs/u);
});

test('ships validation facts without documentation strings and preserves literal values and property names', () => {
  const schemas = {
    ...commandSchemas,
    FactoryConfigurationDefinition: {
      description: 'Documentation only',
      type: 'object',
      properties: {
        description: { type: 'string', maxLength: 17, description: 'Field documentation' },
        payload: { const: { description: 'Literal input value' } },
      },
    },
    FactoryFlowDefinitionDraft: {},
  };
  const result = factoryAuthoringSchemas({ components: { schemas } });
  assert.equal(result.FactoryConfigurationDefinition.description, undefined);
  assert.deepEqual(result.FactoryConfigurationDefinition.properties.description, {
    type: 'string',
    maxLength: 17,
  });
  assert.deepEqual(result.FactoryConfigurationDefinition.properties.payload.const, {
    description: 'Literal input value',
  });
  assert.equal(schemas.FactoryConfigurationDefinition.description, 'Documentation only');
});
