import { useQuery } from '@tanstack/react-query';
import { Link, useSearchParams } from 'react-router-dom';
import { queryKeys } from '../../app/query';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import {
  QueryLoadingNotice,
  QueryFailureNotice,
  QueryBackgroundNotice,
  QueryEmptyNotice,
} from '../../shared/QueryStateNotice';
import { SelectMenu } from '../../shared/SelectMenu';
import { DIAGNOSTIC_LABELS, DiagnosticRecordDetails } from '../../shared/FactoryDiagnosticRecord';
import type { Issue } from './api';
import type { IssueActivityApi, IssueResultCollection } from './activityApi';
import type { IssueExecution, IssueExecutionsApi } from './executionApi';
import { ExecutionReceipt } from './IssueExecutionReceipt';
import { useIssueExecution } from './useIssueExecution';
import { IssueObservation } from './IssueActivity';
import styles from './Issues.module.css';
const collections = [
  'changesets',
  'evidence',
  'review-plans',
  'assessments',
  'decisions',
  'delivery',
  'promotion',
] as const satisfies readonly IssueResultCollection[];
/** Exact execution selection is shared with Runs; a bounded page never establishes overall acceptance. */
export function IssueResults({
  api,
  executions,
  issue,
}: {
  api: IssueActivityApi;
  executions: IssueExecutionsApi;
  issue: Issue;
}) {
  const { t } = usePresentation();
  const [params] = useSearchParams();
  const requestId = params.get('execution');
  const selected = useIssueExecution(executions, issue, requestId);
  const runs = new URLSearchParams(params);
  runs.set('section', 'runs');
  return (
    <section className={styles.panel} aria-label={t('issues.executionResults')}>
      <h2>{t('issues.executionResults')}</h2>
      {!requestId ? (
        <>
          <p>{t('issues.selectExecution')}</p>
          <Link to={{ search: runs.toString() }}>{t('issues.runs')}</Link>
        </>
      ) : selected.isPending ? (
        <QueryLoadingNotice label={t('issues.selectedExecution')} />
      ) : !selected.data ? (
        <QueryFailureNotice
          title={t('issues.selectedExecution')}
          error={selected.error}
          onRetry={selected.refetch}
        />
      ) : (
        <>
          <QueryBackgroundNotice
            label={t('issues.selectedExecution')}
            fetching={selected.isFetching}
            error={selected.error}
            onRetry={selected.refetch}
          />
          <ExecutionReceipt execution={selected.data} currentVersion={issue.version} />
          {selected.data.association ? (
            <ResultCollection
              key={selected.data.association.work_id}
              api={api}
              issue={issue}
              execution={selected.data}
            />
          ) : null}
        </>
      )}
    </section>
  );
}
function ResultCollection({
  api,
  issue,
  execution,
}: {
  api: IssueActivityApi;
  issue: Issue;
  execution: IssueExecution;
}) {
  const { t } = usePresentation();
  const [params, setParams] = useSearchParams();
  const collection = collections.find((c) => c === params.get('result_collection')) ?? 'evidence';
  const after = params.get('result_after');
  const work = execution.association!.work_id;
  const query = useQuery({
    queryKey: queryKeys.issueResults(issue.project_id, issue.id, work, collection, after),
    gcTime: 0,
    queryFn: ({ signal }) =>
      api.results(issue.project_id, issue.id, work, collection, after, signal),
  });
  const change = (key: string, value: string | null) => {
    const next = new URLSearchParams(params);
    if (value === null) next.delete(key);
    else next.set(key, value);
    if (key !== 'result_id') next.delete('result_id');
    if (key === 'result_collection') next.delete('result_after');
    setParams(next);
  };
  return (
    <>
      <SelectMenu
        ariaLabel={t('issues.resultCollection')}
        value={collection}
        options={collections.map((value) => ({ value, label: t(DIAGNOSTIC_LABELS[value]) }))}
        onValueChange={(value) => change('result_collection', value)}
      />
      {query.isPending ? (
        <QueryLoadingNotice label={t('issues.executionResults')} />
      ) : !query.data ? (
        <QueryFailureNotice
          title={t('issues.executionResults')}
          error={query.error}
          onRetry={query.refetch}
        />
      ) : (
        <>
          <QueryBackgroundNotice
            label={t('issues.executionResults')}
            fetching={query.isFetching}
            error={query.error}
            onRetry={query.refetch}
          />
          <IssueObservation page={query.data} />
          <p role="status">{t(`issues.resultCoverage.${query.data.coverage}`)}</p>
          {query.data.items.length ? (
            <ol className={styles.list}>
              {query.data.items.map((item) => {
                const id = item.kind === 'call' ? item.record.node.id : item.record.id;
                const open = params.get('result_id') === id;
                return (
                  <li key={id}>
                    <h3>{t(DIAGNOSTIC_LABELS[collection])}</h3>
                    <button
                      aria-expanded={open}
                      onClick={() => change('result_id', open ? null : id)}
                    >
                      {t('issues.inspectResult')}
                    </button>
                    {open ? <DiagnosticRecordDetails item={item} /> : null}
                  </li>
                );
              })}
            </ol>
          ) : query.data.coverage === 'partial' ? (
            <QueryEmptyNotice>{t('issues.noResults')}</QueryEmptyNotice>
          ) : null}
          <div className={styles.actions}>
            {query.data.next_cursor ? (
              <button
                disabled={query.isFetching || query.error !== null}
                onClick={() => change('result_after', query.data!.next_cursor)}
              >
                {t('issues.nextPage')}
              </button>
            ) : null}
          </div>
        </>
      )}
      {after ? (
        <button onClick={() => change('result_after', null)}>{t('issues.firstPage')}</button>
      ) : null}
      <button disabled={query.isFetching} onClick={() => void query.refetch()}>
        {t('issues.refreshResults')}
      </button>
    </>
  );
}
