// @vitest-environment jsdom
import { useState } from 'react';
import { cleanup, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, expect, it, vi } from 'vitest';
import { emptyFlowDraft } from './emptyFlowDraft';
import { FlowGraphEditor } from './FlowGraphEditor';

afterEach(cleanup);

function setup(
  options: {
    dangling?: boolean;
    typedResults?: boolean;
    existingWorkField?: boolean;
    fullSources?: boolean;
  } = {},
) {
  let observed = emptyFlowDraft();
  const schema = {
    reference: { identity: 'assessment', version: 'v1', digest: 'a'.repeat(64) },
    shape: { type: 'boolean' as const },
  };
  observed.data_schemas = [schema];
  observed.root.entry = 'first';
  observed.root.terminals = [{ key: 'finished', schema: schema.reference }];
  observed.root.nodes = ['first', 'second', 'third'].map((key) => ({
    key,
    kind: 'human_gate' as const,
    required: true,
    subflow: null,
    input_schema: null,
    budget: observed.root.execution.budget,
    permissions: observed.root.execution.permissions,
    outcomes: [{ key: 'observed', kind: 'success' as const, schema: schema.reference }],
  }));
  if (options.dangling)
    observed.root.transitions = [
      {
        predecessor: 'first',
        outcome: 'removed',
        target: { kind: 'node', key: 'deleted' },
        max_repeats: 0,
        exhausted_terminal: null,
      },
    ];
  if (options.typedResults) {
    const producer = {
      reference: { identity: 'assessment-fields', version: 'v1', digest: 'b'.repeat(64) },
      shape: {
        type: 'object' as const,
        fields: {
          fit: { required: true, value: { type: 'boolean' as const } },
          investigation: {
            required: true,
            value: { type: 'string' as const, min_bytes: 0, max_bytes: 2000 },
          },
        },
      },
    };
    const handoff = {
      reference: { identity: 'handoff', version: 'v1', digest: 'c'.repeat(64) },
      shape: {
        type: 'object' as const,
        fields: {
          fit: { required: true, value: { type: 'boolean' as const } },
          ...(options.existingWorkField
            ? {
                ticket: {
                  required: true,
                  value: { type: 'string' as const, min_bytes: 1, max_bytes: 128 },
                },
              }
            : {}),
        },
      },
    };
    observed.data_schemas.push(producer, handoff);
    observed.root.nodes[0]!.outcomes[0]!.schema = producer.reference;
    observed.root.nodes[0]!.context_binding = {
      entries: {
        investigation: {
          source_kind: 'specification',
          source: {
            value: {
              mapping: 'constant',
              value: {
                kind: 'artifact',
                value: {
                  artifact_id: 'private-investigation',
                  content_digest: 'd'.repeat(64),
                  encoded_size: 16,
                },
              },
            },
          },
        },
      },
    };
    observed.root.nodes[1]!.input_schema = handoff.reference;
    if (options.existingWorkField)
      observed.root.nodes[1]!.input_binding = {
        sources: { ticket: { source: 'work', path: '/external_identity' } },
        mapping: {
          value: { mapping: 'object', fields: { ticket: { mapping: 'pointer', path: '/ticket' } } },
        },
      };
    if (options.fullSources)
      observed.root.nodes[1]!.input_binding = {
        sources: Object.fromEntries(
          Array.from({ length: 64 }, (_, index) => [
            `ticket_${index}`,
            { source: 'work' as const, path: '/external_identity' },
          ]),
        ),
        mapping: { value: { mapping: 'object', fields: {} } },
      };
    observed.root.transitions = [
      {
        predecessor: 'first',
        outcome: 'observed',
        target: { kind: 'node', key: 'second' },
        max_repeats: 0,
        exhausted_terminal: null,
      },
    ];
  }
  const onChange = vi.fn();
  function Harness() {
    const [value, setValue] = useState(observed);
    return (
      <FlowGraphEditor
        value={value}
        choices={[]}
        subflows={[]}
        onChange={(next) => {
          observed = next;
          onChange(next);
          setValue(next);
        }}
      />
    );
  }
  render(<Harness />);
  return { onChange, draft: () => observed };
}

it('shows every step in the graph and keeps visual and keyboard selection on the same inspector without changing execution', async () => {
  const { onChange, draft } = setup();
  const before = structuredClone(draft());
  const graph = within(screen.getByRole('region', { name: 'Step graph' }));
  for (const name of ['first', 'second', 'third'])
    expect(graph.getByRole('button', { name: `Edit step: ${name}` })).toBeTruthy();
  await userEvent.click(graph.getByRole('button', { name: 'Edit step: second' }));
  expect(screen.getByRole('textbox', { name: 'Step name' })).toHaveProperty('value', 'second');
  await userEvent.click(graph.getByRole('combobox', { name: 'Graph step' }));
  await userEvent.click(screen.getByRole('option', { name: 'third' }));
  expect(screen.getByRole('textbox', { name: 'Step name' })).toHaveProperty('value', 'third');
  expect(draft()).toEqual(before);
  expect(onChange).not.toHaveBeenCalled();
});

it('connects declared outcomes visually without implicitly forwarding predecessor data or context', async () => {
  const { onChange, draft } = setup();
  const graph = within(screen.getByRole('region', { name: 'Step graph' }));
  await userEvent.click(graph.getByRole('button', { name: 'Connect from first' }));
  await userEvent.click(graph.getByRole('button', { name: 'Edit step: second' }));
  const route = within(graph.getByRole('group', { name: 'New route' }));
  expect(onChange).not.toHaveBeenCalled();
  await userEvent.click(route.getByRole('button', { name: 'Add route' }));
  expect(draft().root.transitions).toEqual([
    {
      predecessor: 'first',
      outcome: 'observed',
      target: { kind: 'node', key: 'second' },
      max_repeats: 0,
      exhausted_terminal: null,
    },
  ]);
  expect(draft().root.data_projections).toEqual([]);
  expect(draft().root.context_projections).toEqual([]);
  expect(onChange).toHaveBeenCalledTimes(1);
});

it('edits and removes the selected route through the same Flow contract as keyboard route controls', async () => {
  const { draft } = setup();
  const graph = within(screen.getByRole('region', { name: 'Step graph' }));
  await userEvent.click(graph.getByRole('button', { name: 'Connect from first' }));
  await userEvent.click(graph.getByRole('button', { name: 'Edit step: second' }));
  await userEvent.click(graph.getByRole('button', { name: 'Add route' }));
  await userEvent.click(
    graph.getByRole('button', { name: 'Edit route: first → second (observed)' }),
  );
  const inspector = within(graph.getByRole('group', { name: 'Selected route' }));
  await userEvent.click(inspector.getByRole('combobox', { name: 'Target' }));
  await userEvent.click(screen.getByRole('option', { name: 'Terminal · finished' }));
  expect(draft().root.transitions[0]!.target).toEqual({ kind: 'terminal', key: 'finished' });
  await userEvent.click(inspector.getByRole('button', { name: 'Remove transition' }));
  expect(draft().root.transitions).toEqual([]);
  expect(draft().root.data_projections).toEqual([]);
  expect(draft().root.context_projections).toEqual([]);
});

it('keeps dangling routes visible with actionable source and outcome guidance instead of dropping them from review', async () => {
  const { draft } = setup({ dangling: true });
  const graph = within(screen.getByRole('region', { name: 'Step graph' }));
  const alert = within(graph.getByRole('alert'));
  expect(
    alert.getByText(
      'first: outcome “removed” is missing. Choose an existing outcome or remove this route.',
    ),
  ).toBeTruthy();
  expect(
    alert.getByText('Destination “deleted” is missing. Choose another step or remove this route.'),
  ).toBeTruthy();
  await userEvent.click(
    alert.getByRole('button', { name: 'Edit route: first → deleted (removed)' }),
  );
  const inspector = within(graph.getByRole('group', { name: 'Selected route' }));
  await userEvent.click(inspector.getByRole('button', { name: 'Remove transition' }));
  expect(draft().root.transitions).toEqual([]);
  expect(graph.queryByRole('alert')).toBeNull();
});

it('adds a step from the graph toolbar and opens its settings without a second competing step list', async () => {
  const { draft } = setup();
  const graph = within(screen.getByRole('region', { name: 'Step graph' }));
  await userEvent.click(graph.getByRole('button', { name: 'Add step' }));
  expect(draft().root.nodes).toHaveLength(4);
  expect(screen.getByRole('textbox', { name: 'Step name' })).toHaveProperty('value', '');
  expect(screen.queryByRole('button', { name: 'second' })).toBeNull();
});

async function selectFit() {
  await userEvent.click(screen.getByRole('button', { name: 'Edit step: second' }));
  const inputs = within(screen.getByRole('group', { name: 'Accepted results' }));
  await userEvent.click(inputs.getByRole('combobox', { name: 'Result from step' }));
  await userEvent.click(screen.getByRole('option', { name: 'first' }));
  await userEvent.click(inputs.getByRole('combobox', { name: 'Result field' }));
  await userEvent.click(screen.getByRole('option', { name: 'fit' }));
  await userEvent.click(inputs.getByRole('combobox', { name: 'Step input field' }));
  await userEvent.click(screen.getByRole('option', { name: 'fit' }));
  return inputs;
}

it('connects only an explicitly selected accepted result field to the next step without copying private context or adding control routes', async () => {
  const { draft } = setup({ typedResults: true });
  const before = structuredClone(draft());
  const inputs = await selectFit();
  expect(draft().root.nodes[1]!.input_binding).toBeUndefined();
  await userEvent.click(inputs.getByRole('button', { name: 'Use selected field' }));
  const binding = draft().root.nodes[1]!.input_binding!;
  expect(Object.values(binding.sources)).toEqual([
    { source: 'record', node: 'first', outcome: 'observed', view: 'payload', path: '/fit' },
  ]);
  expect(binding.mapping.value.mapping).toBe('object');
  if (binding.mapping.value.mapping !== 'object') throw new Error('An object input is required');
  const field = binding.mapping.value.fields.fit!;
  expect(field.mapping).toBe('pointer');
  if (field.mapping !== 'pointer')
    throw new Error('The selected field must reference its explicit source');
  expect(binding.sources[field.path.slice(1)]).toEqual({
    source: 'record',
    node: 'first',
    outcome: 'observed',
    view: 'payload',
    path: '/fit',
  });
  expect(draft().root.nodes[0]).toEqual(before.root.nodes[0]);
  expect(draft().root.nodes[1]!.context_binding).toBeUndefined();
  expect(draft().root.nodes[2]).toEqual(before.root.nodes[2]);
  expect(draft().root.transitions).toEqual(before.root.transitions);
  expect(draft().root.data_projections).toEqual([]);
});

it('shows and disconnects a selected result field while preserving an independently mapped Work input', async () => {
  const { draft } = setup({ typedResults: true, existingWorkField: true });
  const before = structuredClone(draft().root.nodes[1]!.input_binding);
  const inputs = await selectFit();
  await userEvent.click(inputs.getByRole('button', { name: 'Use selected field' }));
  expect(inputs.getByText('fit ← first · observed · fit')).toBeTruthy();
  expect(inputs.queryByRole('alert')).toBeNull();
  expect(draft().root.nodes[1]!.input_binding!.sources.ticket).toEqual({
    source: 'work',
    path: '/external_identity',
  });
  await userEvent.click(inputs.getByRole('button', { name: 'Disconnect input: fit' }));
  expect(draft().root.nodes[1]!.input_binding).toEqual(before);
  expect(inputs.queryByText('fit ← first · observed · fit')).toBeNull();
});

it('explains incompatible result and input fields without creating a binding', async () => {
  const { draft } = setup({ typedResults: true });
  await userEvent.click(screen.getByRole('button', { name: 'Edit step: second' }));
  const inputs = within(screen.getByRole('group', { name: 'Accepted results' }));
  await userEvent.click(inputs.getByRole('combobox', { name: 'Result from step' }));
  await userEvent.click(screen.getByRole('option', { name: 'first' }));
  await userEvent.click(inputs.getByRole('combobox', { name: 'Result field' }));
  await userEvent.click(screen.getByRole('option', { name: 'investigation' }));
  expect(
    inputs.getByText(
      'No input field accepts this result. Choose another result field or change the step input schema.',
    ),
  ).toBeTruthy();
  expect(inputs.getByRole('button', { name: 'Use selected field' })).toHaveProperty(
    'disabled',
    true,
  );
  expect(draft().root.nodes[1]!.input_binding).toBeUndefined();
});

it('explains the installed input-source bound without mistaking it for an occupied field or changing the draft', async () => {
  const { draft, onChange } = setup({ typedResults: true, fullSources: true });
  const before = structuredClone(draft());
  const inputs = await selectFit();
  expect(
    inputs.getByText(
      'This step has reached the input-source limit. Disconnect an input before adding another.',
    ),
  ).toBeTruthy();
  expect(
    inputs.queryByText(
      'This input already has a mapping. Remove that selection or edit the advanced input mapping before replacing it.',
    ),
  ).toBeNull();
  expect(inputs.getByRole('button', { name: 'Use selected field' })).toHaveProperty(
    'disabled',
    true,
  );
  expect(draft()).toEqual(before);
  expect(onChange).not.toHaveBeenCalled();
});

it('offers visible zoom and fit controls while zooming changes only the canvas view', async () => {
  const { onChange, draft } = setup();
  const before = structuredClone(draft());
  const graph = within(screen.getByRole('region', { name: 'Step graph' }));
  expect(graph.getByRole('button', { name: 'Fit graph' })).toBeTruthy();
  await userEvent.click(graph.getByRole('button', { name: 'Zoom in' }));
  expect(graph.getByLabelText('Canvas zoom').textContent).toBe('125%');
  await userEvent.click(graph.getByRole('button', { name: 'Zoom out' }));
  expect(graph.getByLabelText('Canvas zoom').textContent).toBe('100%');
  expect(onChange).not.toHaveBeenCalled();
  expect(draft()).toEqual(before);
});
