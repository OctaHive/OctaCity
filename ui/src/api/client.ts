import createClient, { type Middleware } from 'openapi-fetch';

import type { components, paths } from '../../.generated/api/schema';

const MANAGEMENT_API_PREFIX = '/api/v1/';
const MAX_ERROR_MESSAGE_LENGTH = 4_096;
const MAX_IDEMPOTENCY_KEY_LENGTH = 128;
const MAX_REQUEST_ID_LENGTH = 128;
const MAX_AUTOMATIC_RETRY_AFTER_SECONDS = 60;

type ManagementErrorCode = components['schemas']['ErrorCode'];
type ManagementFetch = (request: Request) => Promise<Response>;

export const MANAGEMENT_ERROR_CODES = {
  invalid_request: true,
  unsupported_media_type: true,
  unsupported_api_version: true,
  payload_too_large: true,
  invalid_idempotency_key: true,
  idempotency_conflict: true,
  precondition_required: true,
  precondition_failed: true,
  forbidden: true,
  not_found: true,
  conflict: true,
  capability_unavailable: true,
  unavailable: true,
  rate_limited: true,
  internal: true,
} as const satisfies Readonly<Record<ManagementErrorCode, true>>;

export type ManagementApiFailureCode =
  ManagementErrorCode | 'invalid_response' | 'transport_failure';

interface ManagementApiErrorOptions {
  code: ManagementApiFailureCode;
  message: string;
  requestId: string | null;
  retryAfterMilliseconds: number | null;
  status: number | null;
}

/** Safe, normalized failure exposed by the management API boundary. */
export class ManagementApiError extends Error {
  readonly code: ManagementApiFailureCode;
  readonly requestId: string | null;
  readonly retryAfterMilliseconds: number | null;
  readonly status: number | null;

  constructor(options: ManagementApiErrorOptions) {
    super(options.message);
    this.name = 'ManagementApiError';
    this.code = options.code;
    this.requestId = options.requestId;
    this.retryAfterMilliseconds = options.retryAfterMilliseconds;
    this.status = options.status;
  }
}

/** Returns the validated correlation identity attached to a management response. */
export function requestIdFromResponse(response: Response): string | null {
  return boundedVisibleAscii(response.headers.get('x-request-id'), MAX_REQUEST_ID_LENGTH);
}

/** Builds the required replay header for an idempotent management mutation. */
export function idempotencyHeaders(
  idempotencyKey: string,
): Readonly<{ 'Idempotency-Key': string }> {
  if (boundedVisibleAscii(idempotencyKey, MAX_IDEMPOTENCY_KEY_LENGTH) === null) {
    throw new TypeError('Invalid idempotency key');
  }
  return Object.freeze({ 'Idempotency-Key': idempotencyKey });
}

/** Builds replay and strong optimistic-concurrency headers for a versioned mutation. */
export function versionedMutationHeaders(
  idempotencyKey: string,
  version: number,
): Readonly<{ 'Idempotency-Key': string; 'If-Match': string }> {
  if (!Number.isSafeInteger(version) || version <= 0) {
    throw new TypeError('Invalid resource version');
  }
  return Object.freeze({
    ...idempotencyHeaders(idempotencyKey),
    'If-Match': `"${version}"`,
  });
}

/** Creates the one OpenAPI-typed transport used by console feature code. */
export function createManagementApiClient(fetchImplementation: ManagementFetch = globalThis.fetch) {
  const client = createClient<paths>({
    baseUrl: '',
    credentials: 'omit',
    fetch: fetchImplementation,
    Request: relativeManagementRequest(),
  });
  client.use(managementBoundaryMiddleware);
  return client;
}

const managementBoundaryMiddleware: Middleware = {
  onRequest({ options, request, schemaPath }) {
    const url = new URL(request.url);
    if (
      options.baseUrl !== '' ||
      !isRelativeManagementPath(schemaPath) ||
      url.origin !== managementOrigin() ||
      !url.pathname.startsWith(MANAGEMENT_API_PREFIX)
    ) {
      throw new TypeError('Management API requests must use relative /api/v1 paths');
    }
    if (request.credentials !== 'omit') {
      throw new TypeError("Management API requests must use credentials: 'omit'");
    }
  },
  async onResponse({ response }) {
    if (!response.ok) {
      throw await normalizeErrorResponse(response);
    }
  },
  onError({ error }) {
    if (error instanceof ManagementApiError) {
      return error;
    }
    return new ManagementApiError({
      code: 'transport_failure',
      message: 'The management API could not be reached.',
      requestId: null,
      retryAfterMilliseconds: null,
      status: null,
    });
  },
};

export const managementApi = createManagementApiClient();

function relativeManagementRequest(): typeof Request {
  const NativeRequest = globalThis.Request;

  return class RelativeManagementRequest extends NativeRequest {
    constructor(input: RequestInfo | URL, init?: RequestInit) {
      if (typeof input !== 'string' || !isRelativeManagementPath(input)) {
        throw new TypeError('Management API requests must use relative /api/v1 paths');
      }
      if (init?.credentials !== undefined && init.credentials !== 'omit') {
        throw new TypeError("Management API requests must use credentials: 'omit'");
      }
      super(new URL(input, managementOrigin()), { ...init, credentials: 'omit' });
    }
  };
}

function managementOrigin(): string {
  if (typeof globalThis.location === 'undefined' || globalThis.location.origin === 'null') {
    throw new TypeError('Management API requests require a same-origin browser location');
  }
  return globalThis.location.origin;
}

function isRelativeManagementPath(value: string): boolean {
  return value.startsWith(MANAGEMENT_API_PREFIX);
}

async function normalizeErrorResponse(response: Response): Promise<ManagementApiError> {
  const headerRequestId = requestIdFromResponse(response);
  let body: unknown;
  try {
    body = await response.clone().json();
  } catch {
    return invalidResponse(response.status, headerRequestId);
  }

  if (!isManagementErrorBody(body)) {
    return invalidResponse(response.status, headerRequestId);
  }
  if (headerRequestId !== null && headerRequestId !== body.request_id) {
    return invalidResponse(response.status, headerRequestId);
  }

  return new ManagementApiError({
    code: body.code,
    message: body.message,
    requestId: body.request_id,
    retryAfterMilliseconds:
      body.code === 'rate_limited'
        ? retryAfterMilliseconds(response.headers.get('retry-after'))
        : null,
    status: response.status,
  });
}

function invalidResponse(status: number, requestId: string | null): ManagementApiError {
  return new ManagementApiError({
    code: 'invalid_response',
    message: 'The management API returned an invalid error response.',
    requestId,
    retryAfterMilliseconds: null,
    status,
  });
}

function isManagementErrorBody(
  value: unknown,
): value is { code: ManagementErrorCode; message: string; request_id: string } {
  if (typeof value !== 'object' || value === null) {
    return false;
  }
  if (!('code' in value) || !('message' in value) || !('request_id' in value)) {
    return false;
  }
  return (
    isManagementErrorCode(value.code) &&
    boundedText(value.message, MAX_ERROR_MESSAGE_LENGTH) !== null &&
    boundedVisibleAscii(value.request_id, MAX_REQUEST_ID_LENGTH) !== null
  );
}

function isManagementErrorCode(value: unknown): value is ManagementErrorCode {
  return typeof value === 'string' && Object.hasOwn(MANAGEMENT_ERROR_CODES, value);
}

function retryAfterMilliseconds(value: string | null): number | null {
  if (value === null || !/^[1-9][0-9]*$/.test(value)) {
    return null;
  }
  const seconds = Number(value);
  return (
    Math.min(
      Number.isSafeInteger(seconds) ? seconds : MAX_AUTOMATIC_RETRY_AFTER_SECONDS,
      MAX_AUTOMATIC_RETRY_AFTER_SECONDS,
    ) * 1_000
  );
}

function boundedText(value: unknown, maximumLength: number): string | null {
  return typeof value === 'string' &&
    value.length > 0 &&
    value.length <= maximumLength &&
    value.trim() === value &&
    !hasControlCharacters(value)
    ? value
    : null;
}

function boundedVisibleAscii(value: unknown, maximumLength: number): string | null {
  if (typeof value !== 'string') {
    return null;
  }
  return boundedText(value, maximumLength) !== null && /^[\x20-\x7e]+$/.test(value) ? value : null;
}

function hasControlCharacters(value: string): boolean {
  return [...value].some((character) => {
    const codePoint = character.codePointAt(0);
    return (
      codePoint !== undefined && (codePoint <= 0x1f || (codePoint >= 0x7f && codePoint <= 0x9f))
    );
  });
}
