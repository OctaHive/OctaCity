// @vitest-environment jsdom
import { cleanup, render, screen, within } from '@testing-library/react';
import { afterEach, expect, it } from 'vitest';
import { MemoryRouter } from 'react-router-dom';
import { configurationFixture } from '../../../tests/fixtures/factory';
import { FactoryFlowRunGraph } from './FactoryFlowRunGraph';
import type { FactoryFlowRun } from './FactoryRunCommands';

afterEach(cleanup);
it('draws sibling and descendant relationships without passing through unrelated Flow cards', () => {
  const definition = configurationFixture().configuration.flow.closure.definitions[0]!.reference;
  const flow = (id: string, parent: string | null): FactoryFlowRun => ({
    id,
    factory_run_id: 'run',
    definition,
    parent: parent ? { flow_run_id: parent, node_attempt_id: `${parent}-call` } : null,
  });
  const flows = [
    flow('root', null),
    flow('first-child', 'root'),
    flow('second-child', 'root'),
    flow('grandchild', 'first-child'),
  ];
  render(
    <MemoryRouter>
      <FactoryFlowRunGraph
        flows={flows}
        getFlowHref={(flow) => `/factory/run?diagnostic_id=${flow.id}`}
      />
    </MemoryRouter>,
  );
  const diagram = screen.getByRole('img', { name: 'Nested Flow Runs' });
  const cards = Array.from(diagram.querySelectorAll('g')).map((group) => ({
    id: group.querySelector('title')!.textContent!,
    rect: group.querySelector('rect')!,
    edge: group.querySelector('path'),
  }));
  for (const card of cards) {
    if (!card.edge) continue;
    const parent = flows.find((flow) => flow.id === card.id)!.parent!.flow_run_id;
    const points = Array.from(
      card.edge.getAttribute('d')!.matchAll(/[ML]\s*(-?[\d.]+)\s+(-?[\d.]+)/gu),
      (match) => ({ x: Number(match[1]), y: Number(match[2]) }),
    );
    for (let index = 1; index < points.length; index++) {
      const a = points[index - 1]!;
      const b = points[index]!;
      for (const other of cards.filter((other) => other.id !== card.id && other.id !== parent)) {
        const x = Number(other.rect.getAttribute('x'));
        const y = Number(other.rect.getAttribute('y'));
        const right = x + Number(other.rect.getAttribute('width'));
        const bottom = y + Number(other.rect.getAttribute('height'));
        const intersects =
          Math.min(a.x, b.x) < right &&
          Math.max(a.x, b.x) > x &&
          Math.min(a.y, b.y) < bottom &&
          Math.max(a.y, b.y) > y;
        expect(intersects, `${parent} → ${card.id} must avoid ${other.id}`).toBe(false);
      }
    }
  }
  const table = screen.getByRole('table', { name: 'Nested Flow Runs' });
  expect(within(table).getAllByText('root → root-call')).toHaveLength(2);
  expect(within(table).getByText('first-child → first-child-call')).toBeTruthy();
});
