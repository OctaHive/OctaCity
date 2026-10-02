import { useInfiniteQuery, useQuery, type InfiniteData } from '@tanstack/react-query';
import { AlertTriangle, ChevronRight, FolderTree, RefreshCw } from 'lucide-react';
import { Link, useParams } from 'react-router-dom';

import { ManagementApiError } from '../../api/client';
import { queryKeys } from '../../app/query';
import { CONSOLE_PATHS, projectPath } from '../../app/routes';
import type { ProjectDetails, ProjectPage, ProjectResource, ProjectsApi } from './api';
import styles from './Projects.module.css';

interface ProjectsViewProps {
  api: ProjectsApi;
}

export function ProjectHierarchyView({ api }: ProjectsViewProps) {
  return (
    <section aria-labelledby="page-title" className={styles.page}>
      <PageHeading
        description="Browse root Projects and continue through their bounded child collections."
        title="Projects"
      />
      <ProjectCollection api={api} emptyLabel="No root Projects are available." parentId={null} />
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
  const project = useQuery({
    queryFn: ({ signal }) => api.getProject(projectId, signal),
    queryKey: queryKeys.project(projectId),
  });

  if (project.isPending) {
    return <ProjectLoading />;
  }
  if (project.data === undefined) {
    return isNotFound(project.error) ? (
      <ProjectNotFound />
    ) : (
      <ProjectFailure error={project.error} onRetry={() => void project.refetch()} />
    );
  }

  return (
    <section aria-labelledby="page-title" className={styles.page}>
      <ProjectBreadcrumbs details={project.data} />
      <div className={styles.headingRow}>
        <PageHeading
          description="Select a child Project or inspect the current Project workspace."
          title={project.data.project.name}
        />
        <button
          className={styles.secondaryButton}
          disabled={project.isFetching}
          onClick={() => void project.refetch()}
          type="button"
        >
          <RefreshCw aria-hidden="true" size={16} />
          {project.isFetching ? 'Refreshing' : 'Refresh'}
        </button>
      </div>
      <p className={styles.identity}>{project.data.project.id}</p>
      {project.error === null ? null : <StaleNotice />}
      <div className={styles.sectionHeading}>
        <div>
          <p className={styles.eyebrow}>Hierarchy</p>
          <h2>Child Projects</h2>
        </div>
      </div>
      <ProjectCollection
        api={api}
        emptyLabel="This Project has no child Projects."
        parentId={projectId}
      />
    </section>
  );
}

function ProjectCollection({
  api,
  emptyLabel,
  parentId,
}: ProjectsViewProps & { emptyLabel: string; parentId: string | null }) {
  const projects = useInfiniteQuery<
    ProjectPage,
    Error,
    InfiniteData<ProjectPage>,
    ReturnType<typeof queryKeys.projectChildren>,
    string | null
  >({
    getNextPageParam: (page) => page.next_cursor ?? undefined,
    initialPageParam: null as string | null,
    queryFn: ({ pageParam, signal }) => api.listProjects(parentId, pageParam, signal),
    queryKey: queryKeys.projectChildren(parentId),
  });

  if (projects.isPending) {
    return <LoadingPanel />;
  }
  if (projects.data === undefined) {
    return <ProjectFailure error={projects.error} onRetry={() => void projects.refetch()} />;
  }

  const items = projects.data.pages.flatMap((page) => page.items);
  return (
    <div className={styles.collection}>
      <div className={styles.collectionToolbar}>
        <p>{items.length === 1 ? '1 Project loaded' : `${items.length} Projects loaded`}</p>
        <button
          className={styles.textButton}
          disabled={projects.isFetching}
          onClick={() => void projects.refetch()}
          type="button"
        >
          <RefreshCw aria-hidden="true" size={15} />
          {projects.isFetching && !projects.isFetchingNextPage ? 'Refreshing' : 'Refresh'}
        </button>
      </div>
      {projects.error === null ? null : <StaleNotice />}
      {items.length === 0 ? (
        <div className={styles.emptyState}>
          <FolderTree aria-hidden="true" size={28} strokeWidth={1.6} />
          <p>{emptyLabel}</p>
        </div>
      ) : (
        <ul className={styles.projectList}>
          {items.map((item) => (
            <ProjectListItem key={item.id} project={item} />
          ))}
        </ul>
      )}
      {projects.hasNextPage ? (
        <button
          className={styles.loadMoreButton}
          disabled={projects.isFetchingNextPage}
          onClick={() => void projects.fetchNextPage()}
          type="button"
        >
          {projects.isFetchingNextPage ? 'Loading Projects' : 'Load more Projects'}
        </button>
      ) : null}
    </div>
  );
}

function ProjectListItem({ project }: { project: ProjectResource }) {
  return (
    <li>
      <Link className={styles.projectLink} to={projectPath(project.id)}>
        <span className={styles.projectIcon} aria-hidden="true">
          <FolderTree size={20} strokeWidth={1.7} />
        </span>
        <span className={styles.projectSummary}>
          <strong>{project.name}</strong>
          <span>{project.id}</span>
        </span>
        <ChevronRight aria-hidden="true" size={18} />
      </Link>
    </li>
  );
}

function ProjectBreadcrumbs({ details }: { details: ProjectDetails }) {
  const trail = [...details.ancestors, details.project];
  return (
    <nav aria-label="Project breadcrumb" className={styles.breadcrumbs}>
      <ol>
        <li>
          <Link to={CONSOLE_PATHS.projects}>Projects</Link>
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
  return (
    <header className={styles.pageHeading}>
      <p className={styles.eyebrow}>Project hierarchy</p>
      <h1 id="page-title">{title}</h1>
      <p>{description}</p>
    </header>
  );
}

function ProjectLoading() {
  return (
    <section aria-labelledby="page-title" className={styles.page}>
      <PageHeading description="Loading the selected Project and its ancestry." title="Project" />
      <LoadingPanel />
    </section>
  );
}

function LoadingPanel() {
  return (
    <div aria-live="polite" className={styles.statePanel} role="status">
      <span className={styles.loadingMark} aria-hidden="true" />
      Loading Projects…
    </div>
  );
}

function ProjectNotFound() {
  return (
    <section aria-labelledby="page-title" className={styles.page}>
      <PageHeading
        description="The Project does not exist or is not visible from this management context."
        title="Project not found"
      />
      <Link className={styles.primaryLink} to={CONSOLE_PATHS.projects}>
        Return to Projects
      </Link>
    </section>
  );
}

function ProjectFailure({ error, onRetry }: { error: Error | null; onRetry: () => void }) {
  const requestId = error instanceof ManagementApiError ? error.requestId : null;
  return (
    <div className={styles.failurePanel} role="alert">
      <AlertTriangle aria-hidden="true" size={20} />
      <div>
        <strong>Projects could not be loaded.</strong>
        <p>
          Try the request again.
          {requestId === null ? null : ` Request ID: ${requestId}`}
        </p>
        <button className={styles.textButton} onClick={onRetry} type="button">
          Retry
        </button>
      </div>
    </div>
  );
}

function StaleNotice() {
  return (
    <p className={styles.staleNotice} role="status">
      <AlertTriangle aria-hidden="true" size={16} />
      Refresh failed. Showing the last loaded Project data.
    </p>
  );
}

function isNotFound(error: Error | null): boolean {
  return error instanceof ManagementApiError && error.code === 'not_found';
}
