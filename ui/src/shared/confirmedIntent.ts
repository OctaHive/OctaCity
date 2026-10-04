import type { QueryClient, QueryKey } from '@tanstack/react-query';

import { ManagementApiError, idempotencyHeaders, versionedMutationHeaders } from '../api/client';

export type ConfirmedIntentState =
  'ready' | 'pending' | 'retryable' | 'succeeded' | 'failed' | 'abandoned';

type Immutable<T> = T extends (...arguments_: never[]) => unknown
  ? T
  : T extends readonly (infer Item)[]
    ? readonly Immutable<Item>[]
    : T extends object
      ? { readonly [Key in keyof T]: Immutable<T[Key]> }
      : T;

export interface ConfirmedMutationAttempt<Request> {
  readonly headers: Readonly<{
    'Idempotency-Key': string;
    'If-Match'?: string;
  }>;
  readonly idempotencyKey: string;
  readonly request: Immutable<Request>;
}

export interface QueryInvalidation {
  readonly exact?: boolean;
  readonly queryKey: QueryKey;
}

interface ConfirmedMutationIntentOptions<Request, Result> {
  readonly execute: (attempt: ConfirmedMutationAttempt<Request>) => Promise<Result>;
  readonly invalidate: readonly QueryInvalidation[];
  readonly queryClient: Pick<QueryClient, 'invalidateQueries'>;
  readonly request: Request;
  readonly version?: number;
}

export interface ConfirmedMutationIntent<Result> {
  readonly retryAtMilliseconds: number | null;
  readonly state: ConfirmedIntentState;
  abandon(): void;
  submit(): Promise<Result>;
}

const UUID_PATTERN = /^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;

/**
 * Captures one confirmed operator command in memory. The same frozen request and UUID are used for
 * every safe retry, while definitive failures close the intent instead of silently resubmitting it.
 */
export function createConfirmedMutationIntent<Request, Result>(
  options: ConfirmedMutationIntentOptions<Request, Result>,
): ConfirmedMutationIntent<Result> {
  return new InMemoryConfirmedMutationIntent(options);
}

class InMemoryConfirmedMutationIntent<Request, Result> implements ConfirmedMutationIntent<Result> {
  private attempt: ConfirmedMutationAttempt<Request> | null;
  private readonly execute: (attempt: ConfirmedMutationAttempt<Request>) => Promise<Result>;
  private failure: unknown = null;
  private inFlight: Promise<Result> | null = null;
  private readonly invalidations: readonly Required<QueryInvalidation>[];
  private readonly queryClient: Pick<QueryClient, 'invalidateQueries'>;
  private retryAt: number | null = null;
  private intentState: ConfirmedIntentState = 'ready';

  constructor(options: ConfirmedMutationIntentOptions<Request, Result>) {
    const idempotencyKey = createUuid();
    if (!UUID_PATTERN.test(idempotencyKey)) {
      throw new TypeError('Confirmed mutation idempotency key must be a UUID');
    }
    const headers =
      options.version === undefined
        ? idempotencyHeaders(idempotencyKey)
        : versionedMutationHeaders(idempotencyKey, options.version);
    this.attempt = Object.freeze({
      headers,
      idempotencyKey,
      request: immutableSnapshot(options.request),
    });
    this.invalidations = Object.freeze(
      options.invalidate.map(({ exact = false, queryKey }) =>
        Object.freeze({ exact, queryKey: immutableSnapshot(queryKey) as QueryKey }),
      ),
    );
    this.execute = options.execute;
    this.queryClient = options.queryClient;
  }

  get retryAtMilliseconds(): number | null {
    return this.retryAt;
  }

  get state(): ConfirmedIntentState {
    return this.intentState;
  }

  submit(): Promise<Result> {
    if (this.inFlight !== null) {
      return this.inFlight;
    }
    if (this.intentState === 'abandoned') {
      return Promise.reject(new Error('Confirmed intent was abandoned'));
    }
    if (this.intentState === 'succeeded') {
      return Promise.reject(new Error('Confirmed intent already succeeded'));
    }
    if (this.intentState === 'failed') {
      return Promise.reject(this.failure);
    }
    if (this.retryAt !== null && Date.now() < this.retryAt) {
      return Promise.reject(this.failure);
    }

    const attempt = this.attempt;
    if (attempt === null) {
      return Promise.reject(new Error('Confirmed intent is no longer active'));
    }
    this.intentState = 'pending';

    let request: Promise<Result>;
    try {
      request = this.execute(attempt);
    } catch (error) {
      request = Promise.reject(error);
    }

    const execution = request.then(
      async (result) => {
        this.failure = null;
        this.retryAt = null;
        this.intentState = 'succeeded';
        this.attempt = null;
        await this.invalidateQueries();
        return result;
      },
      (error: unknown) => {
        this.failure = error;
        if (isSafeRetryFailure(error)) {
          this.intentState = 'retryable';
          this.retryAt = retryAt(error, Date.now());
        } else {
          this.intentState = 'failed';
          this.retryAt = null;
          this.attempt = null;
        }
        throw error;
      },
    );
    this.inFlight = execution;
    void execution.then(
      () => {
        this.inFlight = null;
      },
      () => {
        this.inFlight = null;
      },
    );
    return execution;
  }

  abandon(): void {
    if (this.intentState === 'pending') {
      throw new Error('Cannot abandon a confirmed intent while its request is pending');
    }
    this.attempt = null;
    this.failure = null;
    this.retryAt = null;
    this.intentState = 'abandoned';
  }

  private async invalidateQueries(): Promise<void> {
    await Promise.allSettled(
      this.invalidations.map(({ exact, queryKey }) =>
        Promise.resolve().then(() => this.queryClient.invalidateQueries({ exact, queryKey })),
      ),
    );
  }
}

function createUuid(): string {
  if (typeof globalThis.crypto?.randomUUID !== 'function') {
    throw new Error('Secure UUID generation is unavailable');
  }
  return globalThis.crypto.randomUUID();
}

function isSafeRetryFailure(error: unknown): error is ManagementApiError {
  return (
    error instanceof ManagementApiError &&
    (error.code === 'transport_failure' ||
      (error.code === 'invalid_response' && error.status !== null && error.status >= 500) ||
      error.code === 'rate_limited')
  );
}

function retryAt(error: ManagementApiError, now: number): number | null {
  return error.code === 'rate_limited' && error.retryAfterMilliseconds !== null
    ? now + error.retryAfterMilliseconds
    : null;
}

function immutableSnapshot<T>(value: T): Immutable<T> {
  let snapshot: T;
  try {
    snapshot = structuredClone(value);
  } catch {
    throw new TypeError('Confirmed mutation request must be structured-cloneable');
  }
  return deepFreeze(snapshot) as Immutable<T>;
}

function deepFreeze(value: unknown): unknown {
  if (value === null || typeof value !== 'object') {
    return value;
  }
  if (!Array.isArray(value)) {
    const prototype = Object.getPrototypeOf(value) as unknown;
    if (prototype !== Object.prototype && prototype !== null) {
      throw new TypeError('Confirmed mutation request must contain only plain data');
    }
  }
  for (const item of Object.values(value)) {
    deepFreeze(item);
  }
  return Object.freeze(value);
}
