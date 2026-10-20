import { useEffect, useRef, useState } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { ISSUE_CONSTRAINTS as bounds } from '../../../.generated/api/constraints';
import { queryKeys } from '../../app/query';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { ConfirmedCommand } from '../../shared/ConfirmedCommand';
import type { IssueActivityApi, IssueComment, IssueCommentRequest } from './activityApi';
import type { Issue } from './api';
import styles from './Issues.module.css';
/** Uses the shared frozen command lifecycle; body, parent and key live only in this form. */
export function IssueCommentForm({
  api,
  issue,
  parent,
  onParent,
  onPosted,
  ready,
}: {
  api: IssueActivityApi;
  issue: Issue;
  parent: IssueComment | null;
  onParent: (parent: IssueComment | null) => void;
  onPosted: () => void;
  ready: boolean;
}) {
  const { t } = usePresentation();
  const client = useQueryClient();
  const [body, setBody] = useState('');
  const [review, setReview] = useState<{
    request: IssueCommentRequest;
    trigger: HTMLElement;
  } | null>(null);
  const submittedRef = useRef(false);
  const inputRef = useRef<HTMLTextAreaElement>(null);
  useEffect(() => {
    if (parent) inputRef.current?.focus();
  }, [parent]);
  const bytes = new TextEncoder().encode(body).length;
  return (
    <>
      <form
        className={styles.panel}
        aria-label={t('issues.operatorDiscussion')}
        onSubmit={(event) => {
          event.preventDefault();
          if (!ready || !body.trim() || bytes > bounds.commentMaximumBytes) return;
          submittedRef.current = false;
          setReview({
            request: { body, parent_id: parent?.id ?? null },
            trigger: event.currentTarget.querySelector('button[type="submit"]')!,
          });
        }}
      >
        <h3>{t('issues.operatorDiscussion')}</h3>
        <fieldset disabled={review !== null}>
          {parent ? (
            <p>
              {t('issues.replying', { sequence: parent.sequence })}{' '}
              <button type="button" onClick={() => onParent(null)}>
                {t('issues.cancelReply')}
              </button>
            </p>
          ) : null}
          <label>
            {t('issues.comment')}
            <textarea
              ref={inputRef}
              value={body}
              maxLength={bounds.commentMaximumBytes}
              onChange={(event) => setBody(event.target.value)}
            />
          </label>
          <p>{t('issues.commentBytes', { used: bytes, maximum: bounds.commentMaximumBytes })}</p>
          <button
            type="submit"
            disabled={!ready || !body.trim() || bytes > bounds.commentMaximumBytes}
          >
            {t('issues.reviewComment')}
          </button>
        </fieldset>
      </form>
      {review ? (
        <ConfirmedCommand
          title={t('issues.appendComment')}
          confirmLabel={t('issues.appendComment')}
          consequence={review.request.body}
          request={review.request}
          returnFocus={review.trigger}
          fallbackFocusRef={inputRef}
          queryClient={client}
          ready={ready}
          invalidations={[{ queryKey: queryKeys.issueTimelineRoot(issue.project_id, issue.id) }]}
          execute={async (attempt) => {
            const outcome = await api.append(
              issue.project_id,
              issue.id,
              attempt.request,
              attempt.headers,
            );
            submittedRef.current = true;
            onPosted();
            return outcome;
          }}
          onClose={() => {
            setReview(null);
            if (submittedRef.current) {
              setBody('');
              onParent(null);
            }
          }}
          renderSuccess={() => <p>{t('issues.commentRecorded')}</p>}
        />
      ) : null}
    </>
  );
}
