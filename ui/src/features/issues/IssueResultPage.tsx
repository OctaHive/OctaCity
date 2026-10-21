import type { ReactNode } from 'react';
import type { UseQueryResult } from '@tanstack/react-query';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import {
  QueryLoadingNotice,
  QueryFailureNotice,
  QueryBackgroundNotice,
} from '../../shared/QueryStateNotice';
import type { components } from '../../../.generated/api/schema';
/** Common bounded-page feedback; coverage never becomes a verification verdict. */
export function IssueResultPage<
  T extends { coverage: components['schemas']['IssueResultCoverage']; next_cursor: string | null },
>({
  title,
  query,
  after,
  onPage,
  children,
}: {
  title: string;
  query: UseQueryResult<T>;
  after: string | null;
  onPage: (after: string | null) => void;
  children: (page: T) => ReactNode;
}) {
  const { t } = usePresentation();
  return (
    <section aria-label={title}>
      <h3>{title}</h3>
      {query.isPending ? (
        <QueryLoadingNotice label={title} />
      ) : !query.data ? (
        <QueryFailureNotice title={title} error={query.error} onRetry={query.refetch} />
      ) : (
        <>
          <QueryBackgroundNotice
            label={title}
            error={query.error}
            fetching={query.isFetching}
            onRetry={query.refetch}
          />
          <p role="status">{t(`issues.resultCoverage.${query.data.coverage}`)}</p>
          {children(query.data)}
          {query.data.next_cursor ? (
            <button
              disabled={query.isFetching || !!query.error || query.data.coverage !== 'partial'}
              onClick={() => onPage(query.data!.next_cursor)}
            >
              {t('issues.nextPage')}
            </button>
          ) : null}
        </>
      )}
      {after ? <button onClick={() => onPage(null)}>{t('issues.firstPage')}</button> : null}
      <button disabled={query.isFetching} onClick={() => void query.refetch()}>
        {t('issues.refreshResults')}
      </button>
    </section>
  );
}
