import { QueryClientProvider } from '@tanstack/react-query';
import { createConsoleQueryClient } from '../../app/query';
// @vitest-environment jsdom
import { cleanup, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useState } from 'react';
import { afterEach, expect, it, vi } from 'vitest';
import type { components } from '../../../.generated/api/schema';
import { factoryProfileConfigurationFixture } from '../../../tests/fixtures/factoryRoles';
import { configurationFixture } from '../../../tests/fixtures/factory';
import { ConfigurationEditor } from './ConfigurationEditor';
import { FlowGraphEditor } from './FlowGraphEditor';
import { factoryApi, type FactoryApi } from './api';
import { emptyFlowDraft } from './emptyFlowDraft';

afterEach(cleanup);

it.each([
  'available',
  'missing',
  'edited',
  'disabled',
  'reordered',
  'routed',
  'dependent',
] as const)(
  'opts into local classification only with a usable Project mapping (%s)',
  async (scenario) => {
    const available = scenario !== 'missing';
    const draft = emptyFlowDraft();
    const sample = configurationFixture().configuration.flow.closure.definitions[0]!.nodes[0]!;
    const plugin = { identity: 'local-classification', version: 'v1', digest: 'f'.repeat(64) };
    const assessment = {
      reference: { identity: 'assessment', version: '1', digest: 'a'.repeat(64) },
      shape: {
        type: 'object',
        fields: {
          priority: { required: true, value: { type: 'integer', minimum: 0, maximum: 100 } },
          size: { required: false, value: { type: 'enum', values: ['small'] } },
          internal_notes: {
            required: true,
            value: { type: 'string', min_bytes: 1, max_bytes: 1024 },
          },
        },
      },
    } satisfies components['schemas']['FactoryFlowDataSchema'];
    draft.data_schemas = [assessment];
    draft.root.entry = 'triage';
    draft.root.nodes = [
      {
        key: 'apply',
        kind: 'trusted_action',
        required: true,
        input_schema: null,
        subflow: null,
        outcomes: [],
        budget: draft.root.execution.budget,
        permissions: draft.root.execution.permissions,
      },
      {
        ...sample,
        key: 'triage',
        outcomes: [{ key: 'assessed', kind: 'success', schema: assessment.reference }],
      },
    ];
    draft.root.transitions = [
      {
        predecessor: 'triage',
        outcome: 'assessed',
        target: { kind: 'node', key: 'apply' },
        max_repeats: 0,
        exhausted_terminal: null,
      },
    ];
    const receipt = {
      reference: { identity: 'local-classification-receipt', version: '1', digest: 'b'.repeat(64) },
      shape: {
        type: 'object',
        fields: {
          receipt: { required: true, value: { type: 'string', min_bytes: 64, max_bytes: 64 } },
        },
      },
    } satisfies components['schemas']['FactoryFlowDataSchema'];
    if (scenario === 'reordered') {
      draft.data_schemas.push({
        ...receipt,
        shape: { fields: receipt.shape.fields, type: 'object' },
      });
      draft.root.nodes[0]!.outcomes = [
        { key: 'applied', kind: 'success', schema: receipt.reference },
      ];
    }
    if (scenario === 'routed' || scenario === 'dependent') {
      draft.data_schemas.push(receipt);
      draft.root.nodes[0]!.outcomes = [
        { key: 'applied', kind: 'success', schema: receipt.reference },
      ];
      draft.root.terminals = [{ key: 'finished', schema: receipt.reference }];
      draft.root.transitions.push({
        predecessor: 'apply',
        outcome: 'applied',
        target: { kind: 'terminal', key: 'finished' },
        max_repeats: 0,
        exhausted_terminal: null,
      });
    }
    if (scenario === 'dependent') {
      draft.root.nodes.push({
        ...sample,
        key: 'consumer',
        input_binding: {
          sources: { selected: { source: 'predecessor', view: 'payload', path: '' } },
          mapping: { value: { mapping: 'pointer', path: '/selected' } },
        },
      });
      draft.root.transitions[1]!.target = { kind: 'node', key: 'consumer' };
    }
    const choices = {
      plugin,
      policy: 'project-triage',
      priority_maximum: 100,
      receipt_schema: receipt,
      receipt_output_bytes: 317,
      size_labels: [{ size: 'small', label: 'team-small', name: 'Small work' }],
    };
    let finishNormalization: () => void = () => {};
    const normalization = new Promise<void>((resolve) => {
      finishNormalization = resolve;
    });
    const api = {
      ...factoryApi,
      getAuthoringCapabilities: vi.fn(async () => ({
        commands: [],
        human_gates: [],
        build_resource_modes: ['none' as const],
        source_free_incompatibility: null,
        deterministic_gate: {
          policy: plugin,
          parameter_contract: 'declarative_gate_v1' as const,
          max_parameter_bytes: 65536,
          resource_modes: ['none' as const],
        },
        ...(available ? { issue_classification: choices } : {}),
      })),
      normalizeSchema: vi.fn<FactoryApi['normalizeSchema']>(async (body) => {
        if (scenario === 'edited' && body.identity.startsWith('issue-classification.'))
          await normalization;
        return {
          reference: {
            identity: body.identity,
            version: body.version,
            digest: (body.shape.type === 'object' && 'receipt' in body.shape.fields
              ? 'b'
              : 'c'
            ).repeat(64),
          },
          shape: body.shape,
        };
      }),
    };
    let observed = draft;
    function Harness() {
      const [value, setValue] = useState(draft);
      return (
        <FlowGraphEditor
          value={value}
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
    render(
      <QueryClientProvider client={createConsoleQueryClient()}>
        <Harness />
      </QueryClientProvider>,
    );
    await userEvent.click(screen.getByRole('button', { name: 'Edit step: apply' }));
    const toggle = await screen.findByRole('checkbox', { name: 'Apply classification to Issue' });
    expect(toggle).toHaveProperty('checked', false);
    expect(observed.root.nodes[0]?.action).toBeFalsy();
    expect(api.getAuthoringCapabilities).not.toHaveBeenCalled();
    await userEvent.click(toggle);
    if (!available) {
      expect(await screen.findByText(/Configure an installed classification mapping/)).toBeTruthy();
      expect(screen.getByRole('button', { name: 'Use Project classification' })).toHaveProperty(
        'disabled',
        true,
      );
      expect(observed.root.nodes[0]?.action).toBeFalsy();
      expect(api.normalizeSchema).not.toHaveBeenCalled();
      return;
    }
    expect(await screen.findByText('Small work')).toBeTruthy();
    await userEvent.click(screen.getByRole('button', { name: 'Use Project classification' }));
    if (scenario === 'edited') {
      await userEvent.click(screen.getByRole('button', { name: 'Add result' }));
      await userEvent.click(screen.getAllByText('Technical details and reuse').at(-1)!);
      await userEvent.clear(screen.getByRole('textbox', { name: 'Result key' }));
      await userEvent.type(screen.getByRole('textbox', { name: 'Result key' }), 'manual');
      await userEvent.click(screen.getByRole('button', { name: 'Save result' }));
      expect(observed.root.nodes[0]?.outcomes.map(({ key }) => key)).toContain('manual');
      finishNormalization();
      await waitFor(() => expect(observed.root.nodes[0]?.action).toBeTruthy());
      expect(observed.root.nodes[0]?.outcomes.map(({ key }) => key)).toEqual(['manual', 'applied']);
      return;
    }
    expect(observed.root.nodes[0]?.action).toMatchObject({
      plugin,
      action: 'apply_issue_classification',
      parameters: {
        policy: 'project-triage',
        source_node: 'triage',
        source_outcome: 'assessed',
        size_labels: { small: 'team-small' },
      },
    });
    expect(observed.root.nodes[0]?.outcomes).toContainEqual({
      key: 'applied',
      kind: 'success',
      schema: receipt.reference,
    });
    expect(api.normalizeSchema).toHaveBeenCalledTimes(1);
    expect(observed.root.nodes[0]?.budget.max_output_bytes).toBe(317);
    if (scenario === 'reordered') return;
    if (scenario === 'dependent') {
      expect(toggle).toHaveProperty('disabled', true);
      expect(observed.root.transitions[1]?.target).toEqual({ kind: 'node', key: 'consumer' });
      return;
    }
    if (scenario === 'routed') {
      await userEvent.click(toggle);
      expect(observed.root.nodes.map(({ key }) => key)).toEqual(['triage']);
      expect(observed.root.transitions).toMatchObject([
        {
          predecessor: 'triage',
          outcome: 'assessed',
          target: { kind: 'terminal', key: 'finished' },
        },
      ]);
      expect(observed.root.terminals[0]?.schema).toEqual(assessment.reference);
      return;
    }
    if (scenario === 'disabled') {
      await userEvent.click(toggle);
      expect(observed.root.nodes.map(({ key }) => key)).toEqual(['triage']);
      expect(observed.root.transitions).toEqual([]);
      expect(observed.root.entry).toBe('triage');
      return;
    }
    expect(observed.root.nodes[0]?.permissions.plugins).toEqual([]);
    expect(
      observed.data_schemas.find(
        ({ reference }) => reference.digest === observed.root.nodes[0]?.input_schema?.digest,
      )?.shape,
    ).toEqual({
      type: 'object',
      fields: {
        priority: { required: true, value: { type: 'integer', minimum: 0, maximum: 100 } },
        size: { required: false, value: { type: 'enum', values: ['small'] } },
      },
    });
    expect(observed.root.nodes[0]?.input_binding?.mapping.value).toEqual({
      mapping: 'if_present',
      path: '/assessment/size',
      then: {
        mapping: 'object',
        fields: {
          priority: { mapping: 'pointer', path: '/assessment/priority' },
          size: { mapping: 'pointer', path: '/assessment/size' },
        },
      },
      otherwise: {
        mapping: 'object',
        fields: {
          priority: { mapping: 'pointer', path: '/assessment/priority' },
        },
      },
    });
    const review = screen.getByRole('button', { name: 'Review required access' });
    expect(review).toHaveProperty('disabled', false);
    await userEvent.click(review);
    await userEvent.click(
      within(screen.getByRole('dialog', { name: 'Required access' })).getByRole('button', {
        name: 'Apply to draft',
      }),
    );
    expect(observed.root.nodes[0]?.permissions.plugins).toEqual([plugin]);
    await userEvent.click(screen.getByRole('button', { name: 'Edit step: triage' }));
    const stepName = screen.getByRole('textbox', { name: 'Step name' });
    await userEvent.clear(stepName);
    await userEvent.type(stepName, 'assess');
    expect(observed.root.nodes[0]?.action?.parameters).toMatchObject({ source_node: 'assess' });
  },
);

it('edits and clears the pipeline name without renaming its steps or changing their settings', async () => {
  const flow = configurationFixture().configuration.flow;
  const { reference: _reference, ...root } = structuredClone(flow.closure.definitions[0]!);
  void _reference;
  let observed = { root, limits: flow.limits, data_schemas: flow.data_schemas };
  const original = structuredClone(observed);
  function Harness() {
    const [value, setValue] = useState(observed);
    return (
      <FlowGraphEditor
        value={value}
        choices={[]}
        subflows={[]}
        onChange={(next) => {
          observed = next;
          setValue(next);
        }}
      />
    );
  }
  render(<Harness />);
  const name = screen.getByRole('textbox', { name: 'Pipeline name' });
  await userEvent.type(name, 'Триаж входящих задач');
  expect(observed.root).toMatchObject({ name: 'Триаж входящих задач' });
  expect(observed.root.nodes).toEqual(original.root.nodes);
  expect(observed.root.transitions).toEqual(original.root.transitions);
  await userEvent.clear(name);
  expect(observed.root).toMatchObject({ name: null });
});

it('calculates branch bounds immediately across different outcomes of the same step', async () => {
  const flow = configurationFixture().configuration.flow;
  const schema = flow.data_schemas[0]!.reference;
  const sample = flow.closure.definitions[0]!.nodes[0]!;
  let observed = emptyFlowDraft();
  observed.data_schemas = flow.data_schemas;
  observed.limits.max_fan_out = 64;
  observed.root.entry = 'first';
  observed.root.nodes = ['first', 'second', 'third'].map((key) => ({
    ...structuredClone(sample),
    key,
    outcomes: [
      { key: 'accepted', kind: 'success', schema },
      ...(key === 'first' ? [{ key: 'review', kind: 'failure' as const, schema }] : []),
    ],
  }));
  observed.root.transitions = [
    {
      predecessor: 'first',
      outcome: 'accepted',
      target: { kind: 'node', key: 'second' },
      max_repeats: 0,
      exhausted_terminal: null,
    },
    {
      predecessor: 'first',
      outcome: 'review',
      target: { kind: 'node', key: 'third' },
      max_repeats: 0,
      exhausted_terminal: null,
    },
  ];
  function Harness() {
    const [value, setValue] = useState(observed);
    return (
      <FlowGraphEditor
        value={value}
        choices={[]}
        subflows={[]}
        onChange={(next) => {
          observed = next;
          setValue(next);
        }}
      />
    );
  }
  render(<Harness />);
  await userEvent.click(screen.getByRole('button', { name: 'Flow settings' }));
  await userEvent.click(screen.getByText('Flow admission ceilings'));
  await userEvent.click(screen.getByRole('combobox', { name: 'Graph limits · Calculation' }));
  await userEvent.click(screen.getByRole('option', { name: 'Calculate automatically' }));
  expect(observed.limits.max_fan_out).toBe(2);
  expect(observed.limits.max_expanded_nodes).toBe(3);
});

it('keeps manual step and access settings available inside collapsed advanced sections', async () => {
  const flow = configurationFixture().configuration.flow;
  const { reference: _reference, ...root } = flow.closure.definitions[0]!;
  void _reference;
  root.nodes.find((node) => node.key === root.entry)!.permissions.network_hosts = ['keep.example'];
  root.execution.permissions.network_hosts = ['keep.example'];
  render(
    <FlowGraphEditor
      value={{ root, limits: flow.limits, data_schemas: flow.data_schemas }}
      choices={[]}
      subflows={[]}
      onChange={() => {}}
    />,
  );
  const options = screen.getByText('Advanced step settings');
  const step = within(screen.getByRole('group', { name: 'Step' }));
  expect(
    step.getByRole('textbox', { name: 'Step name' }).compareDocumentPosition(options) &
      Node.DOCUMENT_POSITION_FOLLOWING,
  ).toBeTruthy();
  expect(
    step.getByRole('group', { name: 'Expected result' }).compareDocumentPosition(options) &
      Node.DOCUMENT_POSITION_FOLLOWING,
  ).toBeTruthy();
  expect(step.getAllByRole('button').at(-1)).toBe(
    step.getByRole('button', { name: 'Remove step' }),
  );
  expect(options.closest('details')).toHaveProperty('open', false);
  await userEvent.click(options);
  expect(screen.getByRole('checkbox', { name: 'Required branch' })).toBeTruthy();
  expect(screen.getByRole('spinbutton', { name: 'Maximum execution retries' })).toBeTruthy();
  expect(screen.getByRole('combobox', { name: 'Named phase pool' })).toBeTruthy();
  const access = screen.getByText('Manual step access');
  expect(access.closest('details')).toHaveProperty('open', false);
  await userEvent.click(access);
  expect(screen.getByRole('group', { name: 'Step access request' })).toBeTruthy();
  expect(screen.getByRole('checkbox', { name: 'Network host · keep.example' })).toBeTruthy();
});

it('keeps fresh Flow limits in sync with new steps while leaving every limit editable in manual mode', async () => {
  let observed = emptyFlowDraft();
  function Harness() {
    const [value, setValue] = useState(observed);
    return (
      <FlowGraphEditor
        value={value}
        choices={[]}
        subflows={[]}
        onChange={(next) => {
          observed = next;
          setValue(next);
        }}
      />
    );
  }
  render(<Harness />);
  await userEvent.click(screen.getByRole('button', { name: 'Add step' }));
  await userEvent.type(screen.getByRole('textbox', { name: 'Step name' }), 'first');
  await userEvent.click(screen.getByRole('button', { name: 'Add step' }));
  await userEvent.type(screen.getByRole('textbox', { name: 'Step name' }), 'second');
  await userEvent.click(screen.getByRole('button', { name: 'Flow settings' }));
  expect(observed.limits.max_expanded_nodes).toBe(2);
  expect(observed.root.execution.budget.max_attempts).toBe(2);
  expect(observed.limits.budget.max_attempts).toBe(2);
  await userEvent.click(screen.getByRole('combobox', { name: 'Run budget · Calculation' }));
  await userEvent.click(screen.getByRole('option', { name: 'Set manually' }));
  const attempts = screen.getByRole('spinbutton', { name: 'Run budget · Maximum attempts' });
  await userEvent.clear(attempts);
  await userEvent.type(attempts, '8');
  expect(observed.root.execution.budget.max_attempts).toBe(8);
  expect(observed.limits.budget.max_attempts).toBe(8);
  await userEvent.click(screen.getByRole('button', { name: 'Flow settings' }));
  await userEvent.click(screen.getByRole('button', { name: 'Flow settings' }));
  expect(observed.root.execution.budget.max_attempts).toBe(8);
});

it('calculates a Flow budget from every step and retry without changing its manually selected admission ceiling', async () => {
  let observed = emptyFlowDraft();
  const node = configurationFixture().configuration.flow.closure.definitions[0]!.nodes[0]!;
  observed.root.entry = 'first';
  observed.root.nodes = [
    {
      ...structuredClone(node),
      key: 'first',
      max_execution_retries: 1,
      budget: {
        max_attempts: 2,
        max_elapsed_millis: 1000,
        max_tokens: 100,
        max_cost_micro_units: 10,
        max_output_bytes: 1000,
      },
    },
    {
      ...structuredClone(node),
      key: 'second',
      max_execution_retries: 0,
      budget: {
        max_attempts: 1,
        max_elapsed_millis: 2000,
        max_tokens: 200,
        max_cost_micro_units: 20,
        max_output_bytes: 2000,
      },
    },
  ];
  const admission = structuredClone(observed.limits);
  function Harness() {
    const [value, setValue] = useState(observed);
    return (
      <FlowGraphEditor
        value={value}
        choices={[]}
        subflows={[]}
        onChange={(next) => {
          observed = next;
          setValue(next);
        }}
      />
    );
  }
  render(<Harness />);
  await userEvent.click(screen.getByRole('button', { name: 'Flow settings' }));
  await userEvent.click(screen.getByRole('combobox', { name: 'Run budget · Calculation' }));
  await userEvent.click(screen.getByRole('option', { name: 'Calculate automatically' }));
  expect(observed.root.execution.budget).toEqual({
    max_attempts: 5,
    max_elapsed_millis: 4000,
    max_tokens: 400,
    max_cost_micro_units: 40,
    max_output_bytes: 4000,
  });
  expect(observed.limits).toEqual(admission);
});

it('authors permission declarations for an empty Flow from a visible server preset and retains the selection after collapse', async () => {
  const permissions = structuredClone(configurationFixture().configuration.flow.limits.permissions);
  const executable = { identity: 'visible.checker', version: 'v1', digest: 'a'.repeat(64) };
  permissions.network_hosts = ['api.allowed.example'];
  permissions.mounts = [{ root: '/workspace', mode: 'read_only' }];
  permissions.outputs.kinds = ['report'];
  permissions.executables = [executable];
  permissions.commands = [{ executable, arguments: [{ kind: 'exact', value: '--verify' }] }];
  const choices: components['schemas']['FactoryConfigurationChoiceResource'][] = [
    {
      identity: 'permissions-preset',
      kind: 'permission_ceiling',
      alias: 'Approved declarations',
      reference: {
        kind: 'immutable',
        reference: { identity: 'approved.permissions', version: 'v1', digest: 'b'.repeat(64) },
      },
      metadata: {
        node_kinds: [],
        signal_purposes: [],
        enforcement: [],
        compatible_choices: [],
        permission_declarations: permissions,
      },
    },
  ];
  let observed = emptyFlowDraft();
  function Harness() {
    const [value, setValue] = useState(observed);
    return (
      <FlowGraphEditor
        value={value}
        choices={choices}
        subflows={[]}
        onChange={(next) => {
          observed = next;
          setValue(next);
        }}
      />
    );
  }
  render(<Harness />);
  const settings = screen.getByRole('button', { name: 'Flow settings' });
  await userEvent.click(settings);
  await userEvent.click(screen.getByRole('combobox', { name: 'Admission permission preset' }));
  await userEvent.click(screen.getByRole('option', { name: 'Approved declarations · v1' }));
  const admission = screen.getByRole('group', { name: 'Admission permissions' });
  await userEvent.click(
    within(admission).getByRole('checkbox', { name: 'Network host · api.allowed.example' }),
  );
  await userEvent.click(
    within(admission).getByRole('checkbox', { name: 'Mount · /workspace · read_only' }),
  );
  await userEvent.click(within(admission).getByRole('checkbox', { name: 'Output kind · report' }));
  await userEvent.click(
    within(admission).getByRole('checkbox', {
      name: 'Command permission · visible.checker · v1 · "--verify"',
    }),
  );
  expect(observed.limits.permissions.network_hosts).toEqual(['api.allowed.example']);
  expect(observed.limits.permissions.mounts).toEqual([{ root: '/workspace', mode: 'read_only' }]);
  expect(observed.limits.permissions.outputs.kinds).toEqual(['report']);
  expect(observed.limits.permissions.commands).toEqual([
    { executable, arguments: [{ kind: 'exact', value: '--verify' }] },
  ]);
  expect(observed.limits.permissions.executables).toEqual([executable]);
  expect(observed.root.execution.permissions.network_hosts).toEqual([]);
  expect(observed.root.execution.permissions.commands).toEqual([]);
  await userEvent.click(settings);
  await userEvent.click(settings);
  expect(
    within(screen.getByRole('group', { name: 'Admission permissions' })).getByRole('checkbox', {
      name: 'Network host · api.allowed.example',
    }),
  ).toHaveProperty('checked', true);
});

it('selects a visible published execution profile with exact model and task provenance, without free-form identifiers', async () => {
  const published = configurationFixture().configuration.flow;
  const definition = published.closure.definitions.find(
    (item) => item.reference.id === published.closure.root.id,
  );
  const preset = definition?.nodes.find((node) => node.action);
  if (!definition || !preset?.action) throw new Error('Missing published profile');
  const capability = preset.action.plugin;
  preset.kind = 'reasoning';
  preset.build = {
    build_configuration: {
      id: '467728d6-88ce-4d26-a7ca-4cff47e8e05b',
      project_id: configurationFixture().configuration.reference.project_id,
      version: 1,
      definition_digest: '07'.repeat(32),
    },
    plugin: capability,
    tool: capability,
    model_or_tool: { ...capability, identity: 'exact.model' },
    task_digest: '08'.repeat(32),
    result_outcome: preset.outcomes[0]?.key ?? '',
    result_output: 'result',
    result_output_kind: 'report',
  };
  preset.action = null;
  const { reference: _reference, ...root } = definition;
  void _reference;
  let observed: components['schemas']['FactoryFlowDefinitionDraft'] = {
    root,
    data_schemas: published.data_schemas,
    limits: published.limits,
  };
  const references = [
    preset.build.plugin,
    preset.build.tool,
    preset.build.model_or_tool,
    ...(preset.build.result_tool ? [preset.build.result_tool] : []),
  ];
  const choices: components['schemas']['FactoryConfigurationChoiceResource'][] = references.map(
    (reference, index) => ({
      alias: 'visible-profile-' + index,
      identity: reference.identity,
      kind: 'node_capability',
      reference: { kind: 'immutable', reference },
    }),
  );
  choices.push({
    alias: 'Visible Build Configuration',
    identity: preset.build.build_configuration.id,
    kind: 'build_configuration',
    reference: { kind: 'build_configuration', reference: preset.build.build_configuration },
  });
  const subflows = [
    {
      flow: published,
      project_id: configurationFixture().configuration.reference.project_id,
      published_at_unix_ms: 1,
    },
  ];
  function Harness() {
    const [value, setValue] = useState(observed);
    return (
      <FlowGraphEditor
        value={value}
        choices={choices}
        subflows={subflows}
        onChange={(next) => {
          observed = next;
          setValue(next);
        }}
      />
    );
  }
  render(<Harness />);
  await userEvent.click(screen.getByRole('button', { name: 'Add step' }));
  await userEvent.type(
    screen.getByRole('textbox', { name: 'Step name' }),
    'another_reasoning_call',
  );
  await userEvent.click(screen.getByRole('combobox', { name: 'Step type' }));
  await userEvent.click(screen.getByRole('option', { name: 'AI agent' }));
  await userEvent.click(screen.getByRole('combobox', { name: 'Execution profile' }));
  await userEvent.click(
    screen.getByRole('option', {
      name: `${definition.entry} · ${preset.key} · v${definition.reference.version}`,
    }),
  );
  expect(observed.root.nodes.at(-1)?.build).toEqual(preset.build);
  expect(
    screen.getByText(
      `${preset.build.model_or_tool.identity} · ${preset.build.model_or_tool.version}`,
    ),
  ).toBeTruthy();
  expect(screen.queryByRole('textbox', { name: 'Task digest' })).toBeNull();
  expect(screen.queryByRole('textbox', { name: 'Model alias' })).toBeNull();
  await userEvent.click(screen.getByRole('combobox', { name: 'Execution profile' }));
  await userEvent.click(screen.getByRole('option', { name: 'None' }));
  expect(observed.root.nodes.at(-1)?.build).toBeNull();
});

it('keeps node budgets separate and selects permission references only from visible exact choices', async () => {
  const published = configurationFixture().configuration.flow;
  const definition = published.closure.definitions.find(
    (item) => item.reference.id === published.closure.root.id,
  );
  if (!definition) throw new Error('Missing root');
  const { reference: _reference, ...root } = definition;
  void _reference;
  let observed: components['schemas']['FactoryFlowDefinitionDraft'] = {
    root,
    data_schemas: published.data_schemas,
    limits: published.limits,
  };
  const reference = {
    identity: 'installed.capability',
    version: 'release-7',
    digest: '7'.repeat(64),
  };
  const choices: components['schemas']['FactoryConfigurationChoiceResource'][] = [
    {
      alias: 'Visible capability',
      identity: reference.identity,
      kind: 'node_capability',
      reference: { kind: 'immutable', reference },
    },
  ];
  function Harness() {
    const [value, setValue] = useState(observed);
    return (
      <FlowGraphEditor
        value={value}
        choices={choices}
        subflows={[]}
        onChange={(next) => {
          observed = next;
          setValue(next);
        }}
      />
    );
  }
  render(<Harness />);
  const attempts = screen.getByRole('spinbutton', { name: 'Step budget · Maximum attempts' });
  await userEvent.clear(attempts);
  await userEvent.type(attempts, '2');
  expect(observed.root.nodes.find((node) => node.key === root.entry)?.budget.max_attempts).toBe(2);
  expect(observed.root.execution.budget.max_attempts).toBe(root.execution.budget.max_attempts);
  await userEvent.click(
    screen.getByRole('checkbox', { name: 'Plugin · Visible capability · release-7' }),
  );
  expect(
    observed.root.nodes.find((node) => node.key === root.entry)?.permissions.plugins,
  ).toContainEqual(reference);
  expect(screen.queryByText('Hidden capability')).toBeNull();
  expect(screen.queryByRole('textbox', { name: 'Credential value' })).toBeNull();
});

it('adds typed outcomes and control transitions without implicitly granting context or data access', async () => {
  const published = configurationFixture().configuration.flow;
  const definition = published.closure.definitions.find(
    (item) => item.reference.id === published.closure.root.id,
  );
  if (!definition) throw new Error('Missing root');
  const { reference: _reference, ...root } = definition;
  void _reference;
  let observed: components['schemas']['FactoryFlowDefinitionDraft'] = {
    root,
    data_schemas: published.data_schemas,
    limits: published.limits,
  };
  function Harness() {
    const [value, setValue] = useState(observed);
    return (
      <FlowGraphEditor
        authoring={{
          projectId: 'project',
          api: { ...factoryApi, normalizeSchema: async () => published.data_schemas[0]! },
        }}
        value={value}
        choices={[]}
        subflows={[]}
        onChange={(next) => {
          observed = next;
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
  await userEvent.click(screen.getByRole('button', { name: 'Add step' }));
  await userEvent.type(screen.getByRole('textbox', { name: 'Step name' }), 'custom_check');
  await userEvent.click(screen.getByRole('button', { name: 'Add result' }));
  await userEvent.click(screen.getAllByText('Technical details and reuse').at(-1)!);
  await userEvent.clear(screen.getByRole('textbox', { name: 'Result key' }));
  await userEvent.type(screen.getByRole('textbox', { name: 'Result key' }), 'observed');
  await userEvent.click(screen.getByRole('button', { name: 'Save result' }));
  const schema = published.data_schemas[0]?.reference;
  if (!schema) throw new Error('Missing schema');
  expect(observed.root.nodes.at(-1)?.outcomes).toEqual([
    { key: 'observed', kind: 'success', schema },
  ]);
  await userEvent.click(screen.getByRole('button', { name: 'Add transition' }));
  const predecessor = screen.getAllByRole('combobox', { name: 'Predecessor' }).at(-1);
  if (!predecessor) throw new Error('Missing transition form');
  await userEvent.click(predecessor);
  await userEvent.click(screen.getByRole('option', { name: 'custom_check' }));
  const outcome = screen.getAllByRole('combobox', { name: 'Transition outcome' }).at(-1);
  if (!outcome) throw new Error('Missing outcome form');
  await userEvent.click(outcome);
  await userEvent.click(screen.getByRole('option', { name: 'observed' }));
  expect(observed.root.transitions.at(-1)?.predecessor).toBe('custom_check');
  expect(observed.root.transitions.at(-1)?.outcome).toBe('observed');
  expect(observed.root.context_projections).toEqual(root.context_projections);
  expect(observed.root.data_projections).toEqual(root.data_projections);
});

it('edits arbitrary logical nodes through the closed grammar without modifying the published graph or persisting drafts', async () => {
  const published = configurationFixture().configuration.flow;
  const definition = published.closure.definitions.find(
    (item) => item.reference.id === published.closure.root.id,
  );
  if (definition === undefined) throw new Error('Canonical fixture is missing its root');
  const { reference: _reference, ...root } = definition;
  void _reference;
  const initial: components['schemas']['FactoryFlowDefinitionDraft'] = {
    root,
    data_schemas: published.data_schemas,
    limits: published.limits,
  };
  const changes: components['schemas']['FactoryFlowDefinitionDraft'][] = [];
  function Harness() {
    const [draft, setDraft] = useState(initial);
    return (
      <FlowGraphEditor
        value={draft}
        choices={[]}
        subflows={[]}
        onChange={(next) => {
          changes.push(next);
          setDraft(next);
        }}
      />
    );
  }
  render(<Harness />);
  const name = screen.getByRole('textbox', { name: 'Step name' });
  await userEvent.clear(name);
  await userEvent.type(name, 'accessibility_audit');
  expect(changes.at(-1)?.root.entry).toBe('accessibility_audit');
  expect(
    changes.at(-1)?.root.transitions.some((edge) => edge.predecessor === 'accessibility_audit'),
  ).toBe(true);
  await userEvent.click(screen.getByRole('combobox', { name: 'Step type' }));
  await userEvent.click(screen.getByRole('option', { name: 'Operator decision' }));
  expect(changes.at(-1)?.root.nodes.find((node) => node.key === 'accessibility_audit')?.kind).toBe(
    'human_gate',
  );
  expect(
    changes.at(-1)?.root.nodes.find((node) => node.key === 'accessibility_audit')?.build,
  ).toBeNull();
  expect(definition.entry).toBe('implement');
  expect(screen.queryByRole('textbox', { name: 'Raw JSON' })).toBeNull();
  expect(JSON.stringify(localStorage)).not.toContain('accessibility_audit');
});

it('pins a nested Flow selected from a visible exact published closure', async () => {
  const published = configurationFixture().configuration.flow;
  const definition = published.closure.definitions.find(
    (item) => item.reference.id === published.closure.root.id,
  );
  if (!definition) throw new Error('Missing root');
  const { reference: _reference, ...root } = definition;
  void _reference;
  let observed: components['schemas']['FactoryFlowDefinitionDraft'] = {
    root,
    data_schemas: published.data_schemas,
    limits: published.limits,
  };
  const child = structuredClone(published);
  child.closure.root = {
    id: '33333333-3333-4333-8333-333333333333',
    version: 7,
    digest: '5'.repeat(64),
  };
  child.closure.definitions = [
    { ...definition, reference: child.closure.root, entry: 'child_entry' },
  ];
  function Harness() {
    const [value, setValue] = useState(observed);
    return (
      <FlowGraphEditor
        value={value}
        choices={[]}
        subflows={[
          {
            flow: child,
            project_id: configurationFixture().configuration.reference.project_id,
            published_at_unix_ms: 1,
          },
        ]}
        onChange={(next) => {
          observed = next;
          setValue(next);
        }}
      />
    );
  }
  render(<Harness />);
  await userEvent.click(screen.getByRole('button', { name: 'Add step' }));
  await userEvent.click(screen.getByRole('combobox', { name: 'Step type' }));
  await userEvent.click(screen.getByRole('option', { name: 'Nested Flow' }));
  await userEvent.click(screen.getByRole('combobox', { name: 'Nested Flow' }));
  await userEvent.click(screen.getByRole('option', { name: 'child_entry · v7' }));
  expect(observed.root.nodes.at(-1)?.subflow).toEqual(child.closure.root);
  expect(screen.queryByRole('textbox', { name: 'Subflow identifier' })).toBeNull();
});

it('edits the Flow entry, typed terminals and admission bounds independently of node execution budgets', async () => {
  const published = configurationFixture().configuration.flow;
  const definition = published.closure.definitions.find(
    (item) => item.reference.id === published.closure.root.id,
  );
  if (!definition) throw new Error('Missing root');
  const { reference: _reference, ...root } = definition;
  void _reference;
  let observed: components['schemas']['FactoryFlowDefinitionDraft'] = {
    root,
    data_schemas: published.data_schemas,
    limits: published.limits,
  };
  function Harness() {
    const [value, setValue] = useState(observed);
    return (
      <FlowGraphEditor
        value={value}
        choices={[]}
        subflows={[]}
        onChange={(next) => {
          observed = next;
          setValue(next);
        }}
      />
    );
  }
  render(<Harness />);
  await userEvent.click(screen.getByRole('button', { name: 'Flow settings' }));
  const repeats = screen.getByRole('spinbutton', { name: 'Admission · Maximum repeats' });
  await userEvent.clear(repeats);
  await userEvent.type(repeats, '3');
  expect(observed.limits.max_repeat_count).toBe(3);
  expect(observed.root.nodes).toEqual(root.nodes);
  await userEvent.click(screen.getByRole('button', { name: 'Add terminal' }));
  await userEvent.type(
    screen.getAllByRole('textbox', { name: 'Terminal name' }).at(-1)!,
    'operator_stop',
  );
  expect(observed.root.terminals.at(-1)).toEqual({
    key: 'operator_stop',
    schema: published.data_schemas[0]!.reference,
  });
  expect(
    screen
      .getAllByRole('spinbutton', { name: 'Maximum repeats' })
      .every((input) => input.getAttribute('max') === '3'),
  ).toBe(true);
});

it('removes a selected node and its graph edges while retaining unrelated node declarations', async () => {
  const flow = configurationFixture().configuration.flow;
  const definition = flow.closure.definitions.find(
    (item) => item.reference.id === flow.closure.root.id,
  )!;
  const { reference, ...root } = definition;
  void reference;
  const removed = root.entry;
  const untouched = root.nodes.find((node) => node.key !== removed)!;
  let observed: components['schemas']['FactoryFlowDefinitionDraft'] = {
    root,
    data_schemas: flow.data_schemas,
    limits: flow.limits,
  };
  function Harness() {
    const [value, setValue] = useState(observed);
    return (
      <FlowGraphEditor
        value={value}
        choices={[]}
        subflows={[]}
        onChange={(next) => {
          observed = next;
          setValue(next);
        }}
      />
    );
  }
  render(<Harness />);
  await userEvent.click(screen.getByRole('button', { name: 'Remove step' }));
  expect(observed.root.nodes.some((node) => node.key === removed)).toBe(false);
  expect(observed.root.entry).toBe('');
  expect(observed.root.nodes.find((node) => node.key === untouched.key)).toEqual(untouched);
  expect(
    observed.root.transitions.some(
      (edge) =>
        edge.predecessor === removed ||
        (edge.target.kind === 'node' && edge.target.key === removed),
    ),
  ).toBe(false);
});

it('declares optional branches, bounded execution retries and a named pool from the draft choices', async () => {
  const flow = configurationFixture().configuration.flow;
  const definition = flow.closure.definitions.find(
    (item) => item.reference.id === flow.closure.root.id,
  )!;
  const { reference, ...root } = definition;
  void reference;
  let observed: components['schemas']['FactoryFlowDefinitionDraft'] = {
    root,
    data_schemas: flow.data_schemas,
    limits: flow.limits,
    pools: {
      'operator-queue': {
        policy: {
          phase: 'operator-queue',
          max_wip: 2,
          max_project_wip: 1,
          budget: flow.limits.budget,
          order: ['severity', 'project_priority', 'age'],
        },
        project_priority: 0,
        capabilities: [],
        selection: { value: { mapping: 'constant', value: {} } },
      },
    },
  };
  function Harness() {
    const [value, setValue] = useState(observed);
    return (
      <FlowGraphEditor
        value={value}
        choices={[]}
        subflows={[]}
        onChange={(next) => {
          observed = next;
          setValue(next);
        }}
      />
    );
  }
  render(<Harness />);
  await userEvent.click(screen.getByRole('checkbox', { name: 'Required branch' }));
  const retry = screen.getByRole('spinbutton', { name: 'Maximum execution retries' });
  await userEvent.clear(retry);
  await userEvent.type(retry, '2');
  await userEvent.click(screen.getByRole('combobox', { name: 'Named phase pool' }));
  await userEvent.click(screen.getByRole('option', { name: 'operator-queue' }));
  expect(observed.root.nodes.find((node) => node.key === root.entry)).toEqual(
    expect.objectContaining({
      required: false,
      max_execution_retries: 2,
      phase_pool: 'operator-queue',
    }),
  );
  expect(screen.queryByRole('textbox', { name: 'Policy digest' })).toBeNull();
});

it('imports distinct child contracts when selecting a visible nested Flow', async () => {
  const published = configurationFixture().configuration.flow;
  const definition = published.closure.definitions.find(
    (item) => item.reference.id === published.closure.root.id,
  );
  if (!definition) throw new Error('Missing root');
  const { reference: _reference, ...root } = definition;
  void _reference;
  let observed: components['schemas']['FactoryFlowDefinitionDraft'] = {
    root,
    data_schemas: published.data_schemas,
    limits: published.limits,
  };
  const child = structuredClone(published);
  child.closure.root = {
    id: '33333333-3333-4333-8333-333333333333',
    version: 7,
    digest: '5'.repeat(64),
  };
  const schema = structuredClone(child.data_schemas[0]!);
  schema.reference = { identity: 'nested.result', version: 'v7', digest: '6'.repeat(64) };
  child.data_schemas = [schema];
  child.closure.definitions = [
    {
      ...definition,
      reference: child.closure.root,
      input_schema: schema.reference,
      nodes: definition.nodes.map((node) => ({
        ...node,
        input_schema: schema.reference,
        outcomes: node.outcomes.map((outcome) => ({ ...outcome, schema: schema.reference })),
      })),
      terminals: definition.terminals.map((terminal) => ({
        ...terminal,
        schema: schema.reference,
      })),
    },
  ];
  function Harness() {
    const [value, setValue] = useState(observed);
    return (
      <FlowGraphEditor
        value={value}
        choices={[]}
        subflows={[
          {
            flow: child,
            project_id: configurationFixture().configuration.reference.project_id,
            published_at_unix_ms: 1,
          },
        ]}
        onChange={(next) => {
          observed = next;
          setValue(next);
        }}
      />
    );
  }
  render(<Harness />);
  await userEvent.click(screen.getByRole('button', { name: 'Add step' }));
  await userEvent.click(screen.getByRole('combobox', { name: 'Step type' }));
  await userEvent.click(screen.getByRole('option', { name: 'Nested Flow' }));
  await userEvent.click(screen.getByRole('combobox', { name: 'Nested Flow' }));
  await userEvent.click(screen.getByRole('option', { name: `${definition.entry} · v7` }));
  expect(observed.root.nodes.at(-1)?.subflow).toEqual(child.closure.root);
  expect(observed.data_schemas).toContainEqual(schema);
  expect(observed.data_schemas).toContainEqual(published.data_schemas[0]);
});

it('keeps removed admission permissions selectable when reopening settings', async () => {
  const flow = configurationFixture().configuration.flow;
  const definition = flow.closure.definitions.find((d) => d.reference.id === flow.closure.root.id)!;
  const { reference: _reference, ...root } = definition;
  const initial = { root, data_schemas: flow.data_schemas, limits: flow.limits };
  void _reference;
  initial.limits.permissions.network_hosts = ['api.allowed.example'];
  function Harness() {
    const [value, setValue] = useState(initial);
    return <FlowGraphEditor value={value} choices={[]} subflows={[]} onChange={setValue} />;
  }
  render(<Harness />);
  const toggle = screen.getByRole('button', { name: 'Flow settings' });
  await userEvent.click(toggle);
  const permissions = screen.getByRole('group', { name: 'Admission permissions' });
  await userEvent.click(
    within(permissions).getByRole('checkbox', { name: 'Network host · api.allowed.example' }),
  );
  expect(
    within(permissions).getByRole('checkbox', { name: 'Network host · api.allowed.example' }),
  ).toHaveProperty('checked', false);
  await userEvent.click(toggle);
  await userEvent.click(toggle);
  expect(
    within(screen.getByRole('group', { name: 'Admission permissions' })).queryByRole('checkbox', {
      name: 'Network host · api.allowed.example',
    }),
  ).not.toBeNull();
});

it('separates queue, limits and access while preserving distinct admission and per-Run controls', async () => {
  let observed = emptyFlowDraft();
  function Harness() {
    const [value, setValue] = useState(observed);
    return (
      <FlowGraphEditor
        value={value}
        choices={[]}
        subflows={[]}
        onChange={(next) => {
          observed = next;
          setValue(next);
        }}
      />
    );
  }
  render(<Harness />);
  await userEvent.click(screen.getByRole('button', { name: 'Flow settings' }));
  expect(screen.getByRole('group', { name: 'Start and queue' })).toBeTruthy();
  expect(screen.getByRole('group', { name: 'Execution limits' })).toBeTruthy();
  expect(screen.getByRole('group', { name: 'Pipeline access' })).toBeTruthy();
  await userEvent.click(screen.getByRole('combobox', { name: 'Run budget · Calculation' }));
  await userEvent.click(screen.getByRole('option', { name: 'Set manually' }));
  await userEvent.click(screen.getByText('Flow admission ceilings'));
  await userEvent.click(screen.getByRole('combobox', { name: 'Admission budget · Calculation' }));
  await userEvent.click(screen.getByRole('option', { name: 'Set manually' }));
  const before = structuredClone(observed.limits.budget);
  const tokens = screen.getByRole('spinbutton', { name: 'Run budget · Maximum tokens' });
  await userEvent.clear(tokens);
  await userEvent.type(tokens, '123456');
  expect(observed.root.execution.budget.max_tokens).toBe(123456);
  expect(observed.limits.budget).toEqual(before);
});

it('keeps Configuration queue concurrency separate from the Flow run budget and access ceiling', () => {
  const config = configurationFixture().configuration;
  const value = {
    enabled: false,
    flow: config.flow,
    hard_budget: config.hard_budget,
    wip_limits: config.wip_limits,
  };
  render(
    <ConfigurationEditor
      value={value}
      policies={{
        launch: <span>Launch policy control</span>,
        access: <span>Access profile control</span>,
      }}
      flows={[]}
      onChange={() => {}}
    />,
  );
  expect(
    within(screen.getByRole('group', { name: 'Start and queue' })).getByRole('checkbox', {
      name: 'Enabled',
    }),
  ).toHaveProperty('checked', false);
  expect(
    within(screen.getByRole('group', { name: 'Execution limits' })).getByRole('group', {
      name: 'Configuration run budget',
    }),
  ).toBeTruthy();
  expect(
    within(screen.getByRole('group', { name: 'Pipeline access' })).getByText(
      'Access profile control',
    ),
  ).toBeTruthy();
});

it('renames an explicit accepted code candidate reference together with its producing step', async () => {
  const flow = configurationFixture().configuration.flow;
  let observed = emptyFlowDraft();
  const original = flow.closure.definitions[0]!.nodes[0]!;
  observed.root.entry = 'first';
  observed.root.nodes = [
    { ...structuredClone(original), key: 'first', code: { kind: 'none' } },
    {
      ...structuredClone(original),
      key: 'second',
      code: {
        kind: 'source',
        source: 'code_1',
        revision: { kind: 'accepted_candidate', node: 'first', outcome: 'accepted' },
        access: 'read_only',
        scope: { kind: 'repository' },
      },
    },
  ];
  function Harness() {
    const [value, setValue] = useState(observed);
    return (
      <FlowGraphEditor
        value={value}
        choices={[]}
        subflows={[]}
        onChange={(next) => {
          observed = next;
          setValue(next);
        }}
      />
    );
  }
  render(<Harness />);
  const name = screen.getByRole('textbox', { name: 'Step name' });
  await userEvent.clear(name);
  await userEvent.type(name, 'producer');
  expect(observed.root.nodes[1]!.code).toMatchObject({
    revision: { kind: 'accepted_candidate', node: 'producer', outcome: 'accepted' },
  });
});

it.each(['files', 'folder', 'repository'])(
  'freezes selected verification entrypoints and fixtures from %s',
  async (source) => {
    const draft = emptyFlowDraft();
    const sample =
      factoryProfileConfigurationFixture().configuration.flow.closure.definitions[0]!.nodes[0]!;
    const build = sample.profile_selection!.candidates.default!.build;
    draft.root.nodes = [
      {
        ...sample,
        key: 'prepare_checks',
        profile_selection: null,
        build: {
          ...build,
          result_output: 'reviewed_check_summary',
          evidence: [
            {
              kind: 'frozen_check_files',
              output_kind: 'artifact',
              tool: build.tool,
              plugin: build.plugin,
              schema: sample.outcomes[0]!.schema,
            },
          ],
        },
      },
      { ...sample, key: 'next_step' },
    ];
    draft.root.entry = 'prepare_checks';
    const installed = draft.root.nodes[0]!.build!;
    const authoring = {
      projectId: 'project',
      api: {
        ...factoryApi,
        getAuthoringCapabilities: vi.fn(async () => ({
          commands: [],
          human_gates: [],
          build_resource_modes: ['none' as const],
          source_free_incompatibility: null,
          deterministic_gate: {
            policy: build.plugin,
            parameter_contract: 'declarative_gate_v1' as const,
            max_parameter_bytes: 65536,
            resource_modes: ['none' as const],
          },
          verification_authors: [
            {
              build_configuration: installed.build_configuration,
              tool: installed.tool,
              plugin: installed.plugin,
              model_or_tool: installed.model_or_tool,
              result_output: installed.result_output,
              inventory: installed.evidence![0]!,
            },
          ],
        })),
      },
    };
    const artifact = {
      artifact_id: crypto.randomUUID(),
      content_digest: 'a'.repeat(64),
      encoded_size: 128,
    };
    const receipt = { artifact, project_id: 'project' };
    let finishFreeze!: (value: typeof receipt) => void;
    const pending = new Promise<typeof receipt>((resolve) => {
      finishFreeze = resolve;
    });
    const api = {
      list: vi.fn().mockResolvedValue({ items: [] }),
      read: vi.fn(),
      publish: vi
        .fn()
        .mockRejectedValueOnce(new Error('response lost'))
        .mockReturnValueOnce(pending)
        .mockResolvedValue(receipt),
    };
    const onChange = vi.fn();
    const onStepEditingChange = vi.fn();
    const repository = {
      id: 'checks',
      version: 1,
      name: 'Verification suite',
      project_id: 'project',
    };
    const codeResources = {
      projectId: 'project',
      repositories: {
        listRepositories: vi.fn().mockResolvedValue({ items: [repository] }),
        getRepository: vi.fn(),
      },
    };
    function Harness() {
      const [value, setValue] = useState(draft);
      return (
        <FlowGraphEditor
          choices={[]}
          subflows={[]}
          value={value}
          onChange={(next) => {
            onChange(next);
            setValue(next);
          }}
          onStepEditingChange={onStepEditingChange}
          documents={{ api, projectId: 'project' }}
          codeResources={codeResources}
          authoring={authoring}
        />
      );
    }
    render(
      <QueryClientProvider client={createConsoleQueryClient()}>
        <Harness />
      </QueryClientProvider>,
    );
    await userEvent.click(screen.getByRole('button', { name: 'Edit step: prepare_checks' }));
    const checks = await screen.findByRole('group', { name: 'Additional verification' });
    const run = within(checks).getByRole('button', { name: 'Freeze verification inputs' });
    expect((run as HTMLButtonElement).disabled).toBe(true);
    const prefix = source === 'files' ? '' : 'suite/';
    const entry = `${prefix}check.sh`;
    const fixture = source === 'repository' ? `${prefix}Octafile.yml` : `${prefix}fixture.bin`;
    const revision = '0123456789abcdef';
    if (source === 'repository') {
      await userEvent.click(within(checks).getByLabelText('Source of verification files'));
      await userEvent.click(screen.getByText('Project repository', { exact: true }));
      await userEvent.click(await within(checks).findByLabelText('Project repository'));
      await userEvent.click(screen.getByText('Verification suite · v1', { exact: true }));
      await userEvent.type(within(checks).getByLabelText('Exact revision'), revision);
      await userEvent.type(
        within(checks).getByLabelText('Repository paths (one file per line)'),
        `${entry}\n${fixture}`,
      );
    } else {
      const script = new File(['exit 0\n'], 'check.sh');
      const data = new File([new Uint8Array([0, 255])], 'fixture.bin');
      if (source === 'folder') {
        Object.defineProperty(script, 'webkitRelativePath', { value: entry });
        Object.defineProperty(data, 'webkitRelativePath', { value: fixture });
      }
      await userEvent.upload(
        within(checks).getByLabelText(
          source === 'files' ? 'Upload verification files' : 'Upload a verification folder',
        ),
        [script, data],
      );
    }
    await userEvent.type(
      within(checks).getByLabelText('Shell scripts to run (one path per line)'),
      entry,
    );
    if (source === 'repository') {
      await userEvent.click(within(checks).getByRole('button', { name: 'Add Octafile task' }));
      await userEvent.click(within(checks).getByLabelText('Octafile path'));
      await userEvent.click(screen.getByText(fixture, { exact: true }));
      const task = within(checks).getByLabelText('Task name');
      await userEvent.type(task, 'Checks:*');
      expect((run as HTMLButtonElement).disabled).toBe(true);
      expect(task.getAttribute('aria-invalid')).toBe('true');
      await userEvent.clear(task);
      await userEvent.type(task, 'Checks:_Verify');
    }
    expect(api.publish).not.toHaveBeenCalled();
    await userEvent.click(run);
    await waitFor(() => expect(api.publish).toHaveBeenCalledTimes(1));
    await waitFor(() => expect((run as HTMLButtonElement).disabled).toBe(false));
    if (source === 'repository') {
      await userEvent.click(screen.getByRole('button', { name: 'Flow settings' }));
      await userEvent.click(screen.getByRole('combobox', { name: 'Run budget · Calculation' }));
      await userEvent.click(screen.getByRole('option', { name: 'Calculate automatically' }));
    }
    await userEvent.click(run);
    expect(onStepEditingChange).toHaveBeenLastCalledWith(true);
    const nextStep = screen.getByRole('button', { name: 'Edit step: next_step' });
    expect(nextStep).toHaveProperty('disabled', true);
    expect(screen.getByRole('button', { name: 'Add step' })).toHaveProperty('disabled', true);
    await userEvent.click(nextStep);
    expect(screen.getByRole('textbox', { name: 'Step name' })).toHaveProperty(
      'value',
      'prepare_checks',
    );
    if (source === 'repository') {
      await userEvent.click(screen.getByRole('combobox', { name: 'Run budget · Calculation' }));
      await userEvent.click(screen.getByRole('option', { name: 'Set manually' }));
      const tokens = screen.getByRole('spinbutton', { name: 'Run budget · Maximum tokens' });
      await userEvent.clear(tokens);
      await userEvent.type(tokens, '123456');
      expect(tokens).toHaveProperty('value', '123456');
    }
    finishFreeze(receipt);
    await waitFor(() =>
      expect(onChange.mock.calls.at(-1)?.[0].root.nodes[0].protected_verification).toEqual(
        artifact,
      ),
    );
    if (source === 'repository') {
      expect(
        screen.getByRole('spinbutton', { name: 'Run budget · Maximum tokens' }),
      ).toHaveProperty('value', '123456');
    }
    await waitFor(() => expect(onStepEditingChange).toHaveBeenLastCalledWith(false));
    expect(api.publish.mock.calls[1]?.[2]).toEqual(api.publish.mock.calls[0]?.[2]);
    const [project, body] = api.publish.mock.calls[0]!;
    expect(project).toBe('project');
    expect(body.purpose).toBe('verification');
    expect(JSON.parse(body.content)).toEqual({
      ...(source === 'repository'
        ? { repository: { id: repository.id, version: 1, revision, paths: [entry, fixture] } }
        : {
            files: [
              { path: entry, content_base64: 'ZXhpdCAwCg==' },
              { path: fixture, content_base64: 'AP8=' },
            ],
          }),
      entries: [
        { kind: 'shell', path: entry },
        ...(source === 'repository'
          ? [{ kind: 'octafile', path: fixture, task: 'Checks:_Verify' }]
          : []),
      ],
    });
    await userEvent.click(run);
    await waitFor(() => expect(api.publish).toHaveBeenCalledTimes(3));
    expect(api.publish.mock.calls[2]?.[2]).not.toEqual(api.publish.mock.calls[1]?.[2]);
    await waitFor(() => expect(nextStep).toHaveProperty('disabled', false));
    await userEvent.click(nextStep);
    expect(screen.getByRole('textbox', { name: 'Step name' })).toHaveProperty('value', 'next_step');
  },
);
