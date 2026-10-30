import { it, expect } from 'vitest';
import { configurationFixture } from '../../../tests/fixtures/factory';
import { validateFlowDraft, validateConfigurationDraft } from './validateDraft';
import type { components } from '../../../.generated/api/schema';
function draft(): components['schemas']['FactoryFlowDefinitionDraft'] {
  const flow = configurationFixture().configuration.flow;
  const definition = flow.closure.definitions.find(
    (item) => item.reference.id === flow.closure.root.id,
  )!;
  const { reference, ...root } = definition;
  void reference;
  return { root, data_schemas: flow.data_schemas, limits: flow.limits };
}
it('compares exact decimal schema bounds numerically when reviewing a projection', () => {
  const value = draft();
  const predecessor = value.root.nodes[0]!;
  const successor = value.root.nodes[1]!;
  const source: components['schemas']['FactoryFlowDataSchema'] = {
    reference: { identity: 'operator.decimal-source', version: 'v1', digest: '1'.repeat(64) },
    shape: { type: 'integer', minimum: '10', maximum: '99' },
  };
  const target: components['schemas']['FactoryFlowDataSchema'] = {
    reference: { identity: 'operator.decimal-target', version: 'v1', digest: '2'.repeat(64) },
    shape: { type: 'integer', minimum: '2', maximum: '100' },
  };
  value.data_schemas.push(source, target);
  predecessor.outcomes[0]!.schema = source.reference;
  successor.input_schema = target.reference;
  successor.input_binding = null;
  value.root.data_projections = [
    {
      predecessor: predecessor.key,
      outcome: predecessor.outcomes[0]!.key,
      successor: successor.key,
      schema: source.reference,
    },
  ];
  expect(validateFlowDraft(value)).toEqual([]);
});
it('rejects malformed generated fields, missing contracts, unreachable nodes and unbounded cycles before any publication', () => {
  const value = draft();
  value.root.nodes[0]!.key = 'Bad name';
  value.root.nodes[1]!.budget.max_attempts = Number.NaN;
  value.root.transitions.push({
    predecessor: value.root.entry,
    outcome: value.root.nodes.find((node) => node.key === value.root.entry)!.outcomes[0]!.key,
    target: { kind: 'node', key: value.root.entry },
    max_repeats: 0,
    exhausted_terminal: null,
  });
  value.root.nodes.push({
    ...structuredClone(value.root.nodes[0]!),
    key: 'disconnected',
    outcomes: [],
  });
  value.root.terminals[0]!.schema = {
    identity: 'missing.contract',
    version: 'v1',
    digest: '9'.repeat(64),
  };
  const issues = validateFlowDraft(value);
  expect(issues).toContainEqual({ field: '/root/nodes/0/key', code: 'invalid_contract' });
  expect(issues).toContainEqual({
    field: '/root/nodes/1/budget/max_attempts',
    code: 'invalid_contract',
  });
  expect(issues).toContainEqual({
    field: '/root/terminals/0/schema',
    code: 'unavailable_reference',
  });
  expect(issues.some((issue) => issue.code === 'unbounded_cycle')).toBe(true);
  expect(issues).toContainEqual({ field: '/root/nodes/3', code: 'unreachable_node' });
  const configuration = configurationFixture().configuration;
  expect(
    validateConfigurationDraft({
      enabled: true,
      admission_policy: '',
      permission_ceiling: 'missing alias with spaces',
      flow: configuration.flow,
      hard_budget: configuration.hard_budget,
      wip_limits: configuration.wip_limits,
    }),
  ).toEqual(
    expect.arrayContaining([
      { field: '/admission_policy', code: 'invalid_contract' },
      { field: '/permission_ceiling', code: 'invalid_contract' },
    ]),
  );
});

it('accepts the canonical executable draft and a finite cycle with an explicit exhaustion terminal', () => {
  const value = draft();
  expect(validateFlowDraft(value)).toEqual([]);
  value.root.transitions.push({
    predecessor: value.root.entry,
    outcome: value.root.nodes.find((node) => node.key === value.root.entry)!.outcomes[0]!.key,
    target: { kind: 'node', key: value.root.entry },
    max_repeats: 1,
    exhausted_terminal: value.root.terminals[0]!.key,
  });
  expect(validateFlowDraft(value)).toEqual([]);
});

it('rejects an incompatible projected schema and a widened node budget while preserving valid independent sections', () => {
  const value = draft();
  const predecessor = value.root.nodes[0]!;
  const successor = value.root.nodes[1]!;
  const outcome = predecessor.outcomes[0]!;
  const text = {
    reference: { identity: 'operator.text', version: 'v1', digest: '6'.repeat(64) },
    shape: { type: 'string' as const, min_bytes: 1, max_bytes: 128 },
  };
  value.data_schemas.push(text);
  successor.input_schema = text.reference;
  successor.input_binding = null;
  value.root.data_projections = [
    {
      predecessor: predecessor.key,
      outcome: outcome.key,
      successor: successor.key,
      schema: outcome.schema,
    },
  ];
  predecessor.budget.max_attempts = value.root.execution.budget.max_attempts + 1;
  expect(validateFlowDraft(value)).toEqual(
    expect.arrayContaining([
      { field: '/root/data_projections/0/schema', code: 'incompatible_schema' },
      { field: '/root/nodes/0/budget/max_attempts', code: 'budget_exceeded' },
    ]),
  );
  expect(value.root.context_projections).toEqual([]);
});

it('checks every declared tag and payload range when projecting externally tagged values', () => {
  const value = draft();
  const predecessor = value.root.nodes[0]!;
  const successor = value.root.nodes[1]!;
  const source: components['schemas']['FactoryFlowDataSchema'] = {
    reference: { identity: 'operator.source-tags', version: 'v1', digest: '1'.repeat(64) },
    shape: {
      type: 'externally_tagged',
      variants: { Ready: null, RetryAfter: { type: 'integer', minimum: 1, maximum: 3 } },
    },
  };
  const target: components['schemas']['FactoryFlowDataSchema'] = {
    reference: { identity: 'operator.target-tags', version: 'v1', digest: '2'.repeat(64) },
    shape: {
      type: 'externally_tagged',
      variants: { Ready: null, RetryAfter: { type: 'integer', minimum: 1, maximum: 10 } },
    },
  };
  value.data_schemas.push(source, target);
  predecessor.outcomes[0]!.schema = source.reference;
  successor.input_schema = target.reference;
  successor.input_binding = null;
  value.root.data_projections = [
    {
      predecessor: predecessor.key,
      outcome: predecessor.outcomes[0]!.key,
      successor: successor.key,
      schema: source.reference,
    },
  ];
  expect(validateFlowDraft(value)).toEqual([]);
  const incompatible = { field: '/root/data_projections/0/schema', code: 'incompatible_schema' };
  for (const shape of [
    { type: 'externally_tagged' as const, variants: { RetryAfter: null } },
    { type: 'externally_tagged' as const, variants: { Ready: { type: 'boolean' as const } } },
    {
      type: 'externally_tagged' as const,
      variants: { RetryAfter: { type: 'integer' as const, minimum: 1, maximum: 11 } },
    },
    { type: 'externally_tagged' as const, variants: { ready: null } },
  ]) {
    source.shape = shape;
    expect(validateFlowDraft(value)).toContainEqual(incompatible);
  }
  source.shape = { type: 'enum', values: ['Ready'] };
  expect(validateFlowDraft(value)).toEqual([]);
  source.shape = {
    type: 'externally_tagged',
    variants: Object.fromEntries([
      ['__proto__', null],
      ['constructor', null],
    ]),
  };
  target.shape = {
    type: 'externally_tagged',
    variants: Object.fromEntries([
      ['__proto__', null],
      ['constructor', null],
    ]),
  };
  expect(validateFlowDraft(value)).toEqual([]);
  target.shape = { type: 'externally_tagged', variants: { Ready: null } };
  expect(validateFlowDraft(value)).toContainEqual(incompatible);
  source.shape = {
    type: 'externally_tagged',
    variants: { Wrapped: { type: 'object', fields: {} } },
  };
  target.shape = {
    type: 'externally_tagged',
    variants: {
      Wrapped: {
        type: 'object',
        fields: Object.fromEntries(
          ['constructor'].map((name) => [
            name,
            { required: false, value: { type: 'boolean' as const } },
          ]),
        ),
      },
    },
  };
  expect(validateFlowDraft(value)).toEqual([]);
});
