import { queryKeys } from '../../app/query';
import { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { Link, useParams } from 'react-router-dom';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { issuePath } from '../../app/routes';
import {
  QueryLoadingNotice,
  QueryFailureNotice,
  QueryBackgroundNotice,
  QueryEmptyNotice,
} from '../../shared/QueryStateNotice';
import { IssueProjectNav, type IssueProjectReader } from './IssueProjectNav';
import { IssueEditor } from './IssueEditor';
import type { IssuesApi } from './api';
import styles from './Issues.module.css';
import type { IssueSourcesApi } from './sourceApi';
import { ProjectSources } from './ProjectSources';
export function IssuesView({
  api,
  projects,
  sources,
}: {
  api: IssuesApi;
  projects: IssueProjectReader;
  sources: IssueSourcesApi;
}) {
  const { projectId } = useParams<'projectId'>();
  return projectId ? (
    <ProjectIssues
      key={projectId}
      api={api}
      projects={projects}
      sources={sources}
      projectId={projectId}
    />
  ) : null;
}
function ProjectIssues({
  api,
  projects,
  projectId,
  sources,
}: {
  api: IssuesApi;
  projects: IssueProjectReader;
  projectId: string;
  sources: IssueSourcesApi;
}) {
  const { t } = usePresentation();
  const [creating, setCreating] = useState(false);
  const [showSources, setShowSources] = useState(false);
  const [after, setAfter] = useState<string | null>(null);
  const list = useQuery({
    queryKey: queryKeys.issueList(projectId, after),
    queryFn: ({ signal }) => api.list(projectId, after, signal),
  });
  return (
    <section className={styles.page}>
      <IssueProjectNav projects={projects} projectId={projectId} />
      <h1>{t('issues.tasks')}</h1>
      <p>{t('issues.manualHelp')}</p>
      <button type="button" disabled={list.isFetching} onClick={() => void list.refetch()}>
        {t('issueImport.refreshTasks')}
      </button>
      <button type="button" onClick={() => setCreating(!creating)}>
        {t(creating ? 'command.abandon' : 'issues.create')}
      </button>
      {creating ? <IssueEditor api={api} projectId={projectId} /> : null}
      <button
        type="button"
        aria-expanded={showSources}
        onClick={() => setShowSources(!showSources)}
      >
        {t('issueImport.open')}
      </button>
      {showSources ? <ProjectSources projectId={projectId} api={sources} /> : null}
      {list.isPending ? (
        <QueryLoadingNotice label={t('issues.tasks')} />
      ) : list.data === undefined ? (
        <QueryFailureNotice title={t('issues.tasks')} error={list.error} onRetry={list.refetch} />
      ) : (
        <>
          <QueryBackgroundNotice
            label={t('issues.tasks')}
            fetching={list.isFetching}
            error={list.error}
            onRetry={list.refetch}
          />
          {list.data.items.length === 0 ? (
            <QueryEmptyNotice>{t('issues.empty')}</QueryEmptyNotice>
          ) : (
            <ul className={styles.list}>
              {list.data.items.map((issue) => (
                <li key={issue.id}>
                  <Link to={issuePath(projectId, issue.id)}>
                    {issue.display_key} · {issue.title}
                  </Link>
                </li>
              ))}
            </ul>
          )}
          {after ? <button onClick={() => setAfter(null)}>{t('issues.firstPage')}</button> : null}
          {list.data.next_cursor ? (
            <button onClick={() => setAfter(list.data!.next_cursor)}>{t('issues.nextPage')}</button>
          ) : null}
        </>
      )}
    </section>
  );
}
