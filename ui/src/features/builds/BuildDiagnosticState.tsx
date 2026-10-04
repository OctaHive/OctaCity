import { AlertTriangle } from 'lucide-react';

import { ManagementApiError } from '../../api/client';
import styles from './Builds.module.css';

export function DiagnosticLoading({ label }: { label: string }) {
  return (
    <div aria-live="polite" className={styles.statePanel} role="status">
      <span aria-hidden="true" className={styles.loadingMark} />
      Loading {label}…
    </div>
  );
}

export function DiagnosticFailure({
  error,
  label,
  onRetry,
}: {
  error: Error | null;
  label: string;
  onRetry: () => unknown;
}) {
  const managementError = error instanceof ManagementApiError ? error : null;
  return (
    <div className={styles.failurePanel} role="alert">
      <AlertTriangle aria-hidden="true" size={20} />
      <div>
        <strong>{label} could not be loaded.</strong>
        <p>
          {managementError === null ? 'Try the request again.' : managementError.message}
          {managementError?.requestId === null || managementError?.requestId === undefined
            ? null
            : ` Request ID: ${managementError.requestId}`}
        </p>
        <button className={styles.textButton} onClick={() => void onRetry()} type="button">
          Retry
        </button>
      </div>
    </div>
  );
}

export function DiagnosticStale({ label, onRetry }: { label: string; onRetry: () => unknown }) {
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
