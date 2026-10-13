import { ChevronDown, ChevronRight, Factory } from 'lucide-react';
import { useQuery } from '@tanstack/react-query';
import { Link, matchPath, useLocation } from 'react-router-dom';
import { queryKeys } from '../../app/query';
import { usePresentation } from '../../app/presentation/PresentationProvider';
import type { ProjectHierarchyApi, ProjectSummary } from '../../shared/projectHierarchy';
import {
  PagedExplorerBranch,
  useCursorPage,
  ExplorerBranchFailure,
} from '../../shared/PagedExplorerBranch';
import { useExpansionOverrides } from '../../shared/useExpansionOverrides';
import type { FactoryApi, FactoryConfiguration, FactoryFilters, FactoryRun } from './api';
import { factorySearch, readFactoryFilters, readFactoryProjectId } from './filters';
import styles from './Factory.module.css';
import { FactoryFiltersPanel } from './FactoryFilters';

export interface FactoryExplorerApi extends Pick<
  FactoryApi,
  'listRuns' | 'listConfigurations' | 'getRun'
> {
  getConfiguration(
    id: string,
    version: number,
    signal?: AbortSignal,
  ): Promise<{
    configuration: {
      reference: { id: string; project_id: string; version: number };
      enabled: boolean;
    };
    published_at_unix_ms: number;
  }>;
}
interface Props {
  api: FactoryExplorerApi;
  projects: ProjectHierarchyApi;
}

/** Lazy, cached server pages. Control labels never infer Flow transitions. */
export function FactoryExplorer(props: Props) {
  const { t } = usePresentation();
  const location = useLocation();
  const filters = readFactoryFilters(new URLSearchParams(location.search));
  const expansion = useExpansionOverrides('project');
  const configurationExpansion = useExpansionOverrides('factory_configuration');
  const selectedId =
    matchPath('/factory/:factoryRunId', location.pathname)?.params.factoryRunId ?? null;
  const projectId =
    selectedId === null ? readFactoryProjectId(new URLSearchParams(location.search)) : null;
  const selected = useQuery({
    queryKey: queryKeys.factoryRun(selectedId),
    queryFn: ({ signal }) => props.api.getRun(selectedId!, signal),
    enabled: selectedId !== null,
    refetchOnMount: false,
  });
  const project = useQuery({
    queryKey: queryKeys.project(selected.data?.project_id ?? projectId ?? ''),
    queryFn: ({ signal }) =>
      props.projects.getProject(selected.data?.project_id ?? projectId!, signal),
    enabled: selected.data !== undefined || projectId !== null,
    refetchOnMount: false,
  });
  const configuration = useQuery({
    queryKey: queryKeys.factoryConfiguration(
      selected.data?.configuration_id ?? null,
      selected.data?.configuration_version ?? null,
    ),
    queryFn: ({ signal }) =>
      props.api.getConfiguration(
        selected.data!.configuration_id,
        selected.data!.configuration_version,
        signal,
      ),
    enabled: selected.data !== undefined,
    refetchOnMount: false,
  });
  const selectedChildren = useCursorPage(
    queryKeys.projectChildren(project.data?.project.id ?? null),
    (cursor, signal) =>
      props.projects.listProjects(project.data?.project.id ?? null, cursor, signal),
    { enabled: project.data !== undefined, refetchStaleOnMount: false },
  );
  const revealedProjects =
    project.data === undefined
      ? []
      : [
          ...project.data.ancestors.map((item) => ({ ...item, has_children: true })),
          {
            ...project.data.project,
            has_children:
              (selectedChildren.data?.pages.some((page) => page.items.length > 0) ?? false) ||
              selectedChildren.hasNextPage,
          },
        ];
  const revealedConfiguration =
    configuration.data === undefined
      ? null
      : {
          ...configuration.data.configuration.reference,
          enabled: configuration.data.configuration.enabled,
          created_at_unix_ms: configuration.data.published_at_unix_ms,
          published_at_unix_ms: configuration.data.published_at_unix_ms,
        };
  return (
    <>
      <FactoryFiltersPanel />
      <div className={styles.tree} role="tree" aria-label={t('factory.hierarchy')}>
        {[
          { key: 'run', query: selected },
          { key: 'project', query: project },
          { key: 'configuration', query: configuration },
          { key: 'children', query: selectedChildren },
        ].map(({ query, key }) =>
          query.isError && query.data === undefined ? (
            <ExplorerBranchFailure
              key={key}
              error={query.error}
              label={t('section.factory')}
              onRetry={query.refetch}
            />
          ) : null,
        )}
        <Projects
          {...props}
          parent={null}
          filters={filters}
          expansion={expansion}
          configurationExpansion={configurationExpansion}
          revealedProjects={revealedProjects}
          revealedConfiguration={revealedConfiguration}
          selectedRun={selected.data ?? null}
        />
      </div>
    </>
  );
}

type BranchProps = Props & {
  filters: FactoryFilters;
  expansion: ReturnType<typeof useExpansionOverrides>;
  configurationExpansion: ReturnType<typeof useExpansionOverrides>;
  revealedProjects: ProjectSummary[];
  revealedConfiguration: FactoryConfiguration | null;
  selectedRun: FactoryRun | null;
};

function Projects({ parent, ...props }: BranchProps & { parent: string | null }) {
  const { t } = usePresentation();
  const query = useCursorPage(
    queryKeys.projectChildren(parent),
    (cursor, signal) => props.projects.listProjects(parent, cursor, signal),
    { refetchStaleOnMount: false },
  );
  return (
    <PagedExplorerBranch
      query={query}
      revealed={props.revealedProjects.find((item) => item.parent_id === parent)}
      labels={labels(t('explorer.projects'), t('explorer.loadMoreProjects'))}
      empty={t('explorer.noRootProjects')}
    >
      {(items) => (
        <ul role={parent === null ? undefined : 'group'}>
          {items.map((project) => (
            <Project key={project.id} project={project} {...props} />
          ))}
        </ul>
      )}
    </PagedExplorerBranch>
  );
}

function Project({ project, ...props }: BranchProps & { project: ProjectSummary }) {
  const expanded = props.expansion.isExpanded(
    project.id,
    props.revealedProjects.some((item) => item.id === project.id),
  );
  return (
    <li role="treeitem" aria-expanded={expanded}>
      <BranchToggle
        name={project.name}
        expanded={expanded}
        toggle={() => props.expansion.toggle(project.id, expanded)}
      />
      <Link
        to={`/factory${factorySearch(props.filters)}${Object.keys(props.filters).length ? '&' : '?'}project_id=${encodeURIComponent(project.id)}`}
      >
        {project.name}
      </Link>
      {expanded ? (
        <div className={styles.child}>
          {project.has_children ? <Projects parent={project.id} {...props} /> : null}
          <Configurations projectId={project.id} {...props} />
        </div>
      ) : null}
    </li>
  );
}

function Configurations({ projectId, ...props }: BranchProps & { projectId: string }) {
  const { t } = usePresentation();
  const query = useCursorPage(
    queryKeys.factoryConfigurations(projectId),
    (cursor, signal) => props.api.listConfigurations(projectId, cursor, signal),
    { refetchStaleOnMount: false },
  );
  return (
    <PagedExplorerBranch
      query={query}
      revealed={
        props.revealedConfiguration?.project_id === projectId ? props.revealedConfiguration : null
      }
      labels={labels(t('factory.configurations'), t('factory.moreConfigurations'))}
      empty={t('factory.unconfigured')}
    >
      {(items) => (
        <ul role="group">
          {items
            .filter(
              (item) =>
                props.filters.configuration_id === undefined ||
                item.id === props.filters.configuration_id,
            )
            .map((configuration) => (
              <Configuration key={configuration.id} configuration={configuration} {...props} />
            ))}
        </ul>
      )}
    </PagedExplorerBranch>
  );
}

function Configuration({
  configuration,
  ...props
}: BranchProps & { configuration: FactoryConfiguration }) {
  const expansion = props.configurationExpansion;
  const expanded = expansion.isExpanded(
    configuration.id,
    props.selectedRun?.configuration_id === configuration.id,
  );
  const { t } = usePresentation();
  return (
    <li role="treeitem" aria-expanded={expanded}>
      <BranchToggle
        name={t('factory.configurationVersion', { version: configuration.version })}
        expanded={expanded}
        toggle={() => expansion.toggle(configuration.id, expanded)}
      />
      {expanded ? (
        <div className={styles.child}>
          <Runs
            projectId={configuration.project_id}
            {...props}
            filters={{ ...props.filters, configuration_id: configuration.id }}
          />
        </div>
      ) : null}
    </li>
  );
}

function Runs({ projectId, ...props }: BranchProps & { projectId: string }) {
  const { t } = usePresentation();
  const location = useLocation();
  const query = useCursorPage(
    queryKeys.factoryRuns(projectId, props.filters),
    (cursor, signal) => props.api.listRuns(projectId, props.filters, cursor, signal),
    { refetchStaleOnMount: false },
  );
  return (
    <PagedExplorerBranch
      query={query}
      revealed={
        props.selectedRun?.configuration_id === props.filters.configuration_id
          ? props.selectedRun
          : null
      }
      labels={labels(t('section.factory'), t('factory.moreRuns'))}
      empty={t('factory.noRuns')}
    >
      {(items) => (
        <ul role="group">
          {items.map((run) => (
            <li role="treeitem" key={run.id} className={styles.run}>
              <Link
                aria-current={location.pathname === `/factory/${run.id}` ? 'page' : undefined}
                to={`/factory/${encodeURIComponent(run.id)}${factorySearch(props.filters)}`}
              >
                {run.external_identity}
              </Link>
              <small>
                {run.state} · {run.diagnostics.current_nodes.map((node) => node.node).join(', ')}
              </small>
              {run.diagnostics.latest_decision ? (
                <small>
                  {t('factory.decision')}: {run.diagnostics.latest_decision.outcome}
                </small>
              ) : null}
              {run.diagnostics.escalation ? <small>{t('factory.escalation')}</small> : null}
              {run.diagnostics.delivery ? (
                <small>
                  {t('factory.delivery')}: {run.diagnostics.delivery.state}
                </small>
              ) : null}
            </li>
          ))}
        </ul>
      )}
    </PagedExplorerBranch>
  );
}

function BranchToggle({
  name,
  expanded,
  toggle,
}: {
  name: string;
  expanded: boolean;
  toggle: () => void;
}) {
  const { t } = usePresentation();
  return (
    <div className={styles.branch}>
      <button
        type="button"
        aria-label={t(expanded ? 'explorer.collapse' : 'explorer.expand', { name })}
        onClick={toggle}
      >
        {expanded ? <ChevronDown size={15} /> : <ChevronRight size={15} />}
      </button>
      <Factory aria-hidden="true" size={15} />
      <span>{name}</span>
    </div>
  );
}

function labels(loading: string, loadMore: string) {
  return { loading, loadMore, failure: loading, stale: loading, loadingMore: loading };
}
