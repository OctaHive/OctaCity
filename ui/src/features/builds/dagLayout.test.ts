import { describe, expect, it } from 'vitest';

import { layoutDag } from './dagLayout';

describe('DAG layout', () => {
  it('places dependencies in deterministic layers independent of input order', () => {
    const first = layoutDag({
      edges: [
        { from: 'prepare', to: 'package' },
        { from: 'test', to: 'package' },
        { from: 'prepare', to: 'test' },
      ],
      nodes: [
        { id: 'package', sortKey: 'Package' },
        { id: 'prepare', sortKey: 'Prepare' },
        { id: 'test', sortKey: 'Test' },
      ],
    });
    const second = layoutDag({
      edges: [
        { from: 'prepare', to: 'test' },
        { from: 'test', to: 'package' },
        { from: 'prepare', to: 'package' },
      ],
      nodes: [
        { id: 'test', sortKey: 'Test' },
        { id: 'prepare', sortKey: 'Prepare' },
        { id: 'package', sortKey: 'Package' },
      ],
    });

    expect(second).toEqual(first);
    const positions = new Map(first.nodes.map((node) => [node.id, node]));
    expect(positions.get('prepare')?.x).toBeLessThan(positions.get('test')?.x ?? 0);
    expect(positions.get('test')?.x).toBeLessThan(positions.get('package')?.x ?? 0);
  });

  it('keeps malformed cyclic nodes visible in a deterministic fallback layer', () => {
    const layout = layoutDag({
      edges: [
        { from: 'one', to: 'two' },
        { from: 'two', to: 'one' },
      ],
      nodes: [
        { id: 'two', sortKey: 'Two' },
        { id: 'one', sortKey: 'One' },
      ],
    });

    expect(layout.nodes.map(({ id }) => id)).toEqual(['one', 'two']);
    expect(new Set(layout.nodes.map(({ x }) => x))).toHaveLength(1);
  });
});
