import { it, expect } from 'vitest';
import { configurationFixture } from '../../../tests/fixtures/factory';
import { validateFlowDraft, validateConfigurationDraft } from './validateDraft';
import type { components } from '../../../.generated/api/schema';
import {
  factoryRoleFixture,
  factoryProfileConfigurationFixture,
} from '../../../tests/fixtures/factoryRoles';
import { emptyFactoryPermissions } from './emptyFlowDraft';
function draft(): components['schemas']['FactoryFlowDefinitionDraft'] {
  const flow = configurationFixture().configuration.flow;
  const definition = flow.closure.definitions.find(
    (item) => item.reference.id === flow.closure.root.id,
  )!;
  const { reference, ...root } = definition;
  void reference;
  return { root, data_schemas: flow.data_schemas, limits: flow.limits };
}
it('identifies mismatched entry input and each insufficient enclosing access ceiling', () => {
  const value = draft();
  value.root.input_schema = null;
  value.root.nodes[0]!.permissions = emptyFactoryPermissions();
  value.root.nodes[0]!.permissions.network_hosts = ['read.example'];
  value.root.execution.permissions = emptyFactoryPermissions();
  value.limits.permissions = emptyFactoryPermissions();
  expect(validateFlowDraft(value)).toEqual(
    expect.arrayContaining([
      { field: '/root/input_schema', code: 'entry_input' },
      { field: '/root/execution/permissions', code: 'enclosing_permissions' },
    ]),
  );
  value.root.execution.permissions.network_hosts = ['read.example'];
  expect(validateFlowDraft(value)).toEqual(
    expect.arrayContaining([{ field: '/limits/permissions', code: 'enclosing_permissions' }]),
  );
});
it.each(['prepare-tests', 'analyze-ci'])(
  'rejects leftover access after switching to the %s execution mode without changing the draft',
  (mode) => {
    const { configuration } = factoryProfileConfigurationFixture();
    const { reference: _reference, ...root } = configuration.flow.closure.definitions[0]!;
    void _reference;
    const value = {
      root,
      limits: configuration.flow.limits,
      data_schemas: configuration.flow.data_schemas,
    };
    const index = root.nodes.findIndex((node) => node.profile_selection);
    const node = root.nodes[index]!;
    const candidate = Object.values(node.profile_selection!.candidates)[0]!;
    candidate.profile.definition.execution_binding = 'step';
    candidate.profile.definition.permissions = {
      ...emptyFactoryPermissions(),
      secret_profiles: ['prepare-tests', 'analyze-ci'],
    };
    candidate.build.execution_permissions = {
      ...emptyFactoryPermissions(),
      secret_profiles: [mode],
    };
    node.permissions = structuredClone(candidate.profile.definition.permissions);
    const before = structuredClone(value);
    expect(validateFlowDraft(value)).toContainEqual({
      field: `/root/nodes/${index}/permissions`,
      code: 'execution_mode_permissions',
    });
    expect(value).toEqual(before);
    node.permissions = structuredClone(candidate.build.execution_permissions);
    expect(
      validateFlowDraft(value).filter((issue) => issue.code === 'execution_mode_permissions'),
    ).toEqual([]);
  },
);
it('accepts the same exact plugin access from profile and resource responses regardless of object key order', () => {
  const value = draft();
  const plugin = { identity: 'codex', version: '0.8.0', digest: '7'.repeat(64) };
  value.root.execution.permissions.plugins = [plugin];
  value.limits.permissions.plugins = [plugin];
  value.root.nodes[0]!.permissions.plugins = [
    { digest: plugin.digest, identity: plugin.identity, version: plugin.version },
  ];
  expect(validateFlowDraft(value)).toEqual([]);
  value.root.nodes[0]!.permissions.plugins[0]!.digest = '8'.repeat(64);
  expect(validateFlowDraft(value)).toContainEqual({
    field: '/root/execution/permissions',
    code: 'enclosing_permissions',
  });
});
it('accepts a manually broader mount and fixed-length command pattern that already cover profile requirements', () => {
  const { configuration } = factoryProfileConfigurationFixture();
  const { reference: _reference, ...root } = configuration.flow.closure.definitions[0]!;
  void _reference;
  const node = root.nodes.find((node) => node.profile_selection)!;
  const candidate = Object.values(node.profile_selection!.candidates)[0]!;
  const executable = candidate.profile.definition.toolchain.executable;
  candidate.profile.definition.permissions = emptyFactoryPermissions();
  candidate.profile.definition.permissions.mounts = [
    { root: '/workspace/source', mode: 'read_only' },
  ];
  candidate.profile.definition.permissions.executables = [executable];
  candidate.profile.definition.permissions.commands = [
    { executable, arguments: [{ kind: 'exact', value: '--version' }] },
  ];
  node.permissions = emptyFactoryPermissions();
  node.permissions.mounts = [{ root: '/workspace', mode: 'read_write' }];
  node.permissions.executables = [executable];
  node.permissions.commands = [{ executable, arguments: [{ kind: 'any' }] }];
  const issues = validateFlowDraft({
    root,
    limits: configuration.flow.limits,
    data_schemas: configuration.flow.data_schemas,
  });
  expect(issues.filter((issue) => issue.code === 'profile_permissions')).toEqual([]);
});
it('points to an unrouted outcome and missing completion without requiring a server round trip', () => {
  const value = draft();
  value.root.transitions = [];
  value.root.terminals = [];
  expect(validateFlowDraft(value)).toEqual(
    expect.arrayContaining([
      { field: '/root/terminals', code: 'missing_terminal' },
      { field: '/root/nodes/0/outcomes/0', code: 'missing_route' },
    ]),
  );
});
it('identifies missing executor input and profile access before the server rejects the whole step', () => {
  const value = draft();
  const { profile, role } = factoryRoleFixture();
  profile.definition.permissions = emptyFactoryPermissions();
  profile.definition.permissions.network_hosts = ['required.example'];
  const node = value.root.nodes[0]!;
  node.kind = 'reasoning';
  const binding = {
    build_configuration: profile.definition.build_configuration,
    tool: profile.definition.toolchain.executable,
    plugin: profile.definition.toolchain.plugin,
    model_or_tool: profile.definition.toolchain.model,
    task_digest: 'd'.repeat(64),
    result_output: 'result',
    result_output_kind: 'report' as const,
    result_outcome: node.outcomes[0]!.key,
  };
  node.action = null;
  node.build = null;
  node.input_schema = null;
  node.permissions = emptyFactoryPermissions();
  node.budget = {
    max_attempts: 1,
    max_elapsed_millis: 1,
    max_tokens: 1,
    max_cost_micro_units: 1,
    max_output_bytes: 1,
  };
  profile.definition.context_requirements = ['required_document'];
  node.profile_selection = {
    role: role.reference,
    candidates: { selected: { profile, build: binding } },
    policy: { mode: 'manual', candidate: 'selected' },
    failure: { kind: 'manual_selection' },
    selection_budget: {
      attempts: 0,
      elapsed_millis: 0,
      tokens: 0,
      cost_micro_units: 0,
      output_bytes: 0,
    },
  };
  expect(validateFlowDraft(value)).toEqual(
    expect.arrayContaining([
      { field: '/root/nodes/0/input_schema', code: 'missing_input' },
      { field: '/root/nodes/0/permissions', code: 'profile_permissions' },
      { field: '/root/nodes/0/budget', code: 'profile_budget' },
      { field: '/root/nodes/0/context_binding/entries/required_document', code: 'profile_context' },
    ]),
  );
});
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
  if (value.root.entry === successor.key) value.root.input_schema = target.reference;
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
  if (value.root.entry === successor.key) value.root.input_schema = text.reference;
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
      expect.objectContaining({
        field: '/root/nodes/0/budget/max_attempts',
        code: 'budget_exceeded',
      }),
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
  if (value.root.entry === successor.key) value.root.input_schema = target.reference;
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

it('accepts a HumanGate reached only after exhausting a repeat', () => {
  const value = draft();
  const node = value.root.nodes[0]!;
  const gateIndex = value.root.nodes.length;
  value.root.nodes.push({
    ...node,
    key: 'review-exhaustion',
    kind: 'human_gate',
    build: null,
    operator_requests: { operation: 'escalation', outcomes: { abort: node.outcomes[0]!.key } },
  });
  const edge = {
    predecessor: node.key,
    outcome: node.outcomes[0]!.key,
    target: { kind: 'node' as const, key: node.key },
    max_repeats: 1,
    exhausted_terminal: null,
    exhausted_node: 'review-exhaustion',
  };
  value.root.transitions.push(edge);
  expect(validateFlowDraft(value)).not.toEqual(
    expect.arrayContaining([{ field: `/root/nodes/${gateIndex}`, code: 'unreachable_node' }]),
  );
  expect(
    validateFlowDraft(value).some(
      (issue) =>
        issue.field === `/root/transitions/${value.root.transitions.length - 1}/max_repeats`,
    ),
  ).toBe(false);
});
