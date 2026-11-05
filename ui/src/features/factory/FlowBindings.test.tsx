// @vitest-environment jsdom
import { cleanup, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useState } from 'react';
import { afterEach, it, expect } from 'vitest';
import { configurationFixture } from '../../../tests/fixtures/factory';
import { FlowBindings } from './FlowBindings';
afterEach(cleanup);
it('shows step names for input formats and preserves exact references when selecting an incoming result format', async () => {
  const root = configurationFixture().configuration.flow.closure.definitions[0]!;
  const input = {
    reference: { identity: 'input.generated', version: '1', digest: 'a'.repeat(64) },
    shape: { type: 'object' as const, fields: {} },
  };
  const result = {
    reference: { identity: 'result.generated', version: '1', digest: 'b'.repeat(64) },
    shape: { type: 'boolean' as const },
  };
  const node: (typeof root.nodes)[number] = {
    ...root.nodes[0]!,
    key: 'systems_analysis',
    input_schema: input.reference,
    outcomes: [{ key: 'result', kind: 'success', schema: result.reference }],
    input_binding: {
      sources: { selected: { source: 'incoming', path: '', schema: input.reference } },
      mapping: { value: { mapping: 'object', fields: {} } },
    },
  };
  root.nodes = [node];
  let observed = node;
  render(
    <FlowBindings
      node={node}
      root={root}
      schemas={[input, result]}
      onChange={(next) => {
        observed = next;
      }}
    />,
  );
  await userEvent.click(screen.getByText('Advanced input and Context mappings'));
  expect(screen.getByRole('combobox', { name: 'Input schema' }).textContent).toContain(
    'Step input · systems_analysis',
  );
  const source = screen.getByRole('combobox', { name: 'Source schema' });
  expect(source.textContent).toContain('Step input · systems_analysis');
  await userEvent.click(source);
  await userEvent.click(screen.getByRole('option', { name: 'systems_analysis.result' }));
  expect(observed.input_schema).toEqual(input.reference);
  expect(observed.input_binding?.sources.selected).toEqual({
    source: 'incoming',
    path: '',
    schema: result.reference,
  });
});

it('declares a bounded summary and Work input source through typed controls without copying a transcript', async () => {
  const flow = configurationFixture().configuration.flow;
  const root = flow.closure.definitions.find((item) => item.reference.id === flow.closure.root.id);
  const node = root?.nodes[0];
  if (!root || !node) throw new Error('Missing node');
  let observed = node;
  function Harness() {
    const [value, setValue] = useState(node!);
    return (
      <FlowBindings
        root={root!}
        schemas={flow.data_schemas}
        node={value}
        onChange={(next) => {
          observed = next;
          setValue(next);
        }}
      />
    );
  }
  render(<Harness />);
  expect(
    screen.getByRole('button', { name: 'Add Context entry' }).closest('details'),
  ).toHaveProperty('open', false);
  await userEvent.click(screen.getByText('Advanced input and Context mappings'));
  await userEvent.click(screen.getByRole('button', { name: 'Add Context entry' }));
  await userEvent.type(
    screen.getByRole('textbox', { name: 'Context entry name' }),
    'review_summary',
  );
  await userEvent.click(screen.getByRole('combobox', { name: 'Context source kind' }));
  await userEvent.click(screen.getByRole('option', { name: 'Bounded summary' }));
  await userEvent.type(screen.getByRole('textbox', { name: 'Selection path' }), '/summary');
  expect(observed.context_binding?.entries.review_summary).toEqual({
    source_kind: 'bounded_summary',
    source: { value: { mapping: 'pointer', path: '/summary' } },
  });
  await userEvent.click(screen.getByRole('button', { name: 'Add input source' }));
  await userEvent.type(screen.getByRole('textbox', { name: 'Input source name' }), 'task');
  await userEvent.click(screen.getByRole('combobox', { name: 'Input source' }));
  await userEvent.click(screen.getByRole('option', { name: 'Work' }));
  expect(observed.input_binding?.sources.task).toEqual({ source: 'work', path: '' });
  expect(screen.queryByRole('textbox', { name: 'Transcript' })).toBeNull();
});

it('connects the admitted Work task explicitly without changing payload mappings or other Context entries', async () => {
  const flow = configurationFixture().configuration.flow;
  const root = flow.closure.definitions[0]!;
  const node = structuredClone(root.nodes[0]!);
  node.context_binding = {
    entries: {
      guidance: {
        source_kind: 'policy',
        source: { value: { mapping: 'pointer', path: '/guidance' } },
      },
    },
  };
  const originalInput = structuredClone(node.input_binding);
  let observed = node;
  function Harness() {
    const [value, setValue] = useState(node);
    return (
      <FlowBindings
        node={value}
        root={root}
        schemas={flow.data_schemas}
        onChange={(next) => {
          observed = next;
          setValue(next);
        }}
      />
    );
  }
  render(<Harness />);
  const task = screen.getByRole('checkbox', { name: 'Task from admitted Work' });
  expect(task).toHaveProperty('checked', false);
  await userEvent.click(task);
  expect(observed.context_binding).toMatchObject({
    work_task: 'task',
    entries: node.context_binding.entries,
  });
  expect(observed.input_binding).toEqual(originalInput);
  await userEvent.click(task);
  expect(observed.context_binding?.work_task).toBeUndefined();
  expect(observed.context_binding?.entries).toEqual(node.context_binding.entries);
});

it('keeps an occupied Context key and lets the operator choose another key for the Work task', async () => {
  const flow = configurationFixture().configuration.flow;
  const root = flow.closure.definitions[0]!;
  const node = structuredClone(root.nodes[0]!);
  node.context_binding = {
    entries: {
      task: {
        source_kind: 'specification',
        source: { value: { mapping: 'pointer', path: '/existing' } },
      },
    },
  };
  let observed = node;
  function Harness() {
    const [value, setValue] = useState(node);
    return (
      <FlowBindings
        node={value}
        root={root}
        schemas={flow.data_schemas}
        onChange={(next) => {
          observed = next;
          setValue(next);
        }}
      />
    );
  }
  render(<Harness />);
  const task = screen.getByRole('checkbox', { name: 'Task from admitted Work' });
  expect(task).toHaveProperty('disabled', true);
  expect(
    screen.getByText('This Context name is already in use. Choose another name under Details.'),
  ).toBeTruthy();
  const name = screen.getByRole('textbox', { name: 'Work task Context name', hidden: true });
  await userEvent.click(name.closest('details')!.querySelector('summary')!);
  await userEvent.clear(name);
  await userEvent.type(name, 'original_request');
  await userEvent.click(task);
  expect(observed.context_binding?.work_task).toBe('original_request');
  expect(observed.context_binding?.entries).toEqual(node.context_binding.entries);
});

it('disconnects the sole Work task without retaining an invalid empty Context binding', async () => {
  const flow = configurationFixture().configuration.flow;
  const root = flow.closure.definitions[0]!;
  const node = structuredClone(root.nodes[0]!);
  node.context_binding = { entries: {}, work_task: 'task' };
  let observed = node;
  render(
    <FlowBindings
      node={node}
      root={root}
      schemas={flow.data_schemas}
      onChange={(next) => {
        observed = next;
      }}
    />,
  );
  await userEvent.click(screen.getByRole('checkbox', { name: 'Task from admitted Work' }));
  expect(observed.context_binding).toBeNull();
});

it('connects only selected result fields without asking for an input schema and keeps an independent target format', async () => {
  const { factoryApi } = await import('./api');
  const flow = configurationFixture().configuration.flow;
  const root = structuredClone(flow.closure.definitions[0]!);
  const result = {
    reference: { identity: 'assessment', version: '1', digest: 'a'.repeat(64) },
    shape: {
      type: 'object' as const,
      fields: {
        summary: {
          required: true,
          value: { type: 'string' as const, min_bytes: 0, max_bytes: 200 },
        },
        fits: { required: true, value: { type: 'boolean' as const } },
        private_notes: {
          required: true,
          value: { type: 'string' as const, min_bytes: 0, max_bytes: 200 },
        },
      },
    },
  };
  const producer = {
    ...root.nodes[0]!,
    key: 'assess',
    outcomes: [{ key: 'assessed', kind: 'success' as const, schema: result.reference }],
  };
  const target = {
    ...root.nodes[0]!,
    key: 'classify',
    input_schema: null,
    input_binding: null,
    context_binding: null,
  };
  root.nodes = [producer, target];
  let observed = target as (typeof root.nodes)[number];
  let schemas = [result] as typeof flow.data_schemas;
  const requests: unknown[] = [];
  const normalizeSchema: typeof factoryApi.normalizeSchema = async (body) => {
    requests.push(body);
    return {
      reference: {
        identity: body.identity,
        version: body.version,
        digest: String(requests.length).repeat(64),
      },
      shape: body.shape,
    };
  };
  function Harness() {
    const [node, setNode] = useState(observed);
    const [formats, setFormats] = useState(schemas);
    return (
      <FlowBindings
        node={node}
        root={root}
        schemas={formats}
        authoring={{ projectId: 'project', api: { ...factoryApi, normalizeSchema } }}
        onChange={(next, added = []) => {
          observed = next;
          schemas = [...schemas, ...added];
          setFormats(schemas);
          setNode(next);
        }}
      />
    );
  }
  render(<Harness />);
  expect(screen.queryByRole('combobox', { name: 'Result field' })).toBeNull();
  expect(screen.queryByRole('combobox', { name: 'Step input field' })).toBeNull();
  await userEvent.click(screen.getByRole('combobox', { name: 'Result from step' }));
  await userEvent.click(screen.getByRole('option', { name: 'assess' }));
  expect(screen.getByRole('combobox', { name: 'Result field' })).toBeTruthy();
  expect(screen.queryByRole('combobox', { name: 'Step input field' })).toBeNull();
  for (const field of ['summary', 'fits']) {
    await userEvent.click(screen.getByRole('combobox', { name: 'Result field' }));
    await userEvent.click(screen.getByRole('option', { name: field }));
    expect(
      screen.queryByText('Choose an input schema above before connecting result fields.'),
    ).toBeNull();
    const name = screen.getByRole('textbox', { name: 'Input name' });
    await userEvent.clear(name);
    await userEvent.type(name, field);
    await userEvent.click(screen.getByRole('button', { name: 'Use selected field' }));
  }
  await userEvent.click(screen.getByRole('button', { name: 'Disconnect input: summary' }));
  await userEvent.click(screen.getByRole('combobox', { name: 'Result field' }));
  await userEvent.click(screen.getByRole('option', { name: 'fits' }));
  await userEvent.clear(screen.getByRole('textbox', { name: 'Input name' }));
  await userEvent.type(screen.getByRole('textbox', { name: 'Input name' }), 'summary');
  expect(screen.getByRole('button', { name: 'Use selected field' })).toHaveProperty(
    'disabled',
    true,
  );
  await userEvent.click(screen.getByRole('combobox', { name: 'Result field' }));
  await userEvent.click(screen.getByRole('option', { name: 'summary' }));
  await userEvent.click(screen.getByRole('combobox', { name: 'Step input field' }));
  await userEvent.click(screen.getByRole('option', { name: 'summary' }));
  await userEvent.click(screen.getByRole('button', { name: 'Use selected field' }));
  expect(schemas.at(-1)?.shape).toEqual({
    type: 'object',
    fields: {
      summary: result.shape.fields.summary,
      fits: result.shape.fields.fits,
    },
  });
  expect(observed.input_binding?.mapping.value).toEqual({
    mapping: 'object',
    fields: {
      summary: { mapping: 'pointer', path: '/accepted_0' },
      fits: { mapping: 'pointer', path: '/accepted_1' },
    },
  });
  expect(observed.input_binding?.sources).toEqual({
    accepted_0: {
      source: 'record',
      node: 'assess',
      outcome: 'assessed',
      view: 'payload',
      path: '/summary',
    },
    accepted_1: {
      source: 'record',
      node: 'assess',
      outcome: 'assessed',
      view: 'payload',
      path: '/fits',
    },
  });
  expect(producer.outcomes[0]?.schema).toEqual(result.reference);
  expect(observed.context_binding).toBeNull();
  const connected = structuredClone(observed);
  await userEvent.click(screen.getByRole('combobox', { name: 'Result from step' }));
  await userEvent.click(screen.getByRole('option', { name: 'Choose an option' }));
  expect(screen.queryByRole('combobox', { name: 'Result field' })).toBeNull();
  expect(screen.queryByRole('combobox', { name: 'Step input field' })).toBeNull();
  expect(screen.queryByRole('textbox', { name: 'Input name' })).toBeNull();
  expect(observed).toEqual(connected);
});

it('explicitly selects retained validation input and the exact candidate revision for a trusted step', async () => {
  const root = configurationFixture().configuration.flow.closure.definitions[0]!;
  const validation = root.nodes[0]!;
  const node: (typeof root.nodes)[number] = {
    ...validation,
    key: 'publish',
    kind: 'trusted_action',
    input_binding: {
      sources: {
        validated: {
          source: 'record',
          node: validation.key,
          outcome: validation.outcomes[0]!.key,
          path: '',
        },
      },
      mapping: { value: { mapping: 'pointer', path: '/validated' } },
    },
  };
  let observed = node;
  function Harness() {
    const [value, setValue] = useState(node);
    return (
      <FlowBindings
        root={{ ...root, nodes: [validation, value] }}
        schemas={[]}
        node={value}
        onChange={(next) => {
          observed = next;
          setValue(next);
        }}
      />
    );
  }
  render(<Harness />);
  await userEvent.click(screen.getByText('Advanced input and Context mappings'));
  await userEvent.click(screen.getByRole('combobox', { name: 'Record view' }));
  await userEvent.click(screen.getByRole('option', { name: 'Retained step input' }));
  await userEvent.click(screen.getByRole('checkbox', { name: 'Select a candidate revision' }));
  const candidate = screen.getByRole('group', { name: 'Candidate revision' });
  await userEvent.type(
    within(candidate).getByRole('textbox', { name: 'Selection path' }),
    '/candidate_revision',
  );
  expect(observed.input_binding?.sources.validated).toMatchObject({
    source: 'record',
    node: validation.key,
    outcome: validation.outcomes[0]!.key,
    path: '',
    view: 'input_payload',
  });
  expect(observed.context_binding?.candidate_revision).toEqual({
    value: { mapping: 'pointer', path: '/candidate_revision' },
  });
});
