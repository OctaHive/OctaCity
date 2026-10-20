import { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { Link, useSearchParams } from 'react-router-dom';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { queryKeys } from '../../app/query';
import {
  QueryBackgroundNotice,
  QueryEmptyNotice,
  QueryFailureNotice,
  QueryLoadingNotice,
} from '../../shared/QueryStateNotice';
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
      {history.isPending ? (
        <QueryLoadingNotice label={t('issues.executionHistory')} />
      ) : !history.data ? (
        <QueryFailureNotice
          title={t('issues.executionHistory')}
          error={history.error}
          onRetry={history.refetch}
        />
      ) : (
        <>
          <QueryBackgroundNotice
            label={t('issues.executionHistory')}
            fetching={history.isFetching}
            error={history.error}
            onRetry={history.refetch}
          />
          {history.data.items.length ? (
            <ol className={styles.list}>
              {history.data.items.map((entry) => {
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
          {history.data.next_cursor ? (
            <button onClick={() => setAfter(history.data!.next_cursor)}>
              {t('issues.nextPage')}
            </button>
          ) : null}
        </>
      )}
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
          {selected.isPending ? (
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
                <IssueRunDetails
                  factory={factory}
                  activity={activity}
                  issue={issue}
                  execution={selected.data}
                />
              ) : null}
              <dl>
                <dt>{t('factory.configurations')}</dt>
                <dd>
                  {selected.data.intent.target.configuration_id} · v
                  {selected.data.intent.target.configuration_version}
                </dd>
                <dt>{t('factory.repository')}</dt>
                <dd>
                  {selected.data.intent.target.repository_id} · v
                  {selected.data.intent.target.repository_version}
                </dd>
                <dt>{t('manualBuild.source')}</dt>
                <dd>
                  {selected.data.intent.target.source.kind === 'default_reference'
                    ? t('manualBuild.defaultReference')
                    : selected.data.intent.target.source.value}
                </dd>
              </dl>
            </>
          )}
        </section>
      ) : null}
    </section>
  );
}
