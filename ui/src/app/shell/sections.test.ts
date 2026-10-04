import { describe, expect, it } from 'vitest';

import { sectionForPath } from './sections';

describe('sectionForPath', () => {
  it.each([
    ['/builds', 'builds'],
    ['/builds/build-01', 'builds'],
    ['/agents', 'agents'],
    ['/agents/agent-01', 'agents'],
    ['/agent-pools/pool-01', 'agents'],
    ['/audit', 'audit'],
    ['/projects/project-01', 'projects'],
  ])('maps the stable path %s to %s', (path, expected) => {
    expect(sectionForPath(path).id).toBe(expected);
  });

  it.each(['/builds-old', '/agents-archive', '/agent-pools-old', '/audit-log'])(
    'does not treat a lookalike prefix as a stable section path: %s',
    (path) => {
      expect(sectionForPath(path).id).toBe('projects');
    },
  );
});
