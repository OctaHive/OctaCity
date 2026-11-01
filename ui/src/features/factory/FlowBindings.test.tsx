// @vitest-environment jsdom
import { cleanup, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useState } from 'react';
import { afterEach, it, expect } from 'vitest';
import { configurationFixture } from '../../../tests/fixtures/factory';
import { FlowBindings } from './FlowBindings';
afterEach(cleanup);
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
