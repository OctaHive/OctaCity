import { useInfiniteQuery, useQuery } from '@tanstack/react-query';
import { GitBranch, SlidersHorizontal, Workflow, Zap, type LucideIcon } from 'lucide-react';
import { useState, type ReactNode } from 'react';

import { queryKeys } from '../../app/query';
import type {
  BuildConfigurationResource,
  BuildConfigurationSummary,
  PipelineResource,
  PipelineSummary,
  ProjectDefinitionsApi,
  RepositoryResource,
  RepositorySummary,
  TriggerDefinitionDetails,
  TriggerDefinitionSummary,
} from './api';
import { formatEnumLabel } from '../../shared/display';
import { ProjectDataFailure, ProjectDataStale } from './ProjectDataState';
import definitionStyles from './ProjectDefinitions.module.css';
import styles from './Projects.module.css';

interface ProjectDefinitionsProps {
  api: ProjectDefinitionsApi;
  projectId: string;
}

interface DefinitionSummary {
  id: string;
  published_at_unix_ms: number;
  version: number;
}

interface DefinitionPage<TItem> {
  items: TItem[];
  next_cursor: string | null;
}

interface DefinitionSectionProps<TItem extends DefinitionSummary, TDetails> {
  detailKey: (item: TItem) => readonly unknown[];
  emptyLabel: string;
  getDetails: (item: TItem, signal: AbortSignal) => Promise<TDetails>;
  getPage: (cursor: string | null, signal: AbortSignal) => Promise<DefinitionPage<TItem>>;
  headingId: string;
  icon: LucideIcon;
  itemLabel: (item: TItem) => string;
  queryKey: readonly unknown[];
  renderDetails: (details: TDetails) => ReactNode;
  renderTags?: (item: TItem) => ReactNode;
  title: string;
}

export function ProjectDefinitions({ api, projectId }: ProjectDefinitionsProps) {
  return (
    <section aria-labelledby="definitions-heading" className={definitionStyles.section}>
      <div className={styles.sectionHeading}>
        <div>
          <p className={styles.eyebrow}>Project workspace</p>
          <h2 id="definitions-heading">Current definitions</h2>
        </div>
      </div>
      <p className={styles.sectionDescription}>
        Browse the current published versions and inspect their immutable details.
      </p>
      <div className={definitionStyles.grid}>
        <DefinitionSection<PipelineSummary, PipelineResource>
          detailKey={(item) => queryKeys.pipeline(item.id, item.version)}
          emptyLabel="No current Pipelines are available."
          getDetails={(item, signal) => api.getPipeline(item.id, item.version, signal)}
          getPage={(cursor, signal) => api.listPipelines(projectId, cursor, signal)}
          headingId="pipelines-heading"
          icon={Workflow}
          itemLabel={(item) => item.name}
          queryKey={queryKeys.projectPipelines(projectId)}
          renderDetails={renderPipelineDetails}
          title="Pipelines"
        />
        <DefinitionSection<RepositorySummary, RepositoryResource>
          detailKey={(item) => queryKeys.repository(item.id, item.version)}
          emptyLabel="No current Repositories are available."
          getDetails={(item, signal) => api.getRepository(item.id, item.version, signal)}
          getPage={(cursor, signal) => api.listRepositories(projectId, cursor, signal)}
          headingId="repositories-heading"
          icon={GitBranch}
          itemLabel={(item) => item.name}
          queryKey={queryKeys.projectRepositories(projectId)}
          renderDetails={renderRepositoryDetails}
          title="Repositories"
        />
        <DefinitionSection<BuildConfigurationSummary, BuildConfigurationResource>
          detailKey={(item) => queryKeys.buildConfiguration(item.id, item.version)}
          emptyLabel="No current Build Configurations are available."
          getDetails={(item, signal) => api.getBuildConfiguration(item.id, item.version, signal)}
          getPage={(cursor, signal) => api.listBuildConfigurations(projectId, cursor, signal)}
          headingId="build-configurations-heading"
          icon={SlidersHorizontal}
          itemLabel={(item) => item.name}
          queryKey={queryKeys.projectBuildConfigurations(projectId)}
          renderDetails={renderBuildConfigurationDetails}
          renderTags={(item) => <EnabledTag enabled={item.enabled} />}
          title="Build Configurations"
        />
        <DefinitionSection<TriggerDefinitionSummary, TriggerDefinitionDetails>
          detailKey={(item) => queryKeys.trigger(item.kind, item.id, item.version)}
          emptyLabel="No current Trigger definitions are available."
          getDetails={(item, signal) => api.getTrigger(item, signal)}
          getPage={(cursor, signal) => api.listTriggers(projectId, cursor, signal)}
          headingId="triggers-heading"
          icon={Zap}
          itemLabel={(item) => `${formatEnumLabel(item.kind)} trigger`}
          queryKey={queryKeys.projectTriggers(projectId)}
          renderDetails={renderTriggerDetails}
          renderTags={(item) => (
            <>
              <span className={definitionStyles.kindTag}>{formatEnumLabel(item.kind)}</span>
              <EnabledTag enabled={item.enabled} />
            </>
          )}
          title="Triggers"
        />
      </div>
    </section>
  );
}

function DefinitionSection<TItem extends DefinitionSummary, TDetails>({
  detailKey,
  emptyLabel,
  getDetails,
  getPage,
  headingId,
  icon: Icon,
  itemLabel,
  queryKey,
  renderDetails,
  renderTags,
  title,
}: DefinitionSectionProps<TItem, TDetails>) {
  const [selected, setSelected] = useState<TItem | null>(null);
  const collection = useInfiniteQuery({
    getNextPageParam: (page: DefinitionPage<TItem>) => page.next_cursor ?? undefined,
    initialPageParam: null as string | null,
    queryFn: ({ pageParam, signal }) => getPage(pageParam, signal),
    queryKey,
  });
  const details = useQuery({
    enabled: selected !== null,
    queryFn: ({ signal }) => getDetails(requireSelection(selected), signal),
    queryKey: selected === null ? [...queryKey, 'detail', null] : detailKey(selected),
  });

  const items = collection.data?.pages.flatMap((page) => page.items) ?? [];
  return (
    <section aria-labelledby={headingId} className={definitionStyles.card}>
      <header className={definitionStyles.header}>
        <span aria-hidden="true" className={definitionStyles.icon}>
          <Icon size={18} strokeWidth={1.8} />
        </span>
        <h3 id={headingId}>{title}</h3>
        {collection.data === undefined ? null : <span>{items.length} loaded</span>}
      </header>
      {collection.isPending ? (
        <DefinitionLoading label={title} />
      ) : collection.data === undefined ? (
        <ProjectDataFailure
          compact
          error={collection.error}
          label={title}
          onRetry={collection.refetch}
        />
      ) : items.length === 0 ? (
        <p className={definitionStyles.empty}>{emptyLabel}</p>
      ) : (
        <ul className={definitionStyles.list}>
          {items.map((item) => {
            const expanded = selected?.id === item.id && selected.version === item.version;
            const label = itemLabel(item);
            return (
              <li key={`${item.id}:${item.version}`}>
                <button
                  aria-label={`${expanded ? 'Hide' : 'View'} ${label} details`}
                  aria-expanded={expanded}
                  className={definitionStyles.itemButton}
                  onClick={() => setSelected(expanded ? null : item)}
                  type="button"
                >
                  <span className={definitionStyles.summary}>
                    <strong>{label}</strong>
                    <span>Version {item.version}</span>
                  </span>
                  <span className={definitionStyles.tags}>{renderTags?.(item)}</span>
                  <span className={definitionStyles.detailsAction}>
                    {expanded ? 'Hide details' : `View ${label} details`}
                  </span>
                </button>
                {expanded ? (
                  <DefinitionDetails
                    details={details.data}
                    error={details.error}
                    isPending={details.isPending}
                    label={label}
                    onRetry={details.refetch}
                    render={renderDetails}
                  />
                ) : null}
              </li>
            );
          })}
        </ul>
      )}
      {collection.error === null || collection.data === undefined ? null : (
        <ProjectDataStale
          label={title}
          onRetry={collection.isFetchNextPageError ? collection.fetchNextPage : collection.refetch}
        />
      )}
      {collection.hasNextPage ? (
        <button
          className={styles.loadMoreButton}
          disabled={collection.isFetchingNextPage}
          onClick={() => void collection.fetchNextPage()}
          type="button"
        >
          {collection.isFetchingNextPage ? `Loading ${title}` : `Load more ${title}`}
        </button>
      ) : null}
    </section>
  );
}

function DefinitionDetails<TDetails>({
  details,
  error,
  isPending,
  label,
  onRetry,
  render,
}: {
  details: TDetails | undefined;
  error: Error | null;
  isPending: boolean;
  label: string;
  onRetry: () => unknown;
  render: (details: TDetails) => ReactNode;
}) {
  if (isPending) {
    return <p className={definitionStyles.detailState}>Loading {label} details…</p>;
  }
  if (details === undefined) {
    return (
      <ProjectDataFailure compact error={error} label={`${label} details`} onRetry={onRetry} />
    );
  }
  return <div className={definitionStyles.details}>{render(details)}</div>;
}

function DefinitionLoading({ label }: { label: string }) {
  return (
    <p aria-live="polite" className={definitionStyles.detailState} role="status">
      Loading {label}…
    </p>
  );
}

function EnabledTag({ enabled }: { enabled: boolean }) {
  return (
    <span className={enabled ? definitionStyles.enabledTag : definitionStyles.disabledTag}>
      {enabled ? 'Enabled' : 'Disabled'}
    </span>
  );
}

function renderPipelineDetails(pipeline: PipelineResource) {
  return (
    <DetailList
      items={[
        ['Jobs', `${pipeline.dag.nodes.length} Jobs`],
        ['Dependencies', String(pipeline.dag.edges.length)],
      ]}
    />
  );
}

function renderRepositoryDetails(repository: RepositoryResource) {
  return (
    <DetailList
      items={[
        ['Locator', repository.definition.repository_locator],
        ['Default reference', repository.definition.selection.default_reference ?? 'None'],
        ['Allowed references', String(repository.definition.selection.allowed_references.length)],
      ]}
    />
  );
}

function renderBuildConfigurationDetails(configuration: BuildConfigurationResource) {
  return (
    <DetailList
      items={[
        [
          'Pipeline',
          `${configuration.definition.pipeline_id} v${configuration.definition.pipeline_version}`,
        ],
        [
          'Repository',
          `${configuration.definition.repository_id} v${configuration.definition.repository_version}`,
        ],
        ['Job concurrency', String(configuration.definition.job_concurrency_limit)],
      ]}
    />
  );
}

function renderTriggerDetails(details: TriggerDefinitionDetails) {
  if (details.kind === 'scheduled') {
    return (
      <DetailList
        items={[
          ['Schedule', details.resource.schedule.expression],
          ['Timezone', details.resource.schedule.timezone],
          ['Build Configuration', formatConfiguration(details.resource)],
        ]}
      />
    );
  }
  if (details.kind === 'internal') {
    return (
      <DetailList
        items={[
          ['Outcome', formatEnumLabel(details.resource.outcome)],
          [
            'Upstream Configuration',
            `${details.resource.upstream_configuration_id} v${details.resource.upstream_configuration_version}`,
          ],
          ['Build Configuration', formatConfiguration(details.resource)],
        ]}
      />
    );
  }
  return <DetailList items={[['Build Configuration', formatConfiguration(details.resource)]]} />;
}

function DetailList({ items }: { items: [string, string][] }) {
  return (
    <dl>
      {items.map(([term, value]) => (
        <div key={term}>
          <dt>{term}</dt>
          <dd>{value}</dd>
        </div>
      ))}
    </dl>
  );
}

function formatConfiguration(resource: {
  configuration_id: string;
  configuration_version: number;
}) {
  return `${resource.configuration_id} v${resource.configuration_version}`;
}

function requireSelection<TItem>(selection: TItem | null): TItem {
  if (selection === null) {
    throw new Error('definition details require a selected summary');
  }
  return selection;
}
