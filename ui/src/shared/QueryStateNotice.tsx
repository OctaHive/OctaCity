import { AlertTriangle } from 'lucide-react';

import { ManagementApiError } from '../api/client';
import styles from './QueryStateNotice.module.css';

interface NoticeProps {
  className?: string | undefined;
  onRetry: () => unknown;
}

/** Presents a failed authoritative read with safe request correlation and retry. */
export function QueryFailureNotice({
  className,
  error,
  onRetry,
  title,
}: NoticeProps & { error: Error | null; title: string }) {
  const requestIdentity = error instanceof ManagementApiError ? error.requestId : null;
  return (
    <div className={noticeClassName(styles.failure, className)} role="alert">
      <AlertTriangle aria-hidden="true" size={18} />
      <div className={styles.content}>
        <strong>{title}</strong>
        {requestIdentity === null ? null : (
          <span className={styles.requestIdentity}> Request ID: {requestIdentity}</span>
        )}
      </div>
      <RetryButton onRetry={onRetry} />
    </div>
  );
}

/** Keeps prior query data visible while clearly identifying a failed refresh. */
export function StaleQueryNotice({
  className,
  message,
  onRetry,
}: NoticeProps & { message: string }) {
  return (
    <div className={noticeClassName(styles.stale, className)} role="status">
      <AlertTriangle aria-hidden="true" size={16} />
      <span className={styles.content}>{message}</span>
      <RetryButton onRetry={onRetry} />
    </div>
  );
}

function RetryButton({ onRetry }: { onRetry: () => unknown }) {
  return (
    <button className={styles.retry} onClick={() => void onRetry()} type="button">
      Retry
    </button>
  );
}

function noticeClassName(kind: string | undefined, className: string | undefined): string {
  return [styles.notice, kind, className]
    .filter((value): value is string => value !== undefined)
    .join(' ');
}
