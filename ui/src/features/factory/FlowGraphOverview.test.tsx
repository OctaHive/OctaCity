// @vitest-environment jsdom
import { cleanup, render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, it, expect } from 'vitest';
import { configurationFixture } from '../../../tests/fixtures/factory';
import { FlowGraphOverview } from './FlowGraphOverview';
afterEach(cleanup);
it('renders arbitrary graph identities and distinct control, Context and data edges from the selected definition', async () => {
  const flow = configurationFixture().configuration.flow;
  const root = flow.closure.definitions.find((item) => item.reference.id === flow.closure.root.id)!;
  root.nodes[0]!.key = 'operator_defined_node';
  root.transitions = [
    {
      predecessor: 'operator_defined_node',
      outcome: 'observed',
      target: { kind: 'node', key: root.nodes[1]!.key },
      max_repeats: 0,
      exhausted_terminal: null,
    },
  ];
  root.context_projections = [
    {
      predecessor: 'operator_defined_node',
      outcome: 'observed',
      successor: root.nodes[1]!.key,
      selections: ['summary'],
    },
  ];
  root.data_projections = [
    {
      predecessor: 'operator_defined_node',
      outcome: 'observed',
      successor: root.nodes[1]!.key,
      schema: flow.data_schemas[0]!.reference,
    },
  ];
  root.entry = 'operator_defined_node';
  render(<FlowGraphOverview value={root} />);
  const graph = screen.getByRole('img', { name: 'Flow graph' });
  expect(graph.textContent).toContain('operator_defined_node');
  expect(graph.querySelectorAll('[data-edge-kind="control"]')).toHaveLength(1);
  expect(graph.querySelectorAll('[data-edge-kind="context"]')).toHaveLength(0);
  expect(
    within(screen.getByRole('table', { name: 'Declared routes' })).getByText('observed'),
  ).toBeTruthy();
  await userEvent.click(screen.getByRole('combobox', { name: 'Connection type' }));
  await userEvent.click(screen.getByRole('option', { name: 'Context' }));
  expect(graph.querySelectorAll('[data-edge-kind="context"]')).toHaveLength(1);
  expect(graph.querySelectorAll('[data-edge-kind="control"]')).toHaveLength(0);
  await userEvent.click(screen.getByRole('combobox', { name: 'Connection type' }));
  await userEvent.click(screen.getByRole('option', { name: 'Data' }));
  expect(graph.querySelectorAll('[data-edge-kind="data"]')).toHaveLength(1);
});

it('starts at the declared entry, labels grouped outcomes and follows a selected target without interpreting names', async () => {
  const root = configurationFixture().configuration.flow.closure.definitions[0]!;
  render(<FlowGraphOverview value={root} />);
  const node = screen.getByRole('combobox', { name: 'Graph step' });
  expect(node.textContent).toContain(root.entry);
  const routes = screen.getByRole('table', { name: 'Declared routes' });
  expect(within(routes).getAllByText('retryable')).not.toHaveLength(0);
  expect(within(routes).getByText('10')).toBeTruthy();
  await userEvent.click(node);
  await userEvent.click(screen.getByRole('option', { name: 'validate' }));
  expect(within(routes).queryByText('10')).toBeNull();
  expect(within(routes).getByRole('button', { name: 'evaluate' })).toBeTruthy();
});
