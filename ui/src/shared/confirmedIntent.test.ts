import { QueryClient } from '@tanstack/react-query';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { ManagementApiError } from '../api/client';
import { createConfirmedMutationIntent } from './confirmedIntent';

const FIRST_KEY = '11111111-1111-4111-8111-111111111111';
const SECOND_KEY = '22222222-2222-4222-8222-222222222222';

beforeEach(() => {
  vi.stubGlobal('crypto', { randomUUID: vi.fn(() => FIRST_KEY) });
});

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

function apiError(
  code: ConstructorParameters<typeof ManagementApiError>[0]['code'],
  retryAfterMilliseconds: number | null = null,
) {
  return new ManagementApiError({
    code,
    message: `Safe ${code} failure`,
    requestId: '33333333-3333-4333-8333-333333333333',
    retryAfterMilliseconds,
    status: code === 'transport_failure' ? null : code === 'rate_limited' ? 429 : 412,
  });
}

describe('confirmed mutation intent', () => {
  it('replays a lost response with the same UUID and immutable request', async () => {
    const original = { reason: 'maintenance', scope: { force: false } };
    const contexts: unknown[] = [];
    const execute = vi
      .fn()
      .mockImplementationOnce(async (context) => {
        contexts.push(context);
        throw apiError('transport_failure');
      })
      .mockImplementationOnce(async (context) => {
        contexts.push(context);
        return { accepted: true };
      });
    const queryClient = new QueryClient();
    const invalidate = vi.spyOn(queryClient, 'invalidateQueries').mockResolvedValue();
    const intent = createConfirmedMutationIntent({
      execute,
      invalidate: [
        { exact: true, queryKey: ['builds', 'build-1'] },
        { queryKey: ['projects', 'project-1', 'builds'] },
      ],
      queryClient,
      request: original,
    });

    original.reason = 'changed after confirmation';
    original.scope.force = true;

    await expect(intent.submit()).rejects.toMatchObject({ code: 'transport_failure' });
    expect(intent.state).toBe('retryable');
    await expect(intent.submit()).resolves.toEqual({ accepted: true });

    expect(execute).toHaveBeenCalledTimes(2);
    expect(contexts[0]).toBe(contexts[1]);
    expect(contexts[0]).toMatchObject({
      headers: { 'Idempotency-Key': FIRST_KEY },
      idempotencyKey: FIRST_KEY,
      request: { reason: 'maintenance', scope: { force: false } },
    });
    const firstContext = contexts[0] as { request: { scope: object } };
    expect(Object.isFrozen(firstContext.request)).toBe(true);
    expect(Object.isFrozen(firstContext.request.scope)).toBe(true);
    expect(invalidate).toHaveBeenNthCalledWith(1, {
      exact: true,
      queryKey: ['builds', 'build-1'],
    });
    expect(invalidate).toHaveBeenNthCalledWith(2, {
      exact: false,
      queryKey: ['projects', 'project-1', 'builds'],
    });
    expect(intent.state).toBe('succeeded');
  });

  it('shares one in-flight submission instead of issuing a concurrent duplicate', async () => {
    let resolveExecution: ((value: string) => void) | undefined;
    const execute = vi.fn(
      () =>
        new Promise<string>((resolve) => {
          resolveExecution = resolve;
        }),
    );
    const intent = createConfirmedMutationIntent({
      execute,
      invalidate: [],
      queryClient: new QueryClient(),
      request: undefined,
    });

    const first = intent.submit();
    const duplicate = intent.submit();

    expect(duplicate).toBe(first);
    expect(execute).toHaveBeenCalledOnce();
    resolveExecution?.('accepted');
    await expect(first).resolves.toBe('accepted');
  });

  it('abandons an uncertain intent and gives a later user intent a fresh key', async () => {
    const keys = [FIRST_KEY, SECOND_KEY];
    const createIdempotencyKey = vi.fn(() => {
      const key = keys.shift();
      if (key === undefined) {
        throw new Error('No test UUID remains');
      }
      return key;
    });
    vi.stubGlobal('crypto', { randomUUID: createIdempotencyKey });
    const submittedKeys: string[] = [];
    const execute = vi.fn(async (attempt: { readonly idempotencyKey: string }) => {
      submittedKeys.push(attempt.idempotencyKey);
      throw apiError('transport_failure');
    });
    const options = {
      execute,
      invalidate: [],
      queryClient: new QueryClient(),
      request: { reason: 'operator intent' },
    };

    const abandoned = createConfirmedMutationIntent(options);
    await expect(abandoned.submit()).rejects.toMatchObject({ code: 'transport_failure' });
    abandoned.abandon();
    expect(abandoned.state).toBe('abandoned');
    await expect(abandoned.submit()).rejects.toThrow('Confirmed intent was abandoned');

    const next = createConfirmedMutationIntent(options);
    await expect(next.submit()).rejects.toMatchObject({ code: 'transport_failure' });
    expect(submittedKeys).toEqual([FIRST_KEY, SECOND_KEY]);
  });

  it('applies the confirmed resource version as a strong If-Match header', async () => {
    const execute = vi.fn(async () => 'accepted');
    const intent = createConfirmedMutationIntent({
      execute,
      invalidate: [],
      queryClient: new QueryClient(),
      request: { drain: 'graceful' },
      version: 7,
    });

    await intent.submit();

    expect(execute).toHaveBeenCalledWith({
      headers: { 'Idempotency-Key': FIRST_KEY, 'If-Match': '"7"' },
      idempotencyKey: FIRST_KEY,
      request: { drain: 'graceful' },
    });
  });

  it('does not replay an accepted command when follow-up invalidation fails', async () => {
    const execute = vi.fn(async () => 'accepted');
    const queryClient = new QueryClient();
    vi.spyOn(queryClient, 'invalidateQueries').mockRejectedValue(new Error('refresh failed'));
    const intent = createConfirmedMutationIntent({
      execute,
      invalidate: [{ queryKey: ['builds', 'build-1'] }],
      queryClient,
      request: undefined,
    });

    await expect(intent.submit()).resolves.toBe('accepted');
    expect(intent.state).toBe('succeeded');
    await expect(intent.submit()).rejects.toThrow('Confirmed intent already succeeded');
    expect(execute).toHaveBeenCalledOnce();
  });

  it('treats a stale-version conflict as definitive and never resubmits it', async () => {
    const conflict = apiError('precondition_failed');
    const execute = vi.fn(async () => {
      throw conflict;
    });
    const intent = createConfirmedMutationIntent({
      execute,
      invalidate: [],
      queryClient: new QueryClient(),
      request: undefined,
      version: 3,
    });

    await expect(intent.submit()).rejects.toBe(conflict);
    expect(intent.state).toBe('failed');
    await expect(intent.submit()).rejects.toBe(conflict);
    expect(execute).toHaveBeenCalledOnce();
  });

  it('never replays a malformed forbidden response', async () => {
    const forbidden = new ManagementApiError({
      code: 'invalid_response',
      message: 'The management API returned an invalid error response.',
      requestId: 'request-forbidden',
      retryAfterMilliseconds: null,
      status: 403,
    });
    const execute = vi.fn(async () => {
      throw forbidden;
    });
    const intent = createConfirmedMutationIntent({
      execute,
      invalidate: [],
      queryClient: new QueryClient(),
      request: undefined,
    });

    await expect(intent.submit()).rejects.toBe(forbidden);
    expect(intent.state).toBe('failed');
    await expect(intent.submit()).rejects.toBe(forbidden);
    expect(execute).toHaveBeenCalledOnce();
  });

  it('retains a malformed server failure for safe replay', async () => {
    const serverFailure = new ManagementApiError({
      code: 'invalid_response',
      message: 'The management API returned an invalid error response.',
      requestId: 'request-server-failure',
      retryAfterMilliseconds: null,
      status: 502,
    });
    const execute = vi.fn().mockRejectedValueOnce(serverFailure).mockResolvedValueOnce('accepted');
    const intent = createConfirmedMutationIntent({
      execute,
      invalidate: [],
      queryClient: new QueryClient(),
      request: undefined,
    });

    await expect(intent.submit()).rejects.toBe(serverFailure);
    expect(intent.state).toBe('retryable');
    await expect(intent.submit()).resolves.toBe('accepted');
    expect(execute).toHaveBeenCalledTimes(2);
  });

  it('retains a rate-limited intent but refuses to retry before Retry-After', async () => {
    const now = vi.spyOn(Date, 'now').mockReturnValue(10_000);
    const rateLimit = apiError('rate_limited', 7_000);
    const execute = vi.fn().mockRejectedValueOnce(rateLimit).mockResolvedValueOnce('accepted');
    const intent = createConfirmedMutationIntent({
      execute,
      invalidate: [],
      queryClient: new QueryClient(),
      request: undefined,
    });

    await expect(intent.submit()).rejects.toBe(rateLimit);
    expect(intent.retryAtMilliseconds).toBe(17_000);

    now.mockReturnValue(16_999);
    await expect(intent.submit()).rejects.toBe(rateLimit);
    expect(execute).toHaveBeenCalledOnce();

    now.mockReturnValue(17_000);
    await expect(intent.submit()).resolves.toBe('accepted');
    expect(execute).toHaveBeenCalledTimes(2);
    expect(execute.mock.calls[0]?.[0]).toBe(execute.mock.calls[1]?.[0]);
  });

  it('never writes a request or idempotency key to browser storage', async () => {
    const localWrite = vi.fn();
    const sessionWrite = vi.fn();
    vi.stubGlobal('localStorage', { setItem: localWrite });
    vi.stubGlobal('sessionStorage', { setItem: sessionWrite });
    const intent = createConfirmedMutationIntent({
      execute: async () => 'accepted',
      invalidate: [],
      queryClient: new QueryClient(),
      request: { reason: 'must remain in memory' },
    });

    await intent.submit();

    expect(localWrite).not.toHaveBeenCalled();
    expect(sessionWrite).not.toHaveBeenCalled();
  });
});
