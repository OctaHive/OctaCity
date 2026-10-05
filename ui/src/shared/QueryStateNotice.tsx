import { AlertTriangle, Inbox, LoaderCircle, RefreshCw } from 'lucide-react';
import { useEffect, useState, type ReactNode } from 'react';

import { ManagementApiError } from '../api/client';
import { usePresentation } from '../app/presentation/PresentationProvider';
import styles from './QueryStateNotice.module.css';

interface NoticeProps {
  className?: string | undefined;
  onRetry: () => unknown;
}

export const QUERY_FAILURE_KINDS = [
  'rate-limited',
  'forbidden',
  'not-found',
  'conflict',
  'unavailable',
  'internal-error',
] as const;

export type QueryFailureKind = (typeof QUERY_FAILURE_KINDS)[number];

/** Maps safe management failures onto the finite operator-facing state vocabulary. */
export function queryFailureKind(error: Error | null): QueryFailureKind {
  if (!(error instanceof ManagementApiError)) return 'internal-error';
  switch (error.code) {
    case 'rate_limited':
      return 'rate-limited';
    case 'forbidden':
      return 'forbidden';
    case 'not_found':
      return 'not-found';
    case 'conflict':
    case 'idempotency_conflict':
    case 'precondition_failed':
    case 'precondition_required':
      return 'conflict';
    case 'capability_unavailable':
    case 'transport_failure':
    case 'unavailable':
      return 'unavailable';
    default:
      return 'internal-error';
  }
}

/** Presents initial loading without implying that an empty result was returned. */
export function QueryLoadingNotice({
  className,
  label,
}: {
  className?: string | undefined;
  label: string;
}) {
  const { t } = usePresentation();
  return (
    <div className={noticeClassName(styles.loading, className)} role="status">
      <LoaderCircle aria-hidden="true" size={18} />
      <span className={styles.content}>{t('query.loading', { label })}</span>
    </div>
  );
}

/** Presents an authoritative empty result separately from transport failure. */
export function QueryEmptyNotice({
  children,
  className,
}: {
  children: ReactNode;
  className?: string | undefined;
}) {
  return (
    <div className={noticeClassName(styles.empty, className)} role="status">
      <Inbox aria-hidden="true" size={18} />
      <span className={styles.content}>{children}</span>
    </div>
  );
}

/** Announces an incremental refresh while previously loaded data remains visible. */
export function QueryRefreshingNotice({
  className,
  label,
}: {
  className?: string | undefined;
  label: string;
}) {
  const { t } = usePresentation();
  return (
    <div className={noticeClassName(styles.refreshing, className)} role="status">
      <RefreshCw aria-hidden="true" size={16} />
      <span className={styles.content}>{t('query.refreshing', { label })}</span>
    </div>
  );
}

/** Selects the one background state that may accompany already rendered data. */
export function QueryBackgroundNotice({
  className,
  error,
  fetching,
  label,
  onRetry,
}: NoticeProps & { error: Error | null; fetching: boolean; label: string }) {
  const { t } = usePresentation();
  if (error !== null) {
    return (
      <StaleQueryNotice
        className={className}
        error={error}
        message={t('query.stale', { label })}
        onRetry={onRetry}
      />
    );
  }
  return fetching ? <QueryRefreshingNotice className={className} label={label} /> : null;
}

/** Presents a failed authoritative read with safe request correlation and retry. */
export function QueryFailureNotice({
  className,
  error,
  onRetry,
  title,
}: NoticeProps & { error: Error | null; title: string }) {
  const kind = queryFailureKind(error);
  return (
    <div className={noticeClassName(styles.failure, className)} data-state={kind} role="alert">
      <AlertTriangle aria-hidden="true" size={18} />
      <div className={styles.content}>
        <strong>{title}</strong>
        <FailureDetails error={error} />
      </div>
      <RetryButton error={error} onRetry={onRetry} />
    </div>
  );
}

/** Keeps prior query data visible while clearly identifying a failed refresh. */
export function StaleQueryNotice({
  className,
  error,
  message,
  onRetry,
}: NoticeProps & { error: Error | null; message: string }) {
  const kind = queryFailureKind(error);
  return (
    <div className={noticeClassName(styles.stale, className)} data-state={kind}>
      <AlertTriangle aria-hidden="true" size={16} />
      <span className={styles.content}>
        <span role="status">{message}</span>
        <FailureDetails error={error} />
      </span>
      <RetryButton error={error} onRetry={onRetry} />
    </div>
  );
}

function FailureDetails({ error }: { error: Error | null }) {
  const { t } = usePresentation();
  if (!(error instanceof ManagementApiError)) {
    return <span>{t('query.safeFailure')}</span>;
  }
  return (
    <span className={styles.failureDetails}>
      <span>{error.message}</span>
      <span>{t('query.errorCode', { code: error.code })}</span>
      {error.requestId === null ? null : (
        <span className={styles.requestIdentity}>
          {t('query.requestId', { id: error.requestId })}
        </span>
      )}
      {error.retryAfterMilliseconds === null ? null : (
        <span>
          {t('query.retryDelay', {
            seconds: Math.ceil(error.retryAfterMilliseconds / 1_000),
          })}
        </span>
      )}
    </span>
  );
}

function RetryButton({ error, onRetry }: { error: Error | null; onRetry: () => unknown }) {
  const { t } = usePresentation();
  const retryAfter =
    error instanceof ManagementApiError && error.code === 'rate_limited'
      ? error.retryAfterMilliseconds
      : null;
  const [releasedError, setReleasedError] = useState<Error | null>(null);
  const waiting = retryAfter !== null && retryAfter > 0 && releasedError !== error;
  useEffect(() => {
    if (retryAfter === null || retryAfter <= 0) return;
    const timeout = globalThis.setTimeout(() => setReleasedError(error), retryAfter);
    return () => globalThis.clearTimeout(timeout);
  }, [error, retryAfter]);

  return (
    <button
      className={styles.retry}
      disabled={waiting}
      onClick={() => void onRetry()}
      type="button"
    >
      {waiting && retryAfter !== null
        ? t('query.retryIn', { seconds: Math.ceil(retryAfter / 1_000) })
        : t('query.retry')}
    </button>
  );
}

function noticeClassName(kind: string | undefined, className: string | undefined): string {
  return [styles.notice, kind, className]
    .filter((value): value is string => value !== undefined)
    .join(' ');
}
