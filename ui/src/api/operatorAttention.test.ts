// @vitest-environment jsdom

import { afterEach, describe, expect, it, vi } from 'vitest';

afterEach(() => {
  vi.resetModules();
  vi.unstubAllGlobals();
});

describe('operator attention API', () => {
  it('sends one bounded same-origin scoped page with an opaque cursor', async () => {
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
    const { operatorAttentionApi } = await import('./operatorAttention');

    await operatorAttentionApi.listOperatorAttention(
      {
        agentIds: ['22222222-2222-4222-8222-222222222222'],
        buildIds: ['11111111-1111-4111-8111-111111111111'],
        includeCriticalConditions: true,
        poolIds: ['33333333-3333-4333-8333-333333333333'],
      },
      'opaque.cursor',
    );

    expect(observed).not.toBeNull();
    const request = observed as unknown as Request;
    const url = new URL(request.url);
    expect(url.pathname).toBe('/api/v1/operator-attention');
    expect(url.searchParams.getAll('agent_ids')).toEqual(['22222222-2222-4222-8222-222222222222']);
    expect(url.searchParams.getAll('build_ids')).toEqual(['11111111-1111-4111-8111-111111111111']);
    expect(url.searchParams.getAll('pool_ids')).toEqual(['33333333-3333-4333-8333-333333333333']);
    expect(url.searchParams.get('include_critical_conditions')).toBe('true');
    expect(url.searchParams.get('after')).toBe('opaque.cursor');
    expect(url.searchParams.get('limit')).toBe('50');
    expect(request.credentials).toBe('omit');
  });
});
