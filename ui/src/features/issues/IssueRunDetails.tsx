import { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { Link, useSearchParams } from 'react-router-dom';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { queryKeys } from '../../app/query';
import { QueryContent, QueryEmptyNotice } from '../../shared/QueryStateNotice';
import { IssueJournalEvents } from './IssueJournalEvents';
import { DIAGNOSTIC_LABELS, DiagnosticRecordDetails } from '../../shared/FactoryDiagnosticRecord';
import { SelectMenu } from '../../shared/SelectMenu';
import type { IssueResultCollection } from './activityApi';
import type { IssueRunReader } from './activityApi';
import { FactoryRunSummary } from '../../shared/FactoryRunSummary';
import { IssueObservation } from './IssueActivity';
import type { Issue } from './api';
import type { IssueActivityApi } from './activityApi';
import type { IssueWorkAssociation } from './executionApi';
import styles from './Issues.module.css';
/** Selected execution only; profile facts and ordinary log navigation retain their independent readers. */
export function IssueRunDetails({
  factory,
  activity,
  issue,
  association,
  request,
}: {
  factory: IssueRunReader;
  activity: IssueActivityApi;
  issue: Issue;
  association: IssueWorkAssociation;
  request?: string;
}) {
  const { t } = usePresentation();
  const [params] = useSearchParams();
  const build = params.get('journal_build');
  const job = params.get('journal_job');
  const run = useQuery({
    queryKey: queryKeys.factoryRun(association.run_id),
    gcTime: 0,
    queryFn: ({ signal }) => factory.getRun(association.run_id, signal),
  });
  return (
    <>
      <section className={styles.panel}>
        <QueryContent query={run} label={t('factory.runOverview')}>
          {(data) =>
            data.id !== association.run_id ||
            data.work_id !== association.work_id ||
            data.project_id !== issue.project_id ? (
              <p>{t('issues.resultCoverage.unavailable')}</p>
            ) : (
              <FactoryRunSummary run={data} />
            )
          }
        </QueryContent>
        <p>{t('issues.gateHelp')}</p>
        <Link to={`/factory/${encodeURIComponent(association.run_id)}?diagnostic=node-attempts`}>
          {t('issues.openGate')}
        </Link>
      </section>
      <ExecutionCollection
        api={activity}
        issue={issue}
        request={request}
        work={association.work_id}
        collection="node-attempts"
      />
      {build && job ? (
        <IssueJournalEvents
          key={build + job}
          api={activity}
          issue={issue}
          request={request ?? { work_id: association.work_id }}
          target={{ build_execution: build, job_id: job }}
        />
      ) : null}
      <ExecutionCollection
        api={activity}
        issue={issue}
        request={request}
        work={association.work_id}
        collection="calls"
      />
      <ExecutionCollection
        api={activity}
        issue={issue}
        request={request}
        work={association.work_id}
        collection="builds"
      />
    </>
  );
}
function ExecutionCollection({
  api,
  issue,
  request,
  work,
  collection,
}: {
  api: IssueActivityApi;
  issue: Issue;
  request: string | undefined;
  work: string;
  collection: 'calls' | 'builds' | 'node-attempts';
}) {
  const { t } = usePresentation();
  const [params, setParams] = useSearchParams();
  const stages = collection === 'node-attempts';
  const selected: IssueResultCollection = stages
    ? (stageCollections.find((c) => c === params.get('journal_collection')) ?? collection)
    : collection;
  const [continuation, setContinuation] = useState<{
    collection: IssueResultCollection;
    after: string | null;
  }>({ collection: selected, after: null });
  const after = continuation.collection === selected ? continuation.after : null;
  const setAfter = (after: string | null) => setContinuation({ collection: selected, after });
  const label = t(
    stages
      ? 'issues.executionStages'
      : collection === 'calls'
        ? 'issues.executionProfiles'
        : 'issues.executionJobs',
  );
  const query = useQuery({
    queryKey: queryKeys.issueJournal(
      issue.project_id,
      issue.id,
      request ?? `work:${work}`,
      selected,
      after,
    ),
    gcTime: 0,
    queryFn: async ({ signal }) =>
      request
        ? (await api.journal(issue.project_id, issue.id, request, selected, after, signal)).records
        : api.results(issue.project_id, issue.id, work, selected, after, signal),
  });
  const page = query.data;
  return (
    <section className={styles.panel} aria-label={label}>
      <h3>{label}</h3>
      {stages ? (
        <SelectMenu
          ariaLabel={t('issues.journalCollection')}
          value={selected}
          options={stageCollections.map((value) => ({ value, label: t(DIAGNOSTIC_LABELS[value]) }))}
          onValueChange={(value) => {
            const next = new URLSearchParams(params);
            next.set('journal_collection', value);
            setAfter(null);
            setParams(next);
          }}
        />
      ) : null}
      <QueryContent query={query} label={label}>
        {() => (
          <>
            {page ? <IssueObservation page={page} /> : null}
            <p role="status">{t(`issues.resultCoverage.${page?.coverage ?? 'unavailable'}`)}</p>
            {page?.items.length ? (
              <ul className={styles.list}>
                {page.items.map((item) =>
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
                      <p>{t('issues.stageReference', { id: item.record.node_attempt_id })}</p>
                      {item.record.jobs.map((job) => (
                        <div key={job}>
                          <button
                            onClick={() => {
                              const next = new URLSearchParams(params);
                              next.set('journal_build', item.record.id);
                              next.set('journal_job', job);
                              next.delete('journal_event');
                              setParams(next);
                            }}
                          >
                            {t('issues.inspectJob', { id: job })}
                          </button>
                          <Link
                            to={`/builds/${encodeURIComponent(item.record.build_id)}?${new URLSearchParams({ attempt_id: item.record.attempt_id, job_id: job, log_scope: 'job:' + job })}`}
                          >
                            {t('issues.originalJobLogs')}
                          </Link>
                        </div>
                      ))}
                    </li>
                  ) : (
                    <li key={item.record.id}>
                      {item.kind === 'node_attempt' ? (
                        <>
                          <h4>{item.record.node_key}</h4>
                          <p>
                            {t(item.record.active ? 'issues.stageActive' : 'issues.stageRetained', {
                              number: item.record.number,
                            })}
                          </p>
                          <Link
                            to={`/factory/${encodeURIComponent(executionRun(page))}?diagnostic=node-attempts&node_attempt_id=${encodeURIComponent(item.record.id)}`}
                          >
                            {t('issues.exactRunControls')}
                          </Link>
                        </>
                      ) : null}
                      <details>
                        <summary>{t('issues.inspectResult')}</summary>
                        <DiagnosticRecordDetails item={item} />
                      </details>
                    </li>
                  ),
                )}
              </ul>
            ) : page?.coverage === 'partial' ? (
              <QueryEmptyNotice>
                {t(
                  stages
                    ? 'issues.noResults'
                    : collection === 'calls'
                      ? 'issues.profileNotReported'
                      : 'issues.jobsNotReported',
                )}
              </QueryEmptyNotice>
            ) : null}
            {page?.next_cursor ? (
              <button
                disabled={query.isFetching || query.error !== null}
                onClick={() => setAfter(page!.next_cursor)}
              >
                {t('issues.nextPage')}
              </button>
            ) : null}
          </>
        )}
      </QueryContent>
      {after ? <button onClick={() => setAfter(null)}>{t('issues.firstPage')}</button> : null}
      <button disabled={query.isFetching} onClick={() => void query.refetch()}>
        {t('common.refresh')}
      </button>
    </section>
  );
}

const stageCollections = [
  'node-attempts',
  'flow-runs',
  'workflow-cycles',
  'node-completions',
  'node-stops',
  'budgets',
] as const;
function executionRun(page: import('./activityApi').IssueResultPage) {
  return page.association.run_id;
}
