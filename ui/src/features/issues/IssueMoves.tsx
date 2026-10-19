import { queryKeys } from '../../app/query';
import { useRef, useState } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { ISSUE_CONSTRAINTS as bounds } from '../../../.generated/api/constraints';
import type { components } from '../../../.generated/api/schema';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { ConfirmedCommand } from '../../shared/ConfirmedCommand';
import { requireVersionedMutationHeaders } from '../../shared/confirmedIntent';
import type { WorkflowResource } from '../../api/issueWorkflows';
import type { Issue, IssuesApi } from './api';
import styles from './Issues.module.css';
export function IssueMoves({
  api,
  issue,
  choices,
  workflow,
  ready,
  refresh,
}: {
  api: IssuesApi;
  issue: Issue;
  choices: components['schemas']['IssueTransitionChoices'];
  workflow: WorkflowResource | undefined;
  ready: boolean;
  refresh: () => Promise<unknown>;
}) {
  const { t } = usePresentation();
  const queryClient = useQueryClient();
  const headingRef = useRef<HTMLHeadingElement>(null);
  const [error, setError] = useState<string | null>(null);
  const [reason, setReason] = useState('');
  const [review, setReview] = useState<{
    request: components['schemas']['TransitionIssueRequest'];
    version: number;
    summary: string;
    focus: HTMLElement;
  } | null>(null);
  const status = (id: string) =>
    workflow?.definition.statuses.find((s) => s.id === id)?.name ?? t('issues.unknownStatus');
  return (
    <section className={styles.panel} aria-label={t('issues.moves')}>
      <h2 ref={headingRef} tabIndex={-1}>
        {t('issues.moves')}
      </h2>
      <p>{t('issues.statusHelp')}</p>
      {choices.items.length === 0 ? (
        <p>{t('issues.noMoves')}</p>
      ) : (
        <>
          <label>
            {t('issues.reason')}
            <input value={reason} onChange={(e) => setReason(e.target.value)} />
          </label>
          <ul className={styles.list}>
            {choices.items.map((item) => (
              <li key={item.transition.id}>
                <button
                  disabled={!ready || !item.available}
                  onClick={(event) => {
                    if (!ready || !item.available) return;
                    if (
                      new TextEncoder().encode(reason.trim()).length > bounds.reasonMaximumBytes
                    ) {
                      setError(
                        t('issues.byteLimit', {
                          field: t('issues.reason'),
                          maximum: bounds.reasonMaximumBytes,
                        }),
                      );
                      return;
                    }
                    setError(null);
                    setReview({
                      request: {
                        workflow_id: issue.workflow_id,
                        workflow_version: issue.workflow_version,
                        expected_status_id: issue.status_id,
                        transition_id: item.transition.id,
                        reason: reason.trim() || null,
                      },
                      version: issue.version,
                      summary: `${status(item.transition.from)} → ${status(item.transition.to)}`,
                      focus: event.currentTarget,
                    });
                  }}
                >
                  {item.transition.name}
                </button>
                {item.reason ? <p>{t(`issues.block.${item.reason}`)}</p> : null}
              </li>
            ))}
          </ul>
        </>
      )}
      {error ? <p role="alert">{error}</p> : null}
      {review ? (
        <ConfirmedCommand
          title={t('issues.moves')}
          confirmLabel={t('issues.move')}
          consequence={`${review.summary} · ${t('issues.revision', { version: review.version })}. ${t('issues.statusHelp')}`}
          request={review.request}
          version={review.version}
          returnFocus={
            ready &&
            choices.items.some(
              (item) => item.transition.id === review.request.transition_id && item.available,
            )
              ? review.focus
              : (headingRef.current ?? review.focus)
          }
          queryClient={queryClient}
          invalidations={[{ queryKey: queryKeys.issues(issue.project_id) }]}
          execute={(attempt) =>
            api.move(
              issue.project_id,
              issue.id,
              structuredClone(attempt.request),
              requireVersionedMutationHeaders(attempt.headers),
            )
          }
          onRefresh={refresh}
          onClose={() => setReview(null)}
          renderSuccess={(result) => (
            <p>{t('issues.moved', { version: result.resource.version })}</p>
          )}
        />
      ) : null}
    </section>
  );
}
