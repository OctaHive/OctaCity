import { useInfiniteQuery, useQuery, type InfiniteData } from '@tanstack/react-query';
import { ChevronDown, ChevronRight, FolderTree, RefreshCw } from 'lucide-react';
import { useState } from 'react';
import { Link, matchPath, useLocation } from 'react-router-dom';

import { queryKeys } from '../../app/query';
import { CONSOLE_PATHS, projectPath } from '../../app/routes';
import type { ProjectDetails, ProjectHierarchyApi, ProjectPage, ProjectResource } from './api';
import { ProjectDataFailure, ProjectDataStale } from './ProjectDataState';
import styles from './ProjectExplorer.module.css';

interface ProjectExplorerProps {
  api: ProjectHierarchyApi;
}

type ExpansionOverrides = ReadonlyMap<string, boolean>;
type RevealedChildren = ReadonlyMap<string | null, ProjectResource>;

/** Renders the bounded server-owned Project hierarchy in the contextual explorer. */
export function ProjectExplorer({ api }: ProjectExplorerProps) {
  const location = useLocation();
  const [expansionOverrides, setExpansionOverrides] = useState<ExpansionOverrides>(() => new Map());
  const selectedProjectId =
    matchPath(CONSOLE_PATHS.project, location.pathname)?.params.projectId ?? null;
  const toggleExpanded = (projectId: string, currentlyExpanded: boolean) => {
    setExpansionOverrides((current) => {
      const next = new Map(current);
      next.set(projectId, !currentlyExpanded);
      return next;
    });
  };

  return selectedProjectId === null ? (
    <ProjectTree
      api={api}
      expansionOverrides={expansionOverrides}
      onToggle={toggleExpanded}
      revealedProjects={[]}
      selectedProjectId={null}
    />
  ) : (
    <SelectedProjectTree
      api={api}
      expansionOverrides={expansionOverrides}
      onToggle={toggleExpanded}
      selectedProjectId={selectedProjectId}
    />
  );
}

function SelectedProjectTree({
  api,
  expansionOverrides,
  onToggle,
  selectedProjectId,
}: ProjectExplorerProps & {
  expansionOverrides: ExpansionOverrides;
  onToggle: (projectId: string, currentlyExpanded: boolean) => void;
  selectedProjectId: string;
}) {
  const selectedProject = useQuery({
    queryFn: ({ signal }) => api.getProject(selectedProjectId, signal),
    queryKey: queryKeys.project(selectedProjectId),
  });
  const revealedProjects =
    selectedProject.data === undefined ? [] : projectTrail(selectedProject.data);

  return (
    <ProjectTree
      api={api}
      expansionOverrides={expansionOverrides}
      onToggle={onToggle}
      revealedProjects={revealedProjects}
      selectedProjectId={selectedProjectId}
    />
  );
}

function ProjectTree({
  api,
  expansionOverrides,
  onToggle,
  revealedProjects,
  selectedProjectId,
}: ProjectExplorerProps & {
  expansionOverrides: ExpansionOverrides;
  onToggle: (projectId: string, currentlyExpanded: boolean) => void;
  revealedProjects: readonly ProjectResource[];
  selectedProjectId: string | null;
}) {
  const revealedIds = new Set(revealedProjects.map(({ id }) => id));
  const revealedChildren = indexRevealedChildren(revealedProjects);

  return (
    <ProjectBranch
      api={api}
      depth={0}
      expansionOverrides={expansionOverrides}
      onToggle={onToggle}
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
  expansionOverrides,
  onToggle,
  parentId,
  parentLabel,
  revealedChildren,
  revealedIds,
  selectedProjectId,
}: ProjectExplorerProps & {
  depth: number;
  expansionOverrides: ExpansionOverrides;
  onToggle: (projectId: string, currentlyExpanded: boolean) => void;
  parentId: string | null;
  parentLabel: string | null;
  revealedChildren: RevealedChildren;
  revealedIds: ReadonlySet<string>;
  selectedProjectId: string | null;
}) {
  const projects = useInfiniteQuery<
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
      {items.length === 0 ? (
        <p className={styles.empty}>
          {parentLabel === null ? 'No root Projects are available.' : 'No child Projects.'}
        </p>
      ) : (
        <ul aria-label={depth === 0 ? listLabel : undefined} role={depth === 0 ? 'tree' : 'group'}>
          {items.map((project) => {
            const expanded = expansionOverrides.get(project.id) ?? revealedIds.has(project.id);
            const selected = project.id === selectedProjectId;
            return (
              <li aria-expanded={expanded} key={project.id} role="treeitem">
                <div
                  className={
                    selected ? `${styles.projectRow} ${styles.selected}` : styles.projectRow
                  }
                >
                  <button
                    aria-label={`${expanded ? 'Collapse' : 'Expand'} ${project.name}`}
                    onClick={() => onToggle(project.id, expanded)}
                    type="button"
                  >
                    {expanded ? (
                      <ChevronDown aria-hidden="true" size={15} />
                    ) : (
                      <ChevronRight aria-hidden="true" size={15} />
                    )}
                  </button>
                  <FolderTree aria-hidden="true" size={15} />
                  <Link aria-current={selected ? 'page' : undefined} to={projectPath(project.id)}>
                    {project.name}
                  </Link>
                </div>
                {expanded ? (
                  <ProjectBranch
                    api={api}
                    depth={depth + 1}
                    expansionOverrides={expansionOverrides}
                    onToggle={onToggle}
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
      )}
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

function projectTrail(details: ProjectDetails): readonly ProjectResource[] {
  return [...details.ancestors, details.project];
}

function indexRevealedChildren(projects: readonly ProjectResource[]): RevealedChildren {
  return new Map(projects.map((project) => [project.parent_id, project]));
}
