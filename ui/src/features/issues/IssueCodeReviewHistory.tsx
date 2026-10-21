import { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { queryKeys } from '../../app/query';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import {
  QueryLoadingNotice,
  QueryFailureNotice,
  QueryBackgroundNotice,
  QueryEmptyNotice,
} from '../../shared/QueryStateNotice';
import type { Issue } from './api';
import type { IssueCodeReview, IssueCodeReviewsApi } from './codeReviewApi';
import { ReviewObservation } from './IssueCodeReviewObservation';
import styles from './Issues.module.css';

/** Expansion reads one authorized original-association page, never a fresh provider observation. */
export function IssueCodeReviewHistory({
  issue,
  review,
  api,
}: {
  issue: Issue;
  review: IssueCodeReview;
  api: IssueCodeReviewsApi;
}) {
  const { t } = usePresentation();
  const [after, setAfter] = useState<string | null>(null);
  const query = useQuery({
    queryKey: queryKeys.issueCodeReviewHistory(
      issue.project_id,
      issue.id,
      review.association.id,
      after,
    ),
    gcTime: 0,
    queryFn: ({ signal }) =>
      api.observations(issue.project_id, issue.id, review.association.id, after, signal),
  });
  return (
    <section aria-label={t('issueReviews.retainedHistory')}>
      <h4>{t('issueReviews.retainedHistory')}</h4>
      {query.isPending ? (
        <QueryLoadingNotice label={t('issueReviews.retainedHistory')} />
      ) : !query.data ? (
        <QueryFailureNotice
          title={t('issueReviews.retainedHistory')}
          error={query.error}
          onRetry={query.refetch}
        />
      ) : (
        <>
          <QueryBackgroundNotice
            label={t('issueReviews.retainedHistory')}
            error={query.error}
            fetching={query.isFetching}
            onRetry={query.refetch}
          />
          {query.data.availability === 'blocked' ? <p>{t('issueReviews.blocked')}</p> : null}
          {query.data.items.length ? (
            <ol className={styles.list}>
              {query.data.items.map((record) => (
                <li key={record.id}>
                  <p>
                    {t('issueReviews.observedHead', {
                      head: record.observation.head ?? t('issues.notReported'),
                    })}
                  </p>
                  {record.observation.head &&
                  review.observation?.head &&
                  record.observation.head !== review.observation.head ? (
                    <p>{t('issueReviews.historicalObservation')}</p>
                  ) : null}
                  <ReviewObservation observation={record.observation} />
                </li>
              ))}
            </ol>
          ) : (
            <QueryEmptyNotice>{t('issueReviews.emptyHistory')}</QueryEmptyNotice>
          )}
          {query.data.next_cursor ? (
            <button
              disabled={query.isFetching || !!query.error}
              onClick={() => setAfter(query.data!.next_cursor)}
            >
              {t('issues.nextPage')}
            </button>
          ) : null}
        </>
      )}
      {after ? <button onClick={() => setAfter(null)}>{t('issues.firstPage')}</button> : null}
    </section>
  );
}
