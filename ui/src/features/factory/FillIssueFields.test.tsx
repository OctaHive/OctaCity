// @vitest-environment jsdom
import { cleanup, render, screen, waitFor } from '@testing-library/react';
import { QueryClientProvider } from '@tanstack/react-query';
import userEvent from '@testing-library/user-event';
import { useState } from 'react';
import { afterEach, expect, it, vi } from 'vitest';
import type { components } from '../../../.generated/api/schema';
import { configurationFixture } from '../../../tests/fixtures/factory';
import { factoryIssueFieldsFixture } from '../../../tests/fixtures/factoryIssueFields';
import { createConsoleQueryClient } from '../../app/query';
import { factoryApi, type FactoryApi } from './api';
import { StepResults, type ResultDraft } from './StepResults';

afterEach(cleanup);
type S = components['schemas'];
function catalog(): S['FactoryAuthoringCapabilities'] {
  const data = factoryIssueFieldsFixture();
  return {
    ...data,
    issue_fields: {
      ...data.issue_fields,
      fields: [
        ...data.issue_fields.fields,
        ...[
          {
            id: 'note',
            name: 'Summary',
            kind: { type: 'text', max_bytes: 120 },
            shape: { type: 'string', min_bytes: 0, max_bytes: 120 },
          },
          {
            id: 'passed',
            name: 'Checks passed',
            kind: { type: 'boolean' },
            shape: { type: 'boolean' },
          },
          {
            id: 'effort',
            name: 'Effort',
            kind: { type: 'number', minimum: 1, maximum: 9 },
            shape: { type: 'integer', minimum: 1, maximum: 9 },
          },
          {
            id: 'due',
            name: 'Due date',
            kind: { type: 'date' },
            shape: { type: 'string', min_bytes: 10, max_bytes: 10 },
          },
        ].map(
          ({ id, name, kind, shape }) =>
            ({
              source: { kind: 'property', id },
              property_id: id,
              name,
              write_target: { kind: 'property', id },
              value_key: `property.${id}`,
              value_type: kind,
              required_supported: true,
              shape,
            }) as S['FactoryIssueFieldChoice'],
        ),
      ],
    },
  };
}

it('selects Project fields once to generate the accepted form and automatic save without a schema or apply step', async () => {
  const user = userEvent.setup();
  const capabilities = catalog();
  const base = configurationFixture().configuration.flow;
  const original = base.data_schemas[0]!;
  const node: S['FactoryFlowNodeDefinition'] = {
    ...base.closure.definitions[0]!.nodes[0]!,
    key: 'ci_analysis',
    kind: 'reasoning',
    outcomes: [{ key: 'analysis', kind: 'success', schema: original.reference }],
  };
  const result: S['FactoryFlowDataSchema'] = {
    reference: { identity: 'generated.form', version: '1', digest: 'e'.repeat(64) },
    shape: {
      type: 'object',
      fields: {
        size: { required: true, value: { type: 'enum', values: ['small', 'medium', 'large'] } },
        'property.impact': { required: true, value: { type: 'enum', values: ['major', 'minor'] } },
        'property.note': {
          required: true,
          value: { type: 'string', min_bytes: 0, max_bytes: 120 },
        },
        'property.passed': { required: false, value: { type: 'boolean' } },
        'property.effort': { required: true, value: { type: 'integer', minimum: 1, maximum: 9 } },
        'property.due': { required: true, value: { type: 'string', min_bytes: 10, max_bytes: 10 } },
      },
    },
  };
  const api = {
    ...factoryApi,
    getAuthoringCapabilities: vi.fn<FactoryApi['getAuthoringCapabilities']>(
      async () => capabilities,
    ),
    normalizeSchema: vi.fn<FactoryApi['normalizeSchema']>(async () => result),
  };
  const changed = vi.fn();
  function Harness() {
    const [current, setCurrent] = useState(node);
    const [draft, setDraft] = useState<ResultDraft>();
    const [schemas, setSchemas] = useState([original]);
    return (
      <StepResults
        node={current}
        schemas={schemas}
        services={{ api, projectId: capabilities.issue_fields!.classification!.project_id }}
        draft={draft}
        onDraftChange={setDraft}
        onChange={(patch, added = []) => {
          changed(patch, added);
          setCurrent({ ...current, ...patch });
          setSchemas([...schemas, ...added]);
        }}
      />
    );
  }
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <Harness />
    </QueryClientProvider>,
  );
  await user.click(screen.getByRole('button', { name: 'Edit result' }));
  expect(screen.getByRole('checkbox', { name: 'Fill Issue fields' })).toHaveProperty(
    'checked',
    false,
  );
  await user.click(screen.getByRole('checkbox', { name: 'Fill Issue fields' }));
  for (const name of [
    'Issue size',
    'Project field: Impact',
    'Project field: Summary',
    'Project field: Checks passed',
    'Project field: Effort',
    'Project field: Due date',
  ])
    await user.click(await screen.findByRole('checkbox', { name }));
  await user.click(
    screen.getByRole('checkbox', { name: 'Required: Project field: Checks passed' }),
  );
  expect(screen.queryByText('Result structure', { exact: true })).toBeNull();
  expect(screen.queryByText('Apply classification to Issue', { exact: true })).toBeNull();
  await user.click(screen.getByRole('button', { name: 'Save result' }));
  await waitFor(() => expect(changed).toHaveBeenCalledTimes(1));
  const form = {
    project_id: capabilities.issue_fields!.classification!.project_id,
    definition_version: 7,
    fields: [
      { field: { kind: 'size' }, required: true },
      { field: { kind: 'property', id: 'impact' }, required: true },
      { field: { kind: 'property', id: 'note' }, required: true },
      { field: { kind: 'property', id: 'passed' }, required: false },
      { field: { kind: 'property', id: 'effort' }, required: true },
      { field: { kind: 'property', id: 'due' }, required: true },
    ],
  };
  expect(api.normalizeSchema).toHaveBeenCalledWith(
    expect.objectContaining({ issue_fields: form }),
    expect.any(AbortSignal),
  );
  expect(api.normalizeSchema.mock.calls[0]![0]).not.toHaveProperty('shape');
  expect(changed.mock.calls[0]![0].outcomes).toEqual([
    {
      ...node.outcomes[0],
      schema: result.reference,
      issue_fields: {
        writer: capabilities.issue_classification!.plugin,
        policy: capabilities.issue_classification!.policy,
        form,
      },
    },
  ]);
  await user.click(screen.getByRole('button', { name: 'Edit result' }));
  expect(screen.getByRole('checkbox', { name: 'Project field: Due date' })).toHaveProperty(
    'checked',
    true,
  );
  expect(
    screen.getByRole('checkbox', { name: 'Required: Project field: Checks passed' }),
  ).toHaveProperty('checked', false);
  await user.click(screen.getByRole('button', { name: 'Save result' }));
  await waitFor(() => expect(changed).toHaveBeenCalledTimes(2));
  expect(api.normalizeSchema).toHaveBeenCalledTimes(1);
});

it('does not offer a manual classification apply step on an ordinary trusted action', async () => {
  const { FlowBindings } = await import('./FlowBindings');
  const flow = configurationFixture().configuration.flow;
  const root = flow.closure.definitions[0]!;
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <FlowBindings
        node={{ ...root.nodes[0]!, kind: 'trusted_action', action: null }}
        root={root}
        schemas={flow.data_schemas}
        authoring={{ api: factoryApi, projectId: 'project' }}
        onChange={vi.fn()}
      />
    </QueryClientProvider>,
  );
  expect(screen.queryByRole('checkbox', { name: 'Apply classification to Issue' })).toBeNull();
});

it.each(['catalog', 'writer'] as const)(
  'retains the selected draft after its %s was replaced during normalization',
  async (changedContract) => {
    const user = userEvent.setup();
    const data = catalog();
    const flow = configurationFixture().configuration.flow;
    const node = {
      ...flow.closure.definitions[0]!.nodes[0]!,
      kind: 'reasoning' as const,
      outcomes: [
        { key: 'analysis', kind: 'success' as const, schema: flow.data_schemas[0]!.reference },
      ],
    };
    let finish!: (value: S['FactoryFlowDataSchema']) => void;
    const changed = vi.fn();
    const api = {
      ...factoryApi,
      getAuthoringCapabilities: vi.fn<FactoryApi['getAuthoringCapabilities']>(async () => data),
      normalizeSchema: vi.fn<FactoryApi['normalizeSchema']>(
        () =>
          new Promise((resolve) => {
            finish = resolve;
          }),
      ),
    };
    const client = createConsoleQueryClient();
    function Harness() {
      const [draft, setDraft] = useState<ResultDraft>();
      return (
        <StepResults
          node={node}
          schemas={flow.data_schemas}
          services={{ api, projectId: data.issue_fields!.classification!.project_id }}
          draft={draft}
          onDraftChange={setDraft}
          onChange={changed}
        />
      );
    }
    render(
      <QueryClientProvider client={client}>
        <Harness />
      </QueryClientProvider>,
    );
    await user.click(screen.getByRole('button', { name: 'Edit result' }));
    await user.click(screen.getByRole('checkbox', { name: 'Fill Issue fields' }));
    await user.click(await screen.findByRole('checkbox', { name: 'Project field: Summary' }));
    await user.click(screen.getByRole('button', { name: 'Save result' }));
    await waitFor(() => expect(api.normalizeSchema).toHaveBeenCalledTimes(1));
    const { queryKeys } = await import('../../app/query');
    client.setQueryData(
      queryKeys.factoryAuthoringCapabilities(data.issue_fields!.classification!.project_id),
      changedContract === 'catalog'
        ? { ...data, issue_fields: { ...data.issue_fields, definition_version: 8 } }
        : {
            ...data,
            issue_classification: {
              ...data.issue_classification!,
              plugin: { ...data.issue_classification!.plugin, identity: 'new.installed.writer' },
            },
          },
    );
    await screen.findByText(/Your selection is retained/);
    finish({
      reference: { identity: 'stale', version: '1', digest: 'f'.repeat(64) },
      shape: { type: 'string', min_bytes: 0, max_bytes: 120 },
    });
    await waitFor(() =>
      expect(screen.getByRole('button', { name: 'Cancel result edit' })).toHaveProperty(
        'disabled',
        false,
      ),
    );
    await waitFor(() =>
      expect(screen.getByRole('button', { name: 'Save result' })).toHaveProperty('disabled', true),
    );
    expect(changed).not.toHaveBeenCalled();
    expect(screen.getByRole('checkbox', { name: 'Project field: Summary' })).toHaveProperty(
      'checked',
      true,
    );
    expect(screen.getByRole('button', { name: 'Use current field definitions' })).toBeTruthy();
  },
);

it('allows an explicit optional downgrade without losing a selected field when required values become unavailable', async () => {
  const user = userEvent.setup();
  const data = catalog();
  data.issue_fields!.fields.find((f) => f.value_key === 'property.note')!.required_supported =
    false;
  const form: S['FactoryIssueFieldForm'] = {
    project_id: data.issue_fields!.classification!.project_id,
    definition_version: 7,
    fields: [{ field: { kind: 'property', id: 'note' }, required: true }],
  };
  const base = configurationFixture().configuration.flow;
  const original = base.data_schemas[0]!;
  const node = {
    ...base.closure.definitions[0]!.nodes[0]!,
    kind: 'reasoning' as const,
    outcomes: [{ key: 'analysis', kind: 'success' as const, schema: original.reference }],
  };
  const api = {
    ...factoryApi,
    getAuthoringCapabilities: vi.fn<FactoryApi['getAuthoringCapabilities']>(async () => data),
    normalizeSchema: vi.fn<FactoryApi['normalizeSchema']>(async () => original),
  };
  const changed = vi.fn();
  function Harness() {
    const [draft, setDraft] = useState<ResultDraft | undefined>({
      index: 0,
      name: 'analysis',
      kind: 'success',
      shape: original.shape,
      fillFields: {
        writer: data.issue_classification!.plugin,
        policy: data.issue_classification!.policy,
        form,
      },
    });
    return (
      <StepResults
        node={node}
        schemas={[original]}
        services={{ api, projectId: form.project_id }}
        draft={draft}
        onDraftChange={setDraft}
        onChange={changed}
      />
    );
  }
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <Harness />
    </QueryClientProvider>,
  );
  const required = await screen.findByRole('checkbox', {
    name: 'Required: Project field: Summary',
  });
  expect(screen.getByRole('button', { name: 'Save result' })).toHaveProperty('disabled', true);
  expect(required).toHaveProperty('disabled', false);
  await user.click(required);
  await user.click(screen.getByRole('button', { name: 'Save result' }));
  await waitFor(() => expect(changed).toHaveBeenCalledTimes(1));
  expect(api.normalizeSchema.mock.calls[0]![0]).toMatchObject({
    issue_fields: { fields: [{ field: { kind: 'property', id: 'note' }, required: false }] },
  });
});

it('turns a filled result back into a non-writing result without retaining its local writer in agent access', async () => {
  const user = userEvent.setup();
  const data = catalog();
  const base = configurationFixture().configuration.flow;
  const writer = data.issue_classification!.plugin;
  const other = { identity: 'unrelated.tool', version: '1', digest: 'a'.repeat(64) };
  const original = base.data_schemas[0]!;
  const node: S['FactoryFlowNodeDefinition'] = {
    ...base.closure.definitions[0]!.nodes[0]!,
    kind: 'reasoning',
    permissions: {
      ...base.closure.definitions[0]!.nodes[0]!.permissions,
      plugins: [writer, other],
    },
    outcomes: [
      {
        key: 'analysis',
        kind: 'success',
        schema: original.reference,
        issue_fields: {
          writer,
          policy: data.issue_classification!.policy,
          form: {
            project_id: data.issue_fields!.classification!.project_id,
            definition_version: 7,
            fields: [{ field: { kind: 'size' }, required: true }],
          },
        },
      },
    ],
  };
  const api = {
    ...factoryApi,
    getAuthoringCapabilities: vi.fn<FactoryApi['getAuthoringCapabilities']>(async () => data),
    normalizeSchema: vi.fn<FactoryApi['normalizeSchema']>(),
  };
  const changed = vi.fn();
  function Harness() {
    const [draft, setDraft] = useState<ResultDraft>();
    return (
      <StepResults
        node={node}
        schemas={[original]}
        services={{ api, projectId: data.issue_fields!.classification!.project_id }}
        draft={draft}
        onDraftChange={setDraft}
        onChange={changed}
      />
    );
  }
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <Harness />
    </QueryClientProvider>,
  );
  await user.click(screen.getByRole('button', { name: 'Edit result' }));
  await screen.findByRole('checkbox', { name: 'Issue size' });
  await user.click(screen.getByRole('checkbox', { name: 'Fill Issue fields' }));
  await user.click(screen.getByRole('button', { name: 'Save result' }));
  await waitFor(() => expect(changed).toHaveBeenCalledTimes(1));
  expect(changed.mock.calls[0]![0].outcomes[0]).not.toHaveProperty('issue_fields');
  expect(changed.mock.calls[0]![0].permissions.plugins).toEqual([other]);
  expect(api.normalizeSchema).not.toHaveBeenCalled();
});

it('reconciles a renamed field by stable identity and requires removal of an unavailable selected target', async () => {
  const user = userEvent.setup();
  const data = catalog();
  const client = createConsoleQueryClient();
  const base = configurationFixture().configuration.flow;
  const original = base.data_schemas[0]!;
  const node = {
    ...base.closure.definitions[0]!.nodes[0]!,
    kind: 'reasoning' as const,
    outcomes: [{ key: 'analysis', kind: 'success' as const, schema: original.reference }],
  };
  const api = {
    ...factoryApi,
    getAuthoringCapabilities: vi.fn<FactoryApi['getAuthoringCapabilities']>(async () => data),
    normalizeSchema: vi.fn<FactoryApi['normalizeSchema']>(async () => original),
  };
  function Harness() {
    const [draft, setDraft] = useState<ResultDraft | undefined>({
      index: 0,
      name: 'analysis',
      kind: 'success',
      shape: original.shape,
      fillFields: {
        writer: data.issue_classification!.plugin,
        policy: data.issue_classification!.policy,
        form: {
          project_id: data.issue_fields!.classification!.project_id,
          definition_version: 7,
          fields: [{ field: { kind: 'property', id: 'note' }, required: false }],
        },
      },
    });
    return (
      <StepResults
        node={node}
        schemas={[original]}
        services={{ api, projectId: data.issue_fields!.classification!.project_id }}
        draft={draft}
        onDraftChange={setDraft}
        onChange={vi.fn()}
      />
    );
  }
  render(
    <QueryClientProvider client={client}>
      <Harness />
    </QueryClientProvider>,
  );
  await screen.findByRole('checkbox', { name: 'Project field: Summary' });
  const { queryKeys } = await import('../../app/query');
  const key = queryKeys.factoryAuthoringCapabilities(data.issue_fields!.classification!.project_id);
  const renamed = {
    ...data,
    issue_fields: {
      ...data.issue_fields!,
      definition_version: 8,
      fields: data.issue_fields!.fields.map((f) =>
        f.value_key === 'property.note' ? { ...f, name: 'Renamed summary' } : f,
      ),
    },
  };
  client.setQueryData(key, renamed);
  expect(
    await screen.findByRole('checkbox', { name: 'Project field: Renamed summary' }),
  ).toHaveProperty('checked', true);
  expect(screen.getByRole('button', { name: 'Save result' })).toHaveProperty('disabled', true);
  await user.click(screen.getByRole('button', { name: 'Use current field definitions' }));
  expect(
    screen.getByRole('checkbox', { name: 'Required: Project field: Renamed summary' }),
  ).toHaveProperty('checked', false);
  expect(screen.getByRole('button', { name: 'Save result' })).toHaveProperty('disabled', false);
  client.setQueryData(key, {
    ...renamed,
    issue_fields: {
      ...renamed.issue_fields,
      definition_version: 9,
      fields: renamed.issue_fields.fields.filter((f) => f.value_key !== 'property.note'),
    },
  });
  await screen.findByRole('button', { name: 'Remove field' });
  await user.click(screen.getByRole('button', { name: 'Use current field definitions' }));
  expect(screen.getByRole('button', { name: 'Save result' })).toHaveProperty('disabled', true);
  expect(screen.getByRole('alert').textContent).toContain('note');
  await user.click(screen.getByRole('button', { name: 'Remove field' }));
  expect(screen.getByRole('button', { name: 'Save result' })).toHaveProperty('disabled', true);
  expect(api.normalizeSchema).not.toHaveBeenCalled();
});

it('retains the draft when an existing Issue card is incompatible with the generated field form', async () => {
  const data = catalog();
  const flow = configurationFixture().configuration.flow;
  const node = {
    ...flow.closure.definitions[0]!.nodes[0]!,
    kind: 'reasoning' as const,
    outcomes: [
      { key: 'analysis', kind: 'success' as const, schema: flow.data_schemas[0]!.reference },
    ],
  };
  const api = {
    ...factoryApi,
    getAuthoringCapabilities: vi.fn<FactoryApi['getAuthoringCapabilities']>(async () => data),
    normalizeSchema: vi.fn<FactoryApi['normalizeSchema']>(async () => ({
      reference: { identity: 'generated', version: '1', digest: 'c'.repeat(64) },
      shape: {
        type: 'object',
        fields: {
          size: { required: true, value: { type: 'enum', values: ['small', 'medium', 'large'] } },
        },
      },
    })),
  };
  const changed = vi.fn();
  function Harness() {
    const [draft, setDraft] = useState<ResultDraft | undefined>({
      index: 0,
      name: 'analysis',
      kind: 'success',
      shape: {
        type: 'object',
        fields: {
          summary: { required: true, value: { type: 'string', min_bytes: 1, max_bytes: 100 } },
        },
      },
      issuePresentation: { kind: 'custom_v1', title: 'CI analysis', conclusion_field: 'summary' },
      fillFields: {
        writer: data.issue_classification!.plugin,
        policy: data.issue_classification!.policy,
        form: {
          project_id: data.issue_fields!.classification!.project_id,
          definition_version: 7,
          fields: [{ field: { kind: 'size' }, required: true }],
        },
      },
    });
    return (
      <StepResults
        node={node}
        schemas={flow.data_schemas}
        services={{ api, projectId: data.issue_fields!.classification!.project_id }}
        draft={draft}
        onDraftChange={setDraft}
        onChange={changed}
      />
    );
  }
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <Harness />
    </QueryClientProvider>,
  );
  await screen.findByRole('checkbox', { name: 'Issue size' });
  await userEvent.click(screen.getByRole('button', { name: 'Save result' }));
  expect(
    await screen.findByText(/Issue card does not match the generated field form/),
  ).toBeTruthy();
  expect(changed).not.toHaveBeenCalled();
  expect(screen.getByRole('checkbox', { name: 'Issue size' })).toHaveProperty('checked', true);
});
