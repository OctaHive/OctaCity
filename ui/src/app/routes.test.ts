import { describe, expect, it } from 'vitest';

import { auditRequestPath, buildPath } from './routes';

describe('console route builders', () => {
  it('encodes resource identities at the route boundary', () => {
    expect(buildPath('build/one')).toBe('/builds/build%2Fone');
    expect(auditRequestPath('request / one')).toBe('/audit?request_identity=request+%2F+one');
  });
});
