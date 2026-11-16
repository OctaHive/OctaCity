// @vitest-environment jsdom
import { useState } from 'react';
import { QueryClientProvider } from '@tanstack/react-query';
import { cleanup, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, expect, it, vi } from 'vitest';
import type { components } from '../../../.generated/api/schema';
import { createConsoleQueryClient } from '../../app/query';
import { authoredSchemaShape, configurationFixture } from '../../../tests/fixtures/factory';
import { factoryIssueFieldsFixture } from '../../../tests/fixtures/factoryIssueFields';
import { factoryApi, type FactoryApi } from './api';
import { emptyFlowDraft } from './emptyFlowDraft';
import { FlowIssueSnapshot } from './FlowIssueSnapshot';
import { FlowGraphEditor } from './FlowGraphEditor';

type S = components['schemas'];
afterEach(cleanup);

it.each([1, 2])(
  'preserves snapshot projections after renaming a step with the form open (%s consumers)',
  async (count) => {
    const user = userEvent.setup();
    const { draft, api } = snapshotFixture();
    draft.root.nodes = draft.root.nodes.slice(0, count);
    const savedBindings = draft.root.nodes.map((node) => structuredClone(node.input_binding));
    let latest = draft;
    const changed = vi.fn();
    function Harness() {
      const [value, setValue] = useState(draft);
      return (
        <FlowGraphEditor
          value={value}
          subflows={[]}
          choices={[]}
          authoring={{ api, projectId: 'delivery' }}
          onChange={(next) => {
            latest = next;
            changed(next);
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
    expect(await screen.findByRole('checkbox', { name: 'first: Issue component' })).toHaveProperty(
      'checked',
      true,
    );
    const name = screen.getByRole('textbox', { name: 'Step name' });
    await user.clear(name);
    await user.type(name, 'renamed');
    const renamed = latest;
    const mutations = changed.mock.calls.length;
    expect(screen.getByRole('button', { name: 'Use Issue snapshot' })).toHaveProperty(
      'disabled',
      true,
    );
    expect(
      screen.getByText(
        'The pipeline changed while snapshot settings were open. Cancel and reopen these settings to review the current steps and inputs.',
      ),
    ).toBeTruthy();
    await user.click(screen.getByRole('button', { name: 'Use Issue snapshot' }));
    expect(latest).toBe(renamed);
    expect(changed).toHaveBeenCalledTimes(mutations);
    expect(latest.root.nodes.map((node) => node.input_binding)).toEqual(savedBindings);
    expect(api.normalizeSchema).not.toHaveBeenCalled();
    await user.click(screen.getByRole('button', { name: 'Cancel' }));
    await user.click(screen.getByRole('button', { name: 'Configure Issue snapshot' }));
    expect(
      await screen.findByRole('checkbox', { name: 'renamed: Issue component' }),
    ).toHaveProperty('checked', true);
    await waitFor(() =>
      expect(screen.getByRole('button', { name: 'Use Issue snapshot' })).toHaveProperty(
        'disabled',
        false,
      ),
    );
    await user.click(screen.getByRole('button', { name: 'Use Issue snapshot' }));
    await waitFor(() => expect(changed).toHaveBeenCalledTimes(mutations + 1));
    expect(latest.root.nodes.map((node) => node.input_binding)).toEqual(savedBindings);
    expect(latest.root.nodes[0]!.key).toBe('renamed');
    expect(latest.issue_snapshot).toEqual(draft.issue_snapshot);
  },
);

function snapshotFixture() {
  const draft = emptyFlowDraft();
  const template = configurationFixture().configuration.flow.closure.definitions[0]!.nodes[0]!;
  const reference = { identity: 'snapshot', version: '1', digest: 'a'.repeat(64) };
  const component: S['FactoryFlowFieldSchema'] = {
    required: true,
    value: { type: 'enum', values: ['payments'] },
  };
  const schema: S['FactoryFlowDataSchema'] = {
    reference,
    shape: { type: 'object', fields: { component } },
  };
  draft.data_schemas = [schema];
  draft.issue_snapshot = {
    version: 1,
    definition_version: 7,
    schema: reference,
    fields: { component: { source: { kind: 'component' }, required: true } },
  };
  draft.root.entry = 'first';
  draft.root.input_schema = reference;
  draft.root.nodes = ['first', 'second'].map((key) => ({
    ...template,
    key,
    input_schema: reference,
    input_binding: {
      sources: { issue: { source: 'incoming', schema: reference, path: '' } },
      mapping: { value: { mapping: 'pointer', path: '/issue' } },
    },
  }));
  const api = {
    ...factoryApi,
    getAuthoringCapabilities: vi.fn<FactoryApi['getAuthoringCapabilities']>(async () =>
      factoryIssueFieldsFixture(),
    ),
    normalizeSchema: vi.fn<FactoryApi['normalizeSchema']>(),
  };
  return { draft, schema, component, api };
}

it.each(['renamed field', 'constant field', 'conditional fallback', 'source path'] as const)(
  'preserves an advanced snapshot input with a %s instead of rebuilding it',
  async (kind) => {
    const user = userEvent.setup();
    const { draft, schema, component, api } = snapshotFixture();
    const node = draft.root.nodes[0]!;
    const reference = { identity: 'advanced', version: '1', digest: 'b'.repeat(64) };
    let shape: S['FactoryFlowValueSchema'];
    let mapping: S['FactoryFlowValueMapping'];
    let path = '';
    if (kind === 'renamed field') {
      shape = { type: 'object', fields: { module: component } };
      mapping = {
        mapping: 'object',
        fields: { module: { mapping: 'pointer', path: '/issue/component' } },
      };
    } else if (kind === 'constant field') {
      shape = {
        type: 'object',
        fields: { component, team: { required: true, value: { type: 'enum', values: ['core'] } } },
      };
      mapping = {
        mapping: 'object',
        fields: {
          component: { mapping: 'pointer', path: '/issue/component' },
          team: { mapping: 'constant', value: 'core' },
        },
      };
    } else if (kind === 'conditional fallback') {
      // A supported optional source supplies a required target through an explicit fallback.
      if (schema.shape.type === 'object') schema.shape.fields.component!.required = false;
      draft.issue_snapshot!.fields.component!.required = false;
      shape = { type: 'object', fields: { component: { ...component, required: true } } };
      mapping = {
        mapping: 'object',
        fields: {
          component: {
            mapping: 'if_present',
            path: '/issue/component',
            then: { mapping: 'pointer', path: '/issue/component' },
            otherwise: { mapping: 'constant', value: 'payments' },
          },
        },
      };
    } else {
      path = '/component';
      shape = { type: 'object', fields: { component } };
      mapping = {
        mapping: 'object',
        fields: { component: { mapping: 'pointer', path: '/issue' } },
      };
    }
    draft.data_schemas.push({ reference, shape });
    node.input_schema = reference;
    node.input_binding = {
      sources: { issue: { source: 'incoming', schema: schema.reference, path } },
      mapping: { value: mapping },
    };
    draft.root.input_schema = reference;
    const onChange = vi.fn();
    render(
      <QueryClientProvider client={createConsoleQueryClient()}>
        <FlowIssueSnapshot
          value={draft}
          services={{ api, projectId: 'delivery' }}
          onChange={onChange}
          onEditingChange={() => {}}
        />
      </QueryClientProvider>,
    );
    await user.click(screen.getByRole('button', { name: 'Configure Issue snapshot' }));
    await screen.findByRole('checkbox', { name: 'Issue component' });
    await user.click(
      screen.getByRole('checkbox', { name: 'Include explicitly selected Issue materials' }),
    );
    const apply = screen.getByRole('button', { name: 'Use Issue snapshot' });
    expect(apply).toHaveProperty('disabled', true);
    expect(screen.getByRole('alert').textContent).toContain('Existing input is preserved');
    await user.click(apply);
    expect(onChange).not.toHaveBeenCalled();
    expect(api.normalizeSchema).not.toHaveBeenCalled();
  },
);

it('keeps the snapshot schema catalog bounded across unchanged applies and material-policy edits', async () => {
  const user = userEvent.setup();
  const { draft, schema, api } = snapshotFixture();
  // A separately registered contract must survive cleanup of superseded inputs.
  const retained: S['FactoryFlowDataSchema'] = {
    reference: { identity: 'saved-result', version: '1', digest: 'f'.repeat(64) },
    shape: { type: 'boolean' },
  };
  draft.data_schemas.push(retained);
  let sequence = 0;
  api.normalizeSchema.mockImplementation(async (body) => ({
    reference: {
      identity: body.identity,
      version: body.version,
      digest: (++sequence).toString(16).repeat(64),
    },
    shape: authoredSchemaShape(body),
  }));
  const changed = vi.fn();
  let latest = draft;
  function Harness() {
    const [value, setValue] = useState(draft);
    return (
      <FlowIssueSnapshot
        value={value}
        services={{ api, projectId: 'delivery' }}
        onEditingChange={() => {}}
        onChange={(next) => {
          latest = next;
          changed(next);
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
  for (let revision = 0; revision < 4; revision++) {
    await user.click(screen.getByRole('button', { name: 'Configure Issue snapshot' }));
    await screen.findByRole('checkbox', { name: 'first: Issue component' });
    if (revision > 0) {
      if (revision === 1)
        await user.click(
          screen.getByRole('checkbox', { name: 'Include explicitly selected Issue materials' }),
        );
      const bytes = screen.getByRole('spinbutton', { name: 'Maximum material bytes' });
      await user.clear(bytes);
      await user.type(bytes, String(1024 * revision));
      if (revision === 1)
        await user.click(screen.getByRole('checkbox', { name: 'first: Issue materials' }));
    }
    await waitFor(() =>
      expect(screen.getByRole('button', { name: 'Use Issue snapshot' })).toHaveProperty(
        'disabled',
        false,
      ),
    );
    await user.click(screen.getByRole('button', { name: 'Use Issue snapshot' }));
    await waitFor(() => expect(changed).toHaveBeenCalledTimes(revision + 1));
    // One full input, its optional subset when materials are enabled, and the retained result.
    expect(latest.data_schemas).toHaveLength(revision === 0 ? 2 : 3);
    expect(latest.data_schemas).toContainEqual(retained);
    const second = latest.root.nodes.find((node) => node.key === 'second')!;
    expect(second.input_schema).toEqual(schema.reference);
  }
});

it('round-trips generated optional guards after the server canonicalizes schema field order', async () => {
  const user = userEvent.setup();
  const { draft, schema, component, api } = snapshotFixture();
  const environment: S['FactoryFlowFieldSchema'] = {
    required: false,
    value: { type: 'enum', values: ['test', 'production'] },
  };
  const size: S['FactoryFlowFieldSchema'] = {
    required: false,
    value: { type: 'enum', values: ['small', 'medium', 'large'] },
  };
  schema.shape = { type: 'object', fields: { component, environment, size } };
  draft.issue_snapshot!.fields.environment = {
    source: { kind: 'property', id: 'environment' },
    required: false,
  };
  draft.issue_snapshot!.fields.size = { source: { kind: 'size' }, required: false };
  const target: S['FactoryFlowDataSchema'] = {
    reference: { identity: 'subset', version: '1', digest: 'c'.repeat(64) },
    shape: { type: 'object', fields: { environment, size } },
  };
  draft.data_schemas.push(target);
  draft.root.input_schema = target.reference;
  const node = draft.root.nodes[0]!;
  node.input_schema = target.reference;
  const sizePointer: S['FactoryFlowValueMapping'] = { mapping: 'pointer', path: '/issue/size' };
  const environmentPointer: S['FactoryFlowValueMapping'] = {
    mapping: 'pointer',
    path: '/issue/environment',
  };
  node.input_binding!.mapping.value = {
    mapping: 'if_present',
    path: '/issue/size',
    then: {
      mapping: 'if_present',
      path: '/issue/environment',
      then: { mapping: 'object', fields: { size: sizePointer, environment: environmentPointer } },
      otherwise: { mapping: 'object', fields: { size: sizePointer } },
    },
    otherwise: {
      mapping: 'if_present',
      path: '/issue/environment',
      then: { mapping: 'object', fields: { environment: environmentPointer } },
      otherwise: { mapping: 'object', fields: {} },
    },
  };
  const onChange = vi.fn();
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <FlowIssueSnapshot
        value={draft}
        services={{ api, projectId: 'delivery' }}
        onChange={onChange}
        onEditingChange={() => {}}
      />
    </QueryClientProvider>,
  );
  await user.click(screen.getByRole('button', { name: 'Configure Issue snapshot' }));
  expect(await screen.findByRole('checkbox', { name: 'first: Issue size' })).toHaveProperty(
    'checked',
    true,
  );
  expect(
    screen.getByRole('checkbox', { name: 'first: Project field: Environment' }),
  ).toHaveProperty('checked', true);
  await user.click(screen.getByRole('button', { name: 'Use Issue snapshot' }));
  await waitFor(() => expect(onChange).toHaveBeenCalledTimes(1));
  expect(onChange.mock.calls[0]![0].root.nodes[0].input_schema).toEqual(target.reference);
});

it.each(['retired property', 'removed classification binding'] as const)(
  'lets the author explicitly remove an unavailable snapshot selection (%s)',
  async (kind) => {
    const user = userEvent.setup();
    const { draft, schema, api } = snapshotFixture();
    const capabilities = factoryIssueFieldsFixture();
    capabilities.issue_fields.definition_version = 8;
    capabilities.issue_fields.classification.definition_version = 8;
    const alias = kind === 'retired property' ? 'environment' : 'severity';
    const source: S['FactoryFlowIssueFieldSource'] =
      kind === 'retired property'
        ? { kind: 'property', id: 'environment' }
        : { kind: 'classification', slot: 'severity' };
    const shape: S['FactoryFlowValueSchema'] =
      kind === 'retired property'
        ? { type: 'enum', values: ['test', 'production'] }
        : { type: 'enum', values: ['major', 'minor'] };
    if (schema.shape.type === 'object')
      schema.shape.fields[alias] = { required: false, value: shape };
    draft.issue_snapshot!.fields[alias] = { source, required: false };
    capabilities.issue_fields.fields = capabilities.issue_fields.fields.filter((field) =>
      kind === 'retired property'
        ? !(field.source.kind === 'property' && field.source.id === 'environment')
        : !(field.source.kind === 'classification' && field.source.slot === 'severity'),
    );
    if (kind === 'removed classification binding')
      capabilities.issue_fields.classification.fields =
        capabilities.issue_fields.classification.fields.filter(
          ({ field }) => field.kind !== 'property' || field.id !== 'impact',
        );
    api.getAuthoringCapabilities.mockResolvedValue(capabilities);
    api.normalizeSchema.mockImplementation(async (body) => ({
      reference: { identity: body.identity, version: body.version, digest: 'd'.repeat(64) },
      shape: authoredSchemaShape(body),
    }));
    const onChange = vi.fn();
    render(
      <QueryClientProvider client={createConsoleQueryClient()}>
        <FlowIssueSnapshot
          value={draft}
          services={{ api, projectId: 'delivery' }}
          onChange={onChange}
          onEditingChange={() => {}}
        />
      </QueryClientProvider>,
    );
    await user.click(screen.getByRole('button', { name: 'Configure Issue snapshot' }));
    await screen.findByRole('checkbox', { name: 'Issue component' });
    expect(screen.getByRole('button', { name: 'Use Issue snapshot' })).toHaveProperty(
      'disabled',
      true,
    );
    expect(screen.getByText(`Unavailable field: ${alias}`)).toBeTruthy();
    await user.click(screen.getByRole('button', { name: `Remove unavailable field: ${alias}` }));
    expect(screen.getByRole('button', { name: 'Use Issue snapshot' })).toHaveProperty(
      'disabled',
      false,
    );
    await user.click(screen.getByRole('button', { name: 'Use Issue snapshot' }));
    await waitFor(() => expect(onChange).toHaveBeenCalledTimes(1));
    const result: S['FactoryFlowDefinitionDraft'] = onChange.mock.calls[0]![0];
    expect(result.issue_snapshot).toMatchObject({
      definition_version: 8,
      fields: { component: { source: { kind: 'component' }, required: true } },
    });
    expect(result.issue_snapshot!.fields).not.toHaveProperty(alias);
    for (const node of result.root.nodes) {
      const input = result.data_schemas.find(
        (schema) => schema.reference.digest === node.input_schema?.digest,
      )!.shape;
      expect(input).toEqual({
        type: 'object',
        fields: { component: { required: true, value: { type: 'enum', values: ['payments'] } } },
      });
    }
  },
);

it('recognizes an available source regardless of JSON property order', async () => {
  const user = userEvent.setup();
  const { draft, schema, api } = snapshotFixture();
  if (schema.shape.type === 'object')
    schema.shape.fields.environment = {
      required: false,
      value: { type: 'enum', values: ['test', 'production'] },
    };
  // A different JSON serializer may write the ID before the discriminator.
  draft.issue_snapshot!.fields.environment = {
    source: { id: 'environment', kind: 'property' },
    required: false,
  };
  render(
    <QueryClientProvider client={createConsoleQueryClient()}>
      <FlowIssueSnapshot
        value={draft}
        services={{ api, projectId: 'delivery' }}
        onChange={() => {}}
        onEditingChange={() => {}}
      />
    </QueryClientProvider>,
  );
  await user.click(screen.getByRole('button', { name: 'Configure Issue snapshot' }));
  expect(
    await screen.findByRole('checkbox', { name: 'Project field: Environment' }),
  ).toHaveProperty('checked', true);
  expect(screen.queryByText('Unavailable field: environment')).toBeNull();
  expect(screen.getByRole('button', { name: 'Use Issue snapshot' })).toHaveProperty(
    'disabled',
    false,
  );
});

it.each([false, true])(
  'reopens evidence policy and preserves observation settings and projections (rename: %s)',
  async (rename) => {
    const user = userEvent.setup();
    const base = configurationFixture().configuration.flow;
    const { reference: _reference, ...root } = base.closure.definitions[0]!;
    void _reference;
    const reference = { identity: 'snapshot.evidence', version: '1', digest: 'b'.repeat(64) };
    const criterion = { identity: 'criterion', version: 'v1', digest: 'c'.repeat(64) };
    const environment = { identity: 'environment', version: 'v1', digest: 'd'.repeat(64) };
    const schema: S['FactoryFlowDataSchema'] = {
      reference,
      shape: {
        type: 'object',
        fields: {
          priority: { required: true, value: { type: 'enum', values: ['urgent', 'routine'] } },
          proofs: {
            required: true,
            value: {
              type: 'array',
              min_items: 1,
              max_items: 2,
              item: { type: 'string', min_bytes: 64, max_bytes: 64 },
            },
          },
        },
      },
    };
    const node = {
      ...root.nodes[0]!,
      key: 'consumer',
      input_schema: reference,
      input_binding: {
        sources: { issue: { source: 'incoming' as const, schema: reference, path: '' } },
        mapping: { value: { mapping: 'pointer' as const, path: '/issue' } },
      },
    };
    const draft: S['FactoryFlowDefinitionDraft'] = {
      root: { ...root, entry: 'consumer', nodes: [node], input_schema: reference },
      limits: base.limits,
      data_schemas: [schema],
      issue_snapshot: {
        version: 1,
        definition_version: 7,
        schema: reference,
        fields: { priority: { source: { kind: 'priority' }, required: true } },
        evidence: {
          proofs: {
            field: 'priority',
            required: true,
            maximum: 2,
            observation: { criterion, environment, finding: 'matched' },
          },
        },
      },
    };
    const api = {
      ...factoryApi,
      getAuthoringCapabilities: vi.fn<FactoryApi['getAuthoringCapabilities']>(async () =>
        factoryIssueFieldsFixture(),
      ),
      normalizeSchema: vi.fn<FactoryApi['normalizeSchema']>(async (body) => ({
        reference,
        shape: authoredSchemaShape(body),
      })),
    };
    const changed = vi.fn();
    render(
      <QueryClientProvider client={createConsoleQueryClient()}>
        <FlowIssueSnapshot
          value={draft}
          services={{ api, projectId: 'delivery' }}
          onChange={changed}
          onEditingChange={vi.fn()}
        />
      </QueryClientProvider>,
    );
    await user.click(screen.getByRole('button', { name: 'Configure Issue snapshot' }));
    expect(
      await screen.findByRole('spinbutton', { name: 'Maximum evidence references' }),
    ).toHaveProperty('value', '2');
    expect(screen.getByRole('checkbox', { name: 'consumer: Evidence: proofs' })).toHaveProperty(
      'checked',
      true,
    );
    expect(screen.getByRole('checkbox', { name: 'Require trusted observation' })).toHaveProperty(
      'checked',
      true,
    );
    if (rename) {
      const fieldAlias = screen.getByRole('textbox', { name: 'Input alias: Priority' });
      await user.clear(fieldAlias);
      await user.type(fieldAlias, 'rank');
      const evidenceAlias = screen.getByRole('textbox', { name: 'Evidence input alias' });
      await user.clear(evidenceAlias);
      await user.type(evidenceAlias, 'observations');
      expect(
        screen.getByRole('checkbox', { name: 'consumer: Evidence: observations' }),
      ).toHaveProperty('checked', true);
    }
    await user.click(screen.getByRole('button', { name: 'Use Issue snapshot' }));
    await waitFor(() => expect(changed).toHaveBeenCalledTimes(1));
    const result = changed.mock.calls[0]![0];
    expect(result.issue_snapshot.evidence).toEqual({
      [rename ? 'observations' : 'proofs']: {
        field: rename ? 'rank' : 'priority',
        required: true,
        maximum: 2,
        observation: { criterion, environment, finding: 'matched' },
      },
    });
    expect(result.root.nodes[0].input_binding).toEqual(node.input_binding);
    if (!rename) expect(api.normalizeSchema).not.toHaveBeenCalled();
  },
);
