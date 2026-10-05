import { useQuery } from '@tanstack/react-query';
import { ChevronDown, ChevronRight, FolderKanban, Hammer, Settings2 } from 'lucide-react';
import { Link, matchPath, useLocation } from 'react-router-dom';

import type { components } from '../../../.generated/api/schema';
import { queryKeys } from '../../app/query';
import { buildPath, CONSOLE_PATHS } from '../../app/routes';
import { formatEnumLabel } from '../../shared/display';
import {
  ExplorerBranchFailure as BranchFailure,
  ExplorerBranchLoading as BranchLoading,
  ExplorerBranchBackground as BranchBackground,
  PagedExplorerBranch as PagedBranch,
  useCursorPage,
  type CursorPage,
} from '../../shared/PagedExplorerBranch';
import { useExpansionOverrides } from '../../shared/useExpansionOverrides';
import styles from './BuildExplorer.module.css';

type BuildState = components['schemas']['BuildState'];
type ProjectResource = components['schemas']['ProjectResource'];
type ProjectSelection = components['schemas']['ProjectDetails'];
type ProjectItem = Pick<
  components['schemas']['ProjectSummaryResource'],
  'has_children' | 'id' | 'name' | 'parent_id'
>;
type ConfigurationItem = Pick<
  components['schemas']['BuildConfigurationSummaryResource'],
  'id' | 'name' | 'project_id' | 'version'
>;
type BuildItem = Pick<
  components['schemas']['BuildSummaryResource'],
  'configuration_id' | 'id' | 'project_id' | 'state'
>;
type SelectedBuild = Pick<
  components['schemas']['BuildResource'],
  'configuration_id' | 'configuration_version' | 'id' | 'project_id' | 'state'
>;

/** Bounded reads used by the contextual Build hierarchy and deep-link reveal. */
export interface BuildExplorerApi {
  getBuild(buildId: string, signal?: AbortSignal): Promise<SelectedBuild>;
  getBuildConfiguration(
    configurationId: string,
    version: number,
    signal?: AbortSignal,
  ): Promise<ConfigurationItem>;
  getProject(projectId: string, signal?: AbortSignal): Promise<ProjectSelection>;
  listBuildConfigurations(
    projectId: string,
    cursor: string | null,
    signal?: AbortSignal,
  ): Promise<CursorPage<ConfigurationItem>>;
  listBuilds(
    projectId: string,
    filters: { configurationId: string | null; state: BuildState | null },
    cursor: string | null,
    signal?: AbortSignal,
  ): Promise<CursorPage<BuildItem>>;
  listProjects(
    parentId: string | null,
    cursor: string | null,
    signal?: AbortSignal,
  ): Promise<CursorPage<ProjectItem>>;
}

interface SelectedPath {
  build: BuildItem | null;
  configuration: ConfigurationItem | null;
  projects: readonly ProjectItem[];
}

interface BuildTreeState {
  api: BuildExplorerApi;
  configurationExpansion: ReturnType<typeof useExpansionOverrides>;
  projectExpansion: ReturnType<typeof useExpansionOverrides>;
  revealedChildren: ReadonlyMap<string | null, ProjectItem>;
  revealedProjectIds: ReadonlySet<string>;
  selectedBuildId: string | null;
  selectedPath: SelectedPath;
}

const emptySelectedPath: SelectedPath = { build: null, configuration: null, projects: [] };

/** Renders bounded, independently paginated Project -> Configuration -> Build branches. */
export function BuildExplorer({ api }: { api: BuildExplorerApi }) {
  const location = useLocation();
  const selectedBuildId = matchPath(CONSOLE_PATHS.build, location.pathname)?.params.buildId ?? null;

  return (
    <div className={styles.explorerTree}>
      <p className={styles.boundedNotice}>
        Loaded bounded pages are shown. Use search to find Builds outside expanded branches.
      </p>
      {selectedBuildId === null ? (
        <BuildTree api={api} selectedBuildId={null} selectedPath={emptySelectedPath} />
      ) : (
        <SelectedBuildTree api={api} selectedBuildId={selectedBuildId} />
      )}
    </div>
  );
}

function SelectedBuildTree({
  api,
  selectedBuildId,
}: {
  api: BuildExplorerApi;
  selectedBuildId: string;
}) {
  const selectedBuild = useQuery({
    queryFn: ({ signal }) => api.getBuild(selectedBuildId, signal),
    queryKey: queryKeys.build(selectedBuildId),
  });

  if (selectedBuild.isPending) {
    return (
      <>
        <BranchLoading label="selected Build path" />
        <BuildTree api={api} selectedBuildId={selectedBuildId} selectedPath={emptySelectedPath} />
      </>
    );
  }
  if (selectedBuild.data === undefined) {
    return (
      <>
        <BranchFailure
          error={selectedBuild.error}
          label="Selected Build"
          onRetry={selectedBuild.refetch}
        />
        <BuildTree api={api} selectedBuildId={selectedBuildId} selectedPath={emptySelectedPath} />
      </>
    );
  }

  return (
    <>
      <BranchBackground
        error={selectedBuild.error}
        fetching={selectedBuild.isFetching}
        label="selected Build path"
        onRetry={selectedBuild.refetch}
      />
      <SelectedBuildAncestors api={api} build={selectedBuild.data} />
    </>
  );
}

function SelectedBuildAncestors({ api, build }: { api: BuildExplorerApi; build: SelectedBuild }) {
  const selectedProject = useQuery({
    queryFn: ({ signal }) => api.getProject(build.project_id, signal),
    queryKey: queryKeys.project(build.project_id),
  });
  const selectedConfiguration = useQuery({
    queryFn: ({ signal }) =>
      api.getBuildConfiguration(build.configuration_id, build.configuration_version, signal),
    queryKey: queryKeys.buildConfiguration(build.configuration_id, build.configuration_version),
  });
  const selectedProjectChildren = useProjectPage(api, build.project_id);
  const pending =
    selectedProject.isPending ||
    selectedConfiguration.isPending ||
    selectedProjectChildren.isPending;
  const failed = [
    { label: 'Selected Project', query: selectedProject },
    { label: 'Selected Build Configuration', query: selectedConfiguration },
    { label: 'Selected Project children', query: selectedProjectChildren },
  ].find(({ query }) => !query.isPending && query.data === undefined);
  const stale = [selectedProject, selectedConfiguration, selectedProjectChildren].find(
    (query) => query.error !== null && query.data !== undefined,
  );

  if (pending) {
    return (
      <>
        <BranchLoading label="selected Build path" />
        <BuildTree api={api} selectedBuildId={build.id} selectedPath={emptySelectedPath} />
      </>
    );
  }
  if (failed !== undefined) {
    return (
      <>
        <BranchFailure
          error={failed.query.error}
          label={failed.label}
          onRetry={failed.query.refetch}
        />
        <BuildTree api={api} selectedBuildId={build.id} selectedPath={emptySelectedPath} />
      </>
    );
  }

  const projectDetails = selectedProject.data;
  const configuration = selectedConfiguration.data;
  if (projectDetails === undefined || configuration === undefined) return null;

  const selectedPath: SelectedPath = {
    build: {
      configuration_id: build.configuration_id,
      id: build.id,
      project_id: build.project_id,
      state: build.state,
    },
    configuration,
    projects: projectTrail(
      projectDetails,
      (selectedProjectChildren.data?.pages[0]?.items.length ?? 0) > 0,
    ),
  };
  return (
    <>
      <BranchBackground
        error={stale?.error ?? null}
        fetching={
          selectedProject.isFetching ||
          selectedConfiguration.isFetching ||
          selectedProjectChildren.isFetching
        }
        label="selected Build path"
        onRetry={() => {
          if (stale !== undefined) void stale.refetch();
        }}
      />
      <BuildTree api={api} selectedBuildId={build.id} selectedPath={selectedPath} />
    </>
  );
}

function BuildTree({
  api,
  selectedBuildId,
  selectedPath,
}: {
  api: BuildExplorerApi;
  selectedBuildId: string | null;
  selectedPath: SelectedPath;
}) {
  const projectExpansion = useExpansionOverrides();
  const configurationExpansion = useExpansionOverrides();
  const revealedChildren = new Map(
    selectedPath.projects.map((project) => [project.parent_id, project] as const),
  );
  const revealedProjectIds = new Set(selectedPath.projects.map(({ id }) => id));
  const tree: BuildTreeState = {
    api,
    configurationExpansion,
    projectExpansion,
    revealedChildren,
    revealedProjectIds,
    selectedBuildId,
    selectedPath,
  };

  return <ProjectBranch parentId={null} parentLabel={null} tree={tree} />;
}

function ProjectBranch({
  parentId,
  parentLabel,
  tree,
}: {
  parentId: string | null;
  parentLabel: string | null;
  tree: BuildTreeState;
}) {
  const { api, projectExpansion, revealedChildren, revealedProjectIds } = tree;
  const projects = useProjectPage(api, parentId);
  const branchLabel = parentLabel === null ? 'Projects' : `${parentLabel} child Projects`;

  return (
    <PagedBranch<ProjectItem>
      className={parentId === null ? undefined : styles.childBranch}
      empty={
        parentLabel === null
          ? 'No Project branches are available.'
          : `No child Projects in ${parentLabel}.`
      }
      labels={{
        failure: branchLabel,
        loading: branchLabel,
        loadMore:
          parentLabel === null
            ? 'Load more Projects'
            : `Load more child Projects in ${parentLabel}`,
        loadingMore:
          parentLabel === null ? 'Loading Projects' : `Loading child Projects in ${parentLabel}`,
        stale: branchLabel,
      }}
      query={projects}
      revealed={revealedChildren.get(parentId)}
    >
      {(items) => (
        <ul
          aria-label={parentId === null ? 'Build hierarchy' : undefined}
          role={parentId === null ? 'tree' : 'group'}
        >
          {items.map((project) => {
            const expanded = projectExpansion.isExpanded(
              project.id,
              revealedProjectIds.has(project.id),
            );
            return (
              <li aria-expanded={expanded} key={project.id} role="treeitem">
                <TreeRow
                  expanded={expanded}
                  icon={<FolderKanban aria-hidden="true" size={15} />}
                  label={project.name}
                  onToggle={() => projectExpansion.toggle(project.id, expanded)}
                />
                {expanded ? (
                  <>
                    {project.has_children ? (
                      <ProjectBranch parentId={project.id} parentLabel={project.name} tree={tree} />
                    ) : null}
                    <ConfigurationBranch project={project} tree={tree} />
                  </>
                ) : null}
              </li>
            );
          })}
        </ul>
      )}
    </PagedBranch>
  );
}

function ConfigurationBranch({ project, tree }: { project: ProjectItem; tree: BuildTreeState }) {
  const { api, configurationExpansion, selectedPath } = tree;
  const configurations = useCursorPage(
    queryKeys.projectBuildConfigurations(project.id),
    (cursor, signal) => api.listBuildConfigurations(project.id, cursor, signal),
  );
  const selectedProject = selectedPath.projects.at(-1);
  const revealed = selectedProject?.id === project.id ? selectedPath.configuration : null;

  return (
    <PagedBranch<ConfigurationItem>
      className={styles.childBranch}
      empty={`No Build Configurations in ${project.name}.`}
      labels={{
        failure: `${project.name} Build Configurations`,
        loading: `${project.name} configurations`,
        loadMore: `Load more Build Configurations in ${project.name}`,
        loadingMore: `Loading Build Configurations in ${project.name}`,
        stale: `${project.name} Build Configurations`,
      }}
      query={configurations}
      revealed={revealed}
    >
      {(items) => (
        <ul role="group">
          {items.map((configuration) => {
            const selected = selectedPath.configuration?.id === configuration.id;
            const expanded = configurationExpansion.isExpanded(configuration.id, selected);
            return (
              <li aria-expanded={expanded} key={configuration.id} role="treeitem">
                <TreeRow
                  expanded={expanded}
                  icon={<Settings2 aria-hidden="true" size={14} />}
                  label={configuration.name}
                  meta={`v${configuration.version}`}
                  onToggle={() => configurationExpansion.toggle(configuration.id, expanded)}
                />
                {expanded ? (
                  <BuildBranch configuration={configuration} project={project} tree={tree} />
                ) : null}
              </li>
            );
          })}
        </ul>
      )}
    </PagedBranch>
  );
}

function BuildBranch({
  configuration,
  project,
  tree,
}: {
  configuration: ConfigurationItem;
  project: ProjectItem;
  tree: BuildTreeState;
}) {
  const { api, selectedBuildId, selectedPath } = tree;
  const builds = useCursorPage(
    queryKeys.projectBuilds(project.id, configuration.id, null),
    (cursor, signal) =>
      api.listBuilds(
        project.id,
        { configurationId: configuration.id, state: null },
        cursor,
        signal,
      ),
  );
  const revealed = selectedPath.configuration?.id === configuration.id ? selectedPath.build : null;

  return (
    <PagedBranch<BuildItem>
      className={styles.childBranch}
      empty={`No Builds for ${configuration.name}.`}
      labels={{
        failure: `${configuration.name} Builds`,
        loading: `${configuration.name} Builds`,
        loadMore: `Load more Builds for ${configuration.name}`,
        loadingMore: `Loading Builds for ${configuration.name}`,
        stale: `${configuration.name} Builds`,
      }}
      query={builds}
      revealed={revealed}
    >
      {(items) => (
        <ul role="group">
          {items.map((build) => {
            const selected = build.id === selectedBuildId;
            return (
              <li key={build.id} role="treeitem">
                <div
                  className={selected ? `${styles.buildRow} ${styles.selected}` : styles.buildRow}
                >
                  <Hammer aria-hidden="true" size={14} />
                  <Link aria-current={selected ? 'page' : undefined} to={buildPath(build.id)}>
                    {build.id}
                  </Link>
                  <span className={styles.buildState} data-state={build.state}>
                    {formatEnumLabel(build.state)}
                  </span>
                </div>
              </li>
            );
          })}
        </ul>
      )}
    </PagedBranch>
  );
}

function TreeRow({
  expanded,
  icon,
  label,
  meta,
  onToggle,
}: {
  expanded: boolean;
  icon: React.ReactNode;
  label: string;
  meta?: string;
  onToggle: () => void;
}) {
  return (
    <div className={styles.branchRow}>
      <button
        aria-label={`${expanded ? 'Collapse' : 'Expand'} ${label}`}
        onClick={onToggle}
        type="button"
      >
        {expanded ? (
          <ChevronDown aria-hidden="true" size={15} />
        ) : (
          <ChevronRight aria-hidden="true" size={15} />
        )}
      </button>
      {icon}
      <span title={label}>{label}</span>
      {meta === undefined ? null : <small>{meta}</small>}
    </div>
  );
}

function useProjectPage(api: BuildExplorerApi, parentId: string | null) {
  return useCursorPage(queryKeys.projectChildren(parentId), (cursor, signal) =>
    api.listProjects(parentId, cursor, signal),
  );
}

function projectTrail(details: ProjectSelection, selectedHasChildren: boolean): ProjectItem[] {
  return [
    ...details.ancestors.map((project) => projectSummary(project, true)),
    projectSummary(details.project, selectedHasChildren),
  ];
}

function projectSummary(project: ProjectResource, hasChildren: boolean): ProjectItem {
  return {
    has_children: hasChildren,
    id: project.id,
    name: project.name,
    parent_id: project.parent_id,
  };
}
