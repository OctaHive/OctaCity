import type { ReactNode } from 'react';
import type { UseQueryResult } from '@tanstack/react-query';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { QueryContent } from '../../shared/QueryStateNotice';
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
      <QueryContent query={query} label={title}>
        {(data) => (
          <>
            <p role="status">{t(`issues.resultCoverage.${data.coverage}`)}</p>
            {children(data)}
            {data.next_cursor ? (
              <button
                disabled={query.isFetching || !!query.error || data.coverage !== 'partial'}
                onClick={() => onPage(data.next_cursor)}
              >
                {t('issues.nextPage')}
              </button>
            ) : null}
          </>
        )}
      </QueryContent>
      {after ? <button onClick={() => onPage(null)}>{t('issues.firstPage')}</button> : null}
      <button disabled={query.isFetching} onClick={() => void query.refetch()}>
        {t('issues.refreshResults')}
      </button>
    </section>
  );
}
