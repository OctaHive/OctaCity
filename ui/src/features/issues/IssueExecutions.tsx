import { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { Link, useSearchParams } from 'react-router-dom';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { queryKeys } from '../../app/query';
import { QueryContent, QueryEmptyNotice } from '../../shared/QueryStateNotice';
import type { Issue } from './api';
import type { IssueExecutionsApi } from './executionApi';
import { ExecutionReceipt } from './IssueExecutionReceipt';
import { useIssueExecution } from './useIssueExecution';
import { IssueRunDetails } from './IssueRunDetails';
import { selectIssueExecution } from './issueExecutionSelection';
import type { IssueRunReader } from './activityApi';
import type { IssueActivityApi } from './activityApi';
import styles from './Issues.module.css';

/** Bounded, read-only history; selecting or refreshing an execution never publishes inputs. */
export function IssueExecutions({
  api,
  issue,
  factory,
  activity,
}: {
  api: IssueExecutionsApi;
  issue: Issue;
  factory: IssueRunReader;
  activity: IssueActivityApi;
}) {
  const { t, locale } = usePresentation();
  const [params] = useSearchParams();
  const requestId = params.get('execution');
  const [after, setAfter] = useState<string | null>(null);
  const history = useQuery({
    queryKey: queryKeys.issueExecutions(issue.project_id, issue.id, after),
    gcTime: 0,
    queryFn: ({ signal }) => api.list(issue.project_id, issue.id, after, signal),
  });
  const selected = useIssueExecution(api, issue, requestId);
  return (
    <section className={styles.panel} aria-label={t('issues.executionHistory')}>
      <h2>{t('issues.executionHistory')}</h2>
      <QueryContent query={history} label={t('issues.executionHistory')}>
        {(data) => (
          <>
            {data.items.length ? (
              <ol className={styles.list}>
                {data.items.map((entry) => {
                  const search = selectIssueExecution(params, entry.id);
                  return (
                    <li key={entry.id}>
                      <Link
                        to={{ search: search.toString() }}
                        aria-current={requestId === entry.id ? 'true' : undefined}
                      >
                        {t('issues.executionReview', { version: entry.intent.expected_version })} ·{' '}
                        {new Date(entry.requested_at).toLocaleString(locale)} · {entry.id}
                      </Link>
                      <p>
                        {t(
                          entry.state === 'pending'
                            ? 'issues.executionPending'
                            : 'issues.executionAdmitted',
                        )}
                      </p>
                    </li>
                  );
                })}
              </ol>
            ) : (
              <QueryEmptyNotice>{t('issues.noExecutions')}</QueryEmptyNotice>
            )}
            {after ? <button onClick={() => setAfter(null)}>{t('issues.firstPage')}</button> : null}
            {data.next_cursor ? (
              <button onClick={() => setAfter(data.next_cursor)}>{t('issues.nextPage')}</button>
            ) : null}
          </>
        )}
      </QueryContent>
      <button
        disabled={history.isFetching || selected.isFetching}
        onClick={() => {
          void history.refetch();
          if (requestId) void selected.refetch();
        }}
      >
        {t('issues.refreshExecutions')}
      </button>
      {requestId ? (
        <section className={styles.panel} aria-label={t('issues.selectedExecution')}>
          <h3>{t('issues.selectedExecution')}</h3>
          <QueryContent query={selected} label={t('issues.selectedExecution')}>
            {(data) => (
              <>
                <ExecutionReceipt execution={data} currentVersion={issue.version} />
                {data.association ? (
                  <IssueRunDetails
                    key={data.id}
                    factory={factory}
                    activity={activity}
                    issue={issue}
                    association={data.association}
                    request={data.id}
                  />
                ) : null}
                <dl>
                  <dt>{t('factory.configurations')}</dt>
                  <dd>
                    {data.intent.target.configuration_id} · v
                    {data.intent.target.configuration_version}
                  </dd>
                  <dt>{t('factory.repository')}</dt>
                  <dd>
                    {data.intent.target.repository_id} · v{data.intent.target.repository_version}
                  </dd>
                  <dt>{t('manualBuild.source')}</dt>
                  <dd>
                    {data.intent.target.source.kind === 'default_reference'
                      ? t('manualBuild.defaultReference')
                      : data.intent.target.source.value}
                  </dd>
                </dl>
              </>
            )}
          </QueryContent>
        </section>
      ) : null}
    </section>
  );
}
