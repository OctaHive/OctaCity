import { useQuery } from '@tanstack/react-query';
import { ChevronDown, ChevronRight, FolderKanban, Hammer, Settings2 } from 'lucide-react';
import { Link, matchPath, useLocation } from 'react-router-dom';

import type { components } from '../../../.generated/api/schema';
import { queryKeys } from '../../app/query';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import { buildPath, CONSOLE_PATHS } from '../../app/routes';
import { formatEnumLabel } from '../../shared/display';
import {
  ExplorerBranchFailure as BranchFailure,
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

interface SelectedPathFailure {
  error: Error | null;
  label: string;
  onRetry: () => unknown;
}

interface SelectedPathBackground extends SelectedPathFailure {
  fetching: boolean;
}

interface SelectedPathState {
  background: SelectedPathBackground | null;
  failure: SelectedPathFailure | null;
  path: SelectedPath;
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
  const { t } = usePresentation();
  const location = useLocation();
  const selectedBuildId = matchPath(CONSOLE_PATHS.build, location.pathname)?.params.buildId ?? null;
  const selectedPath = useSelectedBuildPath(api, selectedBuildId);

  return (
    <div className={styles.explorerTree}>
      <p className={styles.boundedNotice}>{t('explorer.boundedBuilds')}</p>
      {selectedPath.failure === null ? null : <BranchFailure {...selectedPath.failure} />}
      {selectedPath.background === null ? null : <BranchBackground {...selectedPath.background} />}
      <BuildTree api={api} selectedBuildId={selectedBuildId} selectedPath={selectedPath.path} />
    </div>
  );
}

function useSelectedBuildPath(
  api: BuildExplorerApi,
  selectedBuildId: string | null,
): SelectedPathState {
  const { t } = usePresentation();
  const selectedBuild = useQuery({
    enabled: selectedBuildId !== null,
    queryFn: ({ signal }) => api.getBuild(requireSelection(selectedBuildId), signal),
    queryKey:
      selectedBuildId === null
        ? queryKeys.disabledExplorerDetail('build')
        : queryKeys.build(selectedBuildId),
  });
  const build = selectedBuild.data;
  const selectedProject = useQuery({
    enabled: build !== undefined,
    queryFn: ({ signal }) => api.getProject(requireBuild(build).project_id, signal),
    queryKey:
      build === undefined
        ? queryKeys.disabledExplorerDetail('project')
        : queryKeys.project(build.project_id),
  });
  const selectedConfiguration = useQuery({
    queryFn: ({ signal }) =>
      api.getBuildConfiguration(
        requireBuild(build).configuration_id,
        requireBuild(build).configuration_version,
        signal,
      ),
    enabled: build !== undefined,
    queryKey:
      build === undefined
        ? queryKeys.disabledExplorerDetail('build-configuration')
        : queryKeys.buildConfiguration(build.configuration_id, build.configuration_version),
  });
  const selectedProjectChildren = useProjectPage(
    api,
    build?.project_id ?? null,
    build !== undefined,
  );

  if (selectedBuildId === null || selectedBuild.isPending) {
    return { background: null, failure: null, path: emptySelectedPath };
  }
  if (build === undefined) {
    return {
      background: null,
      failure: {
        error: selectedBuild.error,
        label: t('explorer.selectedBuild'),
        onRetry: selectedBuild.refetch,
      },
      path: emptySelectedPath,
    };
  }

  const pending =
    selectedProject.isPending ||
    selectedConfiguration.isPending ||
    selectedProjectChildren.isPending;
  const failed = [
    { label: t('explorer.selectedProject'), query: selectedProject },
    { label: t('explorer.selectedBuildConfiguration'), query: selectedConfiguration },
    { label: t('explorer.selectedProjectChildren'), query: selectedProjectChildren },
  ].find(({ query }) => !query.isPending && query.data === undefined);
  const stale = [selectedProject, selectedConfiguration, selectedProjectChildren].find(
    (query) => query.error !== null && query.data !== undefined,
  );

  if (pending) {
    return { background: null, failure: null, path: emptySelectedPath };
  }
  if (failed !== undefined) {
    return {
      background: null,
      failure: {
        error: failed.query.error,
        label: failed.label,
        onRetry: failed.query.refetch,
      },
      path: emptySelectedPath,
    };
  }

  const projectDetails = selectedProject.data;
  const configuration = selectedConfiguration.data;
  if (projectDetails === undefined || configuration === undefined) {
    return { background: null, failure: null, path: emptySelectedPath };
  }

  const backgroundError = stale?.error ?? selectedBuild.error;
  return {
    background:
      backgroundError === null
        ? null
        : {
            error: backgroundError,
            fetching: false,
            label: t('explorer.selectedBuildPath'),
            onRetry: () => {
              if (stale !== undefined) void stale.refetch();
              else void selectedBuild.refetch();
            },
          },
    failure: null,
    path: {
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
    },
  };
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
  const projectExpansion = useExpansionOverrides('project');
  const configurationExpansion = useExpansionOverrides('build_configuration');
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
  const { t } = usePresentation();
  const { api, projectExpansion, revealedChildren, revealedProjectIds } = tree;
  const projects = useProjectPage(api, parentId);
  const branchLabel =
    parentLabel === null
      ? t('explorer.projects')
      : t('explorer.childProjects', { name: parentLabel });

  return (
    <PagedBranch<ProjectItem>
      className={parentId === null ? undefined : styles.childBranch}
      empty={
        parentLabel === null
          ? t('explorer.noProjectBranches')
          : t('explorer.noChildProjects', { name: parentLabel })
      }
      labels={{
        failure: branchLabel,
        loading: branchLabel,
        loadMore:
          parentLabel === null
            ? t('explorer.loadMoreProjects')
            : t('explorer.loadMoreChildProjects', { name: parentLabel }),
        loadingMore:
          parentLabel === null
            ? t('explorer.loadingProjects')
            : t('explorer.loadingChildProjects', { name: parentLabel }),
        stale: branchLabel,
      }}
      query={projects}
      revealed={revealedChildren.get(parentId)}
    >
      {(items) => (
        <ul
          aria-label={parentId === null ? t('explorer.buildHierarchy') : undefined}
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
  const { t } = usePresentation();
  const { api, configurationExpansion, selectedPath } = tree;
  const configurations = useCursorPage(
    queryKeys.projectBuildConfigurations(project.id),
    (cursor, signal) => api.listBuildConfigurations(project.id, cursor, signal),
    { refetchStaleOnMount: false },
  );
  const selectedProject = selectedPath.projects.at(-1);
  const revealed = selectedProject?.id === project.id ? selectedPath.configuration : null;

  return (
    <PagedBranch<ConfigurationItem>
      className={styles.childBranch}
      empty={t('explorer.noConfigurations', { name: project.name })}
      labels={{
        failure: t('explorer.buildConfigurationsInProject', { name: project.name }),
        loading: t('explorer.buildConfigurationsInProject', { name: project.name }),
        loadMore: t('explorer.loadMoreConfigurations', { name: project.name }),
        loadingMore: t('explorer.loadingConfigurations', { name: project.name }),
        stale: t('explorer.buildConfigurationsInProject', { name: project.name }),
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
  const { t } = usePresentation();
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
    { refetchStaleOnMount: false },
  );
  const revealed = selectedPath.configuration?.id === configuration.id ? selectedPath.build : null;

  return (
    <PagedBranch<BuildItem>
      className={styles.childBranch}
      empty={t('explorer.noBuilds', { name: configuration.name })}
      labels={{
        failure: t('explorer.buildsForConfiguration', { name: configuration.name }),
        loading: t('explorer.buildsForConfiguration', { name: configuration.name }),
        loadMore: t('explorer.loadMoreBuilds', { name: configuration.name }),
        loadingMore: t('explorer.loadingBuilds', { name: configuration.name }),
        stale: t('explorer.buildsForConfiguration', { name: configuration.name }),
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
                    {formatEnumLabel(build.state, t)}
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
  const { t } = usePresentation();
  return (
    <div className={styles.branchRow}>
      <button
        aria-label={t(expanded ? 'explorer.collapse' : 'explorer.expand', { name: label })}
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

function useProjectPage(api: BuildExplorerApi, parentId: string | null, enabled = true) {
  return useCursorPage(
    queryKeys.projectChildren(parentId),
    (cursor, signal) => api.listProjects(parentId, cursor, signal),
    { enabled, refetchStaleOnMount: false },
  );
}

function requireSelection(selectedBuildId: string | null): string {
  if (selectedBuildId === null) throw new Error('selected Build query is disabled');
  return selectedBuildId;
}

function requireBuild(build: SelectedBuild | undefined): SelectedBuild {
  if (build === undefined) throw new Error('selected Build dependency query is disabled');
  return build;
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
