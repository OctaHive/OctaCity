// @vitest-environment jsdom
import { cleanup, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useState } from 'react';
import { afterEach, it, expect } from 'vitest';
import { configurationFixture } from '../../../tests/fixtures/factory';
import { FlowProjections } from './FlowProjections';
afterEach(cleanup);
it('declares Context and data access independently of existing control edges using predecessor contracts', async () => {
  const flow = configurationFixture().configuration.flow;
  const root = flow.closure.definitions.find((item) => item.reference.id === flow.closure.root.id);
  if (!root || !root.nodes[0] || !root.nodes[1]) throw new Error('Missing nodes');
  const predecessor = root.nodes[0],
    successor = root.nodes[1],
    outcome = predecessor.outcomes[0];
  if (!outcome) throw new Error('Missing outcome');
  predecessor.context_binding = {
    entries: {
      visible_summary: {
        source_kind: 'bounded_summary',
        source: { value: { mapping: 'pointer', path: '/summary' } },
      },
    },
  };
  let observed = root;
  function Harness() {
    const [value, setValue] = useState(root!);
    return (
      <FlowProjections
        value={value}
        onChange={(next) => {
          observed = { ...root!, ...next };
          setValue(observed);
        }}
      />
    );
  }
  render(<Harness />);
  await userEvent.click(screen.getByRole('button', { name: 'Add Context projection' }));
  await userEvent.click(screen.getByRole('checkbox', { name: 'visible_summary' }));
  expect(observed.context_projections.at(-1)).toEqual({
    predecessor: predecessor.key,
    outcome: outcome.key,
    successor: successor.key,
    selections: ['visible_summary'],
  });
  expect(observed.data_projections).toEqual(root.data_projections);
  await userEvent.click(screen.getByRole('button', { name: 'Add data projection' }));
  expect(observed.data_projections.at(-1)).toEqual({
    predecessor: predecessor.key,
    outcome: outcome.key,
    successor: successor.key,
    schema: outcome.schema,
  });
  expect(observed.transitions).toEqual(root.transitions);
});

it('preserves the remaining projection controls when an earlier row is removed', async () => {
  const flow = configurationFixture().configuration.flow;
  const root = flow.closure.definitions.find((item) => item.reference.id === flow.closure.root.id);
  if (!root || !root.nodes[0] || !root.nodes[1]) throw new Error('Missing nodes');
  root.nodes[0].context_binding = {
    entries: {
      visible_summary: {
        source_kind: 'bounded_summary',
        source: { value: { mapping: 'pointer', path: '/summary' } },
      },
    },
  };
  const edge = {
    predecessor: root.nodes[0].key,
    outcome: root.nodes[0].outcomes[0]!.key,
    successor: root.nodes[1].key,
    selections: ['visible_summary'],
  };
  root.context_projections = [{ ...edge }, { ...edge }];
  function Harness() {
    const [value, setValue] = useState(root!);
    return <FlowProjections value={value} onChange={(next) => setValue({ ...root!, ...next })} />;
  }
  render(<Harness />);
  const remaining = screen.getAllByRole('checkbox', { name: 'visible_summary' })[1];
  await userEvent.click(screen.getAllByRole('button', { name: 'Remove projection' })[0]!);
  expect(screen.getByRole('checkbox', { name: 'visible_summary' })).toBe(remaining);
});
