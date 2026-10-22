import { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { Link, useSearchParams } from 'react-router-dom';
import { queryKeys } from '../../app/query';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { QueryContent } from '../../shared/QueryStateNotice';
import { IssueRunDetails } from './IssueRunDetails';
import { useIssueWork } from './useIssueExecution';
import type { IssueActivityApi, IssueRunReader } from './activityApi';
import type { IssueLegacyWorkApi } from './executionApi';
import type { Issue } from './api';
import styles from './Issues.module.css';

/** Original Artifact lineage stays distinct from reviewed Issue execution requests. */
export function IssueLegacyExecutions({
  api,
  issue,
  factory,
  activity,
}: {
  api: IssueLegacyWorkApi;
  issue: Issue;
  factory: IssueRunReader;
  activity: IssueActivityApi;
}) {
  const { t } = usePresentation();
  const [params] = useSearchParams();
  const work = params.get('work');
  const [after, setAfter] = useState<string | null>(null);
  const history = useQuery({
    queryKey: queryKeys.issueLegacyHistory(issue.project_id, issue.id, after),
    gcTime: 0,
    queryFn: ({ signal }) => api.listLegacy(issue.project_id, issue.id, after, signal),
  });
  const selected = useIssueWork(api, issue, work);
  if (history.data?.items.length === 0 && !work) return null;
  return (
    <section className={styles.panel} aria-label={t('issues.legacyExecutions')}>
      <h3>{t('issues.legacyExecutions')}</h3>
      <QueryContent query={history} label={t('issues.legacyExecutions')}>
        {(data) => (
          <>
            <ul className={styles.list}>
              {data.items.map((entry) => {
                const search = new URLSearchParams({ section: 'runs', work: entry.work_id });
                return (
                  <li key={entry.work_id}>
                    <Link
                      to={{ search: search.toString() }}
                      aria-current={work === entry.work_id ? 'true' : undefined}
                    >
                      {t('issues.legacyExecution', { work: entry.work_id })}
                    </Link>
                  </li>
                );
              })}
            </ul>
            {data.next_cursor ? (
              <button onClick={() => setAfter(data.next_cursor)}>{t('issues.nextPage')}</button>
            ) : null}
          </>
        )}
      </QueryContent>
      {after ? <button onClick={() => setAfter(null)}>{t('issues.firstPage')}</button> : null}
      {work ? (
        <QueryContent query={selected} label={t('issues.selectedExecution')}>
          {(data) => (
            <>
              <p>{t('issues.legacyProvenance')}</p>
              <Link to={`/factory/${encodeURIComponent(data.association.run_id)}`}>
                {t('issues.openOriginalRun')}
              </Link>
              <IssueRunDetails
                key={work}
                factory={factory}
                activity={activity}
                issue={issue}
                association={data.association}
              />
            </>
          )}
        </QueryContent>
      ) : null}
    </section>
  );
}
