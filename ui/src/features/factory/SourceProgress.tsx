import { useState } from 'react';
import { useInfiniteQuery } from '@tanstack/react-query';
import { sourceFailure } from '../../shared/sourceErrors';
import { queryKeys } from '../../app/query';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { QueryFailureNotice, QueryLoadingNotice } from '../../shared/QueryStateNotice';
import { formatTimestamp } from '../../shared/display';
import { SourceAdmissions } from '../../shared/SourceAdmissions';
import type { FactorySourcesApi, SourceRecord } from './sourceApi';

/** Reads retained facts only; opening this panel never polls a provider or dispatches reporting. */
export function SourceProgress({ record, api }: { record: SourceRecord; api: FactorySourcesApi }) {
  const { t } = usePresentation();
  const [expanded, setExpanded] = useState(false);
  return (
    <>
      <button type="button" aria-expanded={expanded} onClick={() => setExpanded(!expanded)}>
        {t('factory.sourceProgress')}
      </button>
      {expanded ? <Progress record={record} api={api} /> : null}
    </>
  );
}
function Progress({ record, api }: { record: SourceRecord; api: FactorySourcesApi }) {
  const { t, locale } = usePresentation();
  const { project_id: project, id, version } = record.definition;
  const history = useInfiniteQuery({
    queryKey: queryKeys.factorySourceProgress(project, record.kind, id, version),
    queryFn: ({ pageParam, signal }) =>
      api.diagnostics(project, { kind: record.kind, id, version }, pageParam, signal),
    initialPageParam: null as string | null,
    getNextPageParam: (page) => page.next_cursor,
  });
  const scan = history.data?.pages[0]?.scan;
  const time = (value: number) => formatTimestamp(value, locale).display;
  return (
    <section aria-label={t('factory.retainedSourceProgress')}>
      <p>{t('factory.reportingIndependent')}</p>
      {history.isPending ? <QueryLoadingNotice label={t('factory.sourceProgress')} /> : null}
      {history.error ? (
        <QueryFailureNotice
          title={t('factory.sourceProgress')}
          error={sourceFailure(history.error, t)}
          onRetry={() => history.refetch()}
        />
      ) : null}
      {history.data ? (
        <>
          {scan ? (
            <dl>
              <dt>{t('factory.lastScan')}</dt>
              <dd>{time(scan.started_at)}</dd>
              <dt>{t('factory.scanMode')}</dt>
              <dd>
                {t(scan.reconciliation ? 'factory.scanReconciliation' : 'factory.scanIncremental')}
              </dd>
              {scan.selection ? (
                <>
                  <dt>{t('factory.lastSelection')}</dt>
                  <dd>
                    <span>
                      {t(
                        scan.selection.matching_inputs.length > 1 && record.kind === 'group'
                          ? 'factory.preview.ambiguous'
                          : scan.selection.matching_inputs.length
                            ? 'factory.matchedNotAdmitted'
                            : 'factory.preview.filtered',
                      )}
                    </span>{' '}
                    · {scan.selection.matching_inputs.join(', ')}
                  </dd>
                </>
              ) : null}
              {scan.schedule ? (
                <>
                  <dt>{t('factory.sourceRetry')}</dt>
                  <dd>
                    {time(scan.schedule.retry_at)} · {scan.schedule.attempts} ·{' '}
                    {scan.schedule.failure
                      ? t(`factory.sourceFailure.${scan.schedule.failure}`)
                      : t('common.none')}
                    {scan.schedule.exhausted ? ` · ${t('factory.scanExhausted')}` : ''}
                  </dd>
                </>
              ) : null}
            </dl>
          ) : (
            <p>{t('factory.noScan')}</p>
          )}
          {!history.data.pages.some((page) => page.admissions.length) ? (
            <p>{t('factory.noVisibleAdmissions')}</p>
          ) : null}
          <SourceAdmissions admissions={history.data.pages.flatMap((page) => page.admissions)} />
        </>
      ) : null}
      {history.hasNextPage ? (
        <button
          type="button"
          disabled={history.isFetchingNextPage}
          onClick={() => void history.fetchNextPage()}
        >
          {t('integrations.moreHistory')}
        </button>
      ) : null}
    </section>
  );
}
