import { useQuery } from '@tanstack/react-query';
import { useSearchParams } from 'react-router-dom';
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
import type { IssueResultPanelsApi } from './resultPanelsApi';
import { IssueResultPage } from './IssueResultPage';
import { IssueCheckState } from './IssueCheckState';
import styles from './Issues.module.css';
/** External observations remain independently pageable even without a Factory execution. */
export function IssueExternalChecks({
  api,
  reviews,
  issue,
}: {
  api: IssueResultPanelsApi;
  reviews: IssueCodeReviewsApi;
  issue: Issue;
}) {
  const { t } = usePresentation();
  const [params, setParams] = useSearchParams();
  const after = params.get('ci_review_after');
  const query = useQuery({
    queryKey: queryKeys.issueCodeReviews(issue.project_id, issue.id, after),
    gcTime: 0,
    queryFn: ({ signal }) => reviews.list(issue.project_id, issue.id, after, signal),
  });
  return (
    <section aria-label={t('issueResults.externalCI')}>
      <h3>{t('issueResults.externalCI')}</h3>
      {query.isPending ? (
        <QueryLoadingNotice label={t('issueResults.externalCI')} />
      ) : !query.data ? (
        <QueryFailureNotice
          title={t('issueResults.externalCI')}
          error={query.error}
          onRetry={query.refetch}
        />
      ) : (
        <>
          <QueryBackgroundNotice
            label={t('issueResults.externalCI')}
            error={query.error}
            fetching={query.isFetching}
            onRetry={query.refetch}
          />
          {!query.data.items.length ? (
            <QueryEmptyNotice>{t('issueReviews.empty')}</QueryEmptyNotice>
          ) : null}
          {query.data.items.map((review) => (
            <ExternalReviewChecks
              key={review.association.id}
              api={api}
              issue={issue}
              review={review}
            />
          ))}
          {query.data.next_cursor ? (
            <button
              disabled={query.isFetching || !!query.error}
              onClick={() => {
                const next = new URLSearchParams(params);
                next.set('ci_review_after', query.data!.next_cursor!);
                next.delete('ci_review');
                next.delete('ci_after');
                setParams(next);
              }}
            >
              {t('issues.nextPage')}
            </button>
          ) : null}
        </>
      )}
      {after ? (
        <button
          onClick={() => {
            const next = new URLSearchParams(params);
            next.delete('ci_review_after');
            next.delete('ci_review');
            next.delete('ci_after');
            setParams(next);
          }}
        >
          {t('issues.firstPage')}
        </button>
      ) : null}
    </section>
  );
}
function ExternalReviewChecks({
  api,
  issue,
  review,
}: {
  api: IssueResultPanelsApi;
  issue: Issue;
  review: IssueCodeReview;
}) {
  const { t, locale } = usePresentation();
  const [params, setParams] = useSearchParams();
  const after = params.get('ci_review') === review.association.id ? params.get('ci_after') : null;
  const query = useQuery({
    queryKey: queryKeys.issueExternalChecks(
      issue.project_id,
      issue.id,
      review.association.id,
      after,
    ),
    gcTime: 0,
    queryFn: ({ signal }) =>
      api.externalChecks(issue.project_id, issue.id, review.association.id, after, signal),
  });
  const label = review.observation?.number ?? review.association.selection.item.item;
  return (
    <IssueResultPage
      title={`${t('issueResults.externalCI')} · ${label}`}
      query={query}
      after={after}
      onPage={(cursor) => {
        const next = new URLSearchParams(params);
        next.set('ci_review', review.association.id);
        if (cursor) next.set('ci_after', cursor);
        else next.delete('ci_after');
        setParams(next);
      }}
    >
      {(page) => (
        <>
          <p>{t('issueReviews.noAuthority')}</p>
          <p>
            {review.association.selection.source.installation.identity} ·{' '}
            {review.association.selection.item.resource}
          </p>
          {page.availability === 'blocked' ? (
            <p role="status">{t('issueReviews.blocked')}</p>
          ) : null}
          {!page.items.length ? <p>{t('issueResults.emptyChecks')}</p> : null}
          <ol className={styles.list}>
            {page.items.map((observation) => (
              <li key={observation.id}>
                <p>
                  {t('issueReviews.observedHead', {
                    head: observation.head ?? t('issues.notReported'),
                  })}
                </p>
                {observation.head &&
                review.observation?.head &&
                observation.head !== review.observation.head ? (
                  <p>{t('issueReviews.historicalObservation')}</p>
                ) : null}
                <p>{new Date(observation.observed_at).toLocaleString(locale)}</p>
                <p>{t('issueReviews.retained')}</p>
                {observation.checks ? (
                  <>
                    {observation.checks.has_more ? <p>{t('issueResults.moreExternal')}</p> : null}
                    <ul>
                      {observation.checks.items.map((check) => (
                        <li key={check.id}>
                          <h4>{check.name}</h4>
                          <IssueCheckState state={check.state} required={check.required} />
                          <p>{check.producer ?? t('issues.notReported')}</p>
                        </li>
                      ))}
                    </ul>
                  </>
                ) : (
                  <p>{t('issues.notReported')}</p>
                )}
              </li>
            ))}
          </ol>
        </>
      )}
    </IssueResultPage>
  );
}
