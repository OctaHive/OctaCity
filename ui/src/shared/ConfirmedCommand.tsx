import { useCallback, useEffect, useRef, useState, type RefObject } from 'react';
import type { QueryClient } from '@tanstack/react-query';
import { Link } from 'react-router-dom';

import { ManagementApiError } from '../api/client';
import { auditRequestPath } from '../app/routes';
import {
  createConfirmedMutationIntent,
  type ConfirmedMutationAttempt,
  type QueryInvalidation,
} from './confirmedIntent';
import { useOverlayFocus } from './useOverlayFocus';
import styles from './ConfirmedCommand.module.css';

interface ConfirmedCommandProps<Request, Result> {
  confirmLabel: string;
  consequence: string;
  execute: (attempt: ConfirmedMutationAttempt<Request>) => Promise<Result>;
  fallbackFocusRef?: RefObject<HTMLElement | null>;
  invalidations: readonly QueryInvalidation[];
  onClose: () => void;
  onRefresh?: () => unknown | Promise<unknown>;
  queryClient: Pick<QueryClient, 'invalidateQueries'>;
  renderSuccess: (result: Result) => React.ReactNode;
  request: Request;
  returnFocus: HTMLElement;
  title: string;
  version?: number;
}

type CommandPhase<Result> =
  | { kind: 'ready' | 'pending'; result?: never }
  | { kind: 'error'; result?: never }
  | { kind: 'succeeded'; result: Result };

/** Runs one explicitly confirmed command and retains its identity only for safe transport replay. */
export function ConfirmedCommand<Request, Result>({
  confirmLabel,
  consequence,
  execute,
  fallbackFocusRef,
  invalidations,
  onClose,
  onRefresh,
  queryClient,
  renderSuccess,
  request,
  returnFocus,
  title,
  version,
}: ConfirmedCommandProps<Request, Result>) {
  const dialogRef = useRef<HTMLElement>(null);
  const primaryButtonRef = useRef<HTMLButtonElement>(null);
  const terminalButtonRef = useRef<HTMLButtonElement>(null);
  const intentRef = useRef<ReturnType<
    typeof createConfirmedMutationIntent<Request, Result>
  > | null>(null);
  const [error, setError] = useState<unknown>(null);
  const [phase, setPhase] = useState<CommandPhase<Result>>({ kind: 'ready' });
  const phaseRef = useRef(phase.kind);
  phaseRef.current = phase.kind;
  const dismiss = useCallback(() => {
    if (phaseRef.current === 'pending') return;
    intentRef.current?.abandon();
    onClose();
  }, [onClose]);
  const restoreFocus = useCallback(() => {
    const target = returnFocus.isConnected ? returnFocus : fallbackFocusRef?.current;
    target?.focus();
  }, [fallbackFocusRef, returnFocus]);
  useOverlayFocus({
    containerRef: dialogRef,
    initialFocusRef: primaryButtonRef,
    onDismiss: dismiss,
    priority: true,
    restoreFocus,
  });

  const submit = async () => {
    const intent =
      intentRef.current ??
      createConfirmedMutationIntent({
        execute,
        invalidate: invalidations,
        queryClient,
        request,
        ...(version === undefined ? {} : { version }),
      });
    intentRef.current = intent;
    setError(null);
    setPhase({ kind: 'pending' });
    try {
      setPhase({ kind: 'succeeded', result: await intent.submit() });
    } catch (caught) {
      setError(caught);
      setPhase({ kind: 'error' });
    }
  };
  const retryable = intentRef.current?.state === 'retryable';
  const staleVersion = error instanceof ManagementApiError && error.code === 'precondition_failed';

  useEffect(() => {
    if (phase.kind === 'succeeded' || (phase.kind === 'error' && !retryable)) {
      terminalButtonRef.current?.focus();
    }
  }, [phase.kind, retryable]);

  const refresh = async () => {
    try {
      await onRefresh?.();
    } finally {
      dismiss();
    }
  };

  return (
    <div className={styles.layer}>
      <button
        aria-label="Close confirmation"
        className={styles.scrim}
        onClick={dismiss}
        tabIndex={-1}
        type="button"
      />
      <section
        aria-labelledby="confirmed-command-title"
        aria-modal="true"
        className={styles.dialog}
        ref={dialogRef}
        role="dialog"
      >
        <h2 id="confirmed-command-title">{title}</h2>
        <p>{consequence}</p>
        {phase.kind === 'succeeded' ? (
          <div aria-live="polite" className={styles.success} role="status">
            {renderSuccess(phase.result)}
          </div>
        ) : null}
        {phase.kind === 'error' ? <CommandFailure error={error} retryable={retryable} /> : null}
        <div className={styles.actions}>
          {phase.kind === 'error' && staleVersion && onRefresh !== undefined ? (
            <button onClick={() => void refresh()} ref={terminalButtonRef} type="button">
              Refresh current data
            </button>
          ) : null}
          {phase.kind === 'succeeded' || (phase.kind === 'error' && !retryable) ? null : (
            <button
              className={styles.primary}
              disabled={phase.kind === 'pending'}
              onClick={() => void submit()}
              ref={primaryButtonRef}
              type="button"
            >
              {phase.kind === 'pending'
                ? 'Submitting…'
                : retryable
                  ? 'Retry same command'
                  : confirmLabel}
            </button>
          )}
          <button
            disabled={phase.kind === 'pending'}
            onClick={dismiss}
            ref={
              phase.kind === 'succeeded' || (phase.kind === 'error' && !staleVersion)
                ? terminalButtonRef
                : undefined
            }
            type="button"
          >
            {phase.kind === 'succeeded' ? 'Close' : 'Abandon'}
          </button>
        </div>
      </section>
    </div>
  );
}

function CommandFailure({ error, retryable }: { error: unknown; retryable: boolean }) {
  if (!(error instanceof ManagementApiError)) {
    return <p className={styles.error}>The command failed safely. No automatic retry was made.</p>;
  }
  return (
    <div className={styles.error} role="alert">
      <strong>{error.message}</strong>
      <span>Error code: {error.code}</span>
      {error.requestId === null ? null : (
        <>
          <span>Request ID: {error.requestId}</span>
          <Link to={auditRequestPath(error.requestId)}>View audit evidence</Link>
        </>
      )}
      <span>
        {retryable
          ? 'The same command can be retried safely.'
          : 'Review the request before trying again.'}
      </span>
    </div>
  );
}
