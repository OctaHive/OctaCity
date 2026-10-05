import { useQuery } from '@tanstack/react-query';
import { ChevronRight, FolderTree, RefreshCw } from 'lucide-react';
import { Link, useParams } from 'react-router-dom';

import { ManagementApiError } from '../../api/client';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { queryKeys } from '../../app/query';
import { CONSOLE_PATHS, projectPath } from '../../app/routes';
import {
  QueryBackgroundNotice,
  QueryFailureNotice,
  QueryLoadingNotice,
} from '../../shared/QueryStateNotice';
import type { ProjectDetails, ProjectsApi } from './api';
import { ProjectBuilds } from './ProjectBuilds';
import { ProjectDefinitions } from './ProjectDefinitions';
import styles from './Projects.module.css';

interface ProjectsViewProps {
  api: ProjectsApi;
}

export function ProjectsLandingView() {
  const { t } = usePresentation();
  return (
    <section aria-labelledby="page-title" className={styles.page}>
      <PageHeading description={t('projects.choose')} title={t('section.projects')} />
      <div className={styles.emptyState}>
        <FolderTree aria-hidden="true" size={28} strokeWidth={1.6} />
        <p>{t('projects.selectLeft')}</p>
      </div>
    </section>
  );
}

export function ProjectView({ api }: ProjectsViewProps) {
  const { projectId } = useParams<'projectId'>();
  if (projectId === undefined) {
    return <ProjectNotFound />;
  }
  return <SelectedProject api={api} projectId={projectId} />;
}

function SelectedProject({ api, projectId }: ProjectsViewProps & { projectId: string }) {
  const { t } = usePresentation();
  const project = useQuery({
    queryFn: ({ signal }) => api.getProject(projectId, signal),
    queryKey: queryKeys.project(projectId),
  });

  if (project.isPending) {
    return <ProjectLoading />;
  }
  if (project.data === undefined) {
    return isNotFound(project.error) ? (
      <ProjectNotFound error={project.error} onRetry={project.refetch} />
    ) : (
      <QueryFailureNotice
        className={styles.failurePanel}
        error={project.error}
        onRetry={project.refetch}
        title={t('projects.loadFailure')}
      />
    );
  }

  return (
    <section aria-labelledby="page-title" className={styles.page}>
      <ProjectBreadcrumbs details={project.data} />
      <div className={styles.headingRow}>
        <PageHeading description={t('projects.inspect')} title={project.data.project.name} />
        <button
          className={styles.secondaryButton}
          disabled={project.isFetching}
          onClick={() => void project.refetch()}
          type="button"
        >
          <RefreshCw aria-hidden="true" size={16} />
          {project.isFetching ? t('common.refreshing') : t('common.refresh')}
        </button>
      </div>
      <p className={styles.identity}>{project.data.project.id}</p>
      <QueryBackgroundNotice
        className={styles.staleNotice}
        error={project.error}
        fetching={project.isFetching}
        label={t('projects.data')}
        onRetry={project.refetch}
      />
      <ProjectDefinitions key={projectId} api={api} projectId={projectId} />
      <ProjectBuilds key={`builds:${projectId}`} api={api} projectId={projectId} />
    </section>
  );
}

function ProjectBreadcrumbs({ details }: { details: ProjectDetails }) {
  const { t } = usePresentation();
  const trail = [...details.ancestors, details.project];
  return (
    <nav aria-label={t('projects.breadcrumb')} className={styles.breadcrumbs}>
      <ol>
        <li>
          <Link to={CONSOLE_PATHS.projects}>{t('section.projects')}</Link>
        </li>
        {trail.map((project, index) => {
          const current = index === trail.length - 1;
          return (
            <li key={project.id}>
              <ChevronRight aria-hidden="true" size={14} />
              {current ? (
                <span aria-current="page">{project.name}</span>
              ) : (
                <Link to={projectPath(project.id)}>{project.name}</Link>
              )}
            </li>
          );
        })}
      </ol>
    </nav>
  );
}

function PageHeading({ description, title }: { description: string; title: string }) {
  const { t } = usePresentation();
  return (
    <header className={styles.pageHeading}>
      <p className={styles.eyebrow}>{t('projects.workspace')}</p>
      <h1 id="page-title">{title}</h1>
      <p>{description}</p>
    </header>
  );
}

function ProjectLoading() {
  const { t } = usePresentation();
  return (
    <section aria-labelledby="page-title" className={styles.page}>
      <PageHeading description={t('projects.loadingSelected')} title={t('projects.project')} />
      <QueryLoadingNotice className={styles.statePanel} label={t('section.projects')} />
    </section>
  );
}

function ProjectNotFound({
  error = null,
  onRetry,
}: {
  error?: Error | null;
  onRetry?: (() => unknown) | undefined;
} = {}) {
  const { t } = usePresentation();
  return (
    <section aria-labelledby="page-title" className={styles.page}>
      <PageHeading description={t('projects.notFoundDescription')} title={t('projects.notFound')} />
      {onRetry === undefined ? null : (
        <QueryFailureNotice
          className={styles.failurePanel}
          error={error}
          onRetry={onRetry}
          title={t('projects.loadFailure')}
        />
      )}
      <Link className={styles.primaryLink} to={CONSOLE_PATHS.projects}>
        {t('route.returnProjects')}
      </Link>
    </section>
  );
}

function isNotFound(error: Error | null): boolean {
  return error instanceof ManagementApiError && error.code === 'not_found';
}
