// @vitest-environment jsdom
import { QueryClientProvider } from '@tanstack/react-query';
import { cleanup, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, expect, it, vi } from 'vitest';
import type { components } from '../../../.generated/api/schema';
import { createConsoleQueryClient } from '../../app/query';
import { configurationFixture } from '../../../tests/fixtures/factory';
import { factoryIssueFieldsFixture } from '../../../tests/fixtures/factoryIssueFields';
import { factoryApi, type FactoryApi } from './api';
import { emptyFlowDraft } from './emptyFlowDraft';
import { FlowGraphEditor } from './FlowGraphEditor';

type S = components['schemas'];
afterEach(cleanup);

it.each(['complete', 'nested', 'unavailable', 'different version'] as const)(
  'retains pinned child schemas when replacing a root input (%s closure)',
  async (availability) => {
    const user = userEvent.setup();
    const config = configurationFixture().configuration;
    const template = config.flow.closure.definitions[0]!.nodes[0]!;
    const shared: S['FactoryFlowDataSchema'] = {
      reference: { identity: 'child-internal', version: '1', digest: 'a'.repeat(64) },
      shape: {
        type: 'object',
        fields: { component: { required: true, value: { type: 'enum', values: ['payments'] } } },
      },
    };
    const rootInput = {
      ...shared,
      reference: { identity: 'root-input', version: '1', digest: 'c'.repeat(64) },
    };
    const output: S['FactoryFlowDataSchema'] = {
      reference: { identity: 'output', version: '1', digest: 'f'.repeat(64) },
      shape: { type: 'boolean' },
    };
    const child = structuredClone(config.flow);
    const definition = child.closure.definitions[0]!;
    definition.reference = {
      ...definition.reference,
      id: 'bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb',
      version: 1,
      digest: 'b'.repeat(64),
    };
    child.closure.root = definition.reference;
    definition.entry = 'start';
    definition.input_schema = null;
    definition.nodes = [
      {
        ...template,
        key: 'start',
        input_schema: null,
        outcomes: [{ key: 'observed', kind: 'success', schema: output.reference }],
      },
      {
        ...template,
        key: 'middle',
        input_schema: output.reference,
        outcomes: [{ key: 'observed', kind: 'success', schema: shared.reference }],
      },
      {
        ...template,
        key: 'finish',
        input_schema: shared.reference,
        outcomes: [{ key: 'done', kind: 'success', schema: output.reference }],
      },
    ];
    definition.transitions = [
      {
        predecessor: 'start',
        outcome: 'observed',
        target: { kind: 'node', key: 'middle' },
        max_repeats: 0,
        exhausted_terminal: null,
      },
      {
        predecessor: 'middle',
        outcome: 'observed',
        target: { kind: 'node', key: 'finish' },
        max_repeats: 0,
        exhausted_terminal: null,
      },
      {
        predecessor: 'finish',
        outcome: 'done',
        target: { kind: 'terminal', key: 'done' },
        max_repeats: 0,
        exhausted_terminal: null,
      },
    ];
    definition.terminals = [{ key: 'done', schema: output.reference }];
    child.closure.definitions = [definition];
    child.data_schemas = [shared, output];
    if (availability === 'nested') {
      const outer: S['FactoryFlowDefinition'] = {
        ...definition,
        reference: {
          ...definition.reference,
          id: 'eeeeeeee-eeee-4eee-8eee-eeeeeeeeeeee',
          digest: 'e'.repeat(64),
        },
        entry: 'nested',
        nodes: [
          {
            ...template,
            key: 'nested',
            kind: 'subflow_call',
            subflow: definition.reference,
            input_schema: null,
            outcomes: [{ key: 'done', kind: 'success', schema: output.reference }],
          },
        ],
        transitions: [
          {
            predecessor: 'nested',
            outcome: 'done',
            target: { kind: 'terminal', key: 'done' },
            max_repeats: 0,
            exhausted_terminal: null,
          },
        ],
      };
      child.closure.root = outer.reference;
      child.closure.definitions = [outer, definition];
    }
    const draft = emptyFlowDraft();
    draft.data_schemas = [shared, rootInput, output];
    draft.root.entry = 'first';
    draft.root.input_schema = rootInput.reference;
    draft.issue_snapshot = {
      version: 1,
      definition_version: 7,
      schema: shared.reference,
      fields: { component: { source: { kind: 'component' }, required: true } },
    };
    draft.root.nodes = [
      {
        ...template,
        key: 'first',
        input_schema: rootInput.reference,
        input_binding: {
          sources: { issue: { source: 'incoming', schema: shared.reference, path: '' } },
          mapping: { value: { mapping: 'pointer', path: '/issue' } },
        },
        outcomes: [{ key: 'done', kind: 'success', schema: output.reference }],
      },
      {
        ...template,
        key: 'child',
        kind: 'subflow_call',
        subflow: child.closure.root,
        input_schema: null,
        input_binding: null,
        outcomes: [{ key: 'done', kind: 'success', schema: output.reference }],
      },
    ];
    const detail: S['FactoryFlowDefinitionDetails'] = {
      flow: child,
      project_id: config.reference.project_id,
      published_at_unix_ms: 1,
    };
    const available =
      availability === 'unavailable'
        ? []
        : [
            availability === 'different version'
              ? {
                  ...detail,
                  flow: {
                    ...child,
                    closure: {
                      root: { ...child.closure.root, version: 2 },
                      definitions: child.closure.definitions.map((item) => ({
                        ...item,
                        reference: { ...item.reference, version: 2 },
                      })),
                    },
                  },
                }
              : detail,
          ];
    const api = {
      ...factoryApi,
      getAuthoringCapabilities: vi.fn<FactoryApi['getAuthoringCapabilities']>(async () =>
        factoryIssueFieldsFixture(),
      ),
      normalizeSchema: vi.fn<FactoryApi['normalizeSchema']>(async (body) => ({
        reference: { identity: body.identity, version: body.version, digest: 'd'.repeat(64) },
        shape: body.shape,
      })),
    };
    const onChange = vi.fn();
    render(
      <QueryClientProvider client={createConsoleQueryClient()}>
        <FlowGraphEditor
          value={draft}
          subflows={available}
          choices={[]}
          authoring={{ api, projectId: config.reference.project_id }}
          onChange={onChange}
        />
      </QueryClientProvider>,
    );
    await user.click(screen.getByRole('button', { name: 'Configure Issue snapshot' }));
    await screen.findByRole('checkbox', { name: 'Issue component' });
    await user.click(
      screen.getByRole('checkbox', { name: 'Include explicitly selected Issue materials' }),
    );
    await user.click(screen.getByRole('checkbox', { name: 'first: Issue materials' }));
    await user.click(screen.getByRole('button', { name: 'Use Issue snapshot' }));
    await waitFor(() => expect(onChange).toHaveBeenCalledTimes(1));
    const result: S['FactoryFlowDefinitionDraft'] = onChange.mock.calls[0]![0];
    expect(result.data_schemas).toContainEqual(shared);
    expect(result.data_schemas).toContainEqual(output);
    if (availability === 'complete' || availability === 'nested')
      expect(result.data_schemas).not.toContainEqual(rootInput);
    else expect(result.data_schemas).toContainEqual(rootInput);
  },
);
