import { useInfiniteQuery, useQuery, type InfiniteData } from '@tanstack/react-query';
import { ChevronDown, ChevronRight, FolderTree, RefreshCw } from 'lucide-react';
import { Link, matchPath, useLocation } from 'react-router-dom';

import { queryKeys } from '../../app/query';
import { CONSOLE_PATHS, projectPath } from '../../app/routes';
import { useExpansionOverrides } from '../../shared/useExpansionOverrides';
import type { ProjectDetails, ProjectHierarchyApi, ProjectPage, ProjectSummary } from './api';
import { ProjectDataFailure, ProjectDataStale } from './ProjectDataState';
import styles from './ProjectExplorer.module.css';

interface ProjectExplorerProps {
  api: ProjectHierarchyApi;
}

type RevealedChildren = ReadonlyMap<string | null, ProjectSummary>;

/** Renders the bounded server-owned Project hierarchy in the contextual explorer. */
export function ProjectExplorer({ api }: ProjectExplorerProps) {
  const location = useLocation();
  const expansion = useExpansionOverrides();
  const selectedProjectId =
    matchPath(CONSOLE_PATHS.project, location.pathname)?.params.projectId ?? null;

  return selectedProjectId === null ? (
    <ProjectTree api={api} expansion={expansion} revealedProjects={[]} selectedProjectId={null} />
  ) : (
    <SelectedProjectTree api={api} expansion={expansion} selectedProjectId={selectedProjectId} />
  );
}

function SelectedProjectTree({
  api,
  expansion,
  selectedProjectId,
}: ProjectExplorerProps & {
  expansion: ReturnType<typeof useExpansionOverrides>;
  selectedProjectId: string;
}) {
  const selectedProject = useQuery({
    queryFn: ({ signal }) => api.getProject(selectedProjectId, signal),
    queryKey: queryKeys.project(selectedProjectId),
  });
  const selectedChildren = useProjectPage(api, selectedProjectId);
  const revealedProjects =
    selectedProject.data === undefined
      ? []
      : projectTrail(
          selectedProject.data,
          (selectedChildren.data?.pages[0]?.items.length ?? 0) > 0,
        );

  return (
    <ProjectTree
      api={api}
      expansion={expansion}
      revealedProjects={revealedProjects}
      selectedProjectId={selectedProjectId}
    />
  );
}

function ProjectTree({
  api,
  expansion,
  revealedProjects,
  selectedProjectId,
}: ProjectExplorerProps & {
  expansion: ReturnType<typeof useExpansionOverrides>;
  revealedProjects: readonly ProjectSummary[];
  selectedProjectId: string | null;
}) {
  const revealedIds = new Set(revealedProjects.map(({ id }) => id));
  const revealedChildren = indexRevealedChildren(revealedProjects);

  return (
    <ProjectBranch
      api={api}
      depth={0}
      expansion={expansion}
      parentId={null}
      parentLabel={null}
      revealedChildren={revealedChildren}
      revealedIds={revealedIds}
      selectedProjectId={selectedProjectId}
    />
  );
}

function ProjectBranch({
  api,
  depth,
  expansion,
  parentId,
  parentLabel,
  revealedChildren,
  revealedIds,
  selectedProjectId,
}: ProjectExplorerProps & {
  depth: number;
  expansion: ReturnType<typeof useExpansionOverrides>;
  parentId: string | null;
  parentLabel: string | null;
  revealedChildren: RevealedChildren;
  revealedIds: ReadonlySet<string>;
  selectedProjectId: string | null;
}) {
  const projects = useProjectPage(api, parentId);

  if (projects.isPending) {
    return <p className={styles.state}>Loading Projects…</p>;
  }
  if (projects.data === undefined) {
    return (
      <ProjectDataFailure
        compact
        error={projects.error}
        label={parentLabel === null ? 'Projects' : `${parentLabel} children`}
        onRetry={projects.refetch}
      />
    );
  }

  const pageItems = projects.data.pages.flatMap((page) => page.items);
  const revealedChild = revealedChildren.get(parentId);
  const items =
    revealedChild === undefined || pageItems.some(({ id }) => id === revealedChild.id)
      ? pageItems
      : [...pageItems, revealedChild];
  const listLabel = parentLabel === null ? 'Project hierarchy' : `${parentLabel} child Projects`;

  return (
    <div className={depth === 0 ? styles.rootBranch : styles.childBranch}>
      {depth === 0 ? (
        <div className={styles.treeToolbar}>
          <span>Project hierarchy</span>
          <button
            aria-label="Refresh Project hierarchy"
            disabled={projects.isFetching}
            onClick={() => void projects.refetch()}
            type="button"
          >
            <RefreshCw aria-hidden="true" size={14} />
          </button>
        </div>
      ) : null}
      {projects.error === null ? null : (
        <ProjectDataStale
          label="Project hierarchy"
          onRetry={projects.isFetchNextPageError ? projects.fetchNextPage : projects.refetch}
        />
      )}
      {items.length === 0 && parentLabel === null ? (
        <p className={styles.empty}>No root Projects are available.</p>
      ) : items.length > 0 ? (
        <ul aria-label={depth === 0 ? listLabel : undefined} role={depth === 0 ? 'tree' : 'group'}>
          {items.map((project) => {
            const expanded =
              project.has_children && expansion.isExpanded(project.id, revealedIds.has(project.id));
            const selected = project.id === selectedProjectId;
            return (
              <li
                aria-expanded={project.has_children ? expanded : undefined}
                key={project.id}
                role="treeitem"
              >
                <div
                  className={
                    selected ? `${styles.projectRow} ${styles.selected}` : styles.projectRow
                  }
                >
                  {project.has_children ? (
                    <button
                      aria-label={`${expanded ? 'Collapse' : 'Expand'} ${project.name}`}
                      onClick={() => expansion.toggle(project.id, expanded)}
                      type="button"
                    >
                      {expanded ? (
                        <ChevronDown aria-hidden="true" size={15} />
                      ) : (
                        <ChevronRight aria-hidden="true" size={15} />
                      )}
                    </button>
                  ) : (
                    <span aria-hidden="true" className={styles.leafIndent} />
                  )}
                  <FolderTree aria-hidden="true" size={15} />
                  <Link aria-current={selected ? 'page' : undefined} to={projectPath(project.id)}>
                    {project.name}
                  </Link>
                </div>
                {project.has_children && expanded ? (
                  <ProjectBranch
                    api={api}
                    depth={depth + 1}
                    expansion={expansion}
                    parentId={project.id}
                    parentLabel={project.name}
                    revealedChildren={revealedChildren}
                    revealedIds={revealedIds}
                    selectedProjectId={selectedProjectId}
                  />
                ) : null}
              </li>
            );
          })}
        </ul>
      ) : null}
      {projects.hasNextPage ? (
        <button
          aria-label={
            parentLabel === null
              ? 'Load more root Projects'
              : `Load more children of ${parentLabel}`
          }
          className={styles.loadMore}
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

function useProjectPage(api: ProjectHierarchyApi, parentId: string | null) {
  return useInfiniteQuery<
    ProjectPage,
    Error,
    InfiniteData<ProjectPage>,
    ReturnType<typeof queryKeys.projectChildren>,
    string | null
  >({
    getNextPageParam: (page) => page.next_cursor ?? undefined,
    initialPageParam: null,
    queryFn: ({ pageParam, signal }) => api.listProjects(parentId, pageParam, signal),
    queryKey: queryKeys.projectChildren(parentId),
  });
}

function projectTrail(
  details: ProjectDetails,
  selectedHasChildren: boolean,
): readonly ProjectSummary[] {
  return [
    ...details.ancestors.map((project) => ({ ...project, has_children: true })),
    { ...details.project, has_children: selectedHasChildren },
  ];
}

function indexRevealedChildren(projects: readonly ProjectSummary[]): RevealedChildren {
  return new Map(projects.map((project) => [project.parent_id, project]));
}
