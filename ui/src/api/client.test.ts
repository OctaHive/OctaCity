import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import {
  MANAGEMENT_ERROR_CODES,
  ManagementApiError,
  createManagementApiClient,
  idempotencyHeaders,
  requestIdFromResponse,
  versionedMutationHeaders,
} from './client';

const OPENAPI_PATH = '/api/v1/openapi.json';
const REQUEST_ID = '33333333-3333-4333-8333-333333333333';
const CONSOLE_ORIGIN = 'https://console.example';

beforeEach(() => {
  vi.stubGlobal('location', { origin: CONSOLE_ORIGIN });
});

afterEach(() => {
  vi.unstubAllGlobals();
});

function jsonResponse(body: unknown, status: number, headers: Record<string, string> = {}) {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'content-type': 'application/json', ...headers },
  });
}

async function captureApiError(request: Promise<unknown>) {
  try {
    await request;
  } catch (error) {
    expect(error).toBeInstanceOf(ManagementApiError);
    if (error instanceof ManagementApiError) {
      return error;
    }
    throw error;
  }
  throw new Error('Expected the management API request to fail');
}

describe('management API request boundary', () => {
  it('uses only the relative management path and omits browser credentials', async () => {
    const requests: Request[] = [];
    const fetch = vi.fn(async (request: Request) => {
      requests.push(request);
      return jsonResponse({}, 200, { 'x-request-id': REQUEST_ID });
    });
    const client = createManagementApiClient(fetch);

    await client.GET(OPENAPI_PATH);

    expect(fetch).toHaveBeenCalledOnce();
    expect(requests).toHaveLength(1);
    expect(requests[0]?.url).toBe(`${CONSOLE_ORIGIN}${OPENAPI_PATH}`);
    expect(requests[0]?.credentials).toBe('omit');
  });

  it('rejects an absolute API origin before fetch', async () => {
    const fetch = vi.fn(async () => jsonResponse({}, 200));
    const client = createManagementApiClient(fetch);

    await expect(
      client.GET(OPENAPI_PATH, { baseUrl: 'https://management.example' }),
    ).rejects.toThrow('Management API requests must use relative /api/v1 paths');
    expect(fetch).not.toHaveBeenCalled();
  });

  it('rejects any attempt to attach browser credentials', async () => {
    const fetch = vi.fn(async () => jsonResponse({}, 200));
    const client = createManagementApiClient(fetch);

    await expect(client.GET(OPENAPI_PATH, { credentials: 'include' })).rejects.toThrow(
      "Management API requests must use credentials: 'omit'",
    );
    expect(fetch).not.toHaveBeenCalled();
  });
});

describe('management API errors', () => {
  it.each(Object.keys(MANAGEMENT_ERROR_CODES))('normalizes the stable %s error', async (code) => {
    const status = code === 'rate_limited' ? 429 : 400;
    const client = createManagementApiClient(async () =>
      jsonResponse(
        { code, message: `Safe ${code} explanation`, request_id: REQUEST_ID },
        status,
        code === 'rate_limited'
          ? { 'retry-after': '7', 'x-request-id': REQUEST_ID }
          : { 'x-request-id': REQUEST_ID },
      ),
    );

    const error = await captureApiError(client.GET(OPENAPI_PATH));

    expect(error).toMatchObject({
      code,
      message: `Safe ${code} explanation`,
      requestId: REQUEST_ID,
      retryAfterMilliseconds: code === 'rate_limited' ? 7_000 : null,
      status,
    });
  });

  it.each([
    ['1', 1_000],
    ['60', 60_000],
    ['61', 60_000],
    ['0', null],
    ['1.5', null],
    ['tomorrow', null],
  ])('bounds Retry-After %s to %s milliseconds', async (retryAfter, expected) => {
    const client = createManagementApiClient(async () =>
      jsonResponse({ code: 'rate_limited', message: 'Slow down', request_id: REQUEST_ID }, 429, {
        'retry-after': retryAfter,
        'x-request-id': REQUEST_ID,
      }),
    );

    const error = await captureApiError(client.GET(OPENAPI_PATH));

    expect(error.retryAfterMilliseconds).toBe(expected);
  });

  it('uses a validated response header for correlation when the error body is invalid', async () => {
    const client = createManagementApiClient(
      async () =>
        new Response('<html>proxy failure</html>', {
          status: 502,
          headers: { 'x-request-id': REQUEST_ID },
        }),
    );

    const error = await captureApiError(client.GET(OPENAPI_PATH));

    expect(error).toMatchObject({
      code: 'invalid_response',
      message: 'The management API returned an invalid error response.',
      requestId: REQUEST_ID,
      retryAfterMilliseconds: null,
      status: 502,
    });
  });

  it('does not expose transport error details', async () => {
    const client = createManagementApiClient(async () => {
      throw new Error('private proxy diagnostics');
    });

    const error = await captureApiError(client.GET(OPENAPI_PATH));

    expect(error).toMatchObject({
      code: 'transport_failure',
      message: 'The management API could not be reached.',
      requestId: null,
      retryAfterMilliseconds: null,
      status: null,
    });
    expect(error.message).not.toContain('private proxy diagnostics');
  });
});

describe('management API request metadata', () => {
  it('extracts only a bounded visible request identity', () => {
    expect(
      requestIdFromResponse(new Response(null, { headers: { 'x-request-id': REQUEST_ID } })),
    ).toBe(REQUEST_ID);
    expect(
      requestIdFromResponse(new Response(null, { headers: { 'x-request-id': '' } })),
    ).toBeNull();
    expect(
      requestIdFromResponse(new Response(null, { headers: { 'x-request-id': 'x'.repeat(129) } })),
    ).toBeNull();
  });

  it('builds validated idempotency headers', () => {
    expect(idempotencyHeaders(REQUEST_ID)).toEqual({ 'Idempotency-Key': REQUEST_ID });
    expect(() => idempotencyHeaders('')).toThrow('Invalid idempotency key');
    expect(() => idempotencyHeaders(' invalid ')).toThrow('Invalid idempotency key');
    expect(() => idempotencyHeaders('ключ')).toThrow('Invalid idempotency key');
    expect(() => idempotencyHeaders('x'.repeat(129))).toThrow('Invalid idempotency key');
  });

  it('builds a canonical strong precondition from a positive safe version', () => {
    expect(versionedMutationHeaders(REQUEST_ID, 7)).toEqual({
      'Idempotency-Key': REQUEST_ID,
      'If-Match': '"7"',
    });
    expect(() => versionedMutationHeaders(REQUEST_ID, 0)).toThrow('Invalid resource version');
    expect(() => versionedMutationHeaders(REQUEST_ID, 1.5)).toThrow('Invalid resource version');
    expect(() => versionedMutationHeaders(REQUEST_ID, Number.MAX_SAFE_INTEGER + 1)).toThrow(
      'Invalid resource version',
    );
  });

  it('applies the generated operation headers to idempotent and versioned mutations', async () => {
    const requests: Request[] = [];
    const client = createManagementApiClient(async (request) => {
      requests.push(request);
      return jsonResponse({}, 200, { 'x-request-id': REQUEST_ID });
    });

    await client.POST('/api/v1/builds/{build_id}/cancel', {
      params: {
        header: idempotencyHeaders(REQUEST_ID),
        path: { build_id: 'build-01' },
      },
    });
    await client.POST('/api/v1/builds/{build_id}/retention/hold/release', {
      params: {
        header: versionedMutationHeaders(REQUEST_ID, 7),
        path: { build_id: 'build-01' },
      },
    });

    expect(requests[0]?.headers.get('idempotency-key')).toBe(REQUEST_ID);
    expect(requests[0]?.headers.has('if-match')).toBe(false);
    expect(requests[1]?.headers.get('idempotency-key')).toBe(REQUEST_ID);
    expect(requests[1]?.headers.get('if-match')).toBe('"7"');
  });
});
