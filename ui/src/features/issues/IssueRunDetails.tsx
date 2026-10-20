import { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { Link } from 'react-router-dom';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { queryKeys } from '../../app/query';
import {
  QueryLoadingNotice,
  QueryFailureNotice,
  QueryBackgroundNotice,
  QueryEmptyNotice,
} from '../../shared/QueryStateNotice';
import type { IssueRunReader } from './activityApi';
import { FactoryRunSummary } from '../../shared/FactoryRunSummary';
import { IssueObservation } from './IssueActivity';
import type { Issue } from './api';
import type { IssueActivityApi } from './activityApi';
import type { IssueExecution } from './executionApi';
import styles from './Issues.module.css';
/** Selected execution only; profile facts and ordinary log navigation retain their independent readers. */
export function IssueRunDetails({
  factory,
  activity,
  issue,
  execution,
}: {
  factory: IssueRunReader;
  activity: IssueActivityApi;
  issue: Issue;
  execution: IssueExecution;
}) {
  const { t } = usePresentation();
  const association = execution.association!;
  const run = useQuery({
    queryKey: queryKeys.factoryRun(association.run_id),
    gcTime: 0,
    queryFn: ({ signal }) => factory.getRun(association.run_id, signal),
  });
  return (
    <>
      <section className={styles.panel}>
        {run.isPending ? (
          <QueryLoadingNotice label={t('factory.runOverview')} />
        ) : !run.data ? (
          <QueryFailureNotice
            title={t('factory.runOverview')}
            error={run.error}
            onRetry={run.refetch}
          />
        ) : run.data.id !== association.run_id ||
          run.data.work_id !== association.work_id ||
          run.data.project_id !== issue.project_id ? (
          <p>{t('issues.resultCoverage.unavailable')}</p>
        ) : (
          <>
            <QueryBackgroundNotice
              label={t('factory.runOverview')}
              error={run.error}
              fetching={run.isFetching}
              onRetry={run.refetch}
            />
            <FactoryRunSummary run={run.data} />
          </>
        )}
        <p>{t('issues.gateHelp')}</p>
        <Link to={`/factory/${encodeURIComponent(association.run_id)}?diagnostic=node-attempts`}>
          {t('issues.openGate')}
        </Link>
      </section>
      <ExecutionCollection
        key={association.work_id + ':calls'}
        api={activity}
        issue={issue}
        work={association.work_id}
        collection="calls"
      />
      <ExecutionCollection
        key={association.work_id + ':builds'}
        api={activity}
        issue={issue}
        work={association.work_id}
        collection="builds"
      />
    </>
  );
}
function ExecutionCollection({
  api,
  issue,
  work,
  collection,
}: {
  api: IssueActivityApi;
  issue: Issue;
  work: string;
  collection: 'calls' | 'builds';
}) {
  const { t } = usePresentation();
  const [after, setAfter] = useState<string | null>(null);
  const label = t(collection === 'calls' ? 'issues.executionProfiles' : 'issues.executionJobs');
  const query = useQuery({
    queryKey: queryKeys.issueResults(issue.project_id, issue.id, work, collection, after),
    gcTime: 0,
    queryFn: ({ signal }) =>
      api.results(issue.project_id, issue.id, work, collection, after, signal),
  });
  return (
    <section className={styles.panel} aria-label={label}>
      <h3>{label}</h3>
      {query.isPending ? (
        <QueryLoadingNotice label={label} />
      ) : !query.data ? (
        <QueryFailureNotice title={label} error={query.error} onRetry={query.refetch} />
      ) : (
        <>
          <QueryBackgroundNotice
            label={label}
            error={query.error}
            fetching={query.isFetching}
            onRetry={query.refetch}
          />
          <IssueObservation page={query.data} />
          <p role="status">{t(`issues.resultCoverage.${query.data.coverage}`)}</p>
          {query.data.items.length ? (
            <ul className={styles.list}>
              {query.data.items.map((item) =>
                item.kind === 'call' ? (
                  <li key={item.record.node.id}>
                    <p>{item.record.node.node_key}</p>
                    <p>
                      {item.record.profile
                        ? `${item.record.profile.model_or_tool.identity} · ${item.record.profile.model_or_tool.version}`
                        : t('issues.profileNotReported')}
                    </p>
                  </li>
                ) : item.kind === 'build' ? (
                  <li key={item.record.id}>
                    {item.record.jobs.map((job) => (
                      <p key={job}>
                        <Link
                          to={`/builds/${encodeURIComponent(item.record.build_id)}?${new URLSearchParams({ attempt_id: item.record.attempt_id, job_id: job, log_scope: 'job:' + job })}`}
                        >
                          {t('issues.originalJobLogs')}
                        </Link>
                      </p>
                    ))}
                  </li>
                ) : null,
              )}
            </ul>
          ) : query.data.coverage === 'partial' ? (
            <QueryEmptyNotice>
              {t(collection === 'calls' ? 'issues.profileNotReported' : 'issues.jobsNotReported')}
            </QueryEmptyNotice>
          ) : null}
          {query.data.next_cursor ? (
            <button
              disabled={query.isFetching || query.error !== null}
              onClick={() => setAfter(query.data!.next_cursor)}
            >
              {t('issues.nextPage')}
            </button>
          ) : null}
        </>
      )}
      {after ? <button onClick={() => setAfter(null)}>{t('issues.firstPage')}</button> : null}
      <button disabled={query.isFetching} onClick={() => void query.refetch()}>
        {t('common.refresh')}
      </button>
    </section>
  );
}
