import { AlertTriangle } from 'lucide-react';

import { ManagementApiError } from '../../api/client';
import styles from './Projects.module.css';

interface RetryStateProps {
  label: string;
  onRetry: () => unknown;
}

export function ProjectDataFailure({
  compact = false,
  error,
  label,
  onRetry,
}: RetryStateProps & { compact?: boolean; error: Error | null }) {
  const requestId = error instanceof ManagementApiError ? error.requestId : null;
  return (
    <div className={compact ? styles.compactFailure : styles.failurePanel} role="alert">
      <AlertTriangle aria-hidden="true" size={compact ? 16 : 20} />
      <div>
        <strong>{label} could not be loaded.</strong>
        <p>
          Try the request again.
          {requestId === null ? null : ` Request ID: ${requestId}`}
        </p>
        <button className={styles.textButton} onClick={() => void onRetry()} type="button">
          Retry
        </button>
      </div>
    </div>
  );
}

export function ProjectDataStale({ label, onRetry }: RetryStateProps) {
  return (
    <div className={styles.staleNotice}>
      <AlertTriangle aria-hidden="true" size={16} />
      <span role="status">Refresh failed. Showing the last loaded {label} data.</span>
      <button className={styles.textButton} onClick={() => void onRetry()} type="button">
        Retry
      </button>
    </div>
  );
}
