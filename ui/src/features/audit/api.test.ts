// @vitest-environment jsdom

import { afterEach, describe, expect, it, vi } from 'vitest';

afterEach(() => {
  vi.resetModules();
  vi.unstubAllGlobals();
});

describe('audit API', () => {
  it('sends one bounded same-origin page with exact filters and an opaque cursor', async () => {
    let observed: Request | null = null;
    vi.stubGlobal(
      'fetch',
      vi.fn(async (request: Request) => {
        observed = request.clone();
        return new Response(JSON.stringify({ items: [], next_cursor: null }), {
          headers: { 'content-type': 'application/json' },
          status: 200,
        });
      }),
    );
    const { auditApi } = await import('./api');

    await auditApi.listAuditFacts(
      {
        actorIdentity: 'agent-7',
        actorKind: 'agent',
        occurredFromUnixMs: 100,
        occurredThroughUnixMs: 200,
        operation: 'drain_agent',
        requestIdentity: 'request-7',
        targetIdentity: 'agent-7',
        targetKind: 'agent',
      },
      'opaque.cursor',
    );

    expect(observed).not.toBeNull();
    const request = observed as unknown as Request;
    const url = new URL(request.url);
    expect(url.pathname).toBe('/api/v1/audit-facts');
    expect(Object.fromEntries(url.searchParams)).toEqual({
      actor_identity: 'agent-7',
      actor_kind: 'agent',
      after: 'opaque.cursor',
      limit: '50',
      occurred_from_unix_ms: '100',
      occurred_through_unix_ms: '200',
      operation: 'drain_agent',
      request_identity: 'request-7',
      target_identity: 'agent-7',
      target_kind: 'agent',
    });
    expect(request.credentials).toBe('omit');
  });
});
