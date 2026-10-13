// @vitest-environment jsdom
import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, it, expect } from 'vitest';
import { configurationFixture } from '../../../tests/fixtures/factory';
import { FlowGraphOverview } from './FlowGraphOverview';
afterEach(cleanup);
it('renders arbitrary graph identities and distinct control, Context and data edges from the selected definition', () => {
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
  render(<FlowGraphOverview value={root} />);
  const graph = screen.getByRole('img', { name: 'Flow graph' });
  expect(graph.textContent).toContain('operator_defined_node');
  expect(graph.querySelectorAll('[data-edge-kind="control"]')).toHaveLength(1);
  expect(graph.querySelectorAll('[data-edge-kind="context"]')).toHaveLength(1);
  expect(graph.querySelectorAll('[data-edge-kind="data"]')).toHaveLength(1);
});
