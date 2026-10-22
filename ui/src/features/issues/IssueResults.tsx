import { IssueExternalChecks } from './IssueExternalChecks';
import type { IssueCodeReviewsApi } from './codeReviewApi';
import { IssueArtifacts } from './IssueArtifacts';
import { IssueChecks } from './IssueChecks';
import type { IssueResultPanelsApi } from './resultPanelsApi';
import { useQuery } from '@tanstack/react-query';
import { Link, useSearchParams } from 'react-router-dom';
import { queryKeys } from '../../app/query';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { QueryContent, QueryEmptyNotice } from '../../shared/QueryStateNotice';
import { SelectMenu } from '../../shared/SelectMenu';
import { DIAGNOSTIC_LABELS, DiagnosticRecordDetails } from '../../shared/FactoryDiagnosticRecord';
import type { Issue } from './api';
import type { IssueActivityApi, IssueResultCollection } from './activityApi';
import type { IssueExecution, IssueExecutionsApi, IssueLegacyWorkApi } from './executionApi';
import { ExecutionReceipt } from './IssueExecutionReceipt';
import { useIssueExecution, useIssueWork } from './useIssueExecution';
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
  panels,
  reviews,
  executions,
  legacy,
  issue,
}: {
  panels: IssueResultPanelsApi;
  reviews: IssueCodeReviewsApi;
  api: IssueActivityApi;
  executions: IssueExecutionsApi;
  legacy: IssueLegacyWorkApi;
  issue: Issue;
}) {
  const { t } = usePresentation();
  const [params] = useSearchParams();
  const requestId = params.get('execution');
  const pane = params.get('result_pane') ?? 'records';
  const selected = useIssueExecution(executions, issue, requestId);
  const workId = requestId ? null : params.get('work');
  const original = useIssueWork(legacy, issue, workId);
  const query = requestId ? selected : original;
  const runs = new URLSearchParams(params);
  runs.set('section', 'runs');
  return (
    <section className={styles.panel} aria-label={t('issues.executionResults')}>
      <h2>{t('issues.executionResults')}</h2>
      <nav className={styles.actions} aria-label={t('issueResults.panes')}>
        {(['records', 'checks', 'artifacts'] as const).map((value) => {
          const next = new URLSearchParams(params);
          next.set('result_pane', value);
          return (
            <Link
              key={value}
              aria-current={pane === value ? 'page' : undefined}
              to={{ search: next.toString() }}
            >
              {t(`issueResults.${value}`)}
            </Link>
          );
        })}
      </nav>
      {pane === 'checks' ? (
        <IssueExternalChecks api={panels} reviews={reviews} issue={issue} />
      ) : null}
      {!requestId && !workId ? (
        <>
          <p>{t('issues.selectExecution')}</p>
          <Link to={{ search: runs.toString() }}>{t('issues.runs')}</Link>
        </>
      ) : (
        <QueryContent<IssueExecution | Awaited<ReturnType<IssueLegacyWorkApi['getWork']>>>
          query={query}
          label={t('issues.selectedExecution')}
        >
          {(data) => (
            <>
              {'intent' in data ? (
                <ExecutionReceipt execution={data} currentVersion={issue.version} />
              ) : (
                <p>{t('issues.legacyProvenance')}</p>
              )}
              {data.association && pane === 'checks' ? (
                <IssueChecks
                  api={panels}
                  activity={api}
                  issue={issue}
                  work={data.association.work_id}
                />
              ) : data.association && pane === 'artifacts' ? (
                <IssueArtifacts
                  key={data.association.work_id}
                  api={panels}
                  issue={issue}
                  work={data.association.work_id}
                />
              ) : data.association ? (
                <ResultCollection
                  key={data.association.work_id}
                  api={api}
                  issue={issue}
                  work={data.association.work_id}
                />
              ) : null}
            </>
          )}
        </QueryContent>
      )}
    </section>
  );
}
function ResultCollection({
  api,
  issue,
  work,
}: {
  api: IssueActivityApi;
  issue: Issue;
  work: string;
}) {
  const { t } = usePresentation();
  const [params, setParams] = useSearchParams();
  const collection = collections.find((c) => c === params.get('result_collection')) ?? 'evidence';
  const after = params.get('result_after');
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
      <QueryContent query={query} label={t('issues.executionResults')}>
        {(data) => (
          <>
            <IssueObservation page={data} />
            <p role="status">{t(`issues.resultCoverage.${data.coverage}`)}</p>
            {data.items.length ? (
              <ol className={styles.list}>
                {data.items.map((item) => {
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
            ) : data.coverage === 'partial' ? (
              <QueryEmptyNotice>{t('issues.noResults')}</QueryEmptyNotice>
            ) : null}
            <div className={styles.actions}>
              {data.next_cursor ? (
                <button
                  disabled={query.isFetching || query.error !== null}
                  onClick={() => change('result_after', data.next_cursor)}
                >
                  {t('issues.nextPage')}
                </button>
              ) : null}
            </div>
          </>
        )}
      </QueryContent>
      {after ? (
        <button onClick={() => change('result_after', null)}>{t('issues.firstPage')}</button>
      ) : null}
      <button disabled={query.isFetching} onClick={() => void query.refetch()}>
        {t('issues.refreshResults')}
      </button>
    </>
  );
}
