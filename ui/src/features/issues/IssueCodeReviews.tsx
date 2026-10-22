import { IssueCodeReviewRefresh } from './IssueCodeReviewRefresh';
import { IssueCodeReviewHistory } from './IssueCodeReviewHistory';
import { ReviewObservation } from './IssueCodeReviewObservation';
import { useEffect, useRef, useState } from 'react';
import { IssueCodeReviewAttachment } from './IssueCodeReviewAttachment';
import { useQuery } from '@tanstack/react-query';
import { Link, useSearchParams } from 'react-router-dom';
import { queryKeys } from '../../app/query';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { QueryContent, QueryEmptyNotice } from '../../shared/QueryStateNotice';
import type { Issue } from './api';
import type { IssueCodeReview, IssueCodeReviewsApi, IssueReviewConnections } from './codeReviewApi';
import styles from './Issues.module.css';

/** Summary and Results use the same bounded retained page; navigation never refreshes the provider. */
export function IssueCodeReviews({
  issue,
  api,
  compact = false,
  connections,
  ready = false,
  onReload,
}: {
  issue: Issue;
  api: IssueCodeReviewsApi;
  compact?: boolean;
  connections?: IssueReviewConnections;
  ready?: boolean;
  onReload?: () => Promise<void>;
}) {
  const { t } = usePresentation();
  const [attaching, setAttaching] = useState(false);
  const [params, setParams] = useSearchParams();
  const after = compact ? null : params.get('review_after');
  const query = useQuery({
    queryKey: queryKeys.issueCodeReviews(issue.project_id, issue.id, after),
    gcTime: 0,
    queryFn: ({ signal }) => api.list(issue.project_id, issue.id, after, signal),
  });
  const title = t(compact ? 'issueReviews.summary' : 'issueReviews.title');
  function location(id: string) {
    const next = new URLSearchParams(params);
    next.set('section', 'results');
    next.set('review', id);
    next.delete('review_after');
    return { search: next.toString() };
  }
  function changePage(cursor: string | null) {
    const next = new URLSearchParams(params);
    if (cursor) next.set('review_after', cursor);
    else next.delete('review_after');
    next.delete('review');
    setParams(next);
  }
  return (
    <section className={styles.panel} aria-label={title}>
      <h2>{title}</h2>
      {!compact && connections && onReload ? (
        <>
          <button type="button" disabled={!ready} onClick={() => setAttaching(!attaching)}>
            {t(attaching ? 'command.abandon' : 'issueReviews.attach')}
          </button>
          {attaching ? (
            <IssueCodeReviewAttachment
              issue={issue}
              api={api}
              connections={connections}
              ready={ready}
              onReload={onReload}
            />
          ) : null}
        </>
      ) : null}
      <QueryContent query={query} label={title}>
        {(data) => (
          <>
            {data.items.length ? (
              <ul className={styles.list}>
                {data.items.map((review) => (
                  <li key={review.association.id}>
                    {compact ? (
                      <Link to={location(review.association.id)}>{reviewLabel(review)}</Link>
                    ) : (
                      <ReviewCard review={review} issue={issue} api={api} />
                    )}
                  </li>
                ))}
              </ul>
            ) : (
              <QueryEmptyNotice>{t('issueReviews.empty')}</QueryEmptyNotice>
            )}
            {!compact && data.next_cursor ? (
              <button
                disabled={query.isFetching || !!query.error}
                onClick={() => changePage(data.next_cursor)}
              >
                {t('issues.nextPage')}
              </button>
            ) : null}
          </>
        )}
      </QueryContent>
      {!compact && after ? (
        <button onClick={() => changePage(null)}>{t('issues.firstPage')}</button>
      ) : null}
    </section>
  );
}
function reviewLabel(review: IssueCodeReview) {
  return [
    review.observation?.number ?? review.association.selection.item.item,
    review.observation?.title,
  ]
    .filter(Boolean)
    .join(' · ');
}
function ReviewCard({
  review,
  issue,
  api,
}: {
  review: IssueCodeReview;
  issue: Issue;
  api: IssueCodeReviewsApi;
}) {
  const { t } = usePresentation();
  const [history, setHistory] = useState(false);
  const binding = review.association.binding;
  const [params] = useSearchParams();
  const selected = params.get('review') === review.association.id;
  const ref = useRef<HTMLElement>(null);
  useEffect(() => {
    if (selected) ref.current?.focus();
  }, [selected]);
  return (
    <article id={`review-${review.association.id}`} ref={ref} tabIndex={-1}>
      <h3>{reviewLabel(review)}</h3>
      <p>
        {review.association.selection.source.installation.identity} ·{' '}
        {review.association.selection.item.resource}
      </p>
      {review.availability === 'blocked' ? <p role="status">{t('issueReviews.blocked')}</p> : null}
      {binding &&
      review.observation?.head &&
      binding.candidate.candidate_revision !== review.observation.head ? (
        <p role="status">
          {t('issueReviews.historicalHead', { head: binding.candidate.candidate_revision })}
        </p>
      ) : null}
      <ReviewObservation observation={review.observation} />
      <IssueCodeReviewRefresh issue={issue} review={review} api={api} />
      <details>
        <summary>{t('issueReviews.provenance')}</summary>
        <p>
          {t(
            review.association.provenance.kind === 'delivery'
              ? 'issueReviews.delivery'
              : 'issueReviews.explicit',
          )}
        </p>
        {binding ? (
          <p>
            {t('issueReviews.boundExecution', {
              request: binding.association.request_id ?? binding.association.run_id,
              head: binding.candidate.candidate_revision,
            })}
          </p>
        ) : (
          <p>{t('issueReviews.organizational')}</p>
        )}
      </details>
      <button type="button" aria-expanded={history} onClick={() => setHistory(!history)}>
        {t('issueReviews.history')}
      </button>
      {history ? <IssueCodeReviewHistory issue={issue} review={review} api={api} /> : null}
    </article>
  );
}
