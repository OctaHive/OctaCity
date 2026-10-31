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
