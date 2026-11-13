// @vitest-environment jsdom
import { QueryClientProvider } from '@tanstack/react-query';
import { cleanup, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, expect, it, vi } from 'vitest';
import { useState } from 'react';
import type { components } from '../../../.generated/api/schema';
import { createConsoleQueryClient } from '../../app/query';
import { configurationFixture } from '../../../tests/fixtures/factory';
import { factoryIssueFieldsFixture } from '../../../tests/fixtures/factoryIssueFields';
import { factoryApi, type FactoryApi } from './api';
import { StepResults, type ResultDraft } from './StepResults';
import { FlowIssueClassification } from './FlowIssueClassification';
import { emptyFlowDraft } from './emptyFlowDraft';
import { FlowGraphEditor } from './FlowGraphEditor';
import { FlowIssueSnapshot } from './FlowIssueSnapshot';
import { IssueInputReview } from './IssueInputReview';

afterEach(cleanup);

it('requires an explicit optional downgrade when a newer catalog cannot provide a required field', async () => {
  const user = userEvent.setup();
  const capabilities = factoryIssueFieldsFixture();
  capabilities.issue_fields.definition_version = 8;
  const component = capabilities.issue_fields.fields.find(
    (field) => field.source.kind === 'component',
  )!;
  component.required_supported = false;
  component.shape = { type: 'string', min_bytes: 1, max_bytes: 128 };
  const draft = emptyFlowDraft();
  const schema = {
    reference: { identity: 'snapshot', version: '1', digest: 'a'.repeat(64) },
    shape: {
      type: 'object' as const,
      fields: {
        component: { required: true, value: { type: 'enum' as const, values: ['payments'] } },
      },
    },
  };
  draft.data_schemas = [schema];
  draft.issue_snapshot = {
    version: 1,
    definition_version: 7,
    schema: schema.reference,
    fields: { component: { source: { kind: 'component' }, required: true } },
  };
  draft.root.nodes = [
    {
      ...configurationFixture().configuration.flow.closure.definitions[0]!.nodes[0]!,
      key: 'coder',
      input_schema: schema.reference,
      input_binding: {
        sources: { issue: { source: 'incoming', schema: schema.reference, path: '' } },
        mapping: { value: { mapping: 'pointer', path: '/issue' } },
      },
    },
  ];
  const api = {
    ...factoryApi,
    getAuthoringCapabilities: vi.fn<FactoryApi['getAuthoringCapabilities']>(
      async () => capabilities,
    ),
  };
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <FlowIssueSnapshot
        value={draft}
        services={{ api, projectId: 'delivery' }}
        onChange={vi.fn()}
        onEditingChange={vi.fn()}
      />
    </QueryClientProvider>,
  );
  await user.click(screen.getByRole('button', { name: 'Configure Issue snapshot' }));
  const required = await screen.findByRole('checkbox', { name: 'Required: Issue component' });
  expect(screen.getByRole('button', { name: 'Use Issue snapshot' })).toHaveProperty(
    'disabled',
    true,
  );
  expect(required).toHaveProperty('disabled', false);
  await user.click(required);
  expect(screen.getByRole('button', { name: 'Use Issue snapshot' })).toHaveProperty(
    'disabled',
    false,
  );
});

it('reviews the declared fields of a complete Incoming snapshot projection', () => {
  const flow = configurationFixture().configuration.flow;
  const reference = flow.data_schemas[0]!.reference;
  flow.issue_snapshot = {
    version: 1,
    definition_version: 7,
    schema: reference,
    fields: { component: { source: { kind: 'component' }, required: true } },
    materials: {
      context: { source: { kind: 'selected', max_entries: 2 }, max_bytes: 2048, required: false },
    },
  };
  const node = flow.closure.definitions[0]!.nodes[0]!;
  node.input_binding = {
    sources: { issue: { source: 'incoming', schema: reference, path: '' } },
    mapping: { value: { mapping: 'pointer', path: '/issue' } },
  };
  render(<IssueInputReview flow={flow} />);
  expect(screen.getByText(`${node.key} · component, context`)).toBeTruthy();
});

it('keeps a mixed Incoming/record binding intact when snapshot projections are changed', async () => {
  const user = userEvent.setup();
  const draft = emptyFlowDraft();
  const template = configurationFixture().configuration.flow.closure.definitions[0]!.nodes[0]!;
  const reference = { identity: 'issue-input', version: '1', digest: 'a'.repeat(64) };
  draft.data_schemas = [
    {
      reference,
      shape: {
        type: 'object',
        fields: { component: { required: true, value: { type: 'enum', values: ['payments'] } } },
      },
    },
  ];
  draft.issue_snapshot = {
    version: 1,
    definition_version: 7,
    schema: reference,
    fields: { component: { source: { kind: 'component' }, required: true } },
  };
  draft.root.entry = 'coder';
  draft.root.nodes = [
    {
      ...template,
      key: 'coder',
      input_schema: reference,
      input_binding: {
        sources: {
          issue: { source: 'incoming', schema: reference, path: '' },
          prior: { source: 'record', node: 'assess', outcome: 'done', path: '', view: 'payload' },
        },
        mapping: { value: { mapping: 'pointer', path: '/issue' } },
      },
    },
    { ...template, key: 'tester', input_schema: null, input_binding: null },
  ];
  const onChange = vi.fn();
  const api = {
    ...factoryApi,
    getAuthoringCapabilities: vi.fn<FactoryApi['getAuthoringCapabilities']>(async () =>
      factoryIssueFieldsFixture(),
    ),
  };
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <FlowIssueSnapshot
        value={draft}
        services={{ api, projectId: 'delivery' }}
        onChange={onChange}
        onEditingChange={vi.fn()}
      />
    </QueryClientProvider>,
  );
  await user.click(screen.getByRole('button', { name: 'Configure Issue snapshot' }));
  await user.click(await screen.findByRole('checkbox', { name: 'coder: Issue component' }));
  await user.click(screen.getByRole('checkbox', { name: 'tester: Issue component' }));
  expect(screen.getByRole('button', { name: 'Use Issue snapshot' })).toHaveProperty(
    'disabled',
    true,
  );
  expect(onChange).not.toHaveBeenCalled();
});

it('rejects an oversized optional snapshot projection before schema normalization', async () => {
  const user = userEvent.setup();
  const draft = emptyFlowDraft();
  draft.root.nodes = [
    {
      ...configurationFixture().configuration.flow.closure.definitions[0]!.nodes[0]!,
      key: 'coder',
      input_schema: null,
      input_binding: null,
    },
  ];
  draft.root.entry = 'coder';
  const api = {
    ...factoryApi,
    getAuthoringCapabilities: vi.fn<FactoryApi['getAuthoringCapabilities']>(async () =>
      factoryIssueFieldsFixture(),
    ),
    normalizeSchema: vi.fn<FactoryApi['normalizeSchema']>(async (body) => ({
      reference: { identity: body.identity, version: body.version, digest: 'c'.repeat(64) },
      shape: body.shape,
    })),
  };
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <FlowIssueSnapshot
        value={draft}
        services={{ api, projectId: 'delivery' }}
        onChange={vi.fn()}
        onEditingChange={vi.fn()}
      />
    </QueryClientProvider>,
  );
  await user.click(screen.getByRole('button', { name: 'Configure Issue snapshot' }));
  await screen.findByRole('checkbox', { name: 'Issue component' });
  for (const name of [
    'Title',
    'Priority',
    'Issue component',
    'Issue type',
    'Issue size',
    'Project field: Environment',
    'Classification: Impact',
  ]) {
    await user.click(screen.getByRole('checkbox', { name }));
    await user.click(screen.getByRole('checkbox', { name: `coder: ${name}` }));
  }
  await user.click(
    screen.getByRole('checkbox', { name: 'Include explicitly selected Issue materials' }),
  );
  await user.click(screen.getByRole('button', { name: 'Use Issue snapshot' }));
  expect(
    await screen.findByText(
      'This projection exceeds the input mapping limits. Select fewer optional fields or use the advanced input editor.',
    ),
  ).toBeTruthy();
  expect(api.normalizeSchema).not.toHaveBeenCalled();
});

it('accepts the complete optional snapshot input without expanding missing fields into a decision tree', async () => {
  const user = userEvent.setup();
  const draft = emptyFlowDraft();
  draft.root.nodes = [
    {
      ...configurationFixture().configuration.flow.closure.definitions[0]!.nodes[0]!,
      key: 'coder',
      input_schema: null,
      input_binding: null,
    },
  ];
  draft.root.entry = 'coder';
  const api = {
    ...factoryApi,
    getAuthoringCapabilities: vi.fn<FactoryApi['getAuthoringCapabilities']>(async () =>
      factoryIssueFieldsFixture(),
    ),
    normalizeSchema: vi.fn<FactoryApi['normalizeSchema']>(async (body) => ({
      reference: { identity: body.identity, version: body.version, digest: 'c'.repeat(64) },
      shape: body.shape,
    })),
  };
  const onChange = vi.fn();
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <FlowIssueSnapshot
        value={draft}
        services={{ api, projectId: 'delivery' }}
        onChange={onChange}
        onEditingChange={vi.fn()}
      />
    </QueryClientProvider>,
  );
  await user.click(screen.getByRole('button', { name: 'Configure Issue snapshot' }));
  await screen.findByRole('checkbox', { name: 'Issue component' });
  for (const name of [
    'Title',
    'Priority',
    'Issue component',
    'Issue type',
    'Issue size',
    'Project field: Environment',
    'Classification: Impact',
  ]) {
    await user.click(screen.getByRole('checkbox', { name }));
    await user.click(screen.getByRole('checkbox', { name: `coder: ${name}` }));
  }
  await user.click(screen.getByRole('button', { name: 'Use Issue snapshot' }));
  await waitFor(() => expect(onChange).toHaveBeenCalledTimes(1));
  const result: components['schemas']['FactoryFlowDefinitionDraft'] = onChange.mock.calls[0]![0];
  expect(result.root.nodes[0]?.input_schema).toEqual(result.issue_snapshot?.schema);
  expect(result.root.nodes[0]?.input_binding?.mapping.value).toEqual({
    mapping: 'pointer',
    path: '/issue',
  });
  expect(api.normalizeSchema).toHaveBeenCalledTimes(1);
});

it('keeps built-in and ordinary Project sources distinct when their aliases collide', async () => {
  const user = userEvent.setup();
  const capabilities = factoryIssueFieldsFixture();
  capabilities.issue_fields.fields.push({
    source: { kind: 'property', id: 'component' },
    property_id: 'component',
    name: 'Custom component',
    required_supported: true,
    shape: { type: 'boolean' },
  });
  const api = {
    ...factoryApi,
    getAuthoringCapabilities: vi.fn<FactoryApi['getAuthoringCapabilities']>(
      async () => capabilities,
    ),
  };
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <FlowIssueSnapshot
        value={emptyFlowDraft()}
        services={{ api, projectId: 'delivery' }}
        onChange={vi.fn()}
        onEditingChange={vi.fn()}
      />
    </QueryClientProvider>,
  );
  await user.click(screen.getByRole('button', { name: 'Configure Issue snapshot' }));
  await user.click(await screen.findByRole('checkbox', { name: 'Issue component' }));
  await user.click(screen.getByRole('checkbox', { name: 'Project field: Custom component' }));
  expect(screen.getByRole('checkbox', { name: 'Issue component' })).toHaveProperty('checked', true);
  expect(screen.getByRole('checkbox', { name: 'Project field: Custom component' })).toHaveProperty(
    'checked',
    true,
  );
  expect(screen.getByRole('textbox', { name: 'Input alias: Issue component' })).toHaveProperty(
    'value',
    'component',
  );
  expect(
    screen.getByRole('textbox', { name: 'Input alias: Project field: Custom component' }),
  ).not.toHaveProperty('value', 'component');
});

it('removes an Issue snapshot and its exclusive bindings while preserving unrelated exact record inputs', async () => {
  const user = userEvent.setup();
  const draft = emptyFlowDraft();
  const schema = configurationFixture().configuration.flow.data_schemas[0]!;
  schema.shape = {
    type: 'object',
    fields: { component: { required: true, value: { type: 'enum', values: ['payments'] } } },
  };
  const node = configurationFixture().configuration.flow.closure.definitions[0]!.nodes[0]!;
  draft.data_schemas = [schema];
  draft.issue_snapshot = {
    version: 1,
    definition_version: 7,
    schema: schema.reference,
    fields: { component: { source: { kind: 'component' }, required: true } },
  };
  draft.root.entry = 'coder';
  draft.root.input_schema = schema.reference;
  draft.root.nodes = [
    {
      ...node,
      key: 'coder',
      input_schema: schema.reference,
      input_binding: {
        sources: { issue: { source: 'incoming', schema: schema.reference, path: '' } },
        mapping: { value: { mapping: 'pointer', path: '/issue' } },
      },
    },
    {
      ...node,
      key: 'apply',
      input_binding: {
        sources: {
          assessment: {
            source: 'record',
            node: 'coder',
            outcome: 'done',
            path: '',
            view: 'payload',
          },
        },
        mapping: { value: { mapping: 'pointer', path: '/assessment' } },
      },
    },
  ];
  const onChange = vi.fn();
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <FlowIssueSnapshot
        value={draft}
        services={{ api: factoryApi, projectId: 'delivery' }}
        onChange={onChange}
        onEditingChange={vi.fn()}
      />
    </QueryClientProvider>,
  );
  await user.click(screen.getByRole('button', { name: 'Remove Issue snapshot' }));
  const result: components['schemas']['FactoryFlowDefinitionDraft'] = onChange.mock.calls[0]![0];
  expect(result.issue_snapshot).toBeUndefined();
  expect(result.root.nodes[0]?.input_binding).toBeNull();
  expect(result.root.input_schema).toBeNull();
  expect(result.root.nodes[1]).toEqual(draft.root.nodes[1]);
  expect(result.data_schemas).toEqual(draft.data_schemas);
});

it('authors a versioned Issue snapshot with optional Project fields, bounded selected materials and per-step projections', async () => {
  const user = userEvent.setup();
  const draft = emptyFlowDraft();
  const template = configurationFixture().configuration.flow.closure.definitions[0]!.nodes[0]!;
  draft.root.nodes = ['coder', 'tester'].map((key) => ({
    ...template,
    key,
    kind: 'reasoning',
    input_schema: null,
    input_binding: null,
  }));
  draft.root.entry = 'coder';
  let schemaSequence = 0;
  const normalize = vi.fn<FactoryApi['normalizeSchema']>(async (body) => ({
    reference: {
      identity: body.identity,
      version: body.version,
      digest: String.fromCharCode(97 + ++schemaSequence).repeat(64),
    },
    shape: body.shape,
  }));
  const api = {
    ...factoryApi,
    normalizeSchema: normalize,
    getAuthoringCapabilities: vi.fn<FactoryApi['getAuthoringCapabilities']>(async () =>
      factoryIssueFieldsFixture(),
    ),
  };
  let selected = draft;
  function Harness() {
    const [value, setValue] = useState(draft);
    return (
      <FlowGraphEditor
        value={value}
        choices={[]}
        subflows={[]}
        authoring={{ api, projectId: 'delivery' }}
        onChange={(next) => {
          selected = next;
          setValue(next);
        }}
      />
    );
  }
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <Harness />
    </QueryClientProvider>,
  );
  await user.click(screen.getByRole('button', { name: 'Configure Issue snapshot' }));
  await user.click(await screen.findByRole('checkbox', { name: 'Issue component' }));
  await user.click(screen.getByRole('checkbox', { name: 'Project field: Environment' }));
  await user.click(screen.getByRole('checkbox', { name: 'Required: Issue component' }));
  await user.click(
    screen.getByRole('checkbox', { name: 'Include explicitly selected Issue materials' }),
  );
  await user.clear(screen.getByRole('spinbutton', { name: 'Maximum material entries' }));
  await user.type(screen.getByRole('spinbutton', { name: 'Maximum material entries' }), '3');
  await user.clear(screen.getByRole('spinbutton', { name: 'Maximum material bytes' }));
  await user.type(screen.getByRole('spinbutton', { name: 'Maximum material bytes' }), '2048');
  await user.click(screen.getByRole('checkbox', { name: 'coder: Issue component' }));
  await user.click(screen.getByRole('checkbox', { name: 'coder: Project field: Environment' }));
  await user.click(screen.getByRole('checkbox', { name: 'coder: Issue materials' }));
  await user.click(screen.getByRole('checkbox', { name: 'tester: Issue component' }));
  await user.click(screen.getByRole('button', { name: 'Use Issue snapshot' }));
  await waitFor(() => expect(selected.issue_snapshot).toBeDefined());
  expect(selected.accepted_predecessor).toBeUndefined();
  expect(selected.issue_snapshot).toMatchObject({
    version: 1,
    definition_version: 7,
    fields: {
      component: { source: { kind: 'component' }, required: true },
      environment: { source: { kind: 'property', id: 'environment' }, required: false },
    },
    materials: {
      materials: { source: { kind: 'selected', max_entries: 3 }, required: false, max_bytes: 2048 },
    },
  });
  const [coder, tester] = selected.root.nodes;
  const coderShape = selected.data_schemas.find(
    (schema) => schema.reference.digest === coder?.input_schema?.digest,
  )?.shape;
  const testerShape = selected.data_schemas.find(
    (schema) => schema.reference.digest === tester?.input_schema?.digest,
  )?.shape;
  expect(coderShape).toMatchObject({
    type: 'object',
    fields: {
      component: { required: true, value: { type: 'enum', values: ['payments'] } },
      environment: { required: false },
      materials: { required: false },
    },
  });
  expect(testerShape).toEqual({
    type: 'object',
    fields: { component: { required: true, value: { type: 'enum', values: ['payments'] } } },
  });
  expect(tester?.input_binding?.sources).toEqual({
    issue: { source: 'incoming', schema: selected.issue_snapshot!.schema, path: '' },
  });
  expect(tester?.input_binding?.mapping.value).toEqual({
    mapping: 'object',
    fields: { component: { mapping: 'pointer', path: '/issue/component' } },
  });
  expect(selected.root.input_schema).toEqual(coder?.input_schema);
  expect(selected.root.nodes.map((node) => node.permissions)).toEqual(
    draft.root.nodes.map((node) => node.permissions),
  );
});

it('applies selected Project fields without legacy labels and retains declared assessment dependencies', async () => {
  const user = userEvent.setup();
  const capabilities = factoryIssueFieldsFixture();
  const draft = emptyFlowDraft();
  const dependency = {
    type: 'object' as const,
    fields: {
      subject: { required: true, value: { type: 'string' as const, min_bytes: 1, max_bytes: 64 } },
    },
  };
  const assessment = {
    reference: { identity: 'assessment', version: 'v1', digest: 'a'.repeat(64) },
    shape: {
      type: 'object' as const,
      fields: {
        component: { required: true, value: { type: 'enum' as const, values: ['payments'] } },
        severity: { required: false, value: { type: 'enum' as const, values: ['major', 'minor'] } },
        private_notes: {
          required: true,
          value: { type: 'string' as const, min_bytes: 0, max_bytes: 1024 },
        },
        assessment_inputs: { required: true, value: dependency },
      },
    },
  };
  const node: components['schemas']['FactoryFlowNodeDefinition'] = {
    ...configurationFixture().configuration.flow.closure.definitions[0]!.nodes[0]!,
    key: 'apply',
    kind: 'trusted_action',
    input_schema: null,
    input_binding: null,
    action: null,
    outcomes: [],
  };
  draft.root.nodes = [
    {
      ...node,
      key: 'assess',
      kind: 'reasoning',
      outcomes: [{ key: 'assessed', kind: 'success', schema: assessment.reference }],
    },
    node,
  ];
  draft.root.entry = 'assess';
  draft.root.transitions = [
    {
      predecessor: 'assess',
      outcome: 'assessed',
      target: { kind: 'node', key: 'apply' },
      max_repeats: 0,
      exhausted_terminal: null,
    },
  ];
  const api = {
    ...factoryApi,
    getAuthoringCapabilities: vi.fn<FactoryApi['getAuthoringCapabilities']>(
      async () => capabilities,
    ),
    normalizeSchema: vi.fn<FactoryApi['normalizeSchema']>(async (body) => ({
      reference: { identity: body.identity, version: body.version, digest: 'c'.repeat(64) },
      shape: body.shape,
    })),
  };
  let selected = node;
  function Harness() {
    const [value, setValue] = useState(node);
    return (
      <FlowIssueClassification
        node={value}
        root={draft.root}
        schemas={[assessment]}
        services={{ api, projectId: 'delivery' }}
        onChange={(next) => {
          selected = next;
          setValue(next);
        }}
      />
    );
  }
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <Harness />
    </QueryClientProvider>,
  );
  await user.click(screen.getByRole('checkbox', { name: 'Apply classification to Issue' }));
  await user.click(await screen.findByRole('checkbox', { name: 'Issue component' }));
  await user.click(screen.getByRole('checkbox', { name: 'Classification: Impact' }));
  await user.click(screen.getByRole('button', { name: 'Use Project classification' }));
  await waitFor(() => expect(api.normalizeSchema).toHaveBeenCalledTimes(1));
  expect(selected.action?.parameters).toEqual({
    source_node: 'assess',
    source_outcome: 'assessed',
    outcome: 'applied',
    policy: 'project-triage',
    fields: {
      definition_version: 7,
      priority: false,
      component: true,
      kind: false,
      size: false,
      properties: { severity: 'impact' },
    },
  });
  expect(api.normalizeSchema.mock.calls[0]![0].shape).toEqual({
    type: 'object',
    fields: {
      component: { required: true, value: { type: 'enum', values: ['payments'] } },
      severity: { required: false, value: { type: 'enum', values: ['major', 'minor'] } },
      assessment_inputs: { required: true, value: dependency },
    },
  });
  await user.click(screen.getByRole('checkbox', { name: 'Classification: Impact' }));
  expect(screen.getByRole('button', { name: 'Use Project classification' })).toHaveProperty(
    'disabled',
    false,
  );
  await user.click(screen.getByRole('button', { name: 'Use Project classification' }));
  await waitFor(() => expect(api.normalizeSchema).toHaveBeenCalledTimes(2));
  expect(selected.action?.parameters).toMatchObject({
    fields: { component: true, properties: {} },
  });
});

it('reuses selected Project classification field contracts in an assessment result without typing enums', async () => {
  const user = userEvent.setup();
  const capabilities = factoryIssueFieldsFixture();
  const node = configurationFixture().configuration.flow.closure.definitions[0]!.nodes[0]!;
  const api = {
    ...factoryApi,
    getAuthoringCapabilities: vi.fn<FactoryApi['getAuthoringCapabilities']>(
      async () => capabilities,
    ),
    normalizeSchema: vi.fn<FactoryApi['normalizeSchema']>(async (body) => ({
      reference: { identity: body.identity, version: body.version, digest: 'c'.repeat(64) },
      shape: body.shape,
    })),
  };
  function Harness() {
    const [draft, setDraft] = useState<ResultDraft>();
    return (
      <StepResults
        node={{ ...node, kind: 'reasoning', outcomes: [] }}
        schemas={[]}
        services={{ api, projectId: 'delivery' }}
        draft={draft}
        onDraftChange={setDraft}
        onChange={() => {}}
      />
    );
  }
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <Harness />
    </QueryClientProvider>,
  );
  await user.click(screen.getByRole('button', { name: 'Add result' }));
  await user.click(screen.getByRole('button', { name: 'Reuse Project classification fields' }));
  await user.click(await screen.findByRole('checkbox', { name: 'Issue component' }));
  await user.click(screen.getByRole('checkbox', { name: 'Classification: Impact' }));
  await user.click(screen.getByRole('button', { name: 'Use selected field contracts' }));
  await user.click(screen.getByRole('button', { name: 'Save result' }));
  await waitFor(() => expect(api.normalizeSchema).toHaveBeenCalledTimes(1));
  expect(api.normalizeSchema.mock.calls[0]![0].shape).toEqual({
    type: 'object',
    fields: {
      component: { required: false, value: { type: 'enum', values: ['payments'] } },
      severity: { required: false, value: { type: 'enum', values: ['major', 'minor'] } },
    },
  });
});
