const READINESS_PATH = '/health/ready';
const READINESS_TIMEOUT_MILLISECONDS = 5_000;

export type ReadinessState = 'ready' | 'unavailable' | 'unreachable';
export type ReadinessProbe = (signal?: AbortSignal) => Promise<ReadinessState>;

interface HealthResponse {
  status: 'ready' | 'unavailable';
}

/** Reads the unauthenticated same-origin readiness probe without browser credentials. */
export async function fetchReadiness(parentSignal?: AbortSignal): Promise<ReadinessState> {
  const request = boundedReadinessRequest(parentSignal);
  try {
    const response = await globalThis.fetch(READINESS_PATH, {
      cache: 'no-store',
      credentials: 'omit',
      headers: { accept: 'application/json' },
      signal: request.signal,
    });
    let body: unknown;
    try {
      body = await response.json();
    } catch {
      return 'unavailable';
    }
    if (!isHealthResponse(body)) {
      return 'unavailable';
    }
    return response.ok && body.status === 'ready' ? 'ready' : 'unavailable';
  } catch {
    return 'unreachable';
  } finally {
    request.dispose();
  }
}

function boundedReadinessRequest(parentSignal?: AbortSignal) {
  const controller = new AbortController();
  const abortFromParent = () => controller.abort(parentSignal?.reason);
  const timeout = globalThis.setTimeout(
    () => controller.abort(new DOMException('Readiness probe timed out', 'TimeoutError')),
    READINESS_TIMEOUT_MILLISECONDS,
  );

  if (parentSignal?.aborted === true) {
    abortFromParent();
  } else {
    parentSignal?.addEventListener('abort', abortFromParent, { once: true });
  }

  return {
    signal: controller.signal,
    dispose() {
      globalThis.clearTimeout(timeout);
      parentSignal?.removeEventListener('abort', abortFromParent);
    },
  };
}

function isHealthResponse(value: unknown): value is HealthResponse {
  return (
    typeof value === 'object' &&
    value !== null &&
    'status' in value &&
    (value.status === 'ready' || value.status === 'unavailable')
  );
}
