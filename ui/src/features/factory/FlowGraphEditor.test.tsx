// @vitest-environment jsdom
import { cleanup, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useState } from 'react';
import { afterEach, expect, it } from 'vitest';
import type { components } from '../../../.generated/api/schema';
import { configurationFixture } from '../../../tests/fixtures/factory';
import { FlowGraphEditor } from './FlowGraphEditor';
import { emptyFlowDraft } from './emptyFlowDraft';

afterEach(cleanup);

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
  await userEvent.click(screen.getByRole('button', { name: 'Add node' }));
  await userEvent.type(
    screen.getByRole('textbox', { name: 'Node name' }),
    'another_reasoning_call',
  );
  await userEvent.click(screen.getByRole('combobox', { name: 'Node kind' }));
  await userEvent.click(screen.getByRole('option', { name: 'Reasoning call' }));
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
  const attempts = screen.getByRole('spinbutton', { name: 'Node budget · Maximum attempts' });
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
  await userEvent.click(screen.getByRole('button', { name: 'Add node' }));
  await userEvent.type(screen.getByRole('textbox', { name: 'Node name' }), 'custom_check');
  await userEvent.click(screen.getByRole('button', { name: 'Add outcome' }));
  await userEvent.type(screen.getByRole('textbox', { name: 'Outcome name' }), 'observed');
  const schema = published.data_schemas[0]?.reference;
  if (!schema) throw new Error('Missing schema');
  await userEvent.click(screen.getByRole('combobox', { name: 'Outcome schema' }));
  await userEvent.click(
    screen.getByRole('option', { name: `${schema.identity} · ${schema.version}` }),
  );
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
  const name = screen.getByRole('textbox', { name: 'Node name' });
  await userEvent.clear(name);
  await userEvent.type(name, 'accessibility_audit');
  expect(changes.at(-1)?.root.entry).toBe('accessibility_audit');
  expect(
    changes.at(-1)?.root.transitions.some((edge) => edge.predecessor === 'accessibility_audit'),
  ).toBe(true);
  await userEvent.click(screen.getByRole('combobox', { name: 'Node kind' }));
  await userEvent.click(screen.getByRole('option', { name: 'Human gate' }));
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
  await userEvent.click(screen.getByRole('button', { name: 'Add node' }));
  await userEvent.click(screen.getByRole('combobox', { name: 'Node kind' }));
  await userEvent.click(screen.getByRole('option', { name: 'Subflow call' }));
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
  await userEvent.click(screen.getByRole('button', { name: 'Remove node' }));
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
  await userEvent.click(screen.getByRole('button', { name: 'Add node' }));
  await userEvent.click(screen.getByRole('combobox', { name: 'Node kind' }));
  await userEvent.click(screen.getByRole('option', { name: 'Subflow call' }));
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
