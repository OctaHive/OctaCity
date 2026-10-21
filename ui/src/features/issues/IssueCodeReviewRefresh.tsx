import { useRef, useState } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { queryKeys } from '../../app/query';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { ConfirmedCommand } from '../../shared/ConfirmedCommand';
import type { Issue } from './api';
import type { IssueCodeReview, IssueCodeReviewsApi } from './codeReviewApi';

/** Provider observation is a separate confirmed read command, never an effect of viewing the card. */
export function IssueCodeReviewRefresh({
  issue,
  review,
  api,
}: {
  issue: Issue;
  review: IssueCodeReview;
  api: IssueCodeReviewsApi;
}) {
  const { t } = usePresentation();
  const client = useQueryClient();
  const triggerRef = useRef<HTMLButtonElement>(null);
  const [active, setActive] = useState(false);
  const [failed, setFailed] = useState(false);
  return (
    <>
      {failed ? <p role="status">{t('issueReviews.refreshFailed')}</p> : null}
      <button
        type="button"
        ref={triggerRef}
        disabled={review.availability === 'blocked'}
        onClick={() => setActive(true)}
      >
        {t('issueReviews.refresh')}
      </button>
      {active ? (
        <ConfirmedCommand
          title={t('issueReviews.refresh')}
          confirmLabel={t('issueReviews.refresh')}
          consequence={t('issueReviews.refreshConsequence')}
          request={{ review: review.association.id }}
          ready={review.availability === 'available'}
          execute={async ({ request, headers }) => {
            try {
              const outcome = await api.refresh(
                issue.project_id,
                issue.id,
                request.review,
                headers,
              );
              setFailed(false);
              return outcome;
            } catch (error) {
              setFailed(true);
              throw error;
            }
          }}
          queryClient={client}
          invalidations={[{ queryKey: queryKeys.issueCodeReviewsRoot(issue.project_id, issue.id) }]}
          onClose={() => setActive(false)}
          returnFocus={triggerRef.current!}
          renderSuccess={() => <p>{t('issueReviews.refreshed')}</p>}
        />
      ) : null}
    </>
  );
}
