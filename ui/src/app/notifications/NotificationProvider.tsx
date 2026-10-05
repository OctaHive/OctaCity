import { CircleAlert, CircleCheck, X } from 'lucide-react';
import { createContext, use, useCallback, useMemo, useRef, useState, type ReactNode } from 'react';

import { usePresentation } from '../presentation/PresentationProvider';
import styles from './NotificationProvider.module.css';

export const MAX_SESSION_ACTIONS = 20;

export type CommandOutcomeKind = 'success' | 'rejected' | 'conflict' | 'failure';
type CommandFeedbackKind = CommandOutcomeKind | 'indeterminate';

export const COMMAND_FEEDBACK_MESSAGE_KEYS = {
  conflict: 'notification.actionConflict',
  failure: 'notification.actionFailure',
  indeterminate: 'notification.actionIndeterminate',
  rejected: 'notification.actionRejected',
  success: 'notification.actionSuccess',
} as const;

export interface SessionActionOutcome {
  readonly id: number;
  readonly kind: CommandOutcomeKind;
  readonly occurredAtUnixMs: number;
  readonly title: string;
}

interface CommandFeedback extends Omit<SessionActionOutcome, 'kind'> {
  readonly kind: CommandFeedbackKind;
}

interface RecordedActionOutcome extends SessionActionOutcome {
  readonly intent: symbol;
}

interface NotificationContextValue {
  readonly actionHistory: readonly SessionActionOutcome[];
  reportCommandIndeterminate(title: string): void;
  reportCommandOutcome(intent: symbol, title: string, kind: CommandOutcomeKind): void;
}

const noOperation = () => undefined;
const NotificationContext = createContext<NotificationContextValue>({
  actionHistory: [],
  reportCommandIndeterminate: noOperation,
  reportCommandOutcome: noOperation,
});

/** Owns bounded current-tab command outcomes and the console's immediate accessible feedback. */
export function NotificationProvider({ children }: { children: ReactNode }) {
  const { t } = usePresentation();
  const nextIdRef = useRef(1);
  const [history, setHistory] = useState<readonly RecordedActionOutcome[]>([]);
  const [feedback, setFeedback] = useState<CommandFeedback | null>(null);
  const reportCommandIndeterminate = useCallback((title: string) => {
    setFeedback({
      id: nextIdRef.current++,
      kind: 'indeterminate',
      occurredAtUnixMs: Date.now(),
      title,
    });
  }, []);
  const reportCommandOutcome = useCallback(
    (intent: symbol, title: string, kind: CommandOutcomeKind) => {
      const outcome: RecordedActionOutcome = {
        id: nextIdRef.current++,
        intent,
        kind,
        occurredAtUnixMs: Date.now(),
        title,
      };
      setHistory((current) =>
        [outcome, ...current.filter((item) => item.intent !== intent)].slice(
          0,
          MAX_SESSION_ACTIONS,
        ),
      );
      setFeedback(outcome);
    },
    [],
  );
  const value = useMemo<NotificationContextValue>(
    () => ({ actionHistory: history, reportCommandIndeterminate, reportCommandOutcome }),
    [history, reportCommandIndeterminate, reportCommandOutcome],
  );

  return (
    <NotificationContext value={value}>
      {children}
      {feedback === null ? null : (
        <aside
          aria-atomic="true"
          aria-live="polite"
          className={`${styles.feedback} ${styles[`feedback_${feedback.kind}`] ?? ''}`}
          role="status"
        >
          {feedback.kind === 'success' ? (
            <CircleCheck aria-hidden="true" size={20} />
          ) : (
            <CircleAlert aria-hidden="true" size={20} />
          )}
          <span>
            <strong>{feedback.title}</strong>
            <small>{t(COMMAND_FEEDBACK_MESSAGE_KEYS[feedback.kind])}</small>
          </span>
          <button
            aria-label={t('notification.dismissFeedback')}
            onClick={() => setFeedback(null)}
            type="button"
          >
            <X aria-hidden="true" size={17} />
          </button>
        </aside>
      )}
    </NotificationContext>
  );
}

export function useNotifications(): NotificationContextValue {
  return use(NotificationContext);
}
