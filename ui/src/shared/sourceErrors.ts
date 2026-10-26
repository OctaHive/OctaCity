import {
  ManagementApiError,
  redactManagementError,
  type ManagementApiFailureCode,
} from '../api/client';
import type { MessageKey } from '../app/presentation/messages';
import type { components } from '../../.generated/api/schema';

/** Normalize Source management queries before React Query retains their rejection. */
export async function sourceRequest<T>(request: Promise<T>): Promise<T> {
  try {
    return await request;
  } catch (error) {
    throw safeSourceError(error);
  }
}

/** Stale authoring must refresh its exact parents before another validation/review. */
export function sourceNeedsRefresh(error: unknown): boolean {
  return (
    error instanceof ManagementApiError &&
    (error.code === 'precondition_failed' || error.code === 'conflict')
  );
}

/** A deferred preview is a classified read failure, never a successful empty scan. */
export function sourcePreviewFailure(
  failure: NonNullable<components['schemas']['WorkSourcePreview']['failure']>,
  retryAfter: number | null,
  t: (key: MessageKey) => string,
): ManagementApiError {
  let code: ManagementApiFailureCode = 'unavailable';
  if (failure === 'rate_limited' || failure === 'conflict') code = failure;
  else if (failure === 'invalid_configuration') code = 'invalid_request';
  else if (failure === 'protocol_fault') code = 'capability_unavailable';
  return sourceFailure(
    new ManagementApiError({
      code,
      message: '',
      status: null,
      requestId: null,
      retryAfterMilliseconds: code === 'rate_limited' ? retryAfter : null,
    }),
    t,
  );
}

/** Local guidance follows closed API classifications, never provider message text. */
export function sourceFailure(error: unknown, t: (key: MessageKey) => string): ManagementApiError {
  let key: MessageKey = 'factory.sourceFailure';
  if (error instanceof ManagementApiError) {
    switch (error.code) {
      case 'invalid_request':
        key = 'factory.sourceInvalid';
        break;
      case 'not_found':
        key = 'factory.sourceMissing';
        break;
      case 'forbidden':
        key = 'factory.sourceDenied';
        break;
      case 'capability_unavailable':
        key = 'factory.sourceSetupRequired';
        break;
      case 'conflict':
      case 'idempotency_conflict':
      case 'precondition_failed':
      case 'precondition_required':
        key = 'factory.sourceStale';
        break;
      case 'rate_limited':
        key = 'factory.sourceDeferred';
        break;
      case 'unavailable':
        key = 'factory.sourceUnavailable';
        break;
      case 'transport_failure':
        key = 'factory.sourceTransport';
        break;
    }
  } else {
    key = 'factory.sourceTransport';
  }
  return safeSourceError(error, t(key));
}

/** Keep closed metadata and server UUID correlation, never remote text or diagnostics. */
export function safeSourceError(
  error: unknown,
  message = 'The source operation could not be completed.',
): ManagementApiError {
  const safe = redactManagementError(error, message);
  const requestId = error instanceof ManagementApiError ? error.requestId : null;
  return new ManagementApiError({
    ...safe,
    message,
    requestId:
      requestId !== null && /^[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}$/u.test(requestId)
        ? requestId
        : null,
  });
}
