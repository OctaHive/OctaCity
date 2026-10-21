import { IssueResultProducer } from './IssueResultProducer';
import { IssueCheckState } from './IssueCheckState';
import { useQuery } from '@tanstack/react-query';
import { Link, useSearchParams } from 'react-router-dom';
import { queryKeys } from '../../app/query';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { SafeFactoryMetadata } from '../../shared/FactoryDiagnosticRecord';
import type { Issue } from './api';
import type { IssueActivityApi } from './activityApi';
import type { IssueResultPanelsApi } from './resultPanelsApi';
import { IssueResultPage } from './IssueResultPage';
import styles from './Issues.module.css';

export function IssueChecks({
  api,
  activity,
  issue,
  work,
}: {
  api: IssueResultPanelsApi;
  activity: IssueActivityApi;
  issue: Issue;
  work: string;
}) {
  const [params] = useSearchParams();
  const record = params.get('check_record');
  return record ? (
    <FactoryChecks key={work + record} api={api} issue={issue} work={work} record={record} />
  ) : (
    <CandidateSelection activity={activity} issue={issue} work={work} />
  );
}
function CandidateSelection({
  activity,
  issue,
  work,
}: {
  activity: IssueActivityApi;
  issue: Issue;
  work: string;
}) {
  const { t } = usePresentation();
  const [params, setParams] = useSearchParams();
  const after = params.get('candidate_after');
  const query = useQuery({
    queryKey: queryKeys.issueResults(issue.project_id, issue.id, work, 'candidates', after),
    gcTime: 0,
    queryFn: ({ signal }) =>
      activity.results(issue.project_id, issue.id, work, 'candidates', after, signal),
  });
  return (
    <IssueResultPage
      title={t('issueResults.candidates')}
      query={query}
      after={after}
      onPage={(cursor) => {
        const next = new URLSearchParams(params);
        if (cursor) next.set('candidate_after', cursor);
        else next.delete('candidate_after');
        setParams(next);
      }}
    >
      {(page) => (
        <>
          <p>{t('issueResults.selectCandidate')}</p>
          <ul className={styles.list}>
            {page.items.map((item) => {
              if (
                item.kind !== 'domain_fact' ||
                (item.record.facts.kind !== 'candidate' && item.record.facts.kind !== 'changeset')
              )
                return null;
              const next = new URLSearchParams(params);
              next.set('check_record', item.record.id);
              next.delete('check_after');
              return (
                <li key={item.record.id}>
                  <Link to={{ search: next.toString() }}>
                    {item.record.facts.subject.candidate_revision}
                  </Link>
                </li>
              );
            })}
          </ul>
        </>
      )}
    </IssueResultPage>
  );
}
function FactoryChecks({
  api,
  issue,
  work,
  record,
}: {
  api: IssueResultPanelsApi;
  issue: Issue;
  work: string;
  record: string;
}) {
  const { t, locale } = usePresentation();
  const [params, setParams] = useSearchParams();
  const after = params.get('check_after');
  const query = useQuery({
    queryKey: queryKeys.issueChecks(issue.project_id, issue.id, work, record, after),
    gcTime: 0,
    queryFn: ({ signal }) => api.checks(issue.project_id, issue.id, work, record, after, signal),
  });
  const candidates = new URLSearchParams(params);
  candidates.delete('check_record');
  candidates.delete('check_after');
  return (
    <IssueResultPage
      title={t('issueResults.factoryChecks')}
      query={query}
      after={after}
      onPage={(cursor) => {
        const next = new URLSearchParams(params);
        if (cursor) next.set('check_after', cursor);
        else next.delete('check_after');
        setParams(next);
      }}
    >
      {(page) => (
        <>
          <p>{page.subject.candidate_revision}</p>
          <Link to={{ search: candidates.toString() }}>{t('issueResults.selectCandidate')}</Link>
          {!page.items.length && page.coverage === 'partial' ? (
            <p>{t('issueResults.emptyChecks')}</p>
          ) : null}
          <ul className={styles.list}>
            {page.items.map((check) => (
              <li key={check.id}>
                <h4>{check.name}</h4>
                <IssueCheckState state={check.state} required={check.required} />
                <p>
                  {check.observed_at === null
                    ? t('issues.notReported')
                    : new Date(check.observed_at).toLocaleString(locale)}
                </p>

                <details>
                  <summary>{t('issues.details')}</summary>
                  <SafeFactoryMetadata value={check} fields={['id', 'record']} />
                  <IssueResultProducer producer={check.producer} />
                </details>
              </li>
            ))}
          </ul>
          <details>
            <summary>{t('issueResults.provenance')}</summary>
            <p>{page.record}</p>
            <SafeFactoryMetadata value={page.subject} />
            <SafeFactoryMetadata
              value={page.association}
              fields={['request_id', 'requirements_version', 'work_id', 'run_id']}
            />
          </details>
        </>
      )}
    </IssueResultPage>
  );
}
