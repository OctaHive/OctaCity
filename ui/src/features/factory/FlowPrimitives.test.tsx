// @vitest-environment jsdom
import { useState } from 'react';
import { QueryClientProvider } from '@tanstack/react-query';
import { act, cleanup, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, expect, it, vi } from 'vitest';
import type { components } from '../../../.generated/api/schema';
import { queryKeys, createConsoleQueryClient } from '../../app/query';
import { factoryApi } from './api';
import { emptyFlowDraft } from './emptyFlowDraft';
import { FlowGraphEditor } from './FlowGraphEditor';

afterEach(cleanup);

function setup(
  gate?: components['schemas']['FactoryFlowGate'],
  kind: components['schemas']['FactoryFlowNodeKind'] = 'deterministic_gate',
) {
  const policy = { identity: 'factory.declarative-gate', version: 'v1', digest: 'c'.repeat(64) };
  const api = {
    ...factoryApi,
    getAuthoringCapabilities: vi.fn(async () => ({
      commands: [
        {
          alias: 'Check formatting',
          execution: {
            build_configuration: {
              project_id: 'delivery',
              id: 'command-build',
              version: 3,
              definition_digest: 'f'.repeat(64),
            },
            tool: policy,
            plugin: policy,
            model_or_tool: { ...policy, identity: 'format-check' },
            task_digest: 'e'.repeat(64),
            result_output: 'observation.json',
            result_output_kind: 'report' as const,
          },
        },
      ],
      deterministic_gate: {
        policy,
        parameter_contract: 'declarative_gate_v1' as const,
        max_parameter_bytes: 65536,
        resource_modes: ['none' as const],
      },
      human_gates: [
        {
          operation: 'escalation' as const,
          resource_modes: ['none' as const],
          responses: [
            { disposition: 'resume' as const, shape: { type: 'boolean' as const } },
            {
              disposition: 'abort' as const,
              shape: { type: 'string' as const, min_bytes: 1, max_bytes: 500 },
            },
          ],
        },
      ],
      build_resource_modes: ['repository' as const],
      source_free_incompatibility: 'executor_support_required' as const,
    })),
    normalizeGateProgram: vi.fn(
      async (
        _project: string,
        body: components['schemas']['ValidateFactoryGateProgramRequest'],
      ) => ({ policy, parameters: body.program }),
    ),
    normalizeSchema: vi.fn(
      async (body: components['schemas']['ValidateFactoryFlowDataSchemaRequest']) => ({
        reference: {
          identity: body.identity,
          version: body.version,
          digest: (body.shape.type === 'boolean' ? 'b' : 'd').repeat(64),
        },
        shape: body.shape,
      }),
    ),
    createFlow: vi.fn(),
  };
  const onEditing = vi.fn();
  let observed = emptyFlowDraft();
  const schema = {
    reference: { identity: 'assessment', version: 'v1', digest: 'a'.repeat(64) },
    shape: { type: 'boolean' as const },
  };
  observed.data_schemas = [schema];
  observed.root.entry = 'check';
  observed.root.nodes = [
    {
      key: 'check',
      kind,
      required: true,
      subflow: null,
      input_schema: schema.reference,
      budget: observed.root.execution.budget,
      permissions: observed.root.execution.permissions,
      outcomes: [
        { key: 'accepted', kind: 'success', schema: schema.reference },
        { key: 'review', kind: 'failure', schema: schema.reference },
      ],
    },
  ];
  if (gate) observed.root.nodes[0]!.gate = gate;
  function Harness() {
    const [value, setValue] = useState(observed);
    return (
      <FlowGraphEditor
        value={value}
        onStepEditingChange={onEditing}
        choices={[]}
        subflows={[]}
        authoring={{ api, projectId: 'delivery' }}
        onChange={(next) => {
          observed = next;
          setValue(next);
        }}
      />
    );
  }
  const client = createConsoleQueryClient();
  render(
    <QueryClientProvider client={client}>
      <Harness />
    </QueryClientProvider>,
  );
  return { api, policy, client, onEditing, draft: () => observed };
}

it('authors a condition check with explicit rules and fallback through the installed normalizer', async () => {
  const { api, policy, draft } = setup();
  const editor = within(await screen.findByRole('group', { name: 'Condition check' }));
  await userEvent.click(editor.getByRole('combobox', { name: 'If no condition matches' }));
  await userEvent.click(screen.getByRole('option', { name: 'review' }));
  await userEvent.click(editor.getByRole('button', { name: 'Add condition' }));
  await userEvent.type(editor.getByRole('textbox', { name: 'Observation path' }), '/eligible');
  await userEvent.click(editor.getByRole('combobox', { name: 'Condition outcome' }));
  await userEvent.click(screen.getByRole('option', { name: 'accepted' }));
  expect(draft().root.nodes[0]!.gate).toBeUndefined();
  await userEvent.click(editor.getByRole('button', { name: 'Validate and use condition' }));
  expect(await editor.findByText('Condition check is configured')).toBeTruthy();
  expect(draft().root.nodes[0]!.gate).toEqual({
    policy,
    parameters: {
      rules: [
        {
          when: { operation: 'exists', operand: { source: 'path', path: '/eligible' } },
          outcome: 'accepted',
        },
      ],
      fallback: 'review',
    },
  });
  expect(api.normalizeGateProgram).toHaveBeenCalledWith(
    'delivery',
    expect.any(Object),
    expect.any(AbortSignal),
  );
  expect(api.createFlow).not.toHaveBeenCalled();
  expect(screen.queryByRole('textbox', { name: /digest|json/i })).toBeNull();
});

it('opens a saved condition for editing without discarding its predicate, explicit projection or fallback', async () => {
  const parameters = {
    rules: [
      {
        when: {
          operation: 'exists' as const,
          operand: { source: 'path' as const, path: '/retained' },
        },
        outcome: 'accepted',
      },
    ],
    fallback: 'review',
    default_output: { value: { mapping: 'pointer' as const, path: '/result' } },
  };
  const gate = {
    policy: { identity: 'factory.declarative-gate', version: 'v1', digest: 'c'.repeat(64) },
    parameters,
  };
  const { api, draft } = setup(gate);
  const editor = within(await screen.findByRole('group', { name: 'Condition check' }));
  expect(editor.getByRole('textbox', { name: 'Observation path' })).toHaveProperty(
    'value',
    '/retained',
  );
  expect(editor.getByRole('combobox', { name: 'If no condition matches' })).toHaveProperty(
    'textContent',
    'review',
  );
  await userEvent.clear(editor.getByRole('textbox', { name: 'Observation path' }));
  await userEvent.type(editor.getByRole('textbox', { name: 'Observation path' }), '/changed');
  expect(draft().root.nodes[0]!.gate).toEqual(gate);
  await userEvent.click(editor.getByRole('button', { name: 'Validate and use condition' }));
  expect(api.normalizeGateProgram).toHaveBeenCalledWith(
    'delivery',
    {
      program: {
        ...parameters,
        rules: [
          {
            ...parameters.rules[0],
            when: { operation: 'exists', operand: { source: 'path', path: '/changed' } },
          },
        ],
      },
    },
    expect.any(AbortSignal),
  );
});

it('maps authorized human responses to explicit outcomes and normalizes their result shapes without publishing or running', async () => {
  const { api, draft } = setup(undefined, 'human_gate');
  const editor = within(await screen.findByRole('group', { name: 'Human decision' }));
  expect(screen.queryByRole('combobox', { name: 'Execution profile' })).toBeNull();
  await userEvent.click(editor.getByRole('combobox', { name: 'Request purpose' }));
  await userEvent.click(screen.getByRole('option', { name: 'Review and continuation' }));
  await userEvent.click(editor.getByRole('combobox', { name: 'Continue → outcome' }));
  await userEvent.click(screen.getByRole('option', { name: 'accepted' }));
  await userEvent.click(editor.getByRole('combobox', { name: 'Stop → outcome' }));
  await userEvent.click(screen.getByRole('option', { name: 'review' }));
  expect(draft().root.nodes[0]!.operator_requests).toBeUndefined();
  await userEvent.click(editor.getByRole('button', { name: 'Use human decision' }));
  expect(await editor.findByText('Human decision is configured')).toBeTruthy();
  expect(draft().root.nodes[0]!.operator_requests).toEqual({
    operation: 'escalation',
    outcomes: { resume: 'accepted', abort: 'review' },
  });
  expect(draft().root.nodes[0]!.outcomes.map((item) => item.schema)).toEqual([
    { identity: 'human.escalation.resume', version: 'v1', digest: 'b'.repeat(64) },
    { identity: 'human.escalation.abort', version: 'v1', digest: 'd'.repeat(64) },
  ]);
  expect(draft().data_schemas).toEqual(
    expect.arrayContaining([
      {
        reference: { identity: 'human.escalation.resume', version: 'v1', digest: 'b'.repeat(64) },
        shape: { type: 'boolean' },
      },
      {
        reference: { identity: 'human.escalation.abort', version: 'v1', digest: 'd'.repeat(64) },
        shape: { type: 'string', min_bytes: 1, max_bytes: 500 },
      },
    ]),
  );
  expect(api.createFlow).not.toHaveBeenCalled();
});

it('selects bounded result data for a condition explicitly without changing its result schema', async () => {
  const { api, draft } = setup();
  const before = structuredClone(draft().root.nodes[0]!.outcomes);
  const editor = within(await screen.findByRole('group', { name: 'Condition check' }));
  await userEvent.click(editor.getByRole('combobox', { name: 'If no condition matches' }));
  await userEvent.click(screen.getByRole('option', { name: 'review' }));
  await userEvent.click(editor.getByRole('checkbox', { name: 'Choose result data' }));
  const result = within(editor.getByRole('group', { name: 'Default result data' }));
  await userEvent.type(result.getByRole('textbox', { name: 'Selection path' }), '/assessment');
  await userEvent.click(editor.getByRole('button', { name: 'Validate and use condition' }));
  expect(await editor.findByText('Condition check is configured')).toBeTruthy();
  expect(api.normalizeGateProgram).toHaveBeenCalledWith(
    'delivery',
    {
      program: {
        rules: [],
        fallback: 'review',
        default_output: { value: { mapping: 'pointer', path: '/assessment' } },
      },
    },
    expect.any(AbortSignal),
  );
  expect(draft().root.nodes[0]!.outcomes).toEqual(before);
});

it('authors a command from exact installed compiler facts with explicit result routing and no copied Flow', async () => {
  const { api, draft } = setup(undefined, 'build_command');
  const editor = within(await screen.findByRole('group', { name: 'Command' }));
  expect(screen.queryByRole('combobox', { name: 'Execution profile' })).toBeNull();
  await userEvent.click(editor.getByRole('combobox', { name: 'Installed command' }));
  await userEvent.click(screen.getByRole('option', { name: 'Check formatting · v3' }));
  expect(draft().root.nodes[0]!.build).toBeUndefined();
  await userEvent.click(editor.getByRole('button', { name: 'Use command' }));
  expect(draft().root.nodes[0]!.build).toEqual({
    build_configuration: {
      project_id: 'delivery',
      id: 'command-build',
      version: 3,
      definition_digest: 'f'.repeat(64),
    },
    tool: { identity: 'factory.declarative-gate', version: 'v1', digest: 'c'.repeat(64) },
    plugin: { identity: 'factory.declarative-gate', version: 'v1', digest: 'c'.repeat(64) },
    model_or_tool: { identity: 'format-check', version: 'v1', digest: 'c'.repeat(64) },
    task_digest: 'e'.repeat(64),
    result_output: 'observation.json',
    result_output_kind: 'report',
    result_outcome: 'accepted',
  });
  expect(api.createFlow).not.toHaveBeenCalled();
  expect(api.normalizeGateProgram).not.toHaveBeenCalled();
  expect(screen.queryByRole('textbox', { name: /digest|argv|json/i })).toBeNull();
});

it('retains the configured condition and draft text when normalization fails without leaking error details', async () => {
  const gate = {
    policy: { identity: 'factory.declarative-gate', version: 'v1', digest: 'c'.repeat(64) },
    parameters: {
      rules: [
        {
          when: {
            operation: 'exists' as const,
            operand: { source: 'path' as const, path: '/retained' },
          },
          outcome: 'accepted',
        },
      ],
      fallback: 'review',
    },
  };
  const { api, draft } = setup(gate);
  api.normalizeGateProgram.mockRejectedValueOnce(new Error('private normalization failure'));
  const editor = within(await screen.findByRole('group', { name: 'Condition check' }));
  await userEvent.clear(editor.getByRole('textbox', { name: 'Observation path' }));
  await userEvent.type(editor.getByRole('textbox', { name: 'Observation path' }), '/changed');
  await userEvent.click(editor.getByRole('button', { name: 'Validate and use condition' }));
  expect(await editor.findByRole('alert')).toBeTruthy();
  expect(draft().root.nodes[0]!.gate).toEqual(gate);
  expect(editor.getByRole('textbox', { name: 'Observation path' })).toHaveProperty(
    'value',
    '/changed',
  );
  expect(screen.queryByText('private normalization failure')).toBeNull();
});

it('requires re-selection when a command contract changes in the installed catalogue', async () => {
  const { api, client, draft } = setup(undefined, 'build_command');
  const editor = within(await screen.findByRole('group', { name: 'Command' }));
  await userEvent.click(editor.getByRole('combobox', { name: 'Installed command' }));
  await userEvent.click(screen.getByRole('option', { name: 'Check formatting · v3' }));
  const refreshed = await api.getAuthoringCapabilities();
  refreshed.commands[0]!.execution.build_configuration.version = 4;
  await act(async () => {
    client.setQueryData(queryKeys.factoryAuthoringCapabilities('delivery'), refreshed);
  });
  await userEvent.click(editor.getByRole('button', { name: 'Use command' }));
  expect(draft().root.nodes[0]!.build).toBeUndefined();
  expect(editor.getByRole('alert').textContent).toContain('Select it again');
});

it('retains unconfirmed condition edits across inspector navigation and blocks publication readiness until applied', async () => {
  const { onEditing, draft } = setup();
  const editor = within(await screen.findByRole('group', { name: 'Condition check' }));
  await userEvent.click(editor.getByRole('combobox', { name: 'If no condition matches' }));
  await userEvent.click(screen.getByRole('option', { name: 'review' }));
  expect(onEditing).toHaveBeenLastCalledWith(true);
  await userEvent.click(screen.getByRole('button', { name: 'Add step' }));
  await userEvent.click(screen.getByRole('button', { name: 'Edit step: check' }));
  const restored = within(await screen.findByRole('group', { name: 'Condition check' }));
  expect(restored.getByRole('combobox', { name: 'If no condition matches' }).textContent).toContain(
    'review',
  );
  await userEvent.click(restored.getByRole('button', { name: 'Validate and use condition' }));
  expect(await restored.findByText('Condition check is configured')).toBeTruthy();
  expect(onEditing).toHaveBeenLastCalledWith(false);
  expect(draft().root.nodes[0]!.gate?.parameters).toEqual({ rules: [], fallback: 'review' });
});

it('applies a delayed normalized condition without replacing graph edits made while waiting', async () => {
  const { api, policy, draft } = setup();
  let finish!: (value: Awaited<ReturnType<typeof api.normalizeGateProgram>>) => void;
  api.normalizeGateProgram.mockImplementationOnce(
    () =>
      new Promise((resolve) => {
        finish = resolve;
      }),
  );
  const editor = within(await screen.findByRole('group', { name: 'Condition check' }));
  await userEvent.click(editor.getByRole('combobox', { name: 'If no condition matches' }));
  await userEvent.click(screen.getByRole('option', { name: 'review' }));
  await userEvent.click(editor.getByRole('button', { name: 'Validate and use condition' }));
  await userEvent.click(screen.getByRole('button', { name: 'Flow settings' }));
  await userEvent.click(screen.getByRole('button', { name: 'Add terminal' }));
  await userEvent.type(screen.getByRole('textbox', { name: 'Terminal name' }), 'done');
  await act(async () => {
    finish({ policy, parameters: { rules: [], fallback: 'review' } });
  });
  expect(draft().root.nodes[0]!.gate).toBeTruthy();
  expect(draft().root.terminals.map((item) => item.key)).toContain('done');
});

it('discards unapplied step changes and ignores a later normalization reply', async () => {
  const { api, policy, onEditing, draft } = setup();
  let finish!: (value: Awaited<ReturnType<typeof api.normalizeGateProgram>>) => void;
  api.normalizeGateProgram.mockImplementationOnce(
    () =>
      new Promise((resolve) => {
        finish = resolve;
      }),
  );
  const editor = within(await screen.findByRole('group', { name: 'Condition check' }));
  await userEvent.click(editor.getByRole('combobox', { name: 'If no condition matches' }));
  await userEvent.click(screen.getByRole('option', { name: 'review' }));
  await userEvent.click(editor.getByRole('button', { name: 'Validate and use condition' }));
  await userEvent.click(screen.getByRole('button', { name: 'Discard unapplied step changes' }));
  await act(async () => {
    finish({ policy, parameters: { rules: [], fallback: 'review' } });
  });
  expect(draft().root.nodes[0]!.gate).toBeUndefined();
  expect(onEditing).toHaveBeenLastCalledWith(false);
  expect(screen.getByRole('combobox', { name: 'If no condition matches' }).textContent).toContain(
    'Choose',
  );
});

it('authors a scalar-result condition with explicit unmatched fallback using readable controls, without a vocabulary list', async () => {
  const { api, draft } = setup();
  const editor = within(await screen.findByRole('group', { name: 'Condition check' }));
  await userEvent.click(editor.getByRole('combobox', { name: 'If no condition matches' }));
  await userEvent.click(screen.getByRole('option', { name: 'review' }));
  await userEvent.click(editor.getByRole('button', { name: 'Add condition' }));
  await userEvent.click(editor.getByRole('combobox', { name: 'Predicate operation' }));
  await userEvent.click(screen.getByRole('option', { name: 'Compare values' }));
  await userEvent.type(editor.getByRole('textbox', { name: 'Observation path' }), '/component');
  await userEvent.type(editor.getByRole('textbox', { name: 'Constant value' }), 'server');
  await userEvent.click(editor.getByRole('combobox', { name: 'Condition outcome' }));
  await userEvent.click(screen.getByRole('option', { name: 'accepted' }));
  await userEvent.click(editor.getByRole('button', { name: 'Validate and use condition' }));
  expect(await editor.findByText('Condition check is configured')).toBeTruthy();
  expect(draft().root.nodes[0]!.gate?.parameters).toEqual({
    rules: [
      {
        when: {
          operation: 'compare',
          comparison: 'equal',
          left: { source: 'path', path: '/component' },
          right: { source: 'literal', value: 'server' },
        },
        outcome: 'accepted',
      },
    ],
    fallback: 'review',
  });
  expect(api.createFlow).not.toHaveBeenCalled();
  expect(screen.queryByRole('textbox', { name: 'Components' })).toBeNull();
});
